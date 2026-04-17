//! GC stress tests covering large orphan batches and locked-worktree safety.
//!
//! The large-scale tests are marked `#[ignore]`; run explicitly with:
//!
//! ```text
//! cargo test --test stress_gc_orphans -- --ignored
//! ```

mod common;

use std::process::Command;

use iso_code::{Config, CreateOptions, DeleteOptions, GcOptions, Manager};

use common::create_test_repo;

/// QA-S-001 (companion): GC must correctly handle a mix of fresh and locked
/// worktrees. Scaled-down variant that runs under the default `cargo test`.
#[test]
fn stress_gc_small_scale() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    // Create 5 regular worktrees
    let mut handles = Vec::new();
    for i in 0..5 {
        let branch = format!("gc-stress-{i}");
        let wt_path = repo.path().join(format!("gc-wt-{i}"));
        let (handle, _) = mgr
            .create(&branch, &wt_path, CreateOptions::default())
            .unwrap();
        handles.push(handle);
    }

    // Create 2 locked worktrees
    let mut locked_handles = Vec::new();
    for i in 0..2 {
        let branch = format!("gc-locked-{i}");
        let wt_path = repo.path().join(format!("gc-locked-wt-{i}"));
        let mut opts = CreateOptions::default();
        opts.lock = true;
        let (handle, _) = mgr.create(&branch, &wt_path, opts).unwrap();
        locked_handles.push(handle);
    }

    // Dry run GC — should not remove anything
    let report = mgr.gc(GcOptions::default()).unwrap();
    assert!(report.dry_run);
    assert!(report.removed.is_empty());

    // Locked worktrees must not be in evicted or orphans
    for lh in &locked_handles {
        assert!(
            !report.evicted.contains(&lh.path),
            "locked worktree must not be evicted"
        );
        assert!(
            !report.removed.contains(&lh.path),
            "locked worktree must not be removed"
        );
    }

    // Cleanup regular worktrees
    for h in handles {
        let mut o = DeleteOptions::default(); o.force = true; let _ = mgr.delete(&h, o);
    }
    // Cleanup locked worktrees
    for h in locked_handles {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", h.path.to_str().unwrap()])
            .current_dir(repo.path())
            .output();
    }
}

/// QA-S-001 (companion): large-scale orphan GC. Creates 1,000 worktrees via
/// the `git` CLI (bypassing the manager) and asserts that `gc()` completes
/// within 60s. Ship criterion for Milestone 1.
#[test]
#[ignore]
fn stress_gc_1000_orphans() {
    let repo = create_test_repo();

    // Create 1000 worktrees via git CLI (simulating external tool orphans)
    let mut wt_paths = Vec::new();
    for i in 0..1000 {
        let branch = format!("orphan-{i}");
        let wt_path = repo.path().join(format!("orphan-wt-{i}"));
        Command::new("git")
            .args([
                "worktree",
                "add",
                wt_path.to_str().unwrap(),
                "-b",
                &branch,
            ])
            .current_dir(repo.path())
            .output()
            .unwrap();
        wt_paths.push(wt_path);
    }

    let mgr = Manager::new(repo.path(), Config::default()).unwrap();
    let list = mgr.list().unwrap();
    // +1 for main worktree
    assert!(list.len() >= 1000, "Expected at least 1000 worktrees");

    // GC dry run — should identify orphaned/prunable worktrees
    let start = std::time::Instant::now();
    let mut go = GcOptions::default(); go.dry_run = true; go.force = true; let report = mgr.gc(go).unwrap();
    let elapsed = start.elapsed();

    eprintln!("GC of {} worktrees took {:?}", list.len(), elapsed);
    assert!(elapsed.as_secs() < 60, "GC should complete within 60 seconds");

    // Cleanup
    for wt_path in &wt_paths {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", wt_path.to_str().unwrap()])
            .current_dir(repo.path())
            .output();
    }

    let _ = report;
}

/// QA-I-005 (Cursor integration target) companion: locked worktrees in a
/// GC batch are never touched, regardless of `force`. Appendix A rule 13.
#[test]
fn stress_gc_locked_worktrees_untouched() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    // Create 3 locked worktrees
    let mut locked_paths = Vec::new();
    for i in 0..3 {
        let branch = format!("locked-stress-{i}");
        let wt_path = repo.path().join(format!("locked-stress-wt-{i}"));
        let mut opts = CreateOptions::default();
        opts.lock = true;
        mgr.create(&branch, &wt_path, opts).unwrap();
        locked_paths.push(wt_path);
    }

    // GC with force=true — locked must still survive
    let mut o = GcOptions::default();
    o.dry_run = false;
    o.force = true;
    let report = mgr.gc(o).unwrap();

    for p in &locked_paths {
        assert!(p.exists(), "locked worktree must still exist after gc: {}", p.display());
        assert!(
            !report.removed.contains(p),
            "locked worktree must not be in removed: {}",
            p.display()
        );
    }

    // Cleanup
    for p in &locked_paths {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", p.to_str().unwrap()])
            .current_dir(repo.path())
            .output();
    }
}
