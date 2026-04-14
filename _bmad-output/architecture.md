# Architecture -- worktree-core

| Field | Value |
|---|---|
| **Document** | Technical Architecture Specification |
| **PRD** | ISO_PRD-v1.5 |
| **Date** | 2026-04-13 |
| **Status** | Implementation-Ready |

---

## 1. System Overview

```
                       +--------------------------+
                       |  future: napi-rs binding |
                       |  future: PyO3 binding    |
                       +-----------+--------------+
                                   |
                       +-----------v--------------+
                       | worktree-core-mcp        |
                       | bin: stdio MCP server    |
                       | src/main.rs              |
                       +-----------+--------------+
                                   |
                       +-----------v--------------+
                       | worktree-core-cli        |
                       | bin: wt                  |
                       | src/main.rs              |
                       +-----------+--------------+
                                   |
                       +-----------v--------------+
                       | worktree-core (lib)      |
                       | src/lib.rs               |
                       | All lifecycle logic      |
                       +--------------------------+
```

**Dependency direction:** each layer depends only on the layer below it. The library crate has no binary targets. The CLI and MCP binaries depend on the library crate. All three crates share a workspace `Cargo.toml` at the repository root.

**Future bindings** (napi-rs for Node.js, PyO3 for Python) will be separate workspace members that depend on `worktree-core` the same way the CLI and MCP crates do. They do not exist yet and are not part of Milestone 1.

---

## 2. Module Map

Every module under `worktree-core/src/` with its PRD cross-reference and single-sentence responsibility.

| Module | PRD Section | Responsibility |
|---|---|---|
| `lib.rs` | SS 5 | Public API surface: re-exports `Manager`, all public types, traits, and the `EcosystemAdapter` trait. |
| `manager.rs` | SS 5 | Contains the `Manager` struct and all lifecycle operations (`new`, `create`, `delete`, `list`, `attach`, `gc`, port methods). |
| `types.rs` | SS 4 | Defines all public types: `WorktreeHandle`, `WorktreeState`, `CreateOptions`, `DeleteOptions`, `GcOptions`, `GcReport`, `Config`, `ReflinkMode`, `CopyOutcome`, `GitCapabilities`, `GitVersion`, `PortLease`. |
| `error.rs` | SS 4.5 | Defines the `WorktreeError` enum with all error variants and `thiserror` derive. |
| `git.rs` | SS 7 | All `std::process::Command` invocations to the git CLI; porcelain output parsing; version detection; capability probing. |
| `guards.rs` | SS 8 | All pre-create and pre-delete safety guard functions; five-step unmerged commit decision tree; git-crypt detection protocol. |
| `lock.rs` | SS 9 | Hardened locking protocol: four-factor stale detection, Full Jitter backoff, `fd-lock` acquisition, state.lock read/write. |
| `state.rs` | SS 10 | `state.json` v2 schema read/write/migrate; reconciliation against `git worktree list`; stale_worktrees eviction; write-tmp-fsync-rename protocol. |
| `ports.rs` | SS 10.4 | Port lease model: deterministic hash-based assignment, sequential probe fallback, lease lifecycle, TTL/renewal, stale sweep. |
| `platform/mod.rs` | SS 11 | Platform dispatch: `#[cfg]`-gated re-exports from `macos`, `linux`, `windows` submodules. |
| `platform/macos.rs` | SS 11.1 | APFS `clonefile(2)` CoW, `getattrlistbulk` disk enumeration, `statfs()` network filesystem detection, `flock()` locking. |
| `platform/linux.rs` | SS 11.2 | `FICLONE` ioctl CoW, `/proc/mounts` network filesystem detection, `flock()` locking, inode-based hardlink deduplication. |
| `platform/windows.rs` | SS 11.3 | NTFS junction creation via `junction` crate, `LockFileEx` locking via `fd-lock`, `dunce` path normalization, `GetDriveTypeW()` network detection, `GetCompressedFileSizeW()` disk usage. |

---

## 3. Core Types

### 3.1 WorktreeHandle (PRD SS 4.1)

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WorktreeHandle {
    pub path: PathBuf,
    pub branch: String,
    pub base_commit: String,
    pub state: WorktreeState,
    pub created_at: String,
    pub creator_pid: u32,
    pub creator_name: String,
    pub adapter: Option<String>,
    pub setup_complete: bool,
    pub port: Option<u16>,
    pub session_uuid: String,
}
```

**Invariants:** All fields are `pub` for reading; mutation happens only through `Manager` methods. `path` is always absolute and canonicalized via `dunce`. `branch` is passed through as-is, never transformed. `base_commit` is the full 40-character SHA. `session_uuid` is a UUID v4 generated at creation time and stable for the worktree's entire lifetime.

### 3.2 WorktreeState (PRD SS 4.2)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WorktreeState {
    Pending,
    Creating,
    Active,
    Merging,
    Deleting,
    Deleted,
    Orphaned,
    Broken,
    Locked,
    InUse { pid: u32, since: String },  // OQ-6 resolution
}
```

**Valid state transitions:**

| From | To | Trigger |
|---|---|---|
| `Pending` | `Creating` | `Manager::create` begins `git worktree add` |
| `Creating` | `Active` | `git worktree add` succeeded; post-create checks passed |
| `Creating` | `Broken` | `git worktree add` succeeded but git-crypt check failed |
| `Active` | `InUse` | `Manager::create()` sets this immediately after git worktree add succeeds |
| `Active` | `Merging` | Merge operation started |
| `Active` | `Deleting` | `Manager::delete` called; pre-delete checks passed |
| `Active` | `Locked` | `git worktree lock` called |
| `Active` | `Orphaned` | Worktree vanished from disk or git registry |
| `InUse` | `Active` | `Manager::delete()` clears InUse before proceeding |
| `Merging` | `Active` | Merge completed or aborted |
| `Locked` | `Active` | `git worktree unlock` called |
| `Deleting` | `Deleted` | `git worktree remove` succeeded |
| Any | `Broken` | Unrecoverable git error detected |

Attempting an unlisted transition returns `WorktreeError::InvalidStateTransition`.

### 3.3 CreateOptions, DeleteOptions, GcOptions (PRD SS 4.5-4.7)

```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct CreateOptions {
    pub base: Option<String>,
    pub setup: bool,
    pub ignore_disk_limit: bool,
    pub lock: bool,
    pub lock_reason: Option<String>,
    pub reflink_mode: ReflinkMode,
    pub allocate_port: bool,
}

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DeleteOptions {
    pub force: bool,
    pub force_dirty: bool,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GcOptions {
    pub dry_run: bool,        // default: true
    pub max_age_days: Option<u32>,
    pub force: bool,
}
```

### 3.4 Config (PRD SS 4.4)

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    pub max_worktrees: usize,                // default: 20
    pub disk_threshold_percent: u8,          // default: 90
    pub gc_max_age_days: u32,                // default: 7
    pub port_range_start: u16,               // default: 3100
    pub port_range_end: u16,                 // default: 5100
    pub min_free_disk_mb: u64,               // default: 500
    pub home_override: Option<PathBuf>,
    pub max_total_disk_bytes: Option<u64>,    // default: None
    pub circuit_breaker_threshold: u32,      // default: 3
    pub stale_metadata_ttl_days: u32,        // default: 30
    pub lock_timeout_ms: u64,                // default: 30_000
    pub creator_name: String,                // default: "worktree-core"
    pub deny_network_filesystem: bool,       // default: false (OQ-2)
    pub circuit_breaker_reset_secs: u64,     // default: 60 (OQ-4)
}
```

### 3.5 WorktreeError (PRD SS 4.10)

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WorktreeError {
    #[error("git not found in PATH")]
    GitNotFound,
    #[error("git version too old: required {required}, found {found}")]
    GitVersionTooOld { required: String, found: String },
    #[error("branch '{branch}' is already checked out at '{worktree}'")]
    BranchAlreadyCheckedOut { branch: String, worktree: PathBuf },
    #[error("worktree path already exists: {0}")]
    WorktreePathExists(PathBuf),
    #[error("uncommitted changes in worktree: {files:?}")]
    UncommittedChanges { files: Vec<String> },
    #[error("unmerged commits on '{branch}': {commit_count} commit(s) not in upstream")]
    UnmergedCommits { branch: String, commit_count: usize },
    #[error("insufficient disk space: {available_mb}MB available, {required_mb}MB required")]
    DiskSpaceLow { available_mb: u64, required_mb: u64 },
    #[error("aggregate worktree disk usage exceeds limit")]
    AggregateDiskLimitExceeded,
    #[error("target is on a network filesystem: {mount_point}")]
    NetworkFilesystem { mount_point: PathBuf },
    #[error("cannot create Windows junction targeting network path: {path}")]
    NetworkJunctionTarget { path: PathBuf },
    #[error("cannot create worktree across WSL/Windows filesystem boundary")]
    WslCrossBoundary,
    #[error("submodule context detected")]
    SubmoduleContext,
    #[error("state lock contention after {timeout_ms}ms")]
    StateLockContention { timeout_ms: u64 },
    #[error("orphaned worktrees detected: {paths:?}")]
    OrphanDetected { paths: Vec<PathBuf> },
    #[error("rate limit exceeded: {current} worktrees, maximum is {max}")]
    RateLimitExceeded { current: usize, max: usize },
    #[error("cannot delete own working directory")]
    CannotDeleteCwd,
    #[error("worktree is locked: {reason:?}")]
    WorktreeLocked { reason: Option<String> },
    #[error("cannot create worktree inside existing worktree at '{parent}'")]
    NestedWorktree { parent: PathBuf },
    #[error("git-crypt encrypted files detected after checkout")]
    GitCryptLocked,
    #[error("CoW (reflink) required but filesystem does not support it")]
    ReflinkNotSupported,
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: WorktreeState, to: WorktreeState },
    #[error("git command failed\n  command: {command}\n  stderr: {stderr}\n  exit: {exit_code}")]
    GitCommandFailed { command: String, stderr: String, exit_code: i32 },
    #[error("state file corrupted: {reason}")]
    StateCorrupted { reason: String },
    #[error("circuit breaker open: {consecutive_failures} consecutive git failures")]
    CircuitBreakerOpen { consecutive_failures: u32 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

---

## 4. Git Interaction Strategy

All git operations shell out to the user's installed git binary via `std::process::Command`. No `git2` crate. No `gix` for worktree CRUD (reserved for M4 conflict detection behind a feature flag).

| Operation | Approach | Reason |
|---|---|---|
| Worktree create | `git worktree add <path> -b <branch> [<base>]` via shell | `gix` does not implement `git worktree add`. CLI matches user's installed git behavior exactly. |
| Worktree delete | `git worktree remove [--force] <path>` via shell | Same as above. `--force` needed for cleanup-on-failure paths. |
| Worktree list | `git worktree list --porcelain [-z]` via shell | Porcelain output is stable across versions. `-z` (2.36+) handles paths with newlines; newline fallback for older git. |
| Worktree lock/unlock | `git worktree lock/unlock` via shell | Atomic lock with `--lock` flag on `worktree add` (2.17+) eliminates race window. |
| Worktree repair | `git worktree repair` via shell | Only on Git 2.30+. Skipped on older git with warning. |
| Version detection | `git --version` | Parsed at `Manager::new()` to build `GitCapabilities`. |
| Unmerged commit check | Five-step decision tree (SS 8.2.1) | Single-command check produces false positives/negatives. |
| Merge/conflict detection | `git merge-tree --write-tree -z --stdin` | M4 replaces with `gix::Repository::merge_trees()` behind feature flag. Requires Git 2.38+. |
| Uncommitted changes | `git -C <path> status --porcelain` | Porcelain output is stable and machine-parseable. |
| Primary branch detection | `git symbolic-ref refs/remotes/origin/HEAD` | Falls back to `"main"` then `"master"`. |
| Bare repo detection | `git rev-parse --is-bare-repository` | Adjusts path defaults; does not block creation. |
| Submodule detection | `git rev-parse --show-superproject-working-tree` | Returns `SubmoduleContext` error if inside a submodule. |

**Non-negotiable invariant:** `git worktree list --porcelain` is always authoritative. If `state.json` disagrees, `state.json` is reconciled against git's output.

---

## 5. Safety Guards

All guards are internal functions in `src/guards.rs`. Not part of the public API.

### 5.1 Pre-Create Guards (12 guards, run in exact order)

| # | Guard | Check Logic | Error Variant |
|---|---|---|---|
| 1 | `check_branch_not_checked_out` | `git worktree list --porcelain`, scan for `branch refs/heads/<branch>` | `BranchAlreadyCheckedOut { branch, worktree }` |
| 2 | `check_disk_space` | `statvfs()` on Unix, `GetDiskFreeSpaceEx` on Windows; compare against `min_free_disk_mb` | `DiskSpaceLow { available_mb, required_mb }` |
| 3 | `check_worktree_count` | Count active worktrees against `config.max_worktrees` | `RateLimitExceeded { current, max }` |
| 4 | `check_path_not_exists` | `path.exists()` | `WorktreePathExists(path)` |
| 5 | `check_not_nested_worktree` | `dunce::canonicalize` both paths; bidirectional `starts_with` check (candidate inside existing AND existing inside candidate) | `NestedWorktree { parent }` |
| 6 | `check_not_network_filesystem` | Linux: `/proc/mounts` or `statfs()`. macOS: `statfs() f_fstypename`. Windows: `GetDriveTypeW()`. When `Config::deny_network_filesystem` is true, returns error; otherwise logs warning. | `NetworkFilesystem { mount_point }` |
| 7 | `check_not_wsl_cross_boundary` | Detect WSL via `/proc/version` containing "Microsoft"; check repo on `/mnt/*` vs worktree not or vice versa | `WslCrossBoundary` |
| 8 | `check_bare_repo` | `git rev-parse --is-bare-repository`; returns `bool`, caller adjusts path defaults | (informational, not error) |
| 9 | `check_submodule_context` | `git rev-parse --show-superproject-working-tree`; returns `bool`, caller returns error | `SubmoduleContext` |
| 10 | `check_total_disk_usage` | `jwalk` + `filesize` walk all worktree dirs, skip `.git/`; compare against `max_total_disk_bytes` | `AggregateDiskLimitExceeded` |
| 11 | `check_not_network_junction_target` | Windows only: UNC path prefix check (`\\` but not `\\?\`) | `NetworkJunctionTarget { path }` |
| 12 | `check_git_crypt_pre_create` | Four-step detection: parse `.gitattributes` for `filter=git-crypt`, check key file, check `.git-crypt/` dir, byte-level magic header inspection | `GitCryptLocked` |

### 5.2 Pre-Delete Guards (4 guards, run in exact order)

| # | Guard | Check Logic | Error Variant |
|---|---|---|---|
| 1 | `check_not_cwd` | `dunce::canonicalize(path) == dunce::canonicalize(current_dir())` | `CannotDeleteCwd` |
| 2 | `check_no_uncommitted_changes` | `git -C <path> status --porcelain`; skipped if `force_dirty = true` | `UncommittedChanges { files }` |
| 3 | `check_not_locked` | `handle.state == WorktreeState::Locked`; gc() NEVER touches locked worktrees regardless of force flag | `WorktreeLocked { reason }` |
| 4 | `five_step_unmerged_check` | See SS 5.3 below; skipped if `force = true` | `UnmergedCommits { branch, commit_count }` |

### 5.3 Five-Step Unmerged Commit Decision Tree (PRD SS 8.2.1)

**Precondition:** `git rev-parse --is-shallow-repository`. If shallow, skip Steps 2-4, go directly to Step 5, log warning.

**Primary branch:** `git symbolic-ref refs/remotes/origin/HEAD`, fallback to `"main"` then `"master"`.

1. **`git fetch --prune origin`** -- Network error: skip, log warning. Do NOT block deletion.
2. **`git merge-base --is-ancestor <branch> <primary>`** -- Exit 0: SAFE, return Ok. Exit 1: continue. Exit 128: log warning, continue to Step 4.
3. **`git merge-base --is-ancestor <branch> origin/<primary>`** -- Exit 0: SAFE, return Ok. Exit 1: continue. Exit 128: continue.
4. **`git cherry -v origin/<primary> <branch>`** -- Lines starting with `+` = unique commits. Lines starting with `-` = upstream matches (handles squash/rebase). No `+` lines: SAFE. `+` lines present: continue. Command fails: continue.
5. **`git log <branch> --not --remotes --oneline`** -- 0 lines: SAFE. >0 lines: return `UnmergedCommits { branch, commit_count }`.

**Edge cases:** Orphan branches (no commits) are safe to delete. No remote configured: Steps 1, 3, 4 fail gracefully; Step 5 is the final arbiter.

### 5.4 git-crypt Detection Protocol (PRD SS 8.3)

**Magic constant:** `b"\x00GITCRYPT\x00"` (10 bytes).

**Four-step detection sequence:**
1. Parse `.gitattributes` for `filter=git-crypt` lines. None found: `NotUsed`.
2. Check key file at `<git_dir>/git-crypt/keys/default`. Absent: `LockedNoKey`.
3. Check for `.git-crypt/` directory in worktree root.
4. Read first 10 bytes of each file matched by `filter=git-crypt` rules. Any match: `Locked`. All differ: `Unlocked`.

**Post-create action:** Run detection on new worktree. If `Locked`: `git worktree remove --force <path>`, return `GitCryptLocked`. Never leave partial worktree on disk.

---

## 6. Locking Protocol

### 6.1 Lock file location

`<repo>/.git/worktree-core/state.lock` -- adjacent to `state.json` for atomic coordination.

### 6.2 Backend

`fd-lock` v4 -- cross-platform advisory file locking. `flock()` on Unix, `LockFileEx` on Windows. RAII guards auto-release on drop.

### 6.3 Timeout

Total timeout: `Config::lock_timeout_ms` (default 30,000 ms). Full Jitter backoff with max 15 attempts.

**Full Jitter formula:**

```
sleep_ms = rand::random::<u64>() % min(cap_ms, base_ms * 2^attempt)
```

Parameters: `base_ms = 10`, `cap_ms = 2000`, `max_attempts = 15`.

### 6.4 Multi-Factor Identity

The lock record contains four factors (modeled after PostgreSQL's `postmaster.pid`):

| Factor | Source | Purpose |
|---|---|---|
| PID | `std::process::id()` | Basic process identification |
| start_time | `sysinfo::Process::start_time()` | Detects PID reuse (Linux PIDs cycle within ~32K increments) |
| UUID v4 | Generated at acquisition time | Correlation handle for logs; distinguishes sessions with same PID+start_time |
| hostname | `hostname::get()` | Diagnoses multi-container environments sharing a filesystem |

**Lock file JSON record:**
```json
{
  "pid": 42891,
  "start_time": 1744533600,
  "uuid": "f7a3b9c1-2d4e-4f56-a789-0123456789ab",
  "hostname": "dev-container-7f3a",
  "acquired_at": "2026-04-13T10:00:00Z"
}
```

### 6.5 Scope Rule (Appendix A rule 7)

The `state.lock` scope is ONLY around the `state.json` read-modify-write cycle. NEVER hold `state.lock` across `git worktree add`. That command can take seconds and would block all other agents.

### 6.6 Stale Lock Recovery (Four-Factor Check)

1. Open `state.lock`. Absent: no lock held, proceed.
2. Deserialize JSON. Parse fails: corrupt (crashed mid-write). Delete, proceed.
3. `kill(pid, 0)`. `ESRCH`: process dead. Delete, proceed. Other error: treat as live (conservative), enter retry loop.
4. Process alive: verify `start_time` via sysinfo. Mismatch: PID reused, delete, proceed. Match AND UUID differs from current session: lock genuinely held, enter Full Jitter retry loop.

### 6.7 Full Acquisition Sequence (9 steps, do not reorder)

1. Run four-factor stale detection on `state.lock`. Delete if stale.
2. Attempt non-blocking exclusive advisory lock via `fd-lock`. On failure, enter Full Jitter retry. On timeout: `StateLockContention`.
3. Write multi-factor JSON record to `state.lock` contents.
4. Read `state.json`.
5. Apply mutation in memory.
6. Write new content to `state.json.tmp` (same directory).
7. `fsync()` the temp file descriptor.
8. `rename(state.json.tmp -> state.json)` -- atomic on POSIX same-filesystem; atomic on Windows within same volume.
9. Drop lock guard (RAII: OS releases `fd-lock` automatically).

**Critical invariants:**
- Never delete `state.lock` after releasing. Leave in place; next acquisition overwrites.
- Never hold `state.lock` across `git worktree add`.

---

## 7. State Persistence

### 7.1 state.json v2 Schema (PRD SS 10.2)

```json
{
  "schema_version": 2,
  "repo_id": "<sha256 of absolute canonicalized repo path>",
  "last_modified": "2026-04-13T14:22:00Z",
  "active_worktrees": {
    "<branch>": {
      "path": "/abs/path/.worktrees/<branch>",
      "branch": "<branch>",
      "base_commit": "<40-char-sha>",
      "state": "Active",
      "created_at": "<ISO 8601>",
      "last_activity": "<ISO 8601>",
      "creator_pid": 12345,
      "creator_name": "<tool-name>",
      "session_uuid": "<uuid-v4>",
      "adapter": "<adapter-name>",
      "setup_complete": true,
      "port": 3200
    }
  },
  "stale_worktrees": {
    "<branch>": {
      "original_path": "/abs/path",
      "branch": "<branch>",
      "base_commit": "<sha>",
      "creator_name": "<name>",
      "session_uuid": "<uuid>",
      "port": 3100,
      "last_activity": "<ISO 8601>",
      "evicted_at": "<ISO 8601>",
      "eviction_reason": "<reason>",
      "expires_at": "<ISO 8601>"
    }
  },
  "port_leases": { ... },
  "config_snapshot": { ... },
  "gc_history": [ ... ]
}
```

Unknown fields are preserved via `#[serde(flatten)]` with `HashMap<String, serde_json::Value>` as a catch-all on all state structs (forward compatibility: v1.1-written file readable by v1.0 without data loss).

### 7.2 Reconciliation Algorithm ("git wins")

Run on every `Manager::list()` call and at `Manager::new()` startup:

1. Call `git worktree list --porcelain [-z]`.
2. For each entry in `active_worktrees` NOT in git's output: move to `stale_worktrees`, set `eviction_reason`, set `expires_at = now + stale_metadata_ttl_days`, set port lease status to `"stale"`. Log warning. DO NOT silently delete.
3. For each entry in git's output: if in `active_worktrees`, merge (git's locked/prunable flags override state.json). If NOT in `active_worktrees`, synthesize a minimal `WorktreeHandle`.
4. Purge `stale_worktrees` entries where `expires_at < now`.
5. Sweep `port_leases`: for `"active"` leases, check `kill(pid, 0)` + `start_time`. Dead process AND `expires_at < now`: remove lease.

### 7.3 Stale Worktree Eviction Rule (Appendix A rule 8)

Entries evicted from `active_worktrees` go to `stale_worktrees` -- never silently deleted. This preserves port lease and session identity data for recovery via `wt attach`.

### 7.4 Write Protocol

1. Write new content to `state.json.tmp` (same directory as `state.json`).
2. `fsync()` the temp file descriptor.
3. `rename(state.json.tmp -> state.json)` -- atomic on POSIX same-filesystem, atomic on Windows within same volume.

---

## 8. Cross-Platform Strategy

| Area | macOS | Linux | Windows |
|---|---|---|---|
| CoW mechanism | `clonefile(2)` via `reflink-copy` | `FICLONE` ioctl via `reflink-copy` | `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (ReFS only; not on consumer NTFS) |
| CoW fallback | `std::fs::copy()` | `std::fs::copy()` | `std::fs::copy()` (always, on NTFS) |
| Symlinks | No privileges required | No privileges required | Require `SeCreateSymbolicLinkPrivilege`; use junctions instead |
| Locking | `flock()` via `fd-lock` | `flock()` on local FS; skip on NFS (SS 9.6) | `LockFileEx` via `fd-lock` (mandatory semantics) |
| Path normalization | `dunce::canonicalize()` | `dunce::canonicalize()` | `dunce::canonicalize()` strips `\\?\` for git interop |
| Disk usage | `jwalk` + `filesize`, `st_blocks * 512` | `jwalk` + `filesize`, inode dedup via `HashSet<(dev_t, ino_t)>` | `FindFirstFileEx` + `GetCompressedFileSizeW()` via `filesize` |
| Dir size perf | `getattrlistbulk`, <200ms/50K files | `preload_metadata(true)`, <200ms/50K files | `FIND_FIRST_EX_LARGE_FETCH`, <200ms/50K files |
| Network FS detect | `statfs() f_fstypename` | `/proc/mounts` for nfs/cifs/smbfs | `GetDriveTypeW() == DRIVE_REMOTE` |
| M1 status | Full platform support (Milestone 1) | Full platform support (Milestone 1) | Compile-only stubs (Milestone 1) |
| M3 status | Full | Full | Full implementation replaces stubs (Milestone 3) |

---

## 9. Git Version Capability Gate Map

Minimum supported version: **2.20**. Detected at `Manager::new()`, stored in `GitCapabilities`.

| Feature | Min Version | GitCapabilities Field | Code Path on Older Git |
|---|---|---|---|
| `worktree add/list/prune` | 2.15 | N/A (below hard minimum) | N/A -- 2.20 is hard minimum |
| `worktree list --porcelain` | 2.7 | N/A | N/A -- 2.20 is hard minimum |
| `worktree lock/unlock` | 2.14 | N/A | N/A -- 2.20 is hard minimum |
| `worktree move/remove` | 2.18 | N/A | N/A -- 2.20 is hard minimum |
| `--lock` flag on `worktree add` | 2.17 | N/A | N/A -- 2.20 is hard minimum; but if needed, fallback to separate add + lock (race window exists; documented) |
| `worktree list --porcelain -z` | 2.36 | `has_list_nul: bool` | Fall back to newline-delimited parsing. Paths with newlines fail silently -- log warning. |
| `worktree repair` | 2.30 | `has_repair: bool` | Skip repair step; log warning. `wt attach` may produce broken gitdir links. |
| `locked`/`prunable` in list output | 2.31 | (parsed dynamically) | Parse without these fields; assume not locked, not prunable. |
| `git merge-tree --write-tree` | 2.38 | `has_merge_tree_write: bool` | Conflict detection unavailable; `wt check` returns error with upgrade instructions. |
| `worktree add --orphan` | 2.42 | `has_orphan: bool` | Orphan branch worktrees not supported; return descriptive error. |
| `worktree.useRelativePaths` | 2.48 | `has_relative_paths: bool` | Skip; use absolute paths (default). |

---

## 10. API Surface

Every public function on `Manager` exactly as specified in PRD SS 5:

```rust
impl Manager {
    pub fn new(
        repo_root: impl AsRef<Path>,
        config: Config,
    ) -> Result<Self, WorktreeError>;

    pub fn create(
        &self,
        branch: impl Into<String>,
        path: impl AsRef<Path>,
        options: CreateOptions,
    ) -> Result<(WorktreeHandle, CopyOutcome), WorktreeError>;

    pub fn delete(
        &self,
        handle: &WorktreeHandle,
        options: DeleteOptions,
    ) -> Result<(), WorktreeError>;

    pub fn list(&self) -> Result<Vec<WorktreeHandle>, WorktreeError>;

    pub fn attach(
        &self,
        path: impl AsRef<Path>,
        setup: bool,
    ) -> Result<WorktreeHandle, WorktreeError>;

    pub fn gc(
        &self,
        options: GcOptions,
    ) -> Result<GcReport, WorktreeError>;

    pub fn git_capabilities(&self) -> &GitCapabilities;

    pub fn port_lease(&self, branch: &str) -> Option<PortLease>;

    pub fn allocate_port(
        &self,
        branch: &str,
        session_uuid: &str,
    ) -> Result<u16, WorktreeError>;

    pub fn release_port(&self, branch: &str) -> Result<(), WorktreeError>;

    pub fn renew_port_lease(
        &self,
        handle: &WorktreeHandle,
    ) -> Result<(), WorktreeError>;  // OQ-1 resolution
}
```

---

## 11. CLI Surface

### 11.1 Commands and Flags (PRD SS 12.1)

```
wt create <branch> [path]
    --base <ref>                            # default: HEAD
    --setup                                 # run ecosystem adapter
    --lock [--reason <reason>]              # lock immediately after creation
    --reflink=<required|preferred|disabled>
    --port                                  # allocate a port lease

wt delete <branch|path>
    --force                                 # skip unmerged commit check
    --force-dirty                           # skip uncommitted changes check

wt attach <path>
    --setup                                 # run ecosystem adapter

wt list
    --json                                  # JSON output
    --porcelain                             # machine-readable output

wt status                                   # aggregate disk usage + worktree states

wt gc
    --dry-run                               # default behavior
    --confirm                               # actually delete
    --max-age <days>                        # override gc_max_age_days
    --force                                 # skip unmerged commit check

wt hook
    --stdin-format claude-code              # Claude Code hook protocol
    --setup                                 # run ecosystem adapter

wt check                                    # conflict detection (reserved, not in v1.0)
```

### 11.2 `wt hook` stdin/stdout Contract (PRD SS 12.2)

**Critical constraint:** Claude Code sends JSON on stdin and expects only the absolute path on stdout. Any extra stdout causes Claude Code to hang silently (confirmed bug `claude-code#27467`).

**stdin (JSON):**
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/repo",
  "hook_event_name": "WorktreeCreate",
  "name": "feature-auth"
}
```

**stdout (exactly one line, nothing else):**
```
/absolute/path/to/created/worktree
```

**stderr (all other output):**
```
[worktree-core] Creating worktree for branch 'feature-auth'...
[worktree-core] Running adapter setup...
[worktree-core] Done.
```

**Implementation requirements:**
1. Read JSON from stdin.
2. Extract `name` field (used as branch name as-is).
3. Create worktree with `Manager::create()`.
4. If `--setup` flag: run adapter.
5. Print only the absolute worktree path to stdout.
6. All subprocess output, git progress, and log messages go to stderr.
7. Exit 0 on success, non-zero on failure.

---

## 12. MCP Tool Table

The `worktree-core-mcp` binary runs as a stdio MCP server. Transport: stdio only in v1.0.

| Tool | readOnlyHint | destructiveHint | idempotentHint | v1.0 |
|---|---|---|---|---|
| `worktree_list` | true | false | true | Yes |
| `worktree_status` | true | false | true | Yes |
| `conflict_check` | true | false | true | Returns `not_implemented` |
| `worktree_create` | false | false | false | Yes |
| `worktree_delete` | false | true | false | Yes |
| `worktree_gc` | false | true | false | Yes |

Tool annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`) are required per MCP spec 2025-03-26+. Read-only tools will not prompt for approval in Claude Code and Cursor.

**WARNING -- VS Code `servers` vs `mcpServers` discrepancy:** VS Code Copilot uses `"servers"` as the root key, not `"mcpServers"`. All other clients (Claude Code, Cursor, OpenCode) use `"mcpServers"`. The README must include config snippets for all formats.

| Client | Config File | Root Key |
|---|---|---|
| Claude Code | `~/.claude.json` or `.mcp.json` | `mcpServers` |
| Cursor | `.cursor/mcp.json` | `mcpServers` |
| VS Code Copilot | `.vscode/mcp.json` | `servers` |
| OpenCode | `opencode.jsonc` | `mcp` |

---

## 13. Dependency Rationale

| Crate | Version | What It Replaces | Why Chosen |
|---|---|---|---|
| `fd-lock` | 4 | Manual `flock()`/`LockFileEx` FFI | Cross-platform advisory file locking with RAII guards. 33M downloads. Auto-release on drop eliminates leak bugs. |
| `sysinfo` | 0.37 | Manual `/proc/<pid>/stat` parsing, `proc_pidinfo`, `GetProcessTimes` | Cross-platform process `start_time` for PID reuse detection in four-factor lock identity. |
| `uuid` | 1 (v4 feature) | Manual random byte generation | UUID v4 for session tracking and multi-factor lock identity. |
| `reflink-copy` | 0.1 | Manual `clonefile(2)`, `FICLONE` ioctl, `FSCTL_DUPLICATE_EXTENTS_TO_FILE` FFI | Cross-platform CoW with automatic fallback to `std::fs::copy()`. |
| `junction` | 1 | Manual `DeviceIoControl` calls | Windows NTFS junction creation without admin privileges. No-op on non-Windows. |
| `jwalk` | 0.8 | `std::fs::read_dir` recursive walk | Rayon-based parallel directory walking. <200ms for 50K files. |
| `filesize` | 0.2 | Manual `st_blocks * 512` / `GetCompressedFileSizeW()` | Cross-platform actual-disk-usage calculation (not logical file size). |
| `directories` | 6 | Manual XDG path computation | XDG-compliant platform-appropriate paths via `ProjectDirs::from("", "", "worktree-core")`. |
| `dunce` | 1 | Manual `\\?\` prefix stripping | Strips verbatim path prefix when passing paths to external tools on Windows. Identity operation on Unix. |
| `thiserror` | 2 | Manual `impl Display` + `impl Error` | Structured error types with `#[error]` derive. Reduces boilerplate. |
| `serde` + `serde_json` | 1 | Manual JSON serialization | `state.json` serialization with `#[serde(flatten)]` for forward compatibility. |
| `chrono` | 0.4 | Manual timestamp formatting | DateTime handling for lease expiry, TTL calculations, ISO 8601 timestamps. |
| `rand` | 0.8 | Manual PRNG | Full Jitter sleep: `rand::random::<u64>() % window`. |
| `sha2` | 0.10 | Manual hash implementation | SHA-256 for `repo_id` field and deterministic port hash assignment. |

**Excluded from core library:** `gix` and `git2` are reserved for v1.1+ conflict detection (`gix::Repository::merge_trees()`) behind a feature flag. They are not dependencies in Milestone 1.

---

## 14. Decisions Log

### OQ-1: Port Lease Renewal Mechanism

**Question (verbatim):** The spec states leases are renewed "every TTL/3 (~2.5 hours) during active use" but does not define what constitutes "active use" or who triggers renewal. Is renewal the responsibility of the Manager (background timer) or the caller? If background, what thread/task model is expected?

**Decision:** Renewal is caller-driven, not background-timer-driven. `Manager` exposes `manager.renew_port_lease(handle)` which callers invoke on any observable activity.

**Reasoning:** A background timer requires a thread or async task runtime (tokio or `std::thread`), which the library must not impose on consumers. The library is synchronous at v1.0. Explicit renewal keeps the library async-runtime-free and gives callers full control over what constitutes "activity." Callers like Claude Squad or workmux already have their own event loops and can call `renew_port_lease` naturally.

**Affected PRD sections:** SS 5.3 (Manager methods), SS 10.4 (Port Lease Model).
**Story impact:** Add `renew_port_lease` to Manager public API in Milestone 1. Document caller responsibility in API docs.

---

### OQ-2: Network Filesystem as Warning vs Error

**Question (verbatim):** The spec says "warning, not hard block by default" but does not expose a Config field to escalate it to an error. Should a `Config::deny_network_filesystem: bool` field be added?

**Decision:** Add `Config::deny_network_filesystem: bool` (default `false`). When `true`, guard #6 returns `NetworkFilesystem` error instead of logging a warning.

**Reasoning:** Enterprise environments with NFS/CIFS home directories need a hard block to prevent data corruption from unreliable `flock()`. The default `false` preserves backward-compatible behavior. Users who know their NFS setup is safe can leave it at default; those who have been bitten by NFS locking failures can escalate to a hard error.

**Affected PRD sections:** SS 4.4 (Config), SS 8.1 (guard #6).
**Story impact:** Add field to Config struct. Modify `check_not_network_filesystem` to branch on config value.

---

### OQ-3: `wt attach` on Bare Repos

**Question (verbatim):** `BareRepositoryUnsupported` was removed in v1.5 (bare repos permitted with explicit paths), but `Manager::attach()` does not specify behavior when attaching a worktree into a bare repo. Clarify: is `attach()` permitted on bare repos?

**Decision:** `Manager::attach()` is permitted on bare repos when path is explicitly provided.

**Reasoning:** Bare repos are a legitimate use case for worktree-based workflows (server-side CI, shared repositories). Since `attach()` operates on an already-existing git worktree (it does not call `git worktree add`), there is no ambiguity about path defaults. The worktree already exists in git's registry; attach simply brings it under worktree-core management.

**Affected PRD sections:** SS 5.3 (Manager::attach).
**Story impact:** Document in function doc comment. No code change needed beyond documentation.

---

### OQ-4: Circuit Breaker Reset

**Question (verbatim):** The `CircuitBreakerOpen` error is documented, but no reset mechanism is specified. Is the circuit breaker reset automatically after a timeout, manually via a Manager method, or on Manager reconstruction?

**Decision:** Auto-reset after configurable timeout (`Config::circuit_breaker_reset_secs: u64`, default 60). No manual reset method; `Manager` reconstruction also resets.

**Reasoning:** A timeout-based reset matches the standard circuit breaker pattern. 60 seconds is long enough to allow transient git issues (network partition, lock contention) to resolve but short enough to avoid permanently bricking a session. Manual reset adds API surface for a rare recovery path. Manager reconstruction (drop and re-create) is the escape hatch for callers who need immediate reset.

**Affected PRD sections:** SS 4.4 (Config), SS 16 (failure modes).
**Story impact:** Add `circuit_breaker_reset_secs` to Config. Track `last_failure_at` timestamp in Manager internal state. Reset counter when `now - last_failure_at > reset_secs`.

---

### OQ-5: Bare Repo `git worktree add` Form

**Question (verbatim):** When the primary repo is bare, `git worktree add` is called from the bare repo root. Confirm that `git worktree add` from a bare repo root works as expected in the minimum supported Git version (2.20) and document the exact command form.

**Decision:** Confirmed safe in Git 2.20. Exact command: `git -C <bare-repo-root> worktree add <path> -b <branch> <base>`.

**Reasoning:** `git worktree add` from a bare repo has been supported since Git 2.5 and is the standard way to create working directories from bare repositories. The `-C` flag ensures the command runs in the bare repo context. Testing against Git 2.20 confirms correct behavior. The path must be absolute (no relative path ambiguity from a bare root).

**Affected PRD sections:** SS 7.2 (exact git commands), SS 8.1 (guard #8 bare repo detection).
**Story impact:** Add bare-repo test case to Milestone 1 ship criteria. Document exact command form in `git.rs`.

---

### OQ-6: `wt gc` Concurrency with Active Agents

**Question (verbatim):** If an agent is actively using a worktree but its process is not the one holding `state.lock`, `gc()` could evict it (if it appears orphaned). Is there a mechanism to mark a worktree as "in use" beyond the `Locked` state (which requires an explicit `git worktree lock` call)?

**Decision:** Add `WorktreeState::InUse { pid: u32, since: String }` variant. `Manager::create()` sets this immediately after `git worktree add` succeeds. `Manager::delete()` clears it. `gc()` skips any worktree in `InUse` state even if `force = true`. PID-liveness check in `gc()` determines if PID is dead before evicting.

**Reasoning:** The `Locked` state requires an explicit `git worktree lock` call, which most agent frameworks do not issue. `InUse` is a lighter-weight signal that the worktree has an active consumer. The PID-liveness check prevents stale `InUse` markers from permanently blocking gc: if the PID is dead (verified via `kill(pid, 0)` + sysinfo start_time check), gc can proceed. This mirrors the stale lock detection logic already in the locking protocol.

**Affected PRD sections:** SS 4.2 (WorktreeState), SS 5.3 (Manager::create, Manager::delete, Manager::gc).
**Story impact:** Add `InUse` variant to `WorktreeState` enum. Modify create/delete/gc sequences. Add state transition rules for `InUse`. Add PID-liveness check to gc path.

---

*End of architecture.md*
