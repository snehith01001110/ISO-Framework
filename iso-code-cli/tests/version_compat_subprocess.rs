//! Subprocess variants of QA-V-008 and QA-V-009.
//!
//! Lives in the `iso-code-cli` crate so `assert_cmd::Command::cargo_bin("wt")`
//! can find the `wt` binary (cargo only sets `CARGO_BIN_EXE_*` for binaries
//! declared in the test's own package).

use std::path::Path;
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

fn create_test_repo() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    run_git(dir.path(), &["init", "-b", "main"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test"]);
    run_git(dir.path(), &["commit", "--allow-empty", "-m", "initial"]);
    dir
}

/// Fixture staging helper. Copies `git-<tag>` from the iso-code crate's
/// fixture dir to a tempdir as `git`, marks it executable, and returns the
/// tempdir (keep alive for the duration of the test) plus the real PATH.
fn stage_mock_git(version_tag: &str) -> (tempfile::TempDir, String) {
    // iso-code-cli tests live at .../iso-code-cli/tests/; fixtures are at
    // .../iso-code/tests/fixtures/mock-git/
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("iso-code/tests/fixtures/mock-git")
        .join(format!("git-{version_tag}"));
    assert!(
        fixture.exists(),
        "mock git fixture missing: {}",
        fixture.display()
    );
    let td = tempfile::TempDir::new().unwrap();
    let staged = td.path().join("git");
    std::fs::copy(&fixture, &staged).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&staged).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&staged, perms).unwrap();
    }

    let real_path = std::env::var("PATH").unwrap_or_default();
    (td, real_path)
}

/// QA-V-008: `wt list` with PATH pointing at a fixture that claims
/// `git version 2.19.0`. The CLI must exit non-zero and stderr must
/// mention the minimum version.
#[cfg(unix)]
#[test]
fn qa_v_008_wt_list_rejects_git_219() {
    let (mock_dir, real_path) = stage_mock_git("2.19");
    let repo = create_test_repo();

    let new_path = format!("{}:{}", mock_dir.path().display(), real_path);
    let output = assert_cmd::Command::cargo_bin("wt")
        .expect("wt binary")
        .current_dir(repo.path())
        .env("PATH", &new_path)
        .env("MOCK_REAL_PATH", &real_path)
        .args(["list"])
        .output()
        .expect("spawn wt list");

    assert!(
        !output.status.success(),
        "wt list must fail when git < 2.20 — stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("2.20") || stderr.contains("too old"),
        "stderr should mention minimum version — got:\n{stderr}"
    );
}

/// QA-V-009: `git` binary unavailable → `GitNotFound`. Empty PATH starves
/// the loader of a git binary.
#[cfg(unix)]
#[test]
fn qa_v_009_wt_list_when_git_not_installed() {
    let repo = create_test_repo();
    let output = assert_cmd::Command::cargo_bin("wt")
        .expect("wt binary")
        .current_dir(repo.path())
        .env_clear()
        .env("PATH", "")
        .args(["list"])
        .output()
        .expect("spawn wt list");

    assert!(
        !output.status.success(),
        "wt list must fail when git is unavailable"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("git not found")
            || stderr.contains("GitNotFound")
            || stderr.to_lowercase().contains("not found"),
        "stderr should surface a missing-git error — got:\n{stderr}"
    );
}
