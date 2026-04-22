//! Regression test for issue #18: `wt create`, `wt delete`, and `wt attach`
//! silently drop extra positional args, producing misleading errors.
//!
//! Before the fix, these subcommands validated only a lower bound on argv
//! count. If the user forgot to quote a path containing spaces, the shell
//! would split it into multiple tokens and the CLI would take the first
//! chunk as the path, discarding the rest. The user would then see a
//! downstream error ("path already exists", "worktree not found", etc.)
//! instead of a clear usage error.
//!
//! Fix: validate exact positional-arg count and emit the existing Usage
//! message when extras are passed.

/// Run the `wt` binary from a throwaway tempdir that is *not* a git repo.
/// This way, even if a test regresses and the binary slips past arg
/// validation into a Manager call, it cannot mutate the real repository
/// hosting the test suite.
fn wt(cwd: &std::path::Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("wt").expect("wt binary");
    cmd.current_dir(cwd);
    cmd
}

fn sandbox() -> tempfile::TempDir {
    tempfile::TempDir::new().expect("tempdir")
}

/// `wt create a b c` — three positionals where only two are expected.
/// Must fail fast with the Usage message, not proceed with `a b` and
/// ignore `c`.
#[test]
fn create_rejects_extra_positional_args() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args(["create", "feature/auth", "/tmp/some/path", "extra-arg"])
        .output()
        .expect("spawn wt");

    assert!(
        !output.status.success(),
        "wt create with extra args must exit non-zero"
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt create <branch> <path>"),
        "expected Usage message, got stderr: {stderr}"
    );
}

/// `wt delete a b` — two positionals where only one is expected.
#[test]
fn delete_rejects_extra_positional_args() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args(["delete", "/tmp/some/path", "extra-arg"])
        .output()
        .expect("spawn wt");

    assert!(
        !output.status.success(),
        "wt delete with extra args must exit non-zero"
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt delete <path>"),
        "expected Usage message, got stderr: {stderr}"
    );
}

/// `wt attach a b` — two positionals where only one is expected.
#[test]
fn attach_rejects_extra_positional_args() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args(["attach", "/tmp/some/path", "extra-arg"])
        .output()
        .expect("spawn wt");

    assert!(
        !output.status.success(),
        "wt attach with extra args must exit non-zero"
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt attach <path>"),
        "expected Usage message, got stderr: {stderr}"
    );
}

/// Scenario from the issue report: an unquoted path with spaces. The
/// shell would split this into four argv entries; previously the CLI
/// would silently take the first as the path and proceed. Now it must
/// reject with the Usage message.
#[test]
fn create_rejects_unquoted_path_with_spaces() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args([
            "create",
            "feature/auth",
            "/Users/kg/Documents/Documents",
            "-",
            "kg/projects/foo/bar",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        !output.status.success(),
        "unquoted path with spaces must exit non-zero"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt create <branch> <path>"),
        "expected Usage message (not a downstream path error), got stderr: {stderr}"
    );
    // The old (buggy) behavior produced "worktree path already exists" or
    // similar. Make sure we no longer surface that misleading error.
    assert!(
        !stderr.contains("already exists"),
        "must not fall through to a misleading downstream error: {stderr}"
    );
}

/// Missing positional args still trigger the Usage message — the
/// lower-bound behavior is preserved.
#[test]
fn create_rejects_too_few_args() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args(["create", "only-one"])
        .output()
        .expect("spawn wt");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt create <branch> <path>"),
        "stderr: {stderr}"
    );
}

#[test]
fn delete_rejects_zero_args() {
    let sb = sandbox();
    let output = wt(sb.path()).args(["delete"]).output().expect("spawn wt");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt delete <path>"),
        "stderr: {stderr}"
    );
}

#[test]
fn attach_rejects_zero_args() {
    let sb = sandbox();
    let output = wt(sb.path()).args(["attach"]).output().expect("spawn wt");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt attach <path>"),
        "stderr: {stderr}"
    );
}
