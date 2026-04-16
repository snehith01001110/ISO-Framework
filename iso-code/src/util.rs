//! Shared filesystem helpers: directory-size walk and filesystem-capacity probe.
//!
//! The .git-skipping, hardlink-deduped walk lives here so guards, Manager, and
//! downstream crates (iso-code-mcp) all agree on what counts as "worktree bytes."

use std::path::Path;

/// Sum the real on-disk size of every file under `roots`, skipping any `.git/`
/// subtree. Hardlinks are deduplicated per-walk by `(dev, ino)` on Unix.
/// Silently ignores missing roots so callers don't have to pre-filter.
pub fn dir_size_skipping_git<'a, I: IntoIterator<Item = &'a Path>>(roots: I) -> u64 {
    let mut total: u64 = 0;
    #[cfg(unix)]
    let mut seen_inodes: std::collections::HashSet<(u64, u64)> =
        std::collections::HashSet::new();

    for root in roots {
        if !root.exists() {
            continue;
        }
        for entry in jwalk::WalkDir::new(root)
            .process_read_dir(|_, _, _, children| {
                children.retain(|child| {
                    child
                        .as_ref()
                        .map(|e| e.file_name().to_string_lossy() != ".git")
                        .unwrap_or(true)
                });
            })
            .into_iter()
            .flatten()
        {
            if let Ok(meta) = std::fs::metadata(entry.path()) {
                if meta.is_file() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        if meta.nlink() > 1 && !seen_inodes.insert((meta.dev(), meta.ino())) {
                            continue;
                        }
                    }
                    total += filesize::file_real_size_fast(entry.path(), &meta)
                        .unwrap_or(meta.len());
                }
            }
        }
    }
    total
}

/// Return the total capacity in bytes of the filesystem hosting `path`.
/// Picks the disk with the longest matching mount-point prefix. Returns
/// `None` when the capacity cannot be determined (e.g. unsupported platform
/// or path outside any known mount), so callers can skip the check.
pub fn filesystem_capacity_bytes(path: &Path) -> Option<u64> {
    use sysinfo::Disks;

    let probe = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new("/")).to_path_buf()
    };

    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<(&sysinfo::Disk, usize)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if probe.starts_with(mount) {
            let len = mount.as_os_str().len();
            if best.map_or(true, |(_, cur)| len > cur) {
                best = Some((disk, len));
            }
        }
    }
    best.map(|(d, _)| d.total_space())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn dir_size_skips_git_dir() {
        // Scope the check at the .git exclusion, not absolute byte counts —
        // filesystems like APFS/ext4 round small-file block sizes up (a 5-byte
        // file reports as 4KB on-disk), so we assert ordering rather than
        // equality.
        let dir = TempDir::new().unwrap();
        let git = dir.path().join(".git");
        std::fs::create_dir_all(&git).unwrap();
        // A large blob under .git that would dominate the total if not skipped.
        std::fs::write(git.join("HEAD"), vec![0u8; 100_000]).unwrap();
        std::fs::write(dir.path().join("a.txt"), b"hello").unwrap();

        let size = dir_size_skipping_git([dir.path()].iter().copied());
        assert!(
            size < 100_000,
            "size {size} must exclude the 100KB blob under .git"
        );
        assert!(size > 0, "size must count a.txt");
    }

    #[test]
    fn dir_size_missing_root_is_zero() {
        let p = std::path::Path::new("/tmp/definitely-not-here-xyz-1234567890");
        let size = dir_size_skipping_git([p].iter().copied());
        assert_eq!(size, 0);
    }

    #[test]
    fn filesystem_capacity_is_some_for_tmp() {
        let cap = filesystem_capacity_bytes(std::env::temp_dir().as_path());
        assert!(cap.is_some_and(|c| c > 0), "expected positive capacity");
    }
}
