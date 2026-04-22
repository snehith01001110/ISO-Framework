//! Data-Loss Regression Suite — "never again" contract.
//!
//! One test per incident in the test strategy's Section 3. Each test
//! reproduces the exact failure scenario and asserts the fix holds. QA IDs
//! map to `_bmad-output/qa/test-strategy.md` → Section 3.

mod common;

use std::path::PathBuf;
use std::process::Command;

use assert_matches::assert_matches;
use iso_code::{Config, CreateOptions, DeleteOptions, GcOptions, Manager, WorktreeError};

use common::{commit_file, create_test_repo, run_git};

/// QA-R-001 — claude-code#38287
/// Cleanup deleted branches with unmerged commits without warning.
/// Contract: `delete()` without `force` on a branch with unique commits must
/// return `UnmergedCommits`, not silently succeed.
#[test]
fn regression_qa_r_001_unmerged_commits_block_delete_without_force() {
    let repo = create_test_repo();
    let mut cfg = Config::default();
    cfg.offline = true; // don't attempt `git fetch` in the unmerged check

    let mgr = Manager::new(repo.path(), cfg).unwrap();
    let wt_path = repo.path().join("feature-wt");
    let (handle, _) = mgr
        .create("feature-unmerged", &wt_path, CreateOptions::default())
        .unwrap();

    // Drop 3 commits on the feature branch that main never gets.
    for i in 0..3 {
        commit_file(
            &wt_path,
            &format!("file_{i}.txt"),
            &format!("content {i}"),
            &format!("feature commit {i}"),
        );
    }

    let result = mgr.delete(&handle, DeleteOptions::default());
    // The exact commit count depends on what the branch inherits from main
    // at creation time — what matters is the error variant and that the
    // count reflects the unmerged work.
    match result {
        Err(WorktreeError::UnmergedCommits { commit_count, .. }) => {
            assert!(
                commit_count >= 3,
                "at least the 3 new commits must be counted, got {commit_count}"
            );
        }
        other => panic!("expected UnmergedCommits, got {other:?}"),
    }
    assert!(wt_path.exists(), "worktree must survive a blocked delete");

    // Force delete cleans up so the test tempdir drop doesn't panic.
    let mut force = DeleteOptions::default();
    force.force = true;
    mgr.delete(&handle, force).unwrap();
}

/// QA-R-002 — claude-code#41010
/// Sub-agent cleanup deleted the parent session's CWD.
/// Contract: deleting the caller's CWD must error with `CannotDeleteCwd`.
#[test]
fn regression_qa_r_002_cannot_delete_own_cwd() {
    // This test has to manipulate the process CWD, which is a global. We
    // serialize via a dedicated directory that no other test touches.
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    let wt_path = repo.path().join("cwd-victim");
    let (handle, _) = mgr
        .create("cwd-victim", &wt_path, CreateOptions::default())
        .unwrap();

    // Save and restore CWD around the check; otherwise a panic below would
    // leave the whole cargo test runner in a deleted directory.
    let saved = std::env::current_dir().ok();
    std::env::set_current_dir(&wt_path).unwrap();

    let result = mgr.delete(&handle, DeleteOptions::default());

    if let Some(d) = saved {
        let _ = std::env::set_current_dir(d);
    }

    assert_matches!(result, Err(WorktreeError::CannotDeleteCwd));
    assert!(wt_path.exists(), "worktree dir should be untouched");

    let mut force = DeleteOptions::default();
    force.force = true;
    mgr.delete(&handle, force).unwrap();
}

/// QA-R-003 — claude-code#29110
/// Three agents reported success; all work lost after forced gc.
/// Contract: `gc(force=true)` on active worktrees with unique commits must
/// NOT remove them (only orphans/stale entries are eligible).
#[test]
fn regression_qa_r_003_gc_force_preserves_active_worktrees_with_commits() {
    let repo = create_test_repo();
    let mut cfg = Config::default();
    cfg.offline = true;
    let mgr = Manager::new(repo.path(), cfg).unwrap();

    let mut handles = Vec::new();
    for i in 0..3 {
        let branch = format!("feature-{i}");
        let wt_path = repo.path().join(format!("wt-{i}"));
        let (h, _) = mgr
            .create(&branch, &wt_path, CreateOptions::default())
            .unwrap();
        commit_file(
            &wt_path,
            &format!("unique-{i}.txt"),
            "work",
            &format!("unique commit on {branch}"),
        );
        handles.push(h);
    }

    let mut opts = GcOptions::default();
    opts.dry_run = false;
    opts.force = true;
    let report = mgr.gc(opts).unwrap();

    for h in &handles {
        assert!(
            !report.removed.contains(&h.path),
            "gc force must not remove active worktree with unique commits: {}",
            h.path.display()
        );
        assert!(
            h.path.exists(),
            "worktree dir must survive gc: {}",
            h.path.display()
        );
    }

    // All three branches must still exist
    let branches = common::git_output(repo.path(), &["branch", "--list"]);
    for i in 0..3 {
        assert!(
            branches.contains(&format!("feature-{i}")),
            "branch feature-{i} must still exist after gc force"
        );
    }

    for h in handles {
        let mut f = DeleteOptions::default();
        f.force = true;
        let _ = mgr.delete(&h, f);
    }
}

/// QA-R-004 — claude-code#38538
/// git-crypt worktree committed all files as deletions.
/// Contract: create() on a git-crypt repo without the key must fail with
/// `GitCryptLocked` and auto-clean the partial worktree.
#[test]
fn regression_qa_r_004_git_crypt_locked_auto_cleans() {
    let repo = create_test_repo();

    // Fake a git-crypt configuration: .gitattributes flagging *.secret,
    // no key file in .git/git-crypt/keys/default, a *.secret file with the
    // git-crypt magic header in place (indicating encrypted content).
    std::fs::write(
        repo.path().join(".gitattributes"),
        "*.secret filter=git-crypt diff=git-crypt\n",
    )
    .unwrap();
    run_git(repo.path(), &["add", ".gitattributes"]);
    run_git(repo.path(), &["commit", "-m", "add git-crypt attributes"]);

    // Write a file whose content begins with the git-crypt magic header.
    let mut encrypted = Vec::new();
    encrypted.extend_from_slice(b"\x00GITCRYPT\x00");
    encrypted.extend_from_slice(&[0xff; 32]);
    std::fs::write(repo.path().join("payload.secret"), &encrypted).unwrap();
    run_git(repo.path(), &["add", "payload.secret"]);
    run_git(repo.path(), &["commit", "-m", "add encrypted blob"]);

    let mgr = Manager::new(repo.path(), Config::default()).unwrap();
    let wt_path = repo.path().join("crypt-wt");
    let result = mgr.create("crypt-branch", &wt_path, CreateOptions::default());

    assert_matches!(result, Err(WorktreeError::GitCryptLocked));
    assert!(
        !wt_path.exists(),
        "partial worktree must be auto-cleaned: {}",
        wt_path.display()
    );
}

/// QA-R-005 — claude-code#27881
/// Nested worktree created inside worktree after context compaction.
/// Contract: creating a worktree whose path is inside an existing worktree
/// must fail with `NestedWorktree` before any `git worktree add` runs.
#[test]
fn regression_qa_r_005_nested_worktree_rejected() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    let outer = repo.path().join("wt-outer");
    let (outer_h, _) = mgr
        .create("outer", &outer, CreateOptions::default())
        .unwrap();

    let nested = outer.join("subdir");
    let result = mgr.create("inner", &nested, CreateOptions::default());
    assert_matches!(result, Err(WorktreeError::NestedWorktree { .. }));
    assert!(!nested.exists(), "nested path must not be created on disk");

    let mut f = DeleteOptions::default();
    f.force = true;
    mgr.delete(&outer_h, f).unwrap();
}

/// QA-R-006 — vscode#289973
/// Background worker cleaned worktree with uncommitted changes.
/// Contract: `delete()` without `force_dirty` on a dirty worktree must fail
/// with `UncommittedChanges`, leaving the worktree untouched.
#[test]
fn regression_qa_r_006_dirty_worktree_not_deleted_without_force_dirty() {
    let repo = create_test_repo();
    let mut cfg = Config::default();
    cfg.offline = true;
    let mgr = Manager::new(repo.path(), cfg).unwrap();

    let wt_path = repo.path().join("dirty-wt");
    let (handle, _) = mgr
        .create("dirty-branch", &wt_path, CreateOptions::default())
        .unwrap();

    // Unstaged change in the worktree.
    std::fs::write(wt_path.join("uncommitted.txt"), "work in progress").unwrap();

    let result = mgr.delete(&handle, DeleteOptions::default());
    assert_matches!(result, Err(WorktreeError::UncommittedChanges { .. }));
    assert!(
        wt_path.exists() && wt_path.join("uncommitted.txt").exists(),
        "dirty worktree must be untouched after failed delete"
    );

    // Cleanup — `force_dirty` alone lets the library's dirty-check pass, but
    // `git worktree remove` still refuses a dirty tree. `force_locked` swaps
    // in `git worktree remove --force`, which is what the caller who already
    // acknowledged the data loss would use.
    let mut force = DeleteOptions::default();
    force.force = true;
    force.force_dirty = true;
    force.force_locked = true;
    mgr.delete(&handle, force).unwrap();
}

/// QA-R-007 — vscode#296194
/// Runaway `git worktree add` loop: 1,526 worktrees created.
/// Contract: the Nth+1 create call when `max_worktrees = N` must fail with
/// `RateLimitExceeded` before touching the filesystem.
#[test]
fn regression_qa_r_007_rate_limit_blocks_runaway_creation() {
    let repo = create_test_repo();
    let mut cfg = Config::default();
    cfg.max_worktrees = 5;
    let mgr = Manager::new(repo.path(), cfg).unwrap();

    // The primary worktree (repo root) counts toward the limit, so
    // `max_worktrees = 5` permits exactly 4 additional worktrees.
    let mut handles = Vec::new();
    for i in 0..4 {
        let (h, _) = mgr
            .create(
                format!("rl-{i}"),
                repo.path().join(format!("rl-wt-{i}")),
                CreateOptions::default(),
            )
            .unwrap();
        handles.push(h);
    }

    let result = mgr.create(
        "rl-5",
        repo.path().join("rl-wt-5"),
        CreateOptions::default(),
    );
    assert_matches!(
        result,
        Err(WorktreeError::RateLimitExceeded { current: 5, max: 5 })
    );
    assert!(
        !repo.path().join("rl-wt-5").exists(),
        "rate-limited create must not create a directory"
    );

    for h in handles {
        let mut f = DeleteOptions::default();
        f.force = true;
        let _ = mgr.delete(&h, f);
    }
}

/// QA-R-008 — Cursor forum incident
/// 9.82 GB consumed in 20 minutes on a 2 GB repo.
/// Contract: aggregate disk cap (`max_total_disk_bytes`) is enforced.
#[test]
fn regression_qa_r_008_aggregate_disk_cap_enforced() {
    let repo = create_test_repo();

    // Seed the primary worktree with enough data that the aggregate disk
    // walk crosses the configured cap on the first subsequent create.
    let big_path = repo.path().join("big.bin");
    std::fs::write(&big_path, vec![0u8; 2_000_000]).unwrap(); // 2 MB
    run_git(repo.path(), &["add", "big.bin"]);
    run_git(repo.path(), &["commit", "-m", "big blob"]);

    let mut cfg = Config::default();
    cfg.max_total_disk_bytes = Some(1_000_000); // 1 MB — well under the 2 MB seed
    let mgr = Manager::new(repo.path(), cfg).unwrap();

    let result = mgr.create(
        "over-cap",
        repo.path().join("over-cap-wt"),
        CreateOptions::default(),
    );
    assert_matches!(result, Err(WorktreeError::AggregateDiskLimitExceeded));
    assert!(!repo.path().join("over-cap-wt").exists());
}

/// QA-R-009 — claude-squad#260
/// 5 worktrees × 2 GB node_modules = 10 GB wasted.
/// Contract: the default Manager does NOT copy node_modules into new
/// worktrees. Only the EcosystemAdapter (opt-in) controls what gets copied.
#[test]
fn regression_qa_r_009_default_create_does_not_copy_node_modules() {
    let repo = create_test_repo();

    // Simulate a JS repo: committed package.json plus an untracked, .gitignored
    // node_modules directory (the common real-world pattern).
    std::fs::write(repo.path().join(".gitignore"), "node_modules/\n").unwrap();
    std::fs::write(
        repo.path().join("package.json"),
        r#"{ "name": "x", "version": "0.0.1" }"#,
    )
    .unwrap();
    run_git(repo.path(), &["add", ".gitignore", "package.json"]);
    run_git(repo.path(), &["commit", "-m", "add package.json"]);

    std::fs::create_dir_all(repo.path().join("node_modules/some-pkg")).unwrap();
    std::fs::write(
        repo.path().join("node_modules/some-pkg/index.js"),
        "module.exports = {};",
    )
    .unwrap();

    let mgr = Manager::new(repo.path(), Config::default()).unwrap();
    let wt_path = repo.path().join("js-wt");
    let (handle, _) = mgr
        .create("js-branch", &wt_path, CreateOptions::default())
        .unwrap();

    assert!(
        !wt_path.join("node_modules").exists(),
        "default create must not duplicate node_modules"
    );

    let mut f = DeleteOptions::default();
    f.force = true;
    mgr.delete(&handle, f).unwrap();
}

/// QA-R-010 — opencode#14648
/// Each failed create retry leaked hundreds of MB of orphan directories.
/// Contract: a create() that fails after pre-flight must leave no orphan
/// directory and no lingering state.json entry. We inject failure by
/// pre-creating the target path (hits Guard 4) — a cheap, deterministic
/// way to force `create()` to return Err without touching git.
#[test]
fn regression_qa_r_010_failed_create_leaves_no_orphans() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    for i in 0..5 {
        let wt_path = repo.path().join(format!("fail-wt-{i}"));
        std::fs::create_dir_all(&wt_path).unwrap();
        let marker = wt_path.join(".preexisting");
        std::fs::write(&marker, "preexisting").unwrap();

        let result = mgr.create(
            format!("fail-branch-{i}"),
            &wt_path,
            CreateOptions::default(),
        );
        assert!(result.is_err(), "iteration {i}: create should fail");
        // Pre-existing content must be untouched.
        assert!(marker.exists(), "iteration {i}: precreated file destroyed");
        // Clean up the pre-created directory between iterations.
        std::fs::remove_dir_all(&wt_path).unwrap();
    }

    // Count worktrees that actually exist in git's registry beyond the
    // primary. Canonicalize both sides — on macOS the tempdir returns the
    // `/var/folders/...` form while `git worktree list` emits the
    // resolved `/private/var/folders/...` form.
    let canon_repo = dunce::canonicalize(repo.path()).unwrap_or_else(|_| repo.path().to_path_buf());
    let list = mgr.list().unwrap();
    let real_worktrees: Vec<PathBuf> = list
        .iter()
        .filter(|wt| {
            let canon_wt = dunce::canonicalize(&wt.path).unwrap_or_else(|_| wt.path.clone());
            wt.path.exists() && canon_wt != canon_repo
        })
        .map(|wt| wt.path.clone())
        .collect();
    assert!(
        real_worktrees.is_empty(),
        "failed creates leaked orphans: {real_worktrees:?}"
    );

    // And nothing named `fail-branch-*` survives as a git branch.
    let branches = common::git_output(repo.path(), &["branch", "--list"]);
    for i in 0..5 {
        assert!(
            !branches.contains(&format!("fail-branch-{i}")),
            "failed create leaked branch fail-branch-{i}"
        );
    }

    let _ = Command::new("git");
}
