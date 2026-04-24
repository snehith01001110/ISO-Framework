use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::{
    adapter::{EcosystemAdapter, SetupContext},
    error::WorktreeError,
};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// ISO_CODE_* values captured at setup time so teardown can replay them.
/// Keyed by worktree path in `env_store`, allowing concurrent worktrees to
/// each carry independent env snapshots under the same adapter instance.
#[derive(Clone)]
struct CapturedEnv {
    branch: String,
    repo: String,
    name: String,
    port: String,
    uuid: String,
}

/// Runs arbitrary shell commands at worktree create and delete time.
///
/// Commands execute via the system shell (`/bin/sh -c` on Unix, `cmd /C` on
/// Windows) with all `ISO_CODE_*` environment variables and their compatibility
/// aliases injected.
///
/// # Construction
///
/// ```rust
/// use iso_code::ShellCommandAdapter;
/// let adapter = ShellCommandAdapter::new()
///     .with_post_create("npm install")
///     .with_pre_delete("npm run cleanup");
/// ```
pub struct ShellCommandAdapter {
    /// Shell command run after a new worktree is created. CWD is the worktree.
    pub post_create: Option<String>,
    /// Shell command run before worktree deletion. CWD is the worktree (still on disk).
    pub pre_delete: Option<String>,
    /// Shell command run as the final teardown step. CWD is the repository root.
    pub post_delete: Option<String>,
    /// Per-command wall-clock timeout in milliseconds. Default: 120 000 (2 min).
    pub timeout_ms: u64,
    env_store: Mutex<HashMap<PathBuf, CapturedEnv>>,
}

impl Default for ShellCommandAdapter {
    fn default() -> Self {
        Self {
            post_create: None,
            pre_delete: None,
            post_delete: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            env_store: Mutex::new(HashMap::new()),
        }
    }
}

impl ShellCommandAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_post_create(mut self, cmd: impl Into<String>) -> Self {
        self.post_create = Some(cmd.into());
        self
    }

    pub fn with_pre_delete(mut self, cmd: impl Into<String>) -> Self {
        self.pre_delete = Some(cmd.into());
        self
    }

    pub fn with_post_delete(mut self, cmd: impl Into<String>) -> Self {
        self.post_delete = Some(cmd.into());
        self
    }

    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }
}

impl EcosystemAdapter for ShellCommandAdapter {
    fn name(&self) -> &str {
        "shell-command"
    }

    fn detect(&self, _worktree_path: &Path) -> bool {
        false
    }

    fn setup(
        &self,
        worktree_path: &Path,
        _source_worktree: &Path,
        _ctx: &SetupContext,
    ) -> Result<(), WorktreeError> {
        // Snapshot the ISO_CODE_* vars injected by the Manager before this call.
        // We store them keyed by worktree path so teardown can replay them even
        // though the Manager does not re-inject them during delete.
        let captured = CapturedEnv {
            branch: std::env::var("ISO_CODE_BRANCH").unwrap_or_default(),
            repo: std::env::var("ISO_CODE_REPO").unwrap_or_default(),
            name: std::env::var("ISO_CODE_NAME").unwrap_or_default(),
            port: std::env::var("ISO_CODE_PORT").unwrap_or_default(),
            uuid: std::env::var("ISO_CODE_UUID").unwrap_or_default(),
        };

        if let Some(cmd) = &self.post_create {
            let pairs = build_env_pairs(worktree_path, Some(&captured));
            run_shell(
                cmd,
                worktree_path,
                "post_create",
                &pairs,
                self.timeout_ms,
                self.name(),
            )?;
        }

        if let Ok(mut guard) = self.env_store.lock() {
            guard.insert(worktree_path.to_path_buf(), captured);
        }

        Ok(())
    }

    fn teardown(&self, worktree_path: &Path) -> Result<(), WorktreeError> {
        // Clone env vars out before running commands — don't hold the lock
        // across a subprocess invocation.
        let captured = self
            .env_store
            .lock()
            .ok()
            .and_then(|g| g.get(worktree_path).cloned());

        let pairs = build_env_pairs(worktree_path, captured.as_ref());

        if let Some(cmd) = &self.pre_delete {
            run_shell(
                cmd,
                worktree_path,
                "pre_delete",
                &pairs,
                self.timeout_ms,
                self.name(),
            )?;
        }

        if let Some(cmd) = &self.post_delete {
            // Use the stored repo root as CWD; the worktree directory is
            // about to be removed by the caller after teardown returns.
            let cwd = captured
                .as_ref()
                .map(|e| PathBuf::from(&e.repo))
                .filter(|p| p.exists())
                .unwrap_or_else(|| {
                    worktree_path
                        .parent()
                        .unwrap_or(worktree_path)
                        .to_path_buf()
                });
            run_shell(
                cmd,
                &cwd,
                "post_delete",
                &pairs,
                self.timeout_ms,
                self.name(),
            )?;
        }

        if let Ok(mut guard) = self.env_store.lock() {
            guard.remove(worktree_path);
        }

        Ok(())
    }
}

fn build_env_pairs(worktree_path: &Path, captured: Option<&CapturedEnv>) -> Vec<(String, String)> {
    let path = worktree_path.to_string_lossy().into_owned();
    match captured {
        Some(env) => vec![
            ("ISO_CODE_PATH".into(), path.clone()),
            ("ISO_CODE_BRANCH".into(), env.branch.clone()),
            ("ISO_CODE_REPO".into(), env.repo.clone()),
            ("ISO_CODE_NAME".into(), env.name.clone()),
            ("ISO_CODE_PORT".into(), env.port.clone()),
            ("ISO_CODE_UUID".into(), env.uuid.clone()),
            ("CCMANAGER_WORKTREE_PATH".into(), path.clone()),
            ("CCMANAGER_BRANCH_NAME".into(), env.branch.clone()),
            ("CCMANAGER_GIT_ROOT".into(), env.repo.clone()),
            ("WM_WORKTREE_PATH".into(), path.clone()),
            ("WM_PROJECT_ROOT".into(), env.repo.clone()),
        ],
        None => vec![
            ("ISO_CODE_PATH".into(), path.clone()),
            ("CCMANAGER_WORKTREE_PATH".into(), path.clone()),
            ("WM_WORKTREE_PATH".into(), path),
        ],
    }
}

#[cfg(windows)]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(cmd);
    c
}

#[cfg(not(windows))]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(cmd);
    c
}

fn run_shell(
    shell_cmd: &str,
    cwd: &Path,
    phase: &str,
    env_pairs: &[(String, String)],
    timeout_ms: u64,
    adapter_name: &str,
) -> Result<(), WorktreeError> {
    let mut cmd = shell_command(shell_cmd);
    cmd.current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .envs(env_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    let mut child = cmd.spawn().map_err(WorktreeError::Io)?;

    // Drain stderr in a background thread to prevent pipe-buffer deadlock on
    // commands that emit more than ~64 KiB of diagnostic output.
    let stderr_thread = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        match child.try_wait().map_err(WorktreeError::Io)? {
            Some(status) => {
                let stderr = stderr_thread
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                if status.success() {
                    return Ok(());
                }
                return Err(WorktreeError::ShellCommandFailed {
                    exit_code: status.code().unwrap_or(-1),
                    stderr: stderr.trim().to_string(),
                });
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(WorktreeError::AdapterTimeout {
                    adapter: adapter_name.to_string(),
                    phase: phase.to_string(),
                    timeout_ms,
                });
            }
            None => std::thread::sleep(POLL_INTERVAL),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    fn ctx() -> SetupContext {
        SetupContext::default()
    }

    #[test]
    fn name_returns_shell_command() {
        assert_eq!(ShellCommandAdapter::default().name(), "shell-command");
    }

    #[test]
    fn detect_always_returns_false() {
        let dir = tmp();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert!(!ShellCommandAdapter::default().detect(dir.path()));
    }

    #[test]
    fn no_commands_set_setup_is_noop() {
        let dir = tmp();
        let adapter = ShellCommandAdapter::default();
        assert!(adapter.setup(dir.path(), dir.path(), &ctx()).is_ok());
    }

    #[test]
    fn no_commands_set_teardown_is_noop() {
        let dir = tmp();
        let adapter = ShellCommandAdapter::default();
        assert!(adapter.teardown(dir.path()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn post_create_echo_succeeds() {
        let dir = tmp();
        let adapter = ShellCommandAdapter {
            post_create: Some("echo hello".into()),
            ..ShellCommandAdapter::default()
        };
        assert!(adapter.setup(dir.path(), dir.path(), &ctx()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn post_create_creates_file_in_worktree() {
        let dir = tmp();
        let adapter = ShellCommandAdapter {
            post_create: Some("touch .setup-done".into()),
            ..ShellCommandAdapter::default()
        };
        adapter.setup(dir.path(), dir.path(), &ctx()).unwrap();
        assert!(dir.path().join(".setup-done").exists());
    }

    #[cfg(unix)]
    #[test]
    fn non_zero_exit_returns_shell_command_failed() {
        let dir = tmp();
        let adapter = ShellCommandAdapter {
            post_create: Some("exit 42".into()),
            ..ShellCommandAdapter::default()
        };
        let err = adapter.setup(dir.path(), dir.path(), &ctx()).unwrap_err();
        match err {
            WorktreeError::ShellCommandFailed { exit_code, .. } => {
                assert_eq!(exit_code, 42);
            }
            other => panic!("expected ShellCommandFailed, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn stderr_captured_in_error() {
        let dir = tmp();
        let adapter = ShellCommandAdapter {
            post_create: Some("echo 'diagnostics here' >&2; exit 1".into()),
            ..ShellCommandAdapter::default()
        };
        let err = adapter.setup(dir.path(), dir.path(), &ctx()).unwrap_err();
        match err {
            WorktreeError::ShellCommandFailed { stderr, .. } => {
                assert!(
                    stderr.contains("diagnostics here"),
                    "stderr not captured: {stderr:?}"
                );
            }
            other => panic!("expected ShellCommandFailed, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn timeout_returns_adapter_timeout() {
        let dir = tmp();
        let adapter = ShellCommandAdapter {
            post_create: Some("sleep 60".into()),
            timeout_ms: 1,
            ..ShellCommandAdapter::default()
        };
        let err = adapter.setup(dir.path(), dir.path(), &ctx()).unwrap_err();
        match err {
            WorktreeError::AdapterTimeout {
                phase, timeout_ms, ..
            } => {
                assert_eq!(phase, "post_create");
                assert_eq!(timeout_ms, 1);
            }
            other => panic!("expected AdapterTimeout, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn pre_delete_runs_in_teardown() {
        let worktree = tmp();
        let signal = tmp();
        let signal_path = signal.path().join("pre-delete-ran");
        let signal_str = signal_path.to_str().unwrap();

        let adapter = ShellCommandAdapter {
            post_create: Some("echo ok".into()),
            pre_delete: Some(format!("touch '{signal_str}'")),
            ..ShellCommandAdapter::default()
        };

        adapter
            .setup(worktree.path(), worktree.path(), &ctx())
            .unwrap();
        adapter.teardown(worktree.path()).unwrap();

        assert!(
            signal_path.exists(),
            "pre_delete must create the signal file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn env_vars_injected_into_post_create() {
        let dir = tmp();
        let out_file = dir.path().join("branch.txt");
        let out_str = out_file.to_str().unwrap();

        // ISO_CODE_BRANCH is set by the Manager before calling setup(); we set
        // it manually here to replicate that behavior in a unit test.
        std::env::set_var("ISO_CODE_BRANCH", "test-branch");

        let adapter = ShellCommandAdapter {
            post_create: Some(format!("echo \"$ISO_CODE_BRANCH\" > '{out_str}'")),
            ..ShellCommandAdapter::default()
        };

        adapter.setup(dir.path(), dir.path(), &ctx()).unwrap();
        std::env::remove_var("ISO_CODE_BRANCH");

        let contents = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            contents.trim() == "test-branch",
            "ISO_CODE_BRANCH not forwarded: {contents:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn env_vars_replayed_into_teardown_commands() {
        let worktree = tmp();
        let signal = tmp();
        let out_file = signal.path().join("branch.txt");
        let out_str = out_file.to_str().unwrap();

        std::env::set_var("ISO_CODE_BRANCH", "teardown-branch");
        std::env::set_var("ISO_CODE_REPO", worktree.path().to_str().unwrap());

        let adapter = ShellCommandAdapter {
            post_create: Some("echo ok".into()),
            pre_delete: Some(format!("echo \"$ISO_CODE_BRANCH\" > '{out_str}'")),
            ..ShellCommandAdapter::default()
        };

        adapter
            .setup(worktree.path(), worktree.path(), &ctx())
            .unwrap();
        std::env::remove_var("ISO_CODE_BRANCH");
        std::env::remove_var("ISO_CODE_REPO");

        // After remove_var the env vars are gone from the process; the adapter
        // must replay the snapshot it captured during setup.
        adapter.teardown(worktree.path()).unwrap();

        let contents = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            contents.trim() == "teardown-branch",
            "ISO_CODE_BRANCH not replayed in teardown: {contents:?}"
        );
    }
}
