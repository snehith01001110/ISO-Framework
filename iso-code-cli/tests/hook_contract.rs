//! QA-H-001 — `wt hook --stdin-format claude-code` contract test.
//!
//! Regression test for claude-code#27467: the wrapper shell pipe
//! `cd "$(wt hook ... )"` breaks if anything but a single absolute path
//! appears on stdout. This test pipes JSON to the binary and asserts the
//! precise byte-level output contract.

use std::path::Path;

fn run_git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
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

#[allow(dead_code)]
fn create_test_repo() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    run_git(dir.path(), &["init", "-b", "main"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test"]);
    run_git(dir.path(), &["commit", "--allow-empty", "-m", "initial"]);
    dir
}

/// QA-H-001: `wt hook --stdin-format claude-code` must emit exactly one
/// newline-terminated absolute path on stdout and nothing else.
#[test]
fn hook_claude_code_emits_single_absolute_path_on_stdout() {
    // Nest the repo one directory deeper so the hook's target path (parent
    // of the repo) is also unique across test runs.
    let outer = tempfile::TempDir::new().unwrap();
    let repo_path = outer.path().join("repo");
    std::fs::create_dir_all(&repo_path).unwrap();
    let repo = {
        run_git(&repo_path, &["init", "-b", "main"]);
        run_git(&repo_path, &["config", "user.email", "test@example.com"]);
        run_git(&repo_path, &["config", "user.name", "Test"]);
        run_git(&repo_path, &["commit", "--allow-empty", "-m", "initial"]);
        repo_path
    };
    let repo_root = repo.to_string_lossy().into_owned();

    let payload = serde_json::json!({
        "session_id": "test-session",
        "cwd": repo_root,
        "hook_event_name": "WorktreeCreate",
        "name": "hook-test-branch",
    })
    .to_string();

    let output = assert_cmd::Command::cargo_bin("wt")
        .expect("wt binary")
        .args(["hook", "--stdin-format", "claude-code"])
        .write_stdin(payload)
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "wt hook exited with non-zero: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout must be valid UTF-8");
    assert!(
        stdout.ends_with('\n'),
        "stdout must be newline-terminated, got {stdout:?}"
    );
    let path_str = stdout.trim_end_matches('\n');

    // Exactly one line — no embedded newlines.
    assert!(
        !path_str.contains('\n'),
        "stdout must be exactly one line, got {stdout:?}"
    );
    assert!(!path_str.is_empty(), "path must be non-empty");

    // Must be an absolute path.
    let path = Path::new(path_str);
    assert!(
        path.is_absolute(),
        "stdout path must be absolute, got {path_str}"
    );
    assert!(path.exists(), "stdout path must exist on disk: {path_str}");

    // stderr is expected to contain progress messages (per PRD) — its
    // presence shouldn't fail the test, but we assert it's non-empty so a
    // future regression that moves progress to stdout is caught.
    assert!(
        !output.stderr.is_empty(),
        "progress should go to stderr, not stdout"
    );

    // The absolute path must also appear in `git worktree list --porcelain`.
    let list = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(&repo)
        .output()
        .expect("git worktree list")
        .stdout;
    let list = String::from_utf8_lossy(&list);
    assert!(
        list.contains(path_str),
        "new worktree must appear in git worktree list — list:\n{list}\npath: {path_str}"
    );
}

/// Defensive sub-test: if the binary is asked with an unsupported
/// `--stdin-format`, it must exit non-zero and not write to stdout.
#[test]
fn hook_rejects_unsupported_stdin_format() {
    let output = assert_cmd::Command::cargo_bin("wt")
        .expect("wt binary")
        .args(["hook", "--stdin-format", "mystery-format"])
        .write_stdin("{}")
        .output()
        .expect("spawn wt");

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "nothing should land on stdout for unsupported format"
    );
}
