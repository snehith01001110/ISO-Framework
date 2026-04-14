# worktree-core Test Strategy

| Field | Value |
|---|---|
| **Project** | worktree-core |
| **PRD Version** | ISO_PRD-v1.5 |
| **Author** | QA Agent |
| **Date** | 2026-04-13 |
| **Status** | DRAFT |

---

## 1. Testing Layers

### 1.1 Unit Tests

**Purpose:** Validate pure logic functions in isolation -- no git binary, no filesystem side effects. Every function in the safety-critical modules must have branch-level coverage. Unit tests are the fastest feedback loop and run on every `cargo check` cycle.

**Tooling:** `#[cfg(test)]` inline modules, `tempfile` crate for any path construction, `assert_matches` for error variant discrimination.

**Where tests live:** `src/guards.rs` (inline `#[cfg(test)]` module), `src/state.rs` (inline), `src/lock.rs` (inline), `src/ports.rs` (inline), `src/error.rs` (inline).

**CI trigger:** Every push, every PR. Must pass before integration tests execute. Target wall-clock: < 5 seconds total.

**Scope:**
- `guards.rs`: `check_worktree_count`, `check_not_nested_worktree` (with synthetic `WorktreeHandle` vectors), `check_disk_space` (with `tempfile` mount-point stubs), `check_path_not_exists`, path canonicalization edge cases.
- `state.rs`: Schema migration v1-to-v2, JSON round-trip serialization of all `WorktreeState` variants, `stale_worktrees` TTL expiry arithmetic, unknown-field preservation via `serde(flatten)`.
- `lock.rs`: Full Jitter backoff distribution validation (10,000 samples, assert mean within 20% of expected), stale detection logic for each of the four factors (PID dead, PID reused, UUID mismatch, corrupt JSON).
- `ports.rs`: Deterministic hash-based port assignment, sequential probe wrap-around, lease TTL expiry, lease renewal timestamp arithmetic.

### 1.2 Integration Tests

**Purpose:** Exercise `Manager` methods end-to-end against a real git repository. Every test creates a fresh repository in a temporary directory, calls Manager methods, and then verifies the result by running `git worktree list --porcelain` independently and comparing against both the returned handles and `state.json` contents.

**Tooling:** `tests/` top-level directory, `tempfile::TempDir`, real `git` binary, `assert_cmd` for CLI smoke tests.

**Where tests live:** `tests/integration/` directory, one file per Manager method (`test_create.rs`, `test_delete.rs`, `test_gc.rs`, `test_attach.rs`, `test_list.rs`).

**CI trigger:** Every push, every PR. Runs after unit tests pass. Target wall-clock: < 60 seconds.

**Key invariant:** After every `Manager::create()` or `Manager::delete()` call, the test independently runs `git worktree list --porcelain` and confirms the worktree appears (or does not appear) in git's registry. The test also reads `state.json` directly and asserts consistency with git's output. This dual-verification catches any drift between git state and library state.

### 1.3 Property-Based Tests

**Purpose:** Fuzz parsers with adversarial input to discover edge cases that hand-written tests miss. The porcelain parser and merge-tree parser are security-critical boundaries between untrusted git output and the library's internal state.

**Tooling:** `proptest` crate. Strategies generate random worktree blocks with valid and malformed field combinations.

**Where tests live:** `tests/proptest/` directory, `test_porcelain_parser.rs`, `test_merge_tree_parser.rs`.

**CI trigger:** Every push. Regression seeds committed to `tests/proptest/regressions/`. Target: 1,000 cases per strategy per run (< 30 seconds).

**Strategies:**
- Porcelain parser (`--porcelain` newline-delimited): Generate blocks with random paths (including spaces, unicode, embedded newlines), random 40-char hex SHAs, random branch names including `refs/heads/` prefix, optional `locked`/`prunable` fields, random ordering of fields within a block.
- Porcelain parser (`--porcelain -z` NUL-delimited): Same strategy but with NUL-terminated fields.
- Merge-tree parser (`-z` output): Random conflict markers, file paths with special characters, empty conflict sections.

### 1.4 Regression Tests

**Purpose:** One named test per data-loss or critical incident in Appendix B. These tests are the "never again" contract -- each test reproduces the exact failure scenario described in the bug report and asserts the fix holds. Naming convention: `test_regression_<bug_id>`.

**Tooling:** Integration test harness (real git repo in tempdir).

**Where tests live:** `tests/regression/` directory.

**CI trigger:** Every push, every PR.

**Coverage:** See Section 3 (Data-Loss Regression Suite) for the complete matrix of 10 incidents.

### 1.5 Stress and Crash-Injection Tests

**Purpose:** Prove that the library produces zero orphans and zero corruption under adversarial concurrent load with process crashes. This is a Milestone 1 ship criterion.

**Tooling:** Custom test harness spawning 10 threads, each running 10 create/delete cycles (100 total). A separate crash-injection thread sends SIGKILL to random Manager processes at random points. After all threads complete, the test asserts zero orphans (via `git worktree list --porcelain`) and zero corruption (via `state.json` parse + reconciliation).

**Where tests live:** `tests/stress/` directory, `test_crash_injection.rs`.

**CI trigger:** Nightly CI only (wall-clock: 2-5 minutes). Also run manually before each milestone ship.

**Pass criteria:**
- Zero orphaned worktrees after 100 create/delete cycles with SIGKILL injection.
- `state.json` parseable after every crash-recovery cycle.
- `Manager::new()` successfully recovers stale locks within 6 seconds.
- No panics in any thread.

### 1.6 Performance Benchmarks

**Purpose:** Enforce quantitative performance budgets defined in the PRD. Regressions beyond 20% trigger CI failure.

**Tooling:** `criterion` crate with statistical analysis. Baselines committed to `benches/baselines/`.

**Where tests live:** `benches/` directory.

**CI trigger:** Nightly CI. Comparison against committed baselines; failure if any benchmark regresses beyond 20%.

**Benchmarks:**

| Benchmark | Target | Setup |
|---|---|---|
| `Manager::new()` cold start | < 500 ms | Fresh repo, no state.json |
| `Manager::create()` on 2 GB repo | < 10 s | Synthetic 2 GB repo with 50K files |
| `Manager::gc()` on 20 worktrees | < 5 s | 20 active worktrees, mixed ages |
| Directory size walk (50K files) | < 200 ms | `calculate_worktree_disk_usage` on synthetic tree |
| `conflict_matrix()` for 20 pairs | < 10 s | 20 worktree pairs with merge-tree checks |
| Port hash assignment (1000 branches) | < 10 ms | Deterministic hash + probe sequence |

### 1.7 Platform-Specific Tests

**Purpose:** Validate platform-dependent code paths that cannot be tested on other platforms.

**Tooling:** Standard test harness with `#[cfg(target_os = "...")]` annotations.

**Where tests live:** `src/platform/macos.rs` (inline `#[cfg(test)]`), `src/platform/linux.rs` (inline), `src/platform/windows.rs` (inline).

**CI trigger:** Platform-specific CI runners. macOS and Linux on every push. Windows nightly (M3 ship criterion).

**Scope:**
- macOS: `clonefile(2)` CoW verification (create reflink, modify source, assert target unchanged), `getattrlistbulk` metadata walk, APFS detection via `statfs`.
- Linux: `FICLONE` ioctl on Btrfs/XFS, `/proc/mounts` parsing for NFS detection, `/proc/<pid>/stat` field 22 for start_time.
- Windows: `LockFileEx` advisory lock, NTFS junction creation and traversal via `junction` crate, `dunce` path canonicalization with `\\?\` prefix, `GetDriveTypeW` for network filesystem detection.

### 1.8 Mock Git Binary Tests

**Purpose:** Test capability fallback paths (PRD Section 17) without requiring multiple git versions. A mock git shell script simulates specific git versions by returning appropriate output or errors for version-gated features.

**Tooling:** Shell scripts in `tests/fixtures/mock-git/` that implement a subset of git subcommands. Tests set `PATH` to the fixture directory so `Manager::new()` finds the mock git first.

**Where tests live:** `tests/mock_git/` directory, `test_capability_fallback.rs`.

**CI trigger:** Every push, every PR.

**Fixture scripts:**
- `git-2.20`: Minimum version. Returns `unknown subcommand` for `repair`. No `-z` support for `list`.
- `git-2.30`: Adds `repair` support.
- `git-2.36`: Adds `-z` NUL-delimited output for `list`.
- `git-2.38`: Adds `merge-tree --write-tree`.
- `git-2.42`: Adds `--orphan` for `worktree add`.

---

## 2. Safety Guard Test Matrix

| Guard Name | Trigger Condition | Setup Precondition | Expected Error Variant | Pass Criterion | Test ID |
|---|---|---|---|---|---|
| `check_branch_not_checked_out` | Branch `feature-x` already checked out in another worktree | Create repo, add worktree on `feature-x`, attempt second create on `feature-x` | `BranchAlreadyCheckedOut { branch: "feature-x", worktree }` | Error returned before `git worktree add` is called; git worktree list shows exactly one entry for `feature-x` | QA-G-001 |
| `check_disk_space` | Available disk below `min_free_disk_mb` threshold (500 MB default) | Create tmpfs mount with < 500 MB free (Linux) or use mock `statvfs` | `DiskSpaceLow { available_mb, required_mb: 500 }` | Error returned; no worktree directory created on disk | QA-G-002 |
| `check_worktree_count` | Active worktree count equals `max_worktrees` (default 20) | Create repo, add 20 worktrees, attempt 21st | `RateLimitExceeded { current: 20, max: 20 }` | Error returned; `git worktree list` shows exactly 20 entries | QA-G-003 |
| `check_path_not_exists` | Target path already exists as a directory | Create repo, `mkdir` at target path, attempt create | `WorktreePathExists(path)` | Error returned; existing directory untouched | QA-G-004 |
| `check_not_nested_worktree` | Candidate path is a subdirectory of an existing worktree (both directions) | Create repo, add worktree at `/tmp/outer`, attempt create at `/tmp/outer/inner` | `NestedWorktree { parent }` | Error returned for both nesting directions; no filesystem mutation | QA-G-005 |
| `check_not_network_filesystem` | Target path on NFS/CIFS/SMB mount | Mount NFS share in CI or mock `statfs()` return value | `NetworkFilesystem { mount_point }` | Warning logged by default; error returned when `Config::deny_network_filesystem = true` | QA-G-006 |
| `check_not_wsl_cross_boundary` | Repo on `/mnt/c` (Windows FS), worktree target on `/home` (Linux FS) | WSL environment with `/proc/version` containing "Microsoft" | `WslCrossBoundary` | Error returned; no worktree created | QA-G-007 |
| `check_bare_repo` | Repository is bare (`git init --bare`) | `git init --bare` in tempdir | Returns `Ok(true)` (not an error -- bare repos are permitted) | Manager adjusts path defaults; `create()` succeeds with explicit path | QA-G-008 |
| `check_submodule_context` | CWD is inside a git submodule | Create repo with submodule, cd into submodule, call `Manager::new()` | `SubmoduleContext` | Error returned at Manager construction; no state files created | QA-G-009 |
| `check_total_disk_usage` | Aggregate worktree disk usage exceeds `max_total_disk_bytes` | Create repo, set `max_total_disk_bytes = 1_000_000`, add worktrees totaling > 1 MB | `AggregateDiskLimitExceeded` | Error returned; no new worktree created | QA-G-010 |
| `check_not_network_junction_target` (Windows only) | Junction target starts with `\\` (UNC path) | Windows environment, target path `\\server\share\wt` | `NetworkJunctionTarget { path }` | Error returned; no junction created | QA-G-011 |
| `check_git_crypt_pre_create` | Repository uses git-crypt; encrypted files detected post-checkout | Create repo with `.gitattributes` containing `filter=git-crypt`, add file with `GIT_CRYPT_MAGIC` header | `GitCryptLocked` | Worktree auto-removed via `git worktree remove --force`; no partial worktree left on disk | QA-G-012 |

---

## 3. Data-Loss Regression Suite

| Incident | Bug ID | Reproduction Method | Assert Statement | Test ID |
|---|---|---|---|---|
| Cleanup deleted branches with unmerged commits -- no warning | `claude-code#38287` | Create worktree on branch with 3 commits not merged to main. Call `delete()` without `force=true`. | `assert!(result.is_err()); assert_matches!(result, Err(WorktreeError::UnmergedCommits { commit_count: 3, .. }))` | QA-R-001 |
| Sub-agent cleanup deleted parent session CWD | `claude-code#41010` | Create two worktrees. Set CWD to worktree A. Attempt `delete()` on worktree A from worktree B's manager context. | `assert_matches!(result, Err(WorktreeError::CannotDeleteCwd))` | QA-R-002 |
| Three agents reported success; all work lost | `claude-code#29110` | Create 3 worktrees. Commit changes to each. Run `gc(force=true)`. Verify all 3 branches still exist via `git branch --list`. | `assert_eq!(surviving_branches.len(), 3)` -- gc with force must not delete branches with unique commits unless the worktree is orphaned | QA-R-003 |
| git-crypt worktree committed all files as deletions | `claude-code#38538` | Create repo with git-crypt configured. Add files with `GIT_CRYPT_MAGIC` header bytes. Attempt `create()`. | `assert_matches!(result, Err(WorktreeError::GitCryptLocked)); assert!(!worktree_path.exists())` -- worktree auto-cleaned | QA-R-004 |
| Nested worktree created inside worktree after context compaction | `claude-code#27881` | Create worktree at `/tmp/wt-outer`. Attempt `create()` with path `/tmp/wt-outer/subdir`. | `assert_matches!(result, Err(WorktreeError::NestedWorktree { .. }))` | QA-R-005 |
| Background worker cleaned worktree with uncommitted changes | `vscode#289973` | Create worktree. Write uncommitted file. Call `delete()` without `force_dirty`. | `assert_matches!(result, Err(WorktreeError::UncommittedChanges { .. })); assert!(worktree_path.exists())` -- worktree untouched | QA-R-006 |
| Runaway `git worktree add` loop: 1,526 worktrees | `vscode#296194` | Set `Config::max_worktrees = 5`. Create 5 worktrees. Attempt 6th. | `assert_matches!(result, Err(WorktreeError::RateLimitExceeded { current: 5, max: 5 }))` -- circuit breaker prevents unbounded creation | QA-R-007 |
| 9.82 GB consumed in 20-minute session on 2 GB repo | Cursor forum | Set `Config::max_total_disk_bytes = 5_000_000_000`. Create worktrees until aggregate limit hit. | `assert_matches!(result, Err(WorktreeError::AggregateDiskLimitExceeded))` -- aggregate cap enforced | QA-R-008 |
| 5 worktrees x 2 GB node_modules = 10 GB wasted | `claude-squad#260` | Create worktree with `DefaultAdapter` configured to copy `.env` only (not `node_modules`). Verify `node_modules` not duplicated. | `assert!(!worktree_path.join("node_modules").exists())` -- adapter controls what is copied, not blind clone | QA-R-009 |
| Each retry creates orphan: hundreds of MB per attempt | `opencode#14648` | Simulate 5 failed `create()` calls (inject git failure after allocation). Verify no orphaned directories remain. | `assert_eq!(orphan_count, 0)` -- failed creates always clean up partial directories | QA-R-010 |

---

## 4. Concurrency Test Suite

All concurrency tests use real git repositories in `tempfile::TempDir`. Thread synchronization uses `std::sync::Barrier` to ensure simultaneous execution.

| Test ID | Name | Description | Setup | Assert |
|---|---|---|---|---|
| QA-C-001 | `test_concurrent_create_same_branch` | 10 threads call `create()` for the same branch simultaneously. | Create repo. Spawn 10 threads with barrier. Each calls `create("feature-x", ...)`. | Exactly 1 thread returns `Ok`; 9 return `BranchAlreadyCheckedOut`. `git worktree list` shows exactly 1 entry for `feature-x`. |
| QA-C-002 | `test_concurrent_remove_racing_gc` | One thread calls `delete()` while another calls `gc()` targeting the same worktree. | Create repo with 1 worktree. Spawn 2 threads: one `delete()`, one `gc(dry_run=false)`. | No panic. At most one operation succeeds. Worktree is removed exactly once. No double-free. |
| QA-C-003 | `test_state_json_read_modify_write_contention` | 20 threads each acquire `state.lock`, read `state.json`, increment a counter field, write back. | Create repo. Spawn 20 threads. Each acquires lock, reads state, increments custom counter, writes state. | Final counter value equals 20. No corrupted JSON. No `StateLockContention` if timeout is sufficiently large (60s). |
| QA-C-004 | `test_circuit_breaker_trips_after_three_failures` | Inject 3 consecutive git command failures. Verify circuit breaker trips on the 4th call. | Create repo. Replace git binary with one that returns exit code 1. Call `list()` 3 times. Call `create()`. | First 3 calls return `GitCommandFailed`. 4th call returns `CircuitBreakerOpen { consecutive_failures: 3 }`. |
| QA-C-005 | `test_stale_lock_recovery_after_sigkill` | Fork a process that acquires `state.lock` then receives SIGKILL. The next `Manager::new()` must recover within 6 seconds. | Fork child process. Child acquires lock, writes lock record, receives SIGKILL. Parent waits 1 second, calls `Manager::new()`. | `Manager::new()` succeeds. Wall-clock for recovery < 6 seconds. Lock record shows new PID and UUID. |
| QA-C-006 | `test_lock_flag_race_window_old_git` | Simulate Git < 2.17 where `--lock` flag is not available for `worktree add`. Verify fallback to separate `add` + `lock` commands. | Use mock git binary returning `unknown option` for `--lock`. Call `create()` with `options.lock = true`. | Worktree created via `git worktree add` then locked via `git worktree lock` as separate commands. `git worktree list` shows `locked` status. |
| QA-C-007 | `test_concurrent_merge_check_no_index_lock` | 20 threads call `merge_check()` simultaneously on different branch pairs. | Create repo with 20 branches. Spawn 20 threads, each checking a different pair. | All 20 threads complete without error. No `index.lock` file left behind after completion. `ls .git/index.lock` returns not-found. |
| QA-C-008 | `test_pid_reuse_false_positive` | Simulate PID reuse: write a lock record with PID X and start_time T1. Spawn a new process that reuses PID X but has start_time T2. | Write lock record `{ pid: X, start_time: T1, ... }`. Mock `sysinfo` to return `start_time: T2` for PID X. Call stale detection. | Lock detected as stale due to start_time mismatch. Log contains "PID reused (start time mismatch)". New lock acquired successfully. |

---

## 5. Git Version Compatibility Matrix

All tests in this section use mock git binaries from `tests/fixtures/mock-git/`. Each mock script responds to `git --version` with a specific version string and simulates the documented fallback behavior.

| Feature | Min Version Simulated | Expected Fallback Behavior | Test ID |
|---|---|---|---|
| `worktree list --porcelain -z` (NUL-delimited) | 2.35 (below 2.36 threshold) | Parser falls back to newline-delimited. Warning logged: "upgrade to git 2.36 for safe parsing". List succeeds with newline-delimited output. | QA-V-001 |
| `worktree repair` | 2.29 (below 2.30 threshold) | `repair` step skipped. Warning logged. `attach()` proceeds without repair. | QA-V-002 |
| `worktree add --orphan` | 2.41 (below 2.42 threshold) | Orphan branch creation returns descriptive error: "orphan branch worktrees require git 2.42+". | QA-V-003 |
| `worktree.useRelativePaths` | 2.47 (below 2.48 threshold) | Config option skipped. Absolute paths used (default behavior). No warning needed. | QA-V-004 |
| `git merge-tree --write-tree` | 2.37 (below 2.38 threshold) | `wt check` returns error with upgrade instructions: "conflict detection requires git 2.38+". `conflict_check` MCP tool returns `not_implemented`. | QA-V-005 |
| `locked`/`prunable` in list output | 2.30 (below 2.31 threshold) | Fields not present in porcelain output. Parser assumes not locked, not prunable. No error. | QA-V-006 |
| `--lock` flag on `worktree add` | 2.16 (below 2.17 threshold) | Fallback to separate `worktree add` + `worktree lock` commands. Race window documented in handle metadata. | QA-V-007 |
| Hard minimum version check | 2.19 (below 2.20 minimum) | `Manager::new()` returns `GitVersionTooOld { required: "2.20", found: "2.19" }`. No state files created. | QA-V-008 |
| Git not installed | N/A (binary absent) | `Manager::new()` returns `GitNotFound`. No state files created. | QA-V-009 |

---

## 6. Integration Smoke Tests

One test per integration target in PRD Section 18. Each test validates that the library can handle the branch naming convention and workflow pattern used by the target tool.

| Integration Target | Test Description | Branch Pattern Used | Key Assertion | Test ID |
|---|---|---|---|---|
| Claude Code | Create worktree via `wt hook --stdin-format claude-code`. Send JSON on stdin. Assert exactly one line on stdout. | `worktree-feature-auth` | stdout is exactly one line containing an absolute path; stderr contains progress messages; exit code 0 | QA-I-001 |
| OpenCode | Create and delete 5 worktrees with random-adjective-noun branch names. Simulate retry-on-failure: no orphans after 3 failed + 2 successful creates. | `opencode-brave-cabin` | Zero orphans after failures; `git worktree list` matches `state.json` | QA-I-002 |
| Gas Town | Create worktree with slash-prefixed branch name. Assert slash is preserved in state.json and git branch list. | `polecat/auth-1713024000` | Branch name round-trips through create/list/delete without transformation | QA-I-003 |
| Claude Squad | Create 5 worktrees with timestamp-suffixed names. Run `gc()` with `max_age_days=0`. Assert all 5 cleaned (none locked). | `cs_auth_1713024000` | `gc_report.removed.len() == 5`; zero worktrees in `git worktree list` (excluding main) | QA-I-004 |
| Cursor | Create worktree with `--lock` option. Attempt `gc()`. Assert locked worktree survives gc regardless of `force` flag. | `cursor-feature-x` | `gc_report.removed` does not contain the locked worktree path | QA-I-005 |
| VS Code Copilot | Create 20 worktrees (at max_worktrees limit). Attempt 21st. Assert rate limit. Delete one. Create again. Assert success. | `vscode-copilot-session-N` | 21st creation fails with `RateLimitExceeded`; after delete, 21st succeeds | QA-I-006 |
| workmux | Create worktree with port allocation. Verify port is deterministic for the same branch name across recreations. | `user-specified-branch` | First and second creation (after delete) yield the same port number | QA-I-007 |
| worktrunk | Create worktree, attach to it from a separate Manager instance. Verify state merge. | `user-specified-branch` | `attach()` returns handle with state `Active`; `list()` from both Manager instances returns same entries | QA-I-008 |

---

## 7. MCP Contract Tests

For each of the 6 MCP tools defined in PRD Section 12.3, spawn the `worktree-core-mcp` binary as a subprocess with stdio transport. Send a JSON-RPC 2.0 request. Assert the response matches the documented schema and tool annotations.

**Common setup:** Create a temporary git repository. Set `WORKTREE_CORE_HOME` to a tempdir. Spawn `worktree-core-mcp` with stdin/stdout pipes.

| MCP Tool | Request | Expected Response Schema | Annotation Checks | Test ID |
|---|---|---|---|---|
| `worktree_list` | `{ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "worktree_list", "arguments": {} }, "id": 1 }` | Response contains `result.content` with JSON array of worktree objects. Each object has `path`, `branch`, `state` fields. | `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true` | QA-M-001 |
| `worktree_status` | `{ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "worktree_status", "arguments": {} }, "id": 2 }` | Response contains `result.content` with status object including `worktree_count`, `disk_usage_bytes`, `port_leases`. | `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true` | QA-M-002 |
| `conflict_check` | `{ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "conflict_check", "arguments": {} }, "id": 3 }` | Response contains `result.content` with `not_implemented` status in v1.0. No error -- graceful degradation. | `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true` | QA-M-003 |
| `worktree_create` | `{ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "worktree_create", "arguments": { "branch": "test-mcp", "path": "<tempdir>/test-mcp" } }, "id": 4 }` | Response contains `result.content` with `path`, `branch`, `state: "Active"`, `session_uuid`. | `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: false` | QA-M-004 |
| `worktree_delete` | Prerequisite: create a worktree first. `{ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "worktree_delete", "arguments": { "branch": "test-mcp" } }, "id": 5 }` | Response contains `result.content` confirming deletion. `git worktree list` no longer contains the path. | `readOnlyHint: false`, `destructiveHint: true`, `idempotentHint: false` | QA-M-005 |
| `worktree_gc` | Prerequisite: create and orphan a worktree. `{ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "worktree_gc", "arguments": { "dry_run": false } }, "id": 6 }` | Response contains `result.content` with `removed` array, `freed_bytes`, `orphans` array. | `readOnlyHint: false`, `destructiveHint: true`, `idempotentHint: false` | QA-M-006 |

---

## 8. wt hook Contract Test

| Test ID | Name | Description | Setup | Assert |
|---|---|---|---|---|
| QA-H-001 | `test_wt_hook_claude_code_stdout_contract` | Verify `wt hook --stdin-format claude-code` produces exactly one line on stdout containing the absolute worktree path. No other stdout output is permitted. Regression test for `claude-code#27467`. | Create a temporary git repository. Pipe JSON `{ "session_id": "test", "cwd": "<repo>", "hook_event_name": "WorktreeCreate", "name": "hook-test" }` to stdin. Run `wt hook --stdin-format claude-code --setup`. | stdout contains exactly 1 line. That line is an absolute path. The path exists on disk. `git worktree list --porcelain` includes that path. stderr is non-empty (progress messages). Exit code is 0. |

---

## 9. Milestone Acceptance Gates

Each milestone requires ALL listed test IDs to pass before the milestone is considered shippable.

| Milestone | Name | Required Test IDs | Ship Criteria Summary |
|---|---|---|---|
| **M1** | Foundation (Weeks 1-6) | QA-G-001 through QA-G-012, QA-R-001 through QA-R-010, QA-C-001 through QA-C-008, QA-V-001 through QA-V-009, QA-M-001 through QA-M-006, QA-H-001, QA-S-001, QA-P-001 through QA-P-006 | Zero data loss in stress test. `cargo clippy -D warnings` clean. `cargo test` passes macOS + Linux. MCP server responds correctly. `wt hook` contract met. |
| **M2** | Environment Lifecycle (Weeks 7-10) | QA-I-001 through QA-I-008, QA-O-001 through QA-O-006 | Adapter setup works. Port allocation unique across 20 worktrees. `attach()` recovers port from stale entries. `.DS_Store` handling on macOS. |
| **M3** | Ecosystem Integration (Weeks 11-16) | All M1 + M2 tests, plus Windows platform tests (QA-G-007, QA-G-011 on real Windows) | Windows CI passing. At least one external project consuming `worktree-core`. Conflict detection MVP. |
| **M4** | Hardening (Weeks 17-20) | All M1 + M2 + M3 tests, plus performance benchmarks (QA-P-001 through QA-P-006) meeting budgets | pnpm adapter shares virtual store. Pool of 5 worktrees available in < 1 second. Node.js bindings published. |

**Stress test gate (applies to M1):**

| Test ID | Name | Description |
|---|---|---|
| QA-S-001 | `test_stress_100_cycles_sigkill` | 100 create/delete cycles across 10 threads with SIGKILL injection at random points. Zero orphans. Zero corruption. Manager recovers stale locks within 6 seconds. |

**Performance benchmark gates (apply to M4, tracked from M1):**

| Test ID | Benchmark | Budget |
|---|---|---|
| QA-P-001 | `Manager::new()` cold start | < 500 ms |
| QA-P-002 | `Manager::create()` on 2 GB repo | < 10 s |
| QA-P-003 | `Manager::gc()` on 20 worktrees | < 5 s |
| QA-P-004 | `calculate_worktree_disk_usage` (50K files) | < 200 ms |
| QA-P-005 | `conflict_matrix()` (20 pairs) | < 10 s |
| QA-P-006 | Port hash assignment (1000 branches) | < 10 ms |

---

## 10. Open Questions Test Coverage

For each of the 6 Open Questions in PRD Section 19, one test ID validates the chosen resolution.

| OQ # | Resolution | Validation Test | Test ID |
|---|---|---|---|
| OQ-1 | Caller-driven port lease renewal via `renew_port_lease()`. The Manager exposes a `renew_port_lease(branch: &str) -> Result<(), WorktreeError>` method. The caller is responsible for calling it before TTL expiry. No background timer. | Create a worktree with port allocation. Wait until TTL/2. Call `renew_port_lease()`. Assert `expires_at` is extended by another full TTL. Assert port is NOT reclaimed by a concurrent `gc()` sweep. | QA-O-001 |
| OQ-2 | `Config::deny_network_filesystem: bool` (default `false`). When `true`, `check_not_network_filesystem` returns `Err(NetworkFilesystem)` instead of logging a warning. | Create Manager with `Config { deny_network_filesystem: true }`. Mock `statfs` to return NFS. Call `create()`. Assert `Err(NetworkFilesystem)`. Repeat with default config: assert `Ok(...)` with warning logged. | QA-O-002 |
| OQ-3 | `attach()` is permitted on bare repos with explicit path. The path must be absolute and must already exist in `git worktree list`. | `git init --bare` a repo. `git worktree add /tmp/wt-bare main`. Call `manager.attach("/tmp/wt-bare")`. Assert returns `Ok(handle)` with `state: Active`. | QA-O-003 |
| OQ-4 | Auto-reset circuit breaker after `Config::circuit_breaker_reset_secs` (default 60). After the configured number of seconds with no git command attempts, the consecutive failure counter resets to zero. | Set `circuit_breaker_threshold = 3`, `circuit_breaker_reset_secs = 2` (shortened for test). Trigger 3 failures. Assert `CircuitBreakerOpen`. Wait 3 seconds. Call `list()`. Assert it does NOT return `CircuitBreakerOpen` -- circuit breaker has auto-reset. | QA-O-004 |
| OQ-5 | `git -C <bare-repo-root> worktree add` confirmed safe in Git 2.20. The library uses `-C <path>` to run `git worktree add` from the bare repo root. | `git init --bare` a repo. Call `manager.create("feature", "/tmp/wt-bare-test")`. Assert worktree created. `git -C <bare-root> worktree list --porcelain` includes the new path. | QA-O-005 |
| OQ-6 | `WorktreeState::InUse { pid, since }` variant. `gc()` skips `InUse` worktrees even if `force=true`. A worktree is marked `InUse` when a process holds an active `Manager` handle referencing it (tracked via PID + start_time in state.json). | Create worktree. Simulate active agent by writing `InUse { pid: current_pid, since: now }` to state.json. Run `gc(force=true, max_age_days=0)`. Assert worktree is NOT in `gc_report.removed`. Assert worktree still exists on disk. | QA-O-006 |

---

*End of test-strategy.md*
