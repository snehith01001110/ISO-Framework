use std::path::Path;
use std::process::Command;

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    if !out.status.success() {
        panic!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

fn create_test_repo() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    run_git(dir.path(), &["init", "-b", "main"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test"]);
    run_git(dir.path(), &["commit", "--allow-empty", "-m", "initial"]);
    dir
}

fn wt(cwd: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("wt").expect("wt binary");
    cmd.current_dir(cwd);
    cmd
}

// ── shell-command adapter ────────────────────────────────────────────────────

#[test]
fn setup_with_shell_command_adapter_runs_post_create() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("feature-wt");

    std::fs::write(
        repo.path().join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .setup-done\"\n",
    )
    .unwrap();

    let out = wt(repo.path())
        .args([
            "create",
            "feature-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "wt create --setup failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        wt_path.join(".setup-done").exists(),
        "post_create must have created .setup-done"
    );
}

#[test]
fn setup_with_shell_command_records_adapter_in_state() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("state-wt");

    std::fs::write(
        repo.path().join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"echo ok\"\n",
    )
    .unwrap();

    let out = wt(repo.path())
        .args([
            "create",
            "state-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // state.json should record the adapter name
    let state_path = repo.path().join(".git").join("iso-code").join("state.json");
    let state = std::fs::read_to_string(&state_path).unwrap();
    assert!(
        state.contains("shell-command"),
        "state.json must record adapter name; got:\n{state}"
    );
}

// ── default adapter ──────────────────────────────────────────────────────────

#[test]
fn setup_with_default_adapter_copies_env_file() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("default-wt");

    std::fs::write(repo.path().join(".env"), "API_KEY=secret").unwrap();
    std::fs::write(
        repo.path().join(".iso-code.toml"),
        "[adapter]\ntype = \"default\"\nfiles_to_copy = [\".env\"]\n",
    )
    .unwrap();

    let out = wt(repo.path())
        .args([
            "create",
            "default-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        wt_path.join(".env").exists(),
        "DefaultAdapter must have copied .env into the new worktree"
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env")).unwrap(),
        "API_KEY=secret"
    );
}

// ── no-config warning ────────────────────────────────────────────────────────

#[test]
fn setup_without_config_warns_and_succeeds() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("no-config-wt");

    // No .iso-code.toml written — adapter is absent.

    let out = wt(repo.path())
        .args([
            "create",
            "no-config-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "--setup without config must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(wt_path.exists(), "worktree directory must still be created");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Warning") && stderr.contains("no adapter configured"),
        "must warn on stderr; got: {stderr}"
    );
}

// ── without --setup flag ─────────────────────────────────────────────────────

#[test]
fn create_without_setup_does_not_run_adapter() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("no-setup-wt");

    std::fs::write(
        repo.path().join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .setup-done\"\n",
    )
    .unwrap();

    // No --setup flag
    let out = wt(repo.path())
        .args(["create", "no-setup-branch", wt_path.to_str().unwrap()])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !wt_path.join(".setup-done").exists(),
        "adapter must NOT run when --setup is absent"
    );
}

// ── stdout purity ────────────────────────────────────────────────────────────

#[test]
fn stdout_contains_only_worktree_path_when_setup_runs() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("clean-stdout-wt");

    // Use a post_create that emits output — it must NOT leak to stdout.
    std::fs::write(
        repo.path().join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"echo THIS_MUST_NOT_APPEAR_ON_STDOUT\"\n",
    )
    .unwrap();

    let out = wt(repo.path())
        .args([
            "create",
            "clean-stdout-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).expect("stdout must be UTF-8");
    assert!(
        !stdout.contains("THIS_MUST_NOT_APPEAR_ON_STDOUT"),
        "adapter output must not leak to stdout; got: {stdout:?}"
    );
    // stdout must be exactly one newline-terminated absolute path
    let path_str = stdout.trim_end_matches('\n');
    assert!(
        !path_str.contains('\n'),
        "stdout must be one line: {stdout:?}"
    );
    assert!(
        Path::new(path_str).is_absolute(),
        "stdout must be an absolute path: {path_str}"
    );
}

// ── project-local config takes precedence ───────────────────────────────────

#[test]
fn project_local_config_takes_precedence_over_user_config() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("precedence-wt");

    // Write a project-local config that creates .project-config-ran
    std::fs::write(
        repo.path().join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .project-config-ran\"\n",
    )
    .unwrap();

    // Simulate a user config by setting HOME to a temp dir that has a
    // config.toml with a different marker.
    let fake_home = tempfile::TempDir::new().unwrap();
    let user_cfg_dir = fake_home.path().join(".config").join("iso-code");
    std::fs::create_dir_all(&user_cfg_dir).unwrap();
    std::fs::write(
        user_cfg_dir.join("config.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .user-config-ran\"\n",
    )
    .unwrap();

    let out = wt(repo.path())
        .env("HOME", fake_home.path())
        .args([
            "create",
            "precedence-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        wt_path.join(".project-config-ran").exists(),
        "project-local config must win"
    );
    assert!(
        !wt_path.join(".user-config-ran").exists(),
        "user config must not run when project-local config is present"
    );
}

// ── user-level config fallback ───────────────────────────────────────────────

#[test]
fn user_config_used_when_no_project_local_config() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("user-cfg-wt");

    // No .iso-code.toml in repo root.
    let fake_home = tempfile::TempDir::new().unwrap();
    let user_cfg_dir = fake_home.path().join(".config").join("iso-code");
    std::fs::create_dir_all(&user_cfg_dir).unwrap();
    std::fs::write(
        user_cfg_dir.join("config.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"touch .user-setup-done\"\n",
    )
    .unwrap();

    let out = wt(repo.path())
        .env("HOME", fake_home.path())
        .args([
            "create",
            "user-cfg-branch",
            wt_path.to_str().unwrap(),
            "--setup",
        ])
        .output()
        .expect("spawn wt");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        wt_path.join(".user-setup-done").exists(),
        "user-level config must be used as fallback"
    );
}
