# Story 1.10: State Persistence

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want worktree metadata to be durably persisted across process restarts with protection against concurrent access and crash corruption so that no state is lost even under adversarial conditions.

## Description
Implement the full state persistence layer defined in PRD Sections 9 and 10: `state.json` v2 schema read/write, the hardened fd-lock protocol with multi-factor identity (PID + start_time + UUID + hostname), Full Jitter exponential backoff for lock contention, PID-reuse detection via `sysinfo`, and atomic write (tmp -> fsync -> rename). This is the backbone that all other Manager operations depend on for durability.

## Acceptance Criteria
- [ ] `state.json` is stored at `<repo>/.git/iso-code/state.json`
- [ ] `state.lock` is stored at `<repo>/.git/iso-code/state.lock`
- [ ] `state.json` conforms to the v2 schema from PRD Section 10.2
- [ ] Unknown fields in `state.json` are preserved via `#[serde(flatten)]` with catch-all `HashMap` (forward compatibility)
- [ ] Lock file contains JSON with `pid`, `start_time`, `uuid`, `hostname`, `acquired_at`
- [ ] Four-factor stale detection: PID existence check, start_time comparison via `sysinfo`, UUID correlation, hostname logging
- [ ] PID-reuse detection: if PID is alive but `start_time` differs from lock record, lock is stale
- [ ] Full Jitter backoff: `sleep_ms = random(0, min(2000, 10 * 2^attempt))`, max 15 attempts (~30s)
- [ ] Lock acquisition timeout returns `StateLockContention` with `timeout_ms`
- [ ] Atomic write sequence: write to `state.json.tmp`, `fsync()`, `rename(tmp -> state.json)`
- [ ] Lock is never held across `git worktree add` (Appendix A rule 7)
- [ ] Lock file is never deleted after releasing -- left in place for next acquisition to overwrite
- [ ] Corrupt `state.json` triggers rebuild from `git worktree list` with logged warning
- [ ] Schema migration from v1 to v2 is implemented
- [ ] `ISO_CODE_HOME` environment variable overrides all state file paths
- [ ] `directories` crate is used for user config/cache paths

## Tasks
- [ ] Implement `state.json` v2 data structures in `state.rs` with serde derives and `#[serde(flatten)]` catch-all
- [ ] Implement `state.lock` record structure: `LockRecord { pid, start_time, uuid, hostname, acquired_at }`
- [ ] Implement `check_stale()` four-factor stale detection per PRD Section 9.3
- [ ] Integrate `sysinfo` crate for cross-platform process start time retrieval
- [ ] Implement `acquire_lock_with_backoff()` per PRD Section 9.4 with Full Jitter formula
- [ ] Implement `try_acquire()` using `fd-lock` crate for exclusive advisory lock
- [ ] Implement atomic write: `write(state.json.tmp)` -> `fsync()` -> `rename(state.json.tmp, state.json)`
- [ ] Implement `read_state()` and `write_state()` functions with lock-around-read-modify-write pattern (PRD Section 9.5)
- [ ] Implement schema migration: `migrate(raw) -> StateV2` with v1->v2 transform per PRD Section 10.5
- [ ] Handle corrupt state.json: catch `serde` parse errors, rebuild from `git worktree list`, log warning
- [ ] Implement `ISO_CODE_HOME` override: read env var, redirect all paths
- [ ] Implement network filesystem detection for lock degradation (PRD Section 9.6): skip advisory lock, use atomic rename only
- [ ] Create `<repo>/.git/iso-code/` directory at `Manager::new()` if absent
- [ ] Write concurrency tests: two processes competing for the lock

## Technical Notes
- PRD Section 9.1: PID-only locks fail because Linux PIDs cycle within ~32768; containers can reuse PID 1 within seconds
- PRD Section 9.2: lock file format is specified JSON with exact fields
- PRD Section 9.3: four-factor stale detection order: absent -> corrupt -> dead PID -> start_time mismatch
- PRD Section 9.4: Full Jitter formula: `base_ms = 10`, `cap_ms = 2000`, `max_attempts = 15`
- PRD Section 9.5: nine-step lock acquisition sequence, must not be reordered
- PRD Section 9.5 critical invariant: "Never delete state.lock after releasing. Leave it in place."
- PRD Section 9.5 critical invariant: "Never hold state.lock across git worktree add."
- PRD Section 9.6: network FS degrades to atomic rename only, skip flock
- PRD Section 10.1: file locations use `directories` crate v6.0.0
- PRD Section 10.2: `stale_worktrees`, `port_leases`, `gc_history`, `config_snapshot` are all v2 fields
- PRD Section 10.5: migration function handles `schema_version` 1 and 2; unknown versions return `StateCorrupted`

## Test Hints
- QA-C-003: two processes acquire lock sequentially (no deadlock, no corruption)
- QA-C-005: crash mid-write (SIGKILL between write and rename) does not corrupt state.json
- QA-C-008: PID-reuse scenario -- lock held by dead process with reused PID is correctly identified as stale
- Unit test: Full Jitter formula produces values within expected range
- Unit test: stale detection correctly identifies dead PID (ESRCH)
- Unit test: stale detection correctly identifies PID reuse (same PID, different start_time)
- Unit test: schema v1 migrates to v2 correctly
- Unit test: unknown schema version returns `StateCorrupted`
- Unit test: corrupt JSON triggers rebuild
- Integration test: atomic write survives SIGKILL between fsync and rename

## Dependencies
ISO-1.1, ISO-1.2

## Estimated Effort
XL

## Priority
P0

## Traceability
- PRD: Section 9 (Locking Protocol), Section 10 (State Persistence)
- FR: FR-P0-003 (state durability)
- Appendix A invariant: Rule 7 (lock scope is state.json read-modify-write only), Rule 8 (evict to stale)
- Bug regression: N/A (preventive -- addresses PID-reuse class of bugs)
- QA ref: QA-C-003, QA-C-005, QA-C-008
