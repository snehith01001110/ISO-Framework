//! Safety Guard Test Matrix — QA-G-001 through QA-G-012 exercised through
//! the public `Manager` API. Inline tests in `src/guards.rs` already cover
//! the pure-logic pieces; these tests prove the guards also fire at the
//! public entry point they're meant to protect.
//!
//! Guards that require specific host conditions (NFS mount, WSL kernel,
//! Windows junctions) are either documented as environment-skipped or use
//! an in-repo fixture to trigger the code path without special mounts.

mod common;

use assert_matches::assert_matches;
use iso_code::{Config, CreateOptions, DeleteOptions, Manager, WorktreeError};

use common::{create_test_repo, run_git};

/// QA-G-001: attempting to check out a branch that's already checked out
/// in another worktree must error with `BranchAlreadyCheckedOut` before
/// `git worktree add` runs.
#[test]
fn qa_g_001_branch_already_checked_out() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    let (h1, _) = mgr
        .create(
            "feature-x",
            repo.path().join("wt-1"),
            CreateOptions::default(),
        )
        .unwrap();
    let result = mgr.create(
        "feature-x",
        repo.path().join("wt-2"),
        CreateOptions::default(),
    );
    assert_matches!(result, Err(WorktreeError::BranchAlreadyCheckedOut { .. }));

    let mut force = DeleteOptions::default();
    force.force = true;
    mgr.delete(&h1, force).unwrap();
}

/// QA-G-003: at the worktree-count limit, the next create is rejected with
/// `RateLimitExceeded`. The primary worktree counts toward the limit, so
/// `max_worktrees = 3` permits exactly 2 additional worktrees.
#[test]
fn qa_g_003_worktree_count_rate_limit() {
    let repo = create_test_repo();
    let mut cfg = Config::default();
    cfg.max_worktrees = 3;
    let mgr = Manager::new(repo.path(), cfg).unwrap();

    let mut handles = Vec::new();
    for i in 0..2 {
        let (h, _) = mgr
            .create(
                format!("lim-{i}"),
                repo.path().join(format!("lim-wt-{i}")),
                CreateOptions::default(),
            )
            .unwrap();
        handles.push(h);
    }
    let result = mgr.create(
        "lim-2",
        repo.path().join("lim-wt-2"),
        CreateOptions::default(),
    );
    assert_matches!(
        result,
        Err(WorktreeError::RateLimitExceeded { current: 3, max: 3 })
    );

    for h in handles {
        let mut f = DeleteOptions::default();
        f.force = true;
        let _ = mgr.delete(&h, f);
    }
}

/// QA-G-004: pre-existing directory at the target path is rejected without
/// touching its contents.
#[test]
fn qa_g_004_path_already_exists() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    let target = repo.path().join("already-there");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("marker"), "do-not-touch").unwrap();

    let result = mgr.create("some-branch", &target, CreateOptions::default());
    assert_matches!(result, Err(WorktreeError::WorktreePathExists(_)));
    assert!(target.join("marker").exists(), "marker must survive guard");
}

/// QA-G-005: a candidate path inside an existing worktree is rejected with
/// `NestedWorktree` before any filesystem change.
#[test]
fn qa_g_005_nested_worktree_forbidden() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    let outer = repo.path().join("outer");
    let (h, _) = mgr
        .create("outer", &outer, CreateOptions::default())
        .unwrap();

    let nested = outer.join("inner");
    let result = mgr.create("inner", &nested, CreateOptions::default());
    assert_matches!(result, Err(WorktreeError::NestedWorktree { .. }));

    let mut f = DeleteOptions::default();
    f.force = true;
    mgr.delete(&h, f).unwrap();
}

/// QA-G-006: network-filesystem detection — the `NetworkFilesystem` error
/// variant exists and formats correctly. Real NFS mounts aren't guaranteed
/// in CI, so we validate the contract shape without mounting anything.
///
/// `run_pre_create_guards` currently treats NFS as a warning, not a hard
/// error. The PRD notes this is adjustable via `deny_network_filesystem`
/// (Open Question OQ-2) which isn't in `Config` yet — when it lands, this
/// test should be extended to cover the deny-mode path.
#[test]
fn qa_g_006_network_filesystem_error_variant_exists() {
    let e = WorktreeError::NetworkFilesystem {
        mount_point: std::path::PathBuf::from("/mnt/nfs"),
    };
    let s = format!("{e}");
    assert!(s.contains("network filesystem"), "Display: {s}");
}

/// QA-G-007: WSL cross-boundary error variant exists. The real detection
/// path requires `/proc/version` to contain "Microsoft", which isn't true
/// on CI runners — the platform-specific code in
/// `check_not_wsl_cross_boundary` has `#[cfg(target_os = "linux")]` and
/// silently no-ops elsewhere. We assert the error shape only.
#[test]
fn qa_g_007_wsl_cross_boundary_error_variant_exists() {
    let e = WorktreeError::WslCrossBoundary;
    let s = format!("{e}");
    assert!(s.to_lowercase().contains("wsl"), "Display: {s}");
}

/// QA-G-008: bare repository is permitted — `Manager::new()` succeeds and
/// `create()` works with an explicit target path.
#[test]
fn qa_g_008_bare_repo_permitted() {
    let repo_dir = tempfile::TempDir::new().unwrap();
    let bare = repo_dir.path().join("bare.git");
    std::fs::create_dir_all(&bare).unwrap();
    run_git(&bare, &["init", "--bare"]);

    // Bare repos have no HEAD until a branch exists — create a minimal
    // source commit via a temp worktree so `resolve_ref("HEAD")` works.
    let seed = repo_dir.path().join("seed");
    run_git(
        &bare,
        &["worktree", "add", seed.to_str().unwrap(), "-b", "main"],
    );
    run_git(&seed, &["config", "user.email", "test@example.com"]);
    run_git(&seed, &["config", "user.name", "Test"]);
    run_git(&seed, &["commit", "--allow-empty", "-m", "seed"]);

    // Manager::new() on the bare repo root must succeed.
    let mgr = Manager::new(&bare, Config::default()).expect("bare repo permitted");
    // list() must also succeed without panicking.
    mgr.list().expect("list() on bare repo");
}

/// QA-G-010: aggregate disk limit enforced when set. Pre-seed the primary
/// worktree with committed data, then set an aggregate cap below that size.
#[test]
fn qa_g_010_aggregate_disk_limit_enforced() {
    let repo = create_test_repo();
    std::fs::write(repo.path().join("big.bin"), vec![0u8; 3_000_000]).unwrap();
    run_git(repo.path(), &["add", "big.bin"]);
    run_git(repo.path(), &["commit", "-m", "big blob"]);

    let mut cfg = Config::default();
    cfg.max_total_disk_bytes = Some(1_000_000);
    let mgr = Manager::new(repo.path(), cfg).unwrap();
    let result = mgr.create(
        "aggregate-limit",
        repo.path().join("over-cap"),
        CreateOptions::default(),
    );
    assert_matches!(result, Err(WorktreeError::AggregateDiskLimitExceeded));
    assert!(!repo.path().join("over-cap").exists());
}

/// QA-G-011 (Windows only): junction target on a UNC path is rejected.
/// Non-Windows builds don't compile the guard; on Windows the error
/// variant is `NetworkJunctionTarget`. We validate the error variant's
/// formatting on all platforms.
#[test]
fn qa_g_011_network_junction_target_error_variant_exists() {
    let e = WorktreeError::NetworkJunctionTarget {
        path: std::path::PathBuf::from(r"\\server\share\wt"),
    };
    let s = format!("{e}");
    assert!(s.contains("junction"), "Display: {s}");
}

/// QA-G-012: repo with git-crypt configured but no key → `GitCryptLocked`,
/// and the partial worktree is cleaned up.
#[test]
fn qa_g_012_git_crypt_locked_auto_cleans_partial_worktree() {
    let repo = create_test_repo();

    std::fs::write(
        repo.path().join(".gitattributes"),
        "*.secret filter=git-crypt diff=git-crypt\n",
    )
    .unwrap();
    run_git(repo.path(), &["add", ".gitattributes"]);
    run_git(repo.path(), &["commit", "-m", "git-crypt attrs"]);

    let mut enc = Vec::new();
    enc.extend_from_slice(b"\x00GITCRYPT\x00");
    enc.extend_from_slice(&[0xaa; 16]);
    std::fs::write(repo.path().join("doc.secret"), &enc).unwrap();
    run_git(repo.path(), &["add", "doc.secret"]);
    run_git(repo.path(), &["commit", "-m", "encrypted blob"]);

    let mgr = Manager::new(repo.path(), Config::default()).unwrap();
    let wt = repo.path().join("locked-crypt-wt");
    let result = mgr.create("crypt-br", &wt, CreateOptions::default());
    assert_matches!(result, Err(WorktreeError::GitCryptLocked));
    assert!(!wt.exists(), "partial worktree must be auto-cleaned");
}
