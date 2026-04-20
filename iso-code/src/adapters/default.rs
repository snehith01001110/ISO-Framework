//! `DefaultAdapter` — copies a configured list of files from the source
//! worktree into the new worktree after creation.
//!
//! Intended for un-tracked configuration (`.env`, `.env.local`, etc.) that
//! agents need to run the project but that `.gitignore` keeps out of history.
//! Missing files are logged as warnings and skipped — not errors — because a
//! stock project layout often has only a subset of the listed files.
//!
//! Copy semantics honor the caller's `ReflinkMode` via [`crate::platform::copy_file`].

use std::path::{Path, PathBuf};

use crate::adapter::{EcosystemAdapter, SetupContext};
use crate::error::WorktreeError;
use crate::platform;

/// Copies files from a configured list, resolving each relative path against
/// the source worktree on read and the new worktree on write.
///
/// PRD Section 6.1 defines the struct shape. See the module-level doc for
/// rationale on missing-file tolerance.
pub struct DefaultAdapter {
    /// Paths relative to the worktree root. Both source resolution (against
    /// `source_worktree`) and destination resolution (against `worktree_path`)
    /// use the same relative layout, so a listed `config/local.toml` will land
    /// at `<new-worktree>/config/local.toml`.
    pub files_to_copy: Vec<PathBuf>,
}

impl DefaultAdapter {
    /// Construct an adapter with the given copy list.
    pub fn new(files_to_copy: Vec<PathBuf>) -> Self {
        Self { files_to_copy }
    }
}

impl EcosystemAdapter for DefaultAdapter {
    fn name(&self) -> &str {
        "default"
    }

    fn detect(&self, worktree_path: &Path) -> bool {
        self.files_to_copy
            .iter()
            .any(|rel| worktree_path.join(rel).exists())
    }

    fn setup(
        &self,
        worktree_path: &Path,
        source_worktree: &Path,
        ctx: &SetupContext,
    ) -> Result<(), WorktreeError> {
        for rel in &self.files_to_copy {
            let src = source_worktree.join(rel);
            let dst = worktree_path.join(rel);

            if !src.exists() {
                eprintln!(
                    "[iso-code] WARNING: DefaultAdapter skipping missing source file: {}",
                    src.display()
                );
                continue;
            }

            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(WorktreeError::Io)?;
            }

            platform::copy_file(&src, &dst, ctx.reflink_mode)?;
        }
        Ok(())
    }

    fn teardown(&self, _worktree_path: &Path) -> Result<(), WorktreeError> {
        // Copied files live inside the worktree directory, which
        // `git worktree remove` deletes wholesale.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ReflinkMode;
    use std::fs;
    use tempfile::TempDir;

    /// Build a source tree + empty destination tree. Returns (source, dest).
    fn make_pair() -> (TempDir, TempDir) {
        (TempDir::new().unwrap(), TempDir::new().unwrap())
    }

    #[test]
    fn name_returns_default() {
        let adapter = DefaultAdapter::new(vec![]);
        assert_eq!(adapter.name(), "default");
    }

    #[test]
    fn setup_copies_env_file() {
        let (src, dst) = make_pair();
        fs::write(src.path().join(".env"), "API_KEY=secret").unwrap();

        let adapter = DefaultAdapter::new(vec![PathBuf::from(".env")]);
        adapter
            .setup(dst.path(), src.path(), &SetupContext::default())
            .unwrap();

        let contents = fs::read_to_string(dst.path().join(".env")).unwrap();
        assert_eq!(contents, "API_KEY=secret");
    }

    #[test]
    fn setup_skips_missing_source_without_error() {
        let (src, dst) = make_pair();
        // No files in src.
        let adapter = DefaultAdapter::new(vec![
            PathBuf::from(".env"),
            PathBuf::from(".env.local"),
        ]);

        adapter
            .setup(dst.path(), src.path(), &SetupContext::default())
            .expect("missing files must not cause setup to fail");

        assert!(!dst.path().join(".env").exists());
        assert!(!dst.path().join(".env.local").exists());
    }

    #[test]
    fn setup_creates_intermediate_directories() {
        let (src, dst) = make_pair();
        fs::create_dir_all(src.path().join("config")).unwrap();
        fs::write(src.path().join("config/local.toml"), "port = 3000").unwrap();

        let adapter = DefaultAdapter::new(vec![PathBuf::from("config/local.toml")]);
        adapter
            .setup(dst.path(), src.path(), &SetupContext::default())
            .unwrap();

        assert!(dst.path().join("config").is_dir());
        let contents = fs::read_to_string(dst.path().join("config/local.toml")).unwrap();
        assert_eq!(contents, "port = 3000");
    }

    #[test]
    fn detect_returns_false_when_no_files_exist() {
        let src = TempDir::new().unwrap();
        let adapter = DefaultAdapter::new(vec![
            PathBuf::from(".env"),
            PathBuf::from(".env.local"),
        ]);
        assert!(!adapter.detect(src.path()));
    }

    #[test]
    fn detect_returns_true_when_any_file_exists() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join(".env.local"), "").unwrap();

        let adapter = DefaultAdapter::new(vec![
            PathBuf::from(".env"),         // missing
            PathBuf::from(".env.local"),   // present
        ]);
        assert!(adapter.detect(src.path()));
    }

    #[cfg(unix)]
    #[test]
    fn setup_preserves_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let (src, dst) = make_pair();
        let src_file = src.path().join("run.sh");
        fs::write(&src_file, "#!/bin/sh\necho ok").unwrap();
        fs::set_permissions(&src_file, fs::Permissions::from_mode(0o750)).unwrap();

        let adapter = DefaultAdapter::new(vec![PathBuf::from("run.sh")]);
        adapter
            .setup(dst.path(), src.path(), &SetupContext::default())
            .unwrap();

        let dst_mode = fs::metadata(dst.path().join("run.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dst_mode, 0o750);
    }

    #[test]
    fn setup_respects_reflink_disabled() {
        // `Disabled` forces std::fs::copy; we can't observe CoW vs. standard
        // copy from user space, so we just assert the copy still succeeds and
        // the content is right. The branch coverage lives in platform::copy_file.
        let (src, dst) = make_pair();
        fs::write(src.path().join(".env"), "x=1").unwrap();

        let adapter = DefaultAdapter::new(vec![PathBuf::from(".env")]);
        adapter
            .setup(
                dst.path(),
                src.path(),
                &SetupContext::new(ReflinkMode::Disabled),
            )
            .unwrap();

        assert_eq!(
            fs::read_to_string(dst.path().join(".env")).unwrap(),
            "x=1"
        );
    }

    #[test]
    fn teardown_is_noop() {
        let dst = TempDir::new().unwrap();
        fs::write(dst.path().join(".env"), "x").unwrap();

        let adapter = DefaultAdapter::new(vec![PathBuf::from(".env")]);
        adapter.teardown(dst.path()).unwrap();

        // File is still there — teardown does nothing; worktree removal is the caller's job.
        assert!(dst.path().join(".env").exists());
    }
}
