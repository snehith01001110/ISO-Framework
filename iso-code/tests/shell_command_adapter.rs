mod common;

use assert_matches::assert_matches;
use iso_code::{Config, CreateOptions, DeleteOptions, Manager, ShellCommandAdapter, WorktreeError};

use common::create_test_repo;

#[test]
fn post_create_command_runs_in_worktree() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("feature-wt");

    let adapter = ShellCommandAdapter::new().with_post_create("touch .setup-done");
    let mgr =
        Manager::with_adapter(repo.path(), Config::default(), Some(Box::new(adapter))).unwrap();

    let mut opts = CreateOptions::default();
    opts.setup = true;
    mgr.create("feature-branch", &wt_path, opts).unwrap();

    assert!(
        wt_path.join(".setup-done").exists(),
        "post_create must have created .setup-done in the worktree"
    );
}

#[test]
fn pre_delete_command_runs_before_worktree_removal() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("del-wt");

    let adapter = ShellCommandAdapter::new()
        .with_post_create("echo ok")
        // Write a signal file one level up (the repo root) so it survives
        // after the worktree directory is removed.
        .with_pre_delete("touch ../pre-delete-ran");
    let mgr =
        Manager::with_adapter(repo.path(), Config::default(), Some(Box::new(adapter))).unwrap();

    let mut opts = CreateOptions::default();
    opts.setup = true;
    let (handle, _) = mgr.create("del-branch", &wt_path, opts).unwrap();

    mgr.delete(&handle, DeleteOptions::default()).unwrap();

    assert!(
        repo.path().join("pre-delete-ran").exists(),
        "pre_delete must have run before worktree removal"
    );
}

#[test]
fn post_delete_command_runs_in_repo_root() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("post-del-wt");

    let adapter = ShellCommandAdapter::new()
        .with_post_create("echo ok")
        .with_post_delete("touch post-delete-ran");
    let mgr =
        Manager::with_adapter(repo.path(), Config::default(), Some(Box::new(adapter))).unwrap();

    let mut opts = CreateOptions::default();
    opts.setup = true;
    let (handle, _) = mgr.create("post-del-branch", &wt_path, opts).unwrap();

    mgr.delete(&handle, DeleteOptions::default()).unwrap();

    assert!(
        repo.path().join("post-delete-ran").exists(),
        "post_delete must have created the signal file in the repo root"
    );
}

#[test]
fn failed_post_create_rolls_back_worktree() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("fail-wt");

    let adapter = ShellCommandAdapter::new().with_post_create("exit 1");
    let mgr =
        Manager::with_adapter(repo.path(), Config::default(), Some(Box::new(adapter))).unwrap();

    let mut opts = CreateOptions::default();
    opts.setup = true;
    let result = mgr.create("fail-branch", &wt_path, opts);

    assert_matches!(result, Err(WorktreeError::AdapterSetupFailed { .. }));
    assert!(
        !wt_path.exists(),
        "worktree directory must be rolled back on setup failure"
    );
    // The primary worktree (repo root) always appears in the list; only check
    // that the failed branch is absent.
    let has_failed = mgr
        .list()
        .unwrap()
        .iter()
        .any(|w| w.branch == "fail-branch");
    assert!(!has_failed, "failed worktree must not appear in the list");
}

#[test]
fn iso_code_env_vars_visible_to_post_create() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("env-wt");

    // ISO_CODE_PATH is injected by the Manager into the subprocess environment
    // before calling setup; the subprocess writes it to a file we can check.
    let adapter =
        ShellCommandAdapter::new().with_post_create("echo \"$ISO_CODE_PATH\" > .iso-path.txt");
    let mgr =
        Manager::with_adapter(repo.path(), Config::default(), Some(Box::new(adapter))).unwrap();

    let mut opts = CreateOptions::default();
    opts.setup = true;
    mgr.create("env-branch", &wt_path, opts).unwrap();

    let recorded = std::fs::read_to_string(wt_path.join(".iso-path.txt"))
        .unwrap()
        .trim()
        .to_string();
    assert!(
        recorded.ends_with("env-wt"),
        "ISO_CODE_PATH must point to the new worktree; got {recorded:?}"
    );
}

#[test]
fn iso_code_path_available_in_pre_delete() {
    let repo = create_test_repo();
    let wt_path = repo.path().join("teardown-env-wt");

    // ISO_CODE_PATH is replayed by the adapter from its setup-time snapshot.
    // It's unique per worktree so this test is safe against concurrent runners.
    let adapter = ShellCommandAdapter::new()
        .with_post_create("echo ok")
        .with_pre_delete("echo \"$ISO_CODE_PATH\" > ../iso-path-teardown.txt");
    let mgr =
        Manager::with_adapter(repo.path(), Config::default(), Some(Box::new(adapter))).unwrap();

    let mut opts = CreateOptions::default();
    opts.setup = true;
    let (handle, _) = mgr.create("teardown-env-branch", &wt_path, opts).unwrap();

    mgr.delete(&handle, DeleteOptions::default()).unwrap();

    let recorded = std::fs::read_to_string(repo.path().join("iso-path-teardown.txt"))
        .unwrap()
        .trim()
        .to_string();
    assert!(
        recorded.ends_with("teardown-env-wt"),
        "ISO_CODE_PATH must be available in pre_delete; got {recorded:?}"
    );
}
