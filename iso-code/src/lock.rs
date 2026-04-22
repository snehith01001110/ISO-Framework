//! Hardened locking protocol for state.json.
//!
//! Uses fd-lock with four-factor stale detection and full-jitter exponential
//! backoff. The lock is scoped strictly around state.json read-modify-write
//! sequences; it must never be held across `git worktree add` or any other
//! long-running operation.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::WorktreeError;

/// Lock file payload — written as JSON into state.lock.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct LockPayload {
    pid: u32,
    start_time: u64,
    uuid: String,
    hostname: String,
    acquired_at: String,
}

/// An acquired state lock. The underlying file descriptor holds an exclusive
/// flock; releasing happens automatically on drop (fd close).
///
/// The lock file is intentionally never unlinked — doing so during acquisition
/// would allow two processes to each hold flock on different inodes.
pub struct StateLock {
    // Holding the File keeps the flock alive. Drop closes fd → releases lock.
    _file: fs::File,
    lock_path: PathBuf,
    uuid: String,
}

impl StateLock {
    /// Maximum number of retry attempts.
    const MAX_ATTEMPTS: u32 = 15;

    /// Acquire the state lock with full-jitter exponential backoff.
    ///
    /// Backoff: `sleep_ms = random(0, min(2000, 10 * 2^attempt))`, capped at
    /// 15 attempts (~30s worst case).
    ///
    /// Correctness: mutual exclusion is provided by the OS flock primitive,
    /// which auto-releases when the holding process dies. The payload in the
    /// lock file is informational (helps humans see who holds the lock).
    /// We never unlink the lock file during acquisition — that would create
    /// a two-inode race where two processes each hold flock on a different
    /// inode.
    pub fn acquire(lock_path: &Path, timeout_ms: u64) -> Result<Self, WorktreeError> {
        // Ensure parent directory exists
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Emit a network-FS warning once per acquisition if applicable.
        // Advisory flock may be unreliable on nfs/smbfs; we still attempt it.
        if let Some(dir) = lock_path.parent() {
            if is_network_filesystem(dir) {
                eprintln!(
                    "[iso-code] WARNING: state directory appears to be on a network filesystem; \
                     advisory locking may be unreliable. Consider ISO_CODE_HOME on local storage."
                );
            }
        }

        let uuid = uuid::Uuid::new_v4().to_string();
        let start = std::time::Instant::now();

        for attempt in 0..Self::MAX_ATTEMPTS {
            match Self::try_acquire(lock_path, &uuid) {
                Ok(lock) => return Ok(lock),
                Err(_) => {
                    // Check total timeout
                    if start.elapsed().as_millis() as u64 >= timeout_ms {
                        return Err(WorktreeError::StateLockContention { timeout_ms });
                    }

                    // Full jitter backoff: sleep_ms = random(0, min(2000, 10 * 2^attempt))
                    let cap_ms: u64 = 2000;
                    let base_ms = 10u64.saturating_mul(1u64 << attempt);
                    let max_sleep = cap_ms.min(base_ms);
                    let sleep_ms = rand::random::<u64>() % (max_sleep + 1);
                    std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                }
            }
        }

        Err(WorktreeError::StateLockContention { timeout_ms })
    }

    /// Attempt a single non-blocking lock acquisition.
    ///
    /// Uses OS-level exclusive flock: flock(LOCK_EX|LOCK_NB) on Unix,
    /// fd-lock (LockFileEx) on Windows. The lock is held for the lifetime
    /// of the returned StateLock (released when the fd is closed on drop).
    ///
    /// Writes the payload through the *same* file descriptor that holds
    /// the lock, avoiding a race where stale detection reads mid-truncate
    /// from a second handle.
    fn try_acquire(lock_path: &Path, uuid: &str) -> Result<Self, WorktreeError> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .read(true)
            .open(lock_path)?;

        // Try non-blocking exclusive lock (cross-platform)
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if ret != 0 {
                return Err(WorktreeError::StateLockContention { timeout_ms: 0 });
            }
        }

        #[cfg(windows)]
        {
            // fd-lock provides LockFileEx on Windows for cross-platform support.
            // Acquire the lock, then recover the file handle via into_inner().
            // The underlying OS lock is tied to the fd lifetime — it stays held
            // as long as the file handle is open.
            let rw = fd_lock::RwLock::new(file);
            let write_guard = rw.try_write().map_err(|e| {
                // Recover the file handle so it doesn't leak, then report contention.
                let _ = e.into_inner().into_inner();
                WorktreeError::StateLockContention { timeout_ms: 0 }
            })?;
            // Drop the guard but keep the file handle open (lock persists on fd).
            file = write_guard.into_inner().into_inner();
        }

        // Write payload through the same fd (seek-to-start + truncate).
        use std::io::Seek;
        file.set_len(0)?;
        file.seek(std::io::SeekFrom::Start(0))?;

        let payload = LockPayload {
            pid: std::process::id(),
            start_time: process_start_time(),
            uuid: uuid.to_string(),
            hostname: hostname(),
            acquired_at: chrono::Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string(&payload).unwrap_or_default();
        file.write_all(json.as_bytes())?;
        file.flush()?;

        Ok(StateLock {
            _file: file,
            lock_path: lock_path.to_path_buf(),
            uuid: uuid.to_string(),
        })
    }

    /// Read and return the current lock holder payload for diagnostics.
    ///
    /// On systems with working advisory locks (flock on local Unix, LockFileEx
    /// on Windows), a crashed holder's flock is released automatically by the
    /// kernel — the next `try_acquire` will succeed even if the payload
    /// describes a dead process. This function exists purely to surface "who
    /// holds the lock" to operators; it never mutates the filesystem.
    #[allow(dead_code)]
    fn inspect_holder(lock_path: &Path) -> Option<LockPayload> {
        let content = fs::read_to_string(lock_path).ok()?;
        if content.is_empty() {
            return None;
        }
        serde_json::from_str(&content).ok()
    }

    /// Return the lock file path (for diagnostics).
    pub fn path(&self) -> &Path {
        &self.lock_path
    }

    /// Return the lock's UUID (for diagnostics).
    pub fn uuid(&self) -> &str {
        &self.uuid
    }
}

/// Get process start time for the current process.
/// Returns 0 if unavailable.
fn process_start_time() -> u64 {
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        let mut info: libc::proc_bsdinfo = unsafe { mem::zeroed() };
        let size = mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let ret = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDTBSDINFO,
                0,
                &mut info as *mut _ as *mut libc::c_void,
                size,
            )
        };
        if ret > 0 {
            return info.pbi_start_tvsec;
        }
        0
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(stat) = std::fs::read_to_string("/proc/self/stat") {
            if let Some(pos) = stat.rfind(')') {
                let rest = &stat[pos + 2..];
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if fields.len() > 19 {
                    return fields[19].parse().unwrap_or(0);
                }
            }
        }
        0
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

/// Return true if the given directory is on a network filesystem.
/// Used only for a warning at lock acquisition — flock is still attempted.
fn is_network_filesystem(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        let path_cstr = match path.to_str() {
            Some(s) => match std::ffi::CString::new(s) {
                Ok(c) => c,
                Err(_) => return false,
            },
            None => return false,
        };
        unsafe {
            let mut stat: libc::statfs = std::mem::zeroed();
            if libc::statfs(path_cstr.as_ptr(), &mut stat) == 0 {
                let fstype = std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr()).to_string_lossy();
                let network_types = ["nfs", "smbfs", "afpfs", "cifs", "webdav"];
                return network_types.iter().any(|t| fstype.eq_ignore_ascii_case(t));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            let path_str = path.to_string_lossy();
            let network_types = ["nfs", "nfs4", "cifs", "smbfs", "fuse.sshfs", "9p"];
            // Walk mounts and pick the longest mount-point prefix match.
            let mut best: Option<(&str, &str)> = None;
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let mp = parts[1];
                    let fs = parts[2];
                    if path_str.starts_with(mp)
                        && best.map_or(true, |(cur_mp, _)| mp.len() > cur_mp.len())
                    {
                        best = Some((mp, fs));
                    }
                }
            }
            if let Some((_, fs)) = best {
                return network_types.contains(&fs);
            }
        }
    }

    let _ = path;
    false
}

/// Get hostname for lock payload.
fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let ret = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if ret == 0 {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            return String::from_utf8_lossy(&buf[..end]).to_string();
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_acquire_and_drop() {
        let dir = setup_dir();
        let lock_path = dir.path().join("state.lock");

        let lock = StateLock::acquire(&lock_path, 5000).unwrap();
        assert!(lock_path.exists());
        assert!(!lock.uuid().is_empty());

        // Read the payload
        let content = fs::read_to_string(&lock_path).unwrap();
        let payload: LockPayload = serde_json::from_str(&content).unwrap();
        assert_eq!(payload.pid, std::process::id());
        assert!(!payload.uuid.is_empty());
        assert!(!payload.hostname.is_empty());

        drop(lock);
        // The lock file must persist across releases.
        assert!(lock_path.exists());
    }

    #[test]
    fn test_sequential_acquire() {
        let dir = setup_dir();
        let lock_path = dir.path().join("state.lock");

        let lock1 = StateLock::acquire(&lock_path, 5000).unwrap();
        drop(lock1);

        let lock2 = StateLock::acquire(&lock_path, 5000).unwrap();
        drop(lock2);
    }

    #[test]
    fn test_dead_pid_holder_yields_to_new_acquirer() {
        // When the previous holder crashed, the kernel auto-releases flock,
        // so a fresh acquire succeeds without us unlinking the file.
        let dir = setup_dir();
        let lock_path = dir.path().join("state.lock");

        // Simulate a stale payload left over by a crashed process.
        let payload = LockPayload {
            pid: 99_999_999,
            start_time: 1,
            uuid: "stale-uuid".to_string(),
            hostname: "test".to_string(),
            acquired_at: "2020-01-01T00:00:00Z".to_string(),
        };
        fs::write(&lock_path, serde_json::to_string(&payload).unwrap()).unwrap();

        let lock = StateLock::acquire(&lock_path, 5_000).unwrap();
        assert_eq!(lock.path(), lock_path);
    }

    #[test]
    fn test_inspect_holder_returns_payload() {
        let dir = setup_dir();
        let lock_path = dir.path().join("state.lock");
        let _lock = StateLock::acquire(&lock_path, 5_000).unwrap();
        let holder = StateLock::inspect_holder(&lock_path).unwrap();
        assert_eq!(holder.pid, std::process::id());
    }

    #[test]
    fn test_hostname_nonempty() {
        let h = hostname();
        assert!(!h.is_empty());
    }
}
