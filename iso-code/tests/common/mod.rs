//! Shared test helpers for integration and regression tests.
//!
//! Each file under `tests/` compiles as its own binary, so utilities that
//! multiple test files need live in this module and are pulled in with
//! `mod common;` inside each test file.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

/// Build a fresh git repository with a single empty commit on `main`.
pub fn create_test_repo() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    run_git(dir.path(), &["init", "-b", "main"]);
    // CI runners rarely have user.name / user.email set globally; set them
    // locally so `git commit` succeeds without further configuration.
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test"]);
    run_git(dir.path(), &["commit", "--allow-empty", "-m", "initial"]);
    dir
}

/// Run a git command in `dir`, panicking on failure.
pub fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?}: {e}"));
    if !out.status.success() {
        panic!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Run a git command and return its stdout as a String.
pub fn git_output(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?}: {e}"));
    if !out.status.success() {
        panic!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Write a file in the repo and commit it on the current branch.
pub fn commit_file(dir: &Path, rel_path: &str, contents: &str, message: &str) {
    let full = dir.join(rel_path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&full, contents).unwrap();
    run_git(dir, &["add", rel_path]);
    run_git(dir, &["commit", "-m", message]);
}
