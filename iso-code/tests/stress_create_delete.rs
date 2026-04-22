//! Durability stress tests — QA-S-001 ship-gate for Milestone 1.
//!
//! Exercises the zero-data-loss contract under adversarial load:
//!   * 10 threads × 10 create/delete cycles (100 total) with a random
//!     SIGKILL-injection thread running in parallel.
//!   * Stale state recovery after a process is SIGKILL'd mid-create.
//!
//! All tests here are `#[ignore]` by default because they are slow and
//! spawn real processes. Run explicitly with:
//!
//! ```text
//! cargo test --test stress_create_delete -- --ignored
//! ```

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use iso_code::{Config, CreateOptions, DeleteOptions, Manager};

use common::create_test_repo;

/// QA-S-001 (scaled): 100 sequential create/delete cycles with list()
/// reconciliation at every step. A cheap regression smoke-test; the full
/// multi-threaded SIGKILL-injection variant is `stress_100_cycles_sigkill`.
#[test]
#[ignore]
fn stress_100_create_delete_cycles_sequential() {
    let repo = create_test_repo();
    let config = Config::default();

    for i in 0..100 {
        let mgr = Manager::new(repo.path(), config.clone()).unwrap();
        let branch = format!("stress-branch-{i}");
        let wt_path = repo.path().join(format!("stress-wt-{i}"));

        let (handle, _) = mgr
            .create(&branch, &wt_path, CreateOptions::default())
            .unwrap_or_else(|e| panic!("cycle {i}: create failed: {e}"));

        assert!(wt_path.exists(), "cycle {i}: worktree dir should exist");

        let mut o = DeleteOptions::default();
        o.force = true;
        mgr.delete(&handle, o)
            .unwrap_or_else(|e| panic!("cycle {i}: delete failed: {e}"));

        assert!(!wt_path.exists(), "cycle {i}: worktree dir should be gone");

        let list = mgr
            .list()
            .unwrap_or_else(|e| panic!("cycle {i}: list failed: {e}"));
        assert!(
            !list.iter().any(|wt| wt.branch == branch),
            "cycle {i}: deleted branch still in list"
        );
    }
}

/// QA-S-001 (full): 10 threads × 10 create/delete cycles with a separate
/// SIGKILL-injection thread.
///
/// The SIGKILL thread sends SIGTERM to forked sentinel processes that each
/// briefly hold an `fd-lock` on `state.lock` — this simulates a Manager
/// process dying mid-operation. The library's stale-lock recovery (full
/// jitter backoff + four-factor identity) must bring the state back to
/// health without any worker thread observing corruption.
///
/// Pass criteria (PRD Section 11.5):
///   * Zero orphaned worktrees after the cycles complete.
///   * `state.json` is parseable at the end.
///   * No worker thread panics.
///   * Manager::new() succeeds after every cycle.
#[cfg(unix)]
#[test]
#[ignore]
fn stress_100_cycles_sigkill() {
    let repo = create_test_repo();
    let repo_path: PathBuf = repo.path().to_path_buf();

    // Warm the state dir up front.
    let _ = Manager::new(&repo_path, Config::default()).unwrap();

    let stop_sigkill = Arc::new(AtomicBool::new(false));
    let sigkill_active = Arc::new(AtomicBool::new(true));
    let sigkill_count = Arc::new(AtomicUsize::new(0));

    // SIGKILL-injection thread: in a loop, fork a sentinel child that
    // grabs the state.lock file and sleeps; parent immediately SIGKILLs
    // the child. The stale flock is released by the OS, not cleanup code.
    let stop_sk = Arc::clone(&stop_sigkill);
    let repo_sk = repo_path.clone();
    let count_sk = Arc::clone(&sigkill_count);
    let active_sk = Arc::clone(&sigkill_active);
    let sigkill_thread = thread::spawn(move || {
        use iso_code::state;

        while !stop_sk.load(Ordering::Relaxed) {
            let lock_path = state::state_lock_path(&repo_sk, None);
            let pid = unsafe { libc::fork() };
            if pid == 0 {
                // Child: grab an exclusive flock on state.lock and sleep.
                let file = match std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&lock_path)
                {
                    Ok(f) => f,
                    Err(_) => unsafe { libc::_exit(0) },
                };
                unsafe {
                    use std::os::unix::io::AsRawFd;
                    let fd = file.as_raw_fd();
                    libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB);
                }
                std::thread::sleep(Duration::from_secs(30));
                unsafe { libc::_exit(0) };
            }
            // Parent side of the fork.
            std::thread::sleep(Duration::from_millis(15));
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, std::ptr::null_mut(), 0);
            }
            count_sk.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(5));
        }
        active_sk.store(false, Ordering::Relaxed);
    });

    // 10 worker threads × 10 cycles each = 100 total.
    let barrier = Arc::new(Barrier::new(10));
    let panics = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    for w in 0..10 {
        let repo_path = repo_path.clone();
        let barrier = Arc::clone(&barrier);
        let panics = Arc::clone(&panics);
        workers.push(thread::spawn(move || {
            let result = std::panic::catch_unwind(|| {
                barrier.wait();
                for c in 0..10 {
                    let mgr = Manager::new(&repo_path, Config::default())
                        .unwrap_or_else(|e| panic!("worker {w} cycle {c}: Manager::new: {e}"));
                    let branch = format!("stress-{w}-{c}");
                    let wt_path = repo_path.join(format!("stress-wt-{w}-{c}"));

                    let (handle, _) = mgr
                        .create(&branch, &wt_path, CreateOptions::default())
                        .unwrap_or_else(|e| panic!("worker {w} cycle {c}: create: {e}"));

                    let mut o = DeleteOptions::default();
                    o.force = true;
                    mgr.delete(&handle, o)
                        .unwrap_or_else(|e| panic!("worker {w} cycle {c}: delete: {e}"));
                }
            });
            if result.is_err() {
                panics.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for w in workers {
        w.join().expect("join worker");
    }

    // Stop the SIGKILL thread.
    stop_sigkill.store(true, Ordering::Relaxed);
    sigkill_thread.join().expect("join sigkill thread");

    assert_eq!(
        panics.load(Ordering::Relaxed),
        0,
        "at least one worker thread panicked"
    );

    let injections = sigkill_count.load(Ordering::Relaxed);
    eprintln!(
        "QA-S-001: completed 100 cycles with {injections} SIGKILL injections (sigkill_active={})",
        sigkill_active.load(Ordering::Relaxed)
    );
    assert!(
        injections >= 1,
        "SIGKILL-injection thread never fired — test setup broken"
    );

    // Post-conditions.
    let mgr = Manager::new(&repo_path, Config::default())
        .expect("post-stress Manager::new() must succeed");
    let list = mgr.list().expect("post-stress list() must succeed");
    // Canonicalize both sides — `list()` returns resolved paths
    // (`/private/var/...` on macOS) while `repo_path` is whatever TempDir
    // reported (`/var/...`).
    let canon_repo = dunce::canonicalize(&repo_path).unwrap_or_else(|_| repo_path.clone());
    let non_primary: Vec<_> = list
        .iter()
        .filter(|wt| {
            let canon = dunce::canonicalize(&wt.path).unwrap_or_else(|_| wt.path.clone());
            canon != canon_repo
        })
        .collect();
    assert!(
        non_primary.is_empty(),
        "zero orphans required after stress; found {non_primary:?}"
    );

    // state.json must be parseable.
    let raw = std::fs::read(iso_code::state::state_json_path(&repo_path, None)).unwrap_or_default();
    if !raw.is_empty() {
        serde_json::from_slice::<serde_json::Value>(&raw)
            .expect("state.json must be valid JSON after stress");
    }
}

/// Recovery assertion: after forking a child that acquires the state.lock
/// and dies from SIGKILL, the parent's next `Manager::new()` must recover
/// the lock and run a full create/delete cycle without error.
///
/// This replaces the old `stress_sigkill_recovery` which forked a child
/// that did nothing — it never actually exercised recovery.
#[cfg(unix)]
#[test]
#[ignore]
fn stress_sigkill_mid_operation_recovery() {
    use iso_code::state;

    let repo = create_test_repo();
    let repo_path: PathBuf = repo.path().to_path_buf();
    let _ = Manager::new(&repo_path, Config::default()).unwrap();

    let lock_path = state::state_lock_path(&repo_path, None);

    let pid = unsafe { libc::fork() };
    if pid == 0 {
        // Child: hold the state.lock and die.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        unsafe {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB);
        }
        std::thread::sleep(Duration::from_secs(60));
        unsafe { libc::_exit(0) };
    }

    // Give the child time to grab the lock, then kill it.
    std::thread::sleep(Duration::from_millis(200));
    unsafe {
        libc::kill(pid, libc::SIGKILL);
        libc::waitpid(pid, std::ptr::null_mut(), 0);
    }

    // Parent must recover and complete a full lifecycle.
    let start = std::time::Instant::now();
    let mgr = Manager::new(&repo_path, Config::default())
        .expect("Manager::new() after sigkill must succeed");
    let (handle, _) = mgr
        .create(
            "post-kill",
            repo_path.join("post-kill-wt"),
            CreateOptions::default(),
        )
        .expect("create after sigkill must succeed");
    let mut o = DeleteOptions::default();
    o.force = true;
    mgr.delete(&handle, o)
        .expect("delete after sigkill must succeed");
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "full recovery + lifecycle should be fast: took {:?}",
        start.elapsed()
    );
}

/// State consistency after an externally removed worktree.
///
/// The old test of the same name asserted nothing (the final `let _ =
/// still_there` was discarded). This version actually verifies the library
/// reconciles state.json when git's registry goes out of sync from under
/// it: after removing a worktree via raw `git worktree remove`, the
/// Manager's next `list()` must either drop or mark the entry — and a
/// subsequent `create()` on the same branch must succeed.
#[test]
fn stress_state_consistency_after_external_removal() {
    let repo = create_test_repo();
    let mgr = Manager::new(repo.path(), Config::default()).unwrap();

    let wt_path = repo.path().join("external-removal-wt");
    let (handle, _) = mgr
        .create("ext-rm", &wt_path, CreateOptions::default())
        .unwrap();

    // External removal via raw git — state.json still references it.
    let out = std::process::Command::new("git")
        .args(["worktree", "remove", "--force", wt_path.to_str().unwrap()])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "external git remove must succeed");
    assert!(!wt_path.exists());

    // A fresh Manager must reconcile without panicking.
    let mgr2 = Manager::new(repo.path(), Config::default()).unwrap();
    let list = mgr2.list().unwrap();
    let still_there = list
        .iter()
        .any(|wt| wt.path == handle.path && wt.branch == handle.branch);
    assert!(
        !still_there,
        "externally removed worktree must not appear in list()"
    );

    // Recreation on the same branch must succeed — the library must have
    // released any state-side references that would block it.
    let (h2, _) = mgr2
        .create(
            "ext-rm",
            repo.path().join("external-removal-wt-2"),
            CreateOptions::default(),
        )
        .expect("recreate after external removal must succeed");
    let mut f = DeleteOptions::default();
    f.force = true;
    mgr2.delete(&h2, f).unwrap();
}
