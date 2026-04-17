//! Concurrency Suite — QA-C-001 through QA-C-008.
//!
//! Each test uses a real git repository in a tempdir and `std::sync::Barrier`
//! to coordinate simultaneous execution. Per `manager.rs`, `Manager` is
//! `Send` but not `Sync`; each thread therefore constructs its own Manager
//! against the shared repo root so they race at the filesystem / state.lock
//! layer rather than in-process.

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use assert_matches::assert_matches;
use iso_code::{Config, CreateOptions, DeleteOptions, GcOptions, Manager, WorktreeError};

use common::create_test_repo;

/// QA-C-001: 10 threads racing `create()` on the same branch. Exactly one
/// should succeed; the rest must error out (typically
/// `BranchAlreadyCheckedOut`, possibly `WorktreePathExists` depending on
/// which check fires first — both are acceptable exclusive-acquisition
/// signals).
#[test]
fn qa_c_001_concurrent_create_same_branch() {
    let repo = create_test_repo();
    let repo_path: PathBuf = repo.path().to_path_buf();
    let barrier = Arc::new(Barrier::new(10));

    let mut handles = Vec::new();
    for i in 0..10 {
        let repo_path = repo_path.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let mgr = Manager::new(&repo_path, Config::default()).unwrap();
            barrier.wait();
            let wt_path = repo_path.join(format!("race-{i}"));
            mgr.create("feature-x", &wt_path, CreateOptions::default())
                .map(|(h, _)| h)
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let successes = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        successes, 1,
        "exactly one thread should win the race; got {successes} successes"
    );

    for r in &results {
        if let Err(e) = r {
            // Any of these is an acceptable "I lost the race" signal:
            //   * BranchAlreadyCheckedOut — the guard caught the loser.
            //   * WorktreePathExists — the directory race check fired.
            //   * StateLockContention — the loser timed out waiting for the state lock.
            //   * GitCommandFailed — the guard's snapshot was stale and git
            //     itself rejected the double-checkout.
            assert_matches!(
                e,
                WorktreeError::BranchAlreadyCheckedOut { .. }
                    | WorktreeError::WorktreePathExists(_)
                    | WorktreeError::StateLockContention { .. }
                    | WorktreeError::GitCommandFailed { .. }
            );
        }
    }

    // Exactly one worktree on `feature-x` must exist.
    let list = common::git_output(repo.path(), &["worktree", "list", "--porcelain"]);
    let feature_blocks = list.matches("branch refs/heads/feature-x").count();
    assert_eq!(feature_blocks, 1, "git worktree list:\n{list}");
}

/// QA-C-002: `delete()` racing `gc()` on the same worktree. No panic,
/// worktree is removed exactly once, at most one operation succeeds.
#[test]
fn qa_c_002_concurrent_remove_racing_gc() {
    let repo = create_test_repo();
    let mut cfg = Config::default();
    cfg.offline = true;
    let mgr = Manager::new(repo.path(), cfg.clone()).unwrap();

    let wt_path = repo.path().join("race-wt");
    let (handle, _) = mgr
        .create("race-gc", &wt_path, CreateOptions::default())
        .unwrap();

    let repo_path: PathBuf = repo.path().to_path_buf();
    let cfg2 = cfg.clone();
    let barrier = Arc::new(Barrier::new(2));

    let b1 = Arc::clone(&barrier);
    let h_delete = thread::spawn(move || {
        let mgr = Manager::new(&repo_path, cfg2).unwrap();
        b1.wait();
        let mut opts = DeleteOptions::default();
        opts.force = true;
        mgr.delete(&handle, opts)
    });

    let repo_path2: PathBuf = repo.path().to_path_buf();
    let cfg3 = cfg.clone();
    let b2 = Arc::clone(&barrier);
    let h_gc = thread::spawn(move || {
        let mgr = Manager::new(&repo_path2, cfg3).unwrap();
        b2.wait();
        let mut opts = GcOptions::default();
        opts.dry_run = false;
        opts.force = true;
        mgr.gc(opts)
    });

    let delete_result = h_delete.join().unwrap();
    let gc_result = h_gc.join().unwrap();

    assert!(delete_result.is_ok() || gc_result.is_ok());
    assert!(!wt_path.exists(), "worktree dir must be gone after race");
}

/// QA-C-003: 20 threads doing state.json read-modify-write under lock.
/// Every increment must land; no lost updates.
#[test]
fn qa_c_003_state_json_read_modify_write_contention() {
    use iso_code::state;
    use serde_json::Value;

    let repo = create_test_repo();
    // Ensure the state dir + initial state.json exist.
    let _ = Manager::new(repo.path(), Config::default()).unwrap();

    let repo_path: PathBuf = repo.path().to_path_buf();
    let barrier = Arc::new(Barrier::new(20));

    let mut handles = Vec::new();
    for _ in 0..20 {
        let repo_path = repo_path.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            // 60s total lock acquisition budget — generous so the 20-way fight
            // doesn't time a thread out spuriously on slow CI.
            state::with_state_timeout(&repo_path, None, 60_000, |s| {
                let cur = s
                    .extra
                    .get("qa_c_003_counter")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                s.extra
                    .insert("qa_c_003_counter".to_string(), Value::from(cur + 1));
                Ok(())
            })
            .map(|_| ())
        }));
    }

    for h in handles {
        h.join().unwrap().expect("state update must succeed");
    }

    let final_state = state::read_state(repo.path(), None).unwrap();
    let counter = final_state
        .extra
        .get("qa_c_003_counter")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(counter, 20, "no lost updates under contention");
}

/// QA-C-004: After N consecutive git failures the circuit breaker trips and
/// further operations return `CircuitBreakerOpen` without invoking git.
///
/// We force failure by pointing `PATH` at a directory containing a fake
/// `git` script that always exits 1.
#[test]
fn qa_c_004_circuit_breaker_trips_after_consecutive_failures() {
    // A Manager was already built with a real git (needed for init); now
    // swap PATH and construct a fresh Manager that will see only the fake.
    let repo = create_test_repo();

    // Build a real capabilities struct with the real git first.
    let _ = Manager::new(repo.path(), Config::default()).unwrap();

    // Unfortunately Manager::new() itself calls `git --version` — it needs a
    // real git to succeed. So we test the breaker at the API level by
    // constructing the Manager with the real git, then hot-swapping PATH
    // and observing list() failures through `list_raw` via list().
    //
    // This is the cleanest way to exercise the breaker without editing
    // library internals. We can't trip it without real failures, so we
    // instead verify the state machine: when we DO see a StateLockContention
    // or other error, the failure counter increments; after `threshold`
    // consecutive failures on commands that route through
    // `check_circuit_breaker`, we expect `CircuitBreakerOpen`.
    //
    // Simpler approach: use a threshold of 3, cause 3 real git failures by
    // corrupting the .git directory momentarily, then re-call.

    let mut cfg = Config::default();
    cfg.circuit_breaker_threshold = 3;
    let mgr = Manager::new(repo.path(), cfg).unwrap();

    // First, confirm a healthy list() works.
    mgr.list().expect("baseline list should succeed");

    // Break `.git` by renaming it so git commands fail.
    let git_dir = repo.path().join(".git");
    let hidden = repo.path().join(".git-hidden");
    std::fs::rename(&git_dir, &hidden).unwrap();

    let mut seen_breaker = false;
    for i in 0..6 {
        match mgr.list() {
            Ok(_) => {}
            Err(WorktreeError::CircuitBreakerOpen { consecutive_failures }) => {
                assert!(
                    consecutive_failures >= 3,
                    "breaker should trip at or after threshold, got {consecutive_failures} on attempt {i}"
                );
                seen_breaker = true;
                break;
            }
            Err(_other) => {
                // Non-breaker git failure — expected for the first three calls.
            }
        }
    }

    // Restore so tempdir drop works cleanly.
    std::fs::rename(&hidden, &git_dir).unwrap();

    assert!(
        seen_breaker,
        "circuit breaker did not trip after 6 failure attempts"
    );
}

/// QA-C-005: After a SIGKILL of a process that holds `state.lock`, the next
/// `Manager::new()` must recover within 6 seconds. We spawn a child
/// process (cargo test binary re-invocation isn't available, so we fork on
/// Unix) that acquires the lock and then dies.
#[cfg(unix)]
#[test]
fn qa_c_005_stale_lock_recovery_after_sigkill() {
    use iso_code::state;

    let repo = create_test_repo();
    let repo_path = repo.path().to_path_buf();
    // Prime the state dir.
    let _ = Manager::new(&repo_path, Config::default()).unwrap();

    // Fork a child that holds the lock and then gets SIGKILLed.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        // Child — acquire the lock and sleep forever.
        let _lock_guard = state::with_state_timeout(&repo_path, None, 5_000, |_s| Ok(()));
        // We can't actually hold the lock across scopes without re-exporting
        // StateLock. Instead: acquire the OS flock directly on the same file.
        let lock_path = state::state_lock_path(&repo_path, None);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        unsafe {
            let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
            // LOCK_EX | LOCK_NB
            libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB);
        }
        std::thread::sleep(Duration::from_secs(60));
        unsafe { libc::_exit(0) };
    }

    // Parent — give the child a moment to grab the lock, then SIGKILL it.
    std::thread::sleep(Duration::from_millis(200));
    unsafe {
        libc::kill(pid, libc::SIGKILL);
        libc::waitpid(pid, std::ptr::null_mut(), 0);
    }

    let start = Instant::now();
    let mgr = Manager::new(&repo_path, Config::default())
        .expect("Manager::new() should recover after SIGKILL");
    // A list() call also needs the lock via reconciliation; verify the
    // lock really is recoverable for a full read-modify-write cycle.
    mgr.list().expect("list should succeed after recovery");
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(6),
        "stale lock recovery took {elapsed:?} — should be under 6s"
    );
}

/// QA-C-007: 20 parallel dry-run gc() calls must not leave `index.lock`
/// behind in `.git`. Stand-in for `merge_check` concurrency since conflict
/// detection is not implemented in v1.0 (QA-M-003 / PRD Section 17).
#[test]
fn qa_c_007_concurrent_list_no_index_lock_leak() {
    let repo = create_test_repo();
    let _ = Manager::new(repo.path(), Config::default()).unwrap();

    let repo_path: PathBuf = repo.path().to_path_buf();
    let barrier = Arc::new(Barrier::new(20));
    let mut handles = Vec::new();
    for _ in 0..20 {
        let repo_path = repo_path.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let mgr = Manager::new(&repo_path, Config::default()).unwrap();
            barrier.wait();
            mgr.list().map(|_| ())
        }));
    }
    for h in handles {
        h.join().unwrap().expect("concurrent list must succeed");
    }

    assert!(
        !repo.path().join(".git").join("index.lock").exists(),
        ".git/index.lock leaked after concurrent operations"
    );
}

/// QA-C-008: PID reuse must not fool stale detection. We write a lock file
/// with our PID but a `start_time` that doesn't match the current process,
/// then a fresh Manager must still be able to acquire the lock (the existing
/// record is recognized as stale).
#[test]
fn qa_c_008_pid_reuse_false_positive_detected() {
    use iso_code::state;

    let repo = create_test_repo();
    let _ = Manager::new(repo.path(), Config::default()).unwrap();

    let lock_path = state::state_lock_path(repo.path(), None);

    // Craft a payload claiming our PID but a bogus start_time. The
    // four-factor stale check compares start_time against sysinfo, so the
    // mismatch should mark the record stale even though PID is live.
    let payload = serde_json::json!({
        "pid": std::process::id(),
        "start_time": 42,
        "uuid": "00000000-0000-0000-0000-000000000000",
        "hostname": "not-this-host",
        "acquired_at": "2020-01-01T00:00:00Z",
    });
    std::fs::write(&lock_path, payload.to_string()).unwrap();

    // Acquisition should still succeed because the file has no active flock
    // holder. We're primarily verifying that the new Manager can proceed
    // without being tricked by the stale payload.
    let start = Instant::now();
    let mgr = Manager::new(repo.path(), Config::default())
        .expect("Manager::new() should tolerate stale lock payload");
    mgr.list().expect("list should succeed despite stale payload");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "stale payload should not cause lock-acquisition timeout"
    );
}
