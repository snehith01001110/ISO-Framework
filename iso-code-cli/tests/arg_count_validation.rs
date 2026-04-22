fn wt(cwd: &std::path::Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("wt").expect("wt binary");
    cmd.current_dir(cwd);
    cmd
}

fn sandbox() -> tempfile::TempDir {
    tempfile::TempDir::new().expect("tempdir")
}

#[test]
fn create_rejects_extra_positional_args() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args(["create", "feature/auth", "/tmp/some/path", "extra-arg"])
        .output()
        .expect("spawn wt");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt create <branch> <path>"),
        "stderr: {stderr}"
    );
}

#[test]
fn delete_rejects_extra_positional_args() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args(["delete", "/tmp/some/path", "extra-arg"])
        .output()
        .expect("spawn wt");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt delete <path>"),
        "stderr: {stderr}"
    );
}

#[test]
fn attach_rejects_extra_positional_args() {
    let sb = sandbox();
    let output = wt(sb.path())
        .args(["attach", "/tmp/some/path", "extra-arg"])
        .output()
        .expect("spawn wt");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt attach <path>"),
        "stderr: {stderr}"
    );
}

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

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: wt create <branch> <path>"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("already exists"), "stderr: {stderr}");
}

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
