use std::path::{Path, PathBuf};
use std::process::Command;

fn run_git(dir: &Path, args: &[&str]) {
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

fn make_repo() -> (tempfile::TempDir, PathBuf) {
    let outer = tempfile::TempDir::new().unwrap();
    let repo = outer.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    run_git(&repo, &["init", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "test@example.com"]);
    run_git(&repo, &["config", "user.name", "Test"]);
    run_git(&repo, &["commit", "--allow-empty", "-m", "initial"]);
    (outer, repo)
}

fn wt(repo: &Path, isolated_home: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("wt").expect("wt binary");
    cmd.current_dir(repo)
        .env("HOME", isolated_home)
        .env("XDG_CONFIG_HOME", isolated_home.join("xdg"));
    cmd
}

#[test]
fn create_setup_with_default_adapter_copies_env() {
    let (outer, repo) = make_repo();
    let home = tempfile::TempDir::new().unwrap();
    let wt_path = outer.path().join("default-wt");

    std::fs::write(repo.join(".env"), "TOKEN=secret\n").unwrap();
    std::fs::write(
        repo.join(".iso-code.toml"),
        "[adapter]\ntype = \"default\"\nfiles_to_copy = [\".env\"]\n",
    )
    .unwrap();

    let output = wt(&repo, home.path())
        .args([
            "create",
            "setup-default-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "status={:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env")).unwrap(),
        "TOKEN=secret\n"
    );

    let state = std::fs::read_to_string(repo.join(".git/iso-code/state.json")).unwrap();
    assert!(state.contains("\"adapter\": \"default\""), "{state}");
    assert!(state.contains("\"setup_complete\": true"), "{state}");
}

#[test]
fn create_setup_with_shell_command_adapter_runs_post_create() {
    let (outer, repo) = make_repo();
    let home = tempfile::TempDir::new().unwrap();
    let wt_path = outer.path().join("shell-wt");

    std::fs::write(
        repo.join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .setup-done\"\n",
    )
    .unwrap();

    let output = wt(&repo, home.path())
        .args([
            "create",
            "setup-shell-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "status={:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(wt_path.join(".setup-done").exists());
}

#[test]
fn project_local_config_takes_precedence_over_user_config() {
    let (outer, repo) = make_repo();
    let home = tempfile::TempDir::new().unwrap();
    let xdg = home.path().join("xdg");
    let user_config_dir = xdg.join("iso-code");
    std::fs::create_dir_all(&user_config_dir).unwrap();
    let wt_path = outer.path().join("precedence-wt");

    std::fs::write(
        user_config_dir.join("config.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .user-config-ran\"\n",
    )
    .unwrap();
    std::fs::write(repo.join(".env"), "PROJECT=local\n").unwrap();
    std::fs::write(
        repo.join(".iso-code.toml"),
        "[adapter]\ntype = \"default\"\nfiles_to_copy = [\".env\"]\n",
    )
    .unwrap();

    let output = wt(&repo, home.path())
        .args([
            "create",
            "setup-precedence-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "status={:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env")).unwrap(),
        "PROJECT=local\n"
    );
    assert!(
        !wt_path.join(".user-config-ran").exists(),
        "user-level config should not run when project config exists"
    );
}

#[test]
fn create_setup_without_config_warns_and_still_creates() {
    let (outer, repo) = make_repo();
    let home = tempfile::TempDir::new().unwrap();
    let wt_path = outer.path().join("no-config-wt");

    let output = wt(&repo, home.path())
        .args([
            "create",
            "setup-no-config-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "status={:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(wt_path.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--setup requested but no adapter is configured"),
        "stderr: {stderr}"
    );

    let state = std::fs::read_to_string(repo.join(".git/iso-code/state.json")).unwrap();
    assert!(state.contains("\"adapter\": null"), "{state}");
    assert!(state.contains("\"setup_complete\": false"), "{state}");
}

#[test]
fn create_without_setup_does_not_run_configured_adapter() {
    let (outer, repo) = make_repo();
    let home = tempfile::TempDir::new().unwrap();
    let wt_path = outer.path().join("no-setup-wt");

    std::fs::write(
        repo.join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .should-not-exist\"\n",
    )
    .unwrap();

    let output = wt(&repo, home.path())
        .args(["create", "no-setup-branch", wt_path.to_str().unwrap()])
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "status={:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !wt_path.join(".should-not-exist").exists(),
        "configured adapter must not run unless --setup is present"
    );

    let state = std::fs::read_to_string(repo.join(".git/iso-code/state.json")).unwrap();
    assert!(state.contains("\"adapter\": null"), "{state}");
    assert!(state.contains("\"setup_complete\": false"), "{state}");
}

#[test]
fn create_setup_keeps_adapter_output_off_stdout() {
    let (outer, repo) = make_repo();
    let home = tempfile::TempDir::new().unwrap();
    let wt_path = outer.path().join("clean-stdout-wt");

    std::fs::write(
        repo.join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"echo noisy-adapter-output\"\n",
    )
    .unwrap();

    let output = wt(&repo, home.path())
        .args([
            "create",
            "clean-stdout-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "status={:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected_path = std::fs::canonicalize(&wt_path).unwrap();
    assert_eq!(stdout, format!("{}\n", expected_path.display()));
    assert!(
        !stdout.contains("noisy-adapter-output"),
        "adapter output leaked to stdout: {stdout:?}"
    );
}
