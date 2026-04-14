# ISO_PRD-v1.5 — worktree-core
## Complete Implementation Specification

| Field | Value |
|---|---|
| **Project** | worktree-core |
| **Version** | 1.5 |
| **Date** | April 2026 |
| **Author** | Snehith |
| **Status** | ACTIVE — Implementation-Ready |
| **Git Minimum** | 2.20 |
| **Rust MSRV** | 1.75 |
| **Sprint Model** | Milestone chunks (~1–2 week sprints) |

---

## How to Read This Document

This document is self-contained. An implementation agent or developer starting from zero can implement `worktree-core` using only this file. It contains:

- The complete problem being solved and the rationale for every design decision
- Every public type, trait, and function signature in Rust
- Every git command the library shells out to, with exact flags and expected output
- Every safety check with its complete implementation logic
- All cross-platform constraints, edge cases, and known failure modes
- The complete state persistence schema and locking protocol
- The full crate dependency list with justifications
- Four implementation milestones with explicit ship criteria

**Rules for implementers:**
- Do not invent types or signatures not defined here.
- Do not rename variants.
- Do not change function signatures without opening an Architectural RFC (Section 13).
- When a section says "exact sequence," implement it exactly — the ordering is safety-critical.

---

## Table of Contents

1. [Problem and Motivation](#1-problem-and-motivation)
2. [Scope: What This Library Is and Is Not](#2-scope)
3. [Crate Structure](#3-crate-structure)
4. [Complete Type System](#4-complete-type-system)
5. [Manager — Primary Entry Point](#5-manager)
6. [EcosystemAdapter Trait](#6-ecosystemadapter-trait)
7. [Git Interaction Rules — Non-Negotiable Invariants](#7-git-interaction-rules)
8. [SafetyGuards — Complete Implementation](#8-safety-guards)
9. [Hardened Locking Protocol](#9-locking-protocol)
10. [State Persistence — Schema and Protocol](#10-state-persistence)
11. [Cross-Platform Implementation](#11-cross-platform)
12. [CLI and MCP Surface](#12-cli-and-mcp-surface)
13. [Architectural RFC Process](#13-rfc-process)
14. [Crate Dependencies](#14-crate-dependencies)
15. [Implementation Milestones](#15-implementation-milestones)
16. [Known Failure Modes and Recovery](#16-failure-modes)
17. [Git Version Capability Matrix](#17-git-version-matrix)
18. [Integration Targets](#18-integration-targets)
19. [Open Questions](#19-open-questions)

---

## 1. Problem and Motivation

### 1.1 Background

Every major AI coding orchestrator in 2026 — Claude Code, Claude Squad, Cursor, OpenCode, Gas Town, VS Code Copilot — uses git worktrees as the isolation mechanism for parallel agent sessions. None of these tools share any worktree management code. Each has independently implemented creation, deletion, and cleanup, and each has critical bugs that a shared, battle-tested library would prevent.

This pattern — many small tools solving overlapping problems with no shared foundation — is exactly what precedes a successful shared library. `worktree-core` is that library.

### 1.2 Documented Failures

These are not hypothetical. They are filed bugs on public repositories.

**Silent data loss:**
- `claude-code#38287` (`data-loss`): Cleanup deleted branches with unmerged commits, no warning.
- `claude-code#41010` (`data-loss`): Sub-agent cleanup deleted the parent session's working directory due to agent ID collision, making the entire session unusable.
- `claude-code#29110`: Three agents reported successful task completion; all work discovered lost at verification.
- `vscode#289973`: Background worker cleaned a worktree containing uncommitted changes.

**Unbounded resource consumption:**
- `opencode#14648`: Each retry on failure generated a new random worktree name with no cleanup. A 2 GB repo accumulated hundreds of MB per retry with no upper bound.
- `vscode#296194`: A logic flaw called `git worktree add` on every diff paste, reaching 1,526 worktrees with no circuit breaker.
- Cursor forum: 9.82 GB consumed in a 20-minute session against a ~2 GB codebase.
- `claude-squad#260`: 5 worktrees × 2 GB `node_modules` = 10 GB wasted.

**Broken environment isolation:**
- Two worktrees both trying to bind port 3000.
- Missing `.env` files in new worktrees.
- Database namespace collisions.
- Docker namespace clobbering.

**git-crypt corruption:**
- `claude-code#38538`: Worktree creation produced commits deleting all files in git-crypt repos. The smudge filter had not run, leaving encrypted binary blobs staged as deletions.

**Nested worktree creation:**
- `claude-code#27881`: After context compaction, CWD drifted and `EnterWorktree` created worktrees inside other worktrees.

### 1.3 Design Analogy

`worktree-core` follows two proven models:
- **Testcontainers** — clean lifecycle abstraction (`create → configure → start → use → stop → cleanup`) with framework-specific extensions.
- **simple-git** — thin, unopinionated wrapper around the git CLI (not a reimplementation). simple-git has 4M+ weekly npm downloads. GitKraken is actively migrating away from libgit2. The CLI-wrapping approach is the correct default.

---

## 2. Scope

### 2.1 This Library IS

- A Rust library crate (`worktree-core`) providing safe, concurrent worktree lifecycle management.
- A thin CLI (`wt`) wrapping the library for human and hook use.
- A stdio MCP server (`worktree-core-mcp`) exposing the library to Claude Code, Cursor, and VS Code.
- A git CLI wrapper — all git operations shell out to the user's installed git binary.
- The canonical solution to the data-loss, orphan accumulation, and resource explosion problems documented in Section 1.2.

### 2.2 This Library Is NOT

- A reimplementation of git internals — no `libgit2`, no `gix` for worktree CRUD.
- A replacement for git — it calls git, it does not replace it.
- A container or VM isolation solution — worktrees isolate code, not processes.
- An agent orchestrator — it manages worktree lifecycle, not agent coordination logic.
- A network-aware distributed system — single-machine operation only in v1.0.

### 2.3 Core Design Principles

1. **Shell out to git CLI for everything.** `gix` does not implement `git worktree add/remove/list`. The `git2` crate introduces a C dependency and diverges from the user's installed git behavior.
2. **`git worktree list --porcelain` is always authoritative.** `state.json` is a supplementary cache. If they disagree, git wins.
3. **Safety before speed.** Every deletion path checks for unmerged commits. Every creation path checks disk space, rate limits, and nested paths. Defaults are safe; unsafe behavior requires explicit force flags.

---

## 3. Crate Structure

```
worktree-core/          # Library crate
  src/
    lib.rs              # Public API surface
    manager.rs          # Manager struct and all lifecycle operations
    types.rs            # All public types
    error.rs            # WorktreeError enum
    git.rs              # All git CLI invocations
    guards.rs           # SafetyGuards implementations
    lock.rs             # Hardened locking protocol
    state.rs            # state.json read/write/migrate
    ports.rs            # Port lease model
    platform/
      mod.rs
      macos.rs          # APFS clonefile, getattrlistbulk
      linux.rs          # FICLONE ioctl, /proc/mounts
      windows.rs        # NTFS junctions, LockFileEx, dunce paths
  Cargo.toml

worktree-core-cli/      # Binary crate — `wt` command
  src/main.rs
  Cargo.toml

worktree-core-mcp/      # Binary crate — MCP server (stdio transport)
  src/main.rs
  Cargo.toml
```

All three crates share a workspace `Cargo.toml` at the repo root. The library crate has no binary targets. The CLI and MCP binaries depend on the library crate.

---

## 4. Complete Type System

### 4.1 WorktreeHandle

The opaque reference to a managed worktree. The unit of all operations. All fields are `pub` for reading; mutation happens only through `Manager` methods.

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WorktreeHandle {
    /// Absolute path to the worktree directory on disk.
    pub path: PathBuf,
    /// Branch name exactly as passed to create() — never transformed.
    pub branch: String,
    /// Full 40-char commit SHA at creation time (the --base ref resolved).
    pub base_commit: String,
    /// Current lifecycle state.
    pub state: WorktreeState,
    /// ISO 8601 creation timestamp (UTC).
    pub created_at: String,
    /// PID of the process that called Manager::create().
    pub creator_pid: u32,
    /// Human-readable name of the tool that created this worktree.
    /// Examples: "claude-squad", "workmux", "claude-code", "manual"
    pub creator_name: String,
    /// Name of the EcosystemAdapter used, if any.
    pub adapter: Option<String>,
    /// Whether adapter.setup() completed without error.
    pub setup_complete: bool,
    /// Allocated port number (the actual port, not an offset).
    /// None if port allocation was not requested.
    pub port: Option<u16>,
    /// Stable UUID for this worktree's entire lifetime.
    /// Used in multi-factor lock identity and port lease keying.
    pub session_uuid: String,
}
```

### 4.2 WorktreeState

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WorktreeState {
    /// Allocated in state.json but git worktree add not yet run.
    Pending,
    /// git worktree add is in progress.
    Creating,
    /// Ready for use. Normal operating state.
    Active,
    /// A merge operation involving this worktree is in progress.
    Merging,
    /// git worktree remove is in progress.
    Deleting,
    /// Successfully deleted. Terminal state.
    Deleted,
    /// Present on disk but absent from git worktree list, OR
    /// present in state.json but absent from both disk and git.
    Orphaned,
    /// git references broken, metadata corrupt, or post-create
    /// verification failed (e.g. git-crypt files still encrypted).
    Broken,
    /// git worktree lock has been called on this worktree.
    Locked,
}
```

**Valid state transitions:**

| From | To | Trigger |
|---|---|---|
| `Pending` | `Creating` | `Manager::create` begins `git worktree add` |
| `Creating` | `Active` | `git worktree add` succeeded; post-create checks passed |
| `Creating` | `Broken` | `git worktree add` succeeded but git-crypt check failed; library runs `git worktree remove --force` and returns error |
| `Active` | `Merging` | Merge operation started |
| `Active` | `Deleting` | `Manager::delete` called; pre-delete checks passed |
| `Active` | `Locked` | `git worktree lock` called |
| `Active` | `Orphaned` | Worktree vanished from disk or git registry |
| `Merging` | `Active` | Merge completed or aborted |
| `Locked` | `Active` | `git worktree unlock` called |
| `Deleting` | `Deleted` | `git worktree remove` succeeded |
| Any | `Broken` | Unrecoverable git error detected |

Attempting an unlisted transition returns `WorktreeError::InvalidStateTransition`.

### 4.3 ReflinkMode

```rust
/// Controls Copy-on-Write behavior when copying files into a new worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReflinkMode {
    /// Fail immediately if the filesystem does not support CoW.
    /// Returns: WorktreeError::ReflinkNotSupported
    Required,
    /// Try CoW, fall back to standard copy if unsupported. Default.
    #[default]
    Preferred,
    /// Never attempt CoW. Always use standard copy.
    Disabled,
}

/// Returned by Manager::create() to report what actually happened during file copy steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOutcome {
    Reflinked,
    StandardCopy { bytes_written: u64 },
    /// No file copying occurred (worktree created via git checkout only).
    None,
}
```

### 4.4 Config

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Maximum managed worktrees per repository. Default: 20.
    pub max_worktrees: usize,
    /// Refuse creation if aggregate worktree disk usage exceeds this
    /// percentage of the filesystem. Default: 90.
    pub disk_threshold_percent: u8,
    /// Auto-GC worktrees older than this many days. Default: 7.
    pub gc_max_age_days: u32,
    /// Start of the port range for lease allocation. Default: 3100.
    pub port_range_start: u16,
    /// End of the port range for lease allocation (exclusive). Default: 5100.
    pub port_range_end: u16,
    /// Minimum free disk space required to create a worktree. Default: 500 MB.
    pub min_free_disk_mb: u64,
    /// Override all state file paths (useful for CI and containers).
    /// Mirrors the WORKTREE_CORE_HOME environment variable.
    pub home_override: Option<PathBuf>,
    /// Maximum aggregate disk usage across all managed worktrees in bytes.
    /// None = unlimited. Default: None.
    pub max_total_disk_bytes: Option<u64>,
    /// Trip circuit breaker after this many consecutive git command failures.
    /// Default: 3.
    pub circuit_breaker_threshold: u32,
    /// How long evicted metadata is preserved in state.json before
    /// permanent deletion. Default: 30 days.
    pub stale_metadata_ttl_days: u32,
    /// Total timeout for state.lock acquisition including all retries.
    /// Default: 30,000 ms.
    pub lock_timeout_ms: u64,
    /// Name recorded in state.json as creator_name for this Manager instance.
    /// Example: "claude-squad", "workmux", "my-orchestrator"
    pub creator_name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_worktrees: 20,
            disk_threshold_percent: 90,
            gc_max_age_days: 7,
            port_range_start: 3100,
            port_range_end: 5100,
            min_free_disk_mb: 500,
            home_override: None,
            max_total_disk_bytes: None,
            circuit_breaker_threshold: 3,
            stale_metadata_ttl_days: 30,
            lock_timeout_ms: 30_000,
            creator_name: "worktree-core".to_string(),
        }
    }
}
```

### 4.5 CreateOptions

```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct CreateOptions {
    /// Base ref to create the worktree from. Default: HEAD.
    pub base: Option<String>,
    /// Run the registered EcosystemAdapter after creation. Default: false.
    pub setup: bool,
    /// Skip aggregate disk limit check. Default: false.
    pub ignore_disk_limit: bool,
    /// Call git worktree lock immediately after creation (atomic — no race
    /// window between creation and locking). Default: false.
    pub lock: bool,
    /// Reason string for git worktree lock. Requires lock = true.
    pub lock_reason: Option<String>,
    /// Controls Copy-on-Write behavior for file operations. Default: Preferred.
    pub reflink_mode: ReflinkMode,
    /// Allocate a port lease for this worktree. Default: false.
    pub allocate_port: bool,
}
```

### 4.6 DeleteOptions

```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DeleteOptions {
    /// Skip the five-step unmerged commits check.
    /// WARNING: Can cause data loss. Requires explicit opt-in. Default: false.
    pub force: bool,
    /// Skip the uncommitted changes check.
    /// WARNING: Destroys uncommitted work. Default: false.
    pub force_dirty: bool,
}
```

### 4.7 GcOptions and GcReport

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GcOptions {
    /// Report what would happen without doing anything. Default: true.
    /// Always run with dry_run = true first to verify scope.
    pub dry_run: bool,
    /// Override gc_max_age_days from Config for this run.
    pub max_age_days: Option<u32>,
    /// Skip unmerged commit check during deletion. Default: false.
    pub force: bool,
}

impl Default for GcOptions {
    fn default() -> Self {
        Self { dry_run: true, max_age_days: None, force: false }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GcReport {
    /// Worktrees identified as orphaned.
    pub orphans: Vec<PathBuf>,
    /// Worktrees actually removed (empty if dry_run = true).
    pub removed: Vec<PathBuf>,
    /// Worktrees moved to stale_worktrees (not deleted; metadata preserved for recovery).
    pub evicted: Vec<PathBuf>,
    /// Total disk space freed in bytes (0 if dry_run = true).
    pub freed_bytes: u64,
    /// Whether this was a dry run.
    pub dry_run: bool,
}
```

### 4.8 GitCapabilities

Detected once at `Manager::new()`. Used to gate feature usage for the Manager's lifetime.

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GitCapabilities {
    pub version: GitVersion,
    /// git worktree list --porcelain -z (2.36+)
    /// When false, parser uses newline-delimited output.
    /// NOTE: Paths containing newlines will fail silently on < 2.36.
    pub has_list_nul: bool,
    /// git worktree repair (2.30+)
    pub has_repair: bool,
    /// git worktree add --orphan (2.42+)
    pub has_orphan: bool,
    /// worktree.useRelativePaths config (2.48+)
    pub has_relative_paths: bool,
    /// git merge-tree --write-tree (2.38+). Required for v1.1 conflict detection.
    pub has_merge_tree_write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GitVersion {
    pub const MINIMUM: GitVersion = GitVersion { major: 2, minor: 20, patch: 0 };
    pub const HAS_LIST_NUL: GitVersion = GitVersion { major: 2, minor: 36, patch: 0 };
    pub const HAS_REPAIR: GitVersion = GitVersion { major: 2, minor: 30, patch: 0 };
    pub const HAS_MERGE_TREE_WRITE: GitVersion = GitVersion { major: 2, minor: 38, patch: 0 };
}
```

### 4.9 PortLease

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PortLease {
    pub port: u16,
    pub branch: String,
    pub session_uuid: String,
    pub pid: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// "active" or "stale" (after worktree eviction, before TTL expiry).
    pub status: String,
}
```

### 4.10 WorktreeError

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WorktreeError {
    #[error("git not found in PATH — install git 2.20 or later")]
    GitNotFound,
    #[error("git version too old: required {required}, found {found}")]
    GitVersionTooOld { required: String, found: String },
    #[error("branch '{branch}' is already checked out at '{worktree}'")]
    BranchAlreadyCheckedOut { branch: String, worktree: PathBuf },
    #[error("worktree path already exists: {0}")]
    WorktreePathExists(PathBuf),
    #[error("uncommitted changes in worktree — use force_dirty to override: {files:?}")]
    UncommittedChanges { files: Vec<String> },
    #[error("unmerged commits on '{branch}': {commit_count} commit(s) not in upstream — use force to override")]
    UnmergedCommits { branch: String, commit_count: usize },
    #[error("insufficient disk space: {available_mb}MB available, {required_mb}MB required")]
    DiskSpaceLow { available_mb: u64, required_mb: u64 },
    #[error("aggregate worktree disk usage exceeds limit")]
    AggregateDiskLimitExceeded,
    #[error("target is on a network filesystem — performance not guaranteed: {mount_point}")]
    NetworkFilesystem { mount_point: PathBuf },
    #[error("cannot create Windows junction targeting network path: {path}")]
    NetworkJunctionTarget { path: PathBuf },
    #[error("cannot create worktree across WSL/Windows filesystem boundary")]
    WslCrossBoundary,
    #[error("submodule context detected — run from superproject root")]
    SubmoduleContext,
    #[error("state lock contention — another process holds the lock after {timeout_ms}ms")]
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
    #[error("git-crypt encrypted files detected after checkout — unlock the repository first")]
    GitCryptLocked,
    #[error("CoW (reflink) required but filesystem does not support it")]
    ReflinkNotSupported,
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: WorktreeState, to: WorktreeState },
    #[error("git command failed\n  command: {command}\n  stderr: {stderr}\n  exit: {exit_code}")]
    GitCommandFailed { command: String, stderr: String, exit_code: i32 },
    #[error("state file corrupted: {reason} — rebuild from git worktree list")]
    StateCorrupted { reason: String },
    #[error("circuit breaker open: {consecutive_failures} consecutive git failures")]
    CircuitBreakerOpen { consecutive_failures: u32 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

> **Note:** `BareRepositoryUnsupported` is removed. Bare repos are supported; callers must supply explicit absolute paths. Detection is handled by the `check_bare_repo` guard (Section 8.1), which adjusts path defaults rather than blocking creation.

---

## 5. Manager

### 5.1 Struct

```rust
pub struct Manager {
    repo_root: PathBuf,        // Absolute, canonicalized via dunce
    config: Config,
    capabilities: GitCapabilities,
    // additional private fields
}
```

### 5.2 Constructor

```rust
/// Construct a Manager for the given repository root.
///
/// Startup sequence (in order — abort on first failure):
/// 1. Run `git --version`, parse version. Return GitVersionTooOld if < 2.20.
/// 2. Canonicalize repo_root via dunce::canonicalize().
/// 3. Confirm this is a git repository via `git rev-parse --git-dir`.
/// 4. Detect and store git capabilities (see Section 4.8).
/// 5. Create <repo>/.git/worktree-core/ directory if absent.
/// 6. Acquire state.lock and read or initialize state.json (see Section 9).
/// 7. Run startup orphan scan (non-blocking; logs warnings only).
/// 8. Sweep expired port leases.
pub fn new(
    repo_root: impl AsRef<Path>,
    config: Config,
) -> Result<Self, WorktreeError>;
```

### 5.3 Public Methods

```rust
impl Manager {
    /// Create a new managed worktree.
    ///
    /// Branch name is passed through as-is — never transformed, never
    /// validated for git-legal characters (git will reject invalid names).
    ///
    /// Returns (WorktreeHandle, CopyOutcome). CopyOutcome reflects file-copy
    /// steps (adapter setup, .env copying, etc.), not the git checkout.
    ///
    /// Exact sequence — do not reorder:
    ///   1. Run all pre-create guards (Section 8.1).
    ///   2. Write Pending entry to state.json.
    ///   3. Transition state to Creating.
    ///   4. Run `git worktree add` (with --lock if options.lock = true).
    ///   5. Run post-create verification (Section 8.3: git-crypt check).
    ///   6. Run EcosystemAdapter::setup() if options.setup = true.
    ///   7. Transition state to Active.
    ///   8. Write final state to state.json (with lock).
    ///
    /// On any failure after step 4 (git worktree add) succeeds:
    ///   Run `git worktree remove --force <path>` before returning error.
    ///   Never leave a partial worktree on disk.
    pub fn create(
        &self,
        branch: impl Into<String>,
        path: impl AsRef<Path>,
        options: CreateOptions,
    ) -> Result<(WorktreeHandle, CopyOutcome), WorktreeError>;

    /// Delete a managed worktree.
    ///
    /// Exact sequence — do not reorder:
    ///   1. check_not_cwd()
    ///   2. check_no_uncommitted_changes()  [skipped if options.force_dirty]
    ///   3. five_step_unmerged_check()      [skipped if options.force]
    ///   4. check_not_locked()
    ///   5. Transition state to Deleting; write state.
    ///   6. Run `git worktree remove <path>`.
    ///   7. Transition state to Deleted.
    ///   8. Release port lease if held.
    ///   9. Write final state to state.json (with lock).
    pub fn delete(
        &self,
        handle: &WorktreeHandle,
        options: DeleteOptions,
    ) -> Result<(), WorktreeError>;

    /// List all worktrees known to git for this repository.
    ///
    /// Always calls `git worktree list --porcelain [-z]`.
    /// Reconciles with state.json: entries in state.json missing from git
    /// output are moved to stale_worktrees, not silently dropped.
    pub fn list(&self) -> Result<Vec<WorktreeHandle>, WorktreeError>;

    /// Register an existing worktree under worktree-core management.
    ///
    /// Preconditions:
    ///   - The worktree must already exist in git's registry.
    ///   - Does NOT call `git worktree add`.
    ///
    /// If a stale_worktrees entry exists for this path, recovers its
    /// port lease and session_uuid.
    pub fn attach(
        &self,
        path: impl AsRef<Path>,
        setup: bool,
    ) -> Result<WorktreeHandle, WorktreeError>;

    /// Run garbage collection on orphaned and stale worktrees.
    ///
    /// Rules:
    ///   - Default behavior (GcOptions::default()) is dry_run = true.
    ///   - Never touches locked worktrees regardless of options.force.
    ///   - Runs the five-step unmerged commit check before any deletion
    ///     unless options.force = true.
    pub fn gc(
        &self,
        options: GcOptions,
    ) -> Result<GcReport, WorktreeError>;

    /// Return the detected git capability map.
    pub fn git_capabilities(&self) -> &GitCapabilities;

    /// Return the active port lease for a branch, if any.
    pub fn port_lease(&self, branch: &str) -> Option<PortLease>;

    /// Allocate a port lease for a branch without creating a worktree.
    pub fn allocate_port(&self, branch: &str, session_uuid: &str) -> Result<u16, WorktreeError>;

    /// Release a port lease explicitly.
    pub fn release_port(&self, branch: &str) -> Result<(), WorktreeError>;
}
```

---

## 6. EcosystemAdapter Trait

```rust
pub trait EcosystemAdapter: Send + Sync {
    /// Name used in state.json and log messages.
    fn name(&self) -> &str;

    /// Return true if this adapter should run for the given worktree path.
    /// Called during auto-detection. Inspect package.json, Cargo.toml, etc.
    fn detect(&self, worktree_path: &Path) -> bool;

    /// Set up the environment in the new worktree.
    ///
    /// source_worktree is the main worktree path (for copying files from).
    ///
    /// Environment variables set before this call:
    ///   WORKTREE_CORE_PATH   — absolute path to the new worktree
    ///   WORKTREE_CORE_BRANCH — branch name
    ///   WORKTREE_CORE_REPO   — absolute path to the main repo
    ///   WORKTREE_CORE_NAME   — branch name (alias for compatibility with
    ///                          CCManager and workmux conventions)
    ///   WORKTREE_CORE_PORT   — allocated port as string, or "" if none
    ///   WORKTREE_CORE_UUID   — session UUID
    ///
    /// Compatibility mapping:
    ///   CCManager: CCMANAGER_WORKTREE_PATH, CCMANAGER_BRANCH_NAME, CCMANAGER_GIT_ROOT
    ///   workmux:   WM_WORKTREE_PATH, WM_PROJECT_ROOT
    fn setup(
        &self,
        worktree_path: &Path,
        source_worktree: &Path,
    ) -> Result<(), WorktreeError>;

    /// Clean up adapter-managed resources when the worktree is deleted.
    fn teardown(&self, worktree_path: &Path) -> Result<(), WorktreeError>;

    /// Optionally transform the branch name before use.
    /// Default: identity (no transformation).
    /// The core library NEVER calls this internally. Only adapters that opt in use it.
    fn branch_name(&self, input: &str) -> String {
        input.to_string()
    }
}
```

### 6.1 Built-in Adapters (v1.0)

**DefaultAdapter** — copies files from a configurable list into the new worktree.

```rust
pub struct DefaultAdapter {
    /// Relative paths resolved against source_worktree.
    /// Examples: [".env", ".env.local", "config/local.toml"]
    pub files_to_copy: Vec<PathBuf>,
}
```

**ShellCommandAdapter** — runs arbitrary shell commands at create/delete time. Receives all `WORKTREE_CORE_*` environment variables. Mirrors Cursor's `.cursor/worktrees.json` and workmux's `post_create` hooks.

```rust
pub struct ShellCommandAdapter {
    pub post_create: Option<String>,
    pub pre_delete: Option<String>,
    pub post_delete: Option<String>,
}
```

Ecosystem-specific adapters (pnpm, npm, uv/pip, cargo) are deferred to community contributions. The `EcosystemAdapter` trait interface is stable and documented to enable this.

---

## 7. Git Interaction Rules — Non-Negotiable Invariants

These invariants must never be violated. If a design requires violating one, open an Architectural RFC (Section 13) before proceeding.

1. **Shell out to git CLI for all worktree operations.** Use `std::process::Command`. Do not use `git2` or `gix` for worktree add/remove/list.
2. **`git worktree list --porcelain` is always authoritative.** If `state.json` disagrees, `state.json` is reconciled against git's output, not the reverse.
3. **Never write to `.git/worktrees/` directly.** Writing `.git/worktrees/*/gitdir` directly is the root cause of the Cursor repository corruption bug.
4. **Never invoke `git gc` or `git prune`.** Use `git maintenance run --auto` only. `git gc --prune=all` is known to corrupt linked worktrees in Git < 2.14 and remains risky in concurrent scenarios.
5. **Handle lock failures with retry.** `index.lock exists` and `cannot lock ref` are transient errors. Implement bounded Full Jitter exponential backoff (Section 9.4). Never treat them as fatal on first occurrence.
6. **Parse `--porcelain` output, not human-readable output.** The human-readable format is not stable across git versions.
7. **On `git worktree add` failure, clean up immediately.** If `git worktree add` returns non-zero, delete any partially-created directory with `rm -rf` (not `git worktree remove`, which may also fail).
8. **On any failure after `git worktree add` succeeds, run `git worktree remove --force <path>` before returning the error.** Never leave an unregistered worktree directory on disk.

### 7.1 `git worktree list --porcelain` Output Format

At Git ≥ 2.20, output is newline-delimited. At Git ≥ 2.36, `-z` makes output NUL-delimited (required for paths with special characters). The library uses `-z` when available.

**Format (one worktree per blank-line-separated block):**
```
worktree /absolute/path/to/main
HEAD abc1234abc1234abc1234abc1234abc1234abc1234
branch refs/heads/main

worktree /absolute/path/to/feature
HEAD def5678def5678def5678def5678def5678def5678
branch refs/heads/feature
locked reason why it is locked

worktree /absolute/path/to/detached
HEAD 9ab0123
detached
prunable gitdir file points to non-existent location
```

**Fields per block:**
- `worktree <path>` — always first, always present
- `HEAD <40-char-sha>` — present for all non-bare worktrees
- Exactly one of: `branch refs/heads/<name>` | `detached` | `bare`
- Optional: `locked` or `locked <reason>` (Git 2.31+)
- Optional: `prunable <reason>` (Git 2.31+)

**Parser behavior:**
- Split on blank lines to get blocks.
- For each block, split on newlines and parse key-value pairs.
- If `locked` is present, set `WorktreeState::Locked`.
- If `prunable` is present, set `WorktreeState::Orphaned`.
- If a path appears truncated (no `-z` support), log a warning: `"Worktree path may contain newlines — upgrade to git 2.36 for safe parsing"`.

### 7.2 Exact git Commands Used

```bash
# Version detection
git --version

# Repository and configuration detection
git rev-parse --git-dir
git rev-parse --is-bare-repository
git rev-parse --show-superproject-working-tree
git rev-parse --is-shallow-repository
git symbolic-ref refs/remotes/origin/HEAD   # primary branch detection

# List worktrees
git worktree list --porcelain
git worktree list --porcelain -z            # Git >= 2.36

# Create worktree (new branch)
git worktree add <path> -b <branch> [<base>]
# Create worktree (existing branch)
git worktree add <path> <branch>
# Create and immediately lock (no race window) — Git >= 2.17
git worktree add --lock [--reason <reason>] <path> -b <branch> [<base>]

# Remove
git worktree remove <path>
git worktree remove --force <path>

# Prune stale metadata
git worktree prune

# Repair broken gitdir links — Git >= 2.30
git worktree repair

# Lock/unlock
git worktree lock [--reason <reason>] <path>
git worktree unlock <path>

# Five-step unmerged commit check (Section 8.2.1)
git fetch --prune origin
git merge-base --is-ancestor <branch> <primary>
git merge-base --is-ancestor <branch> origin/<primary>
git cherry -v origin/<primary> <branch>
git log <branch> --not --remotes --oneline

# Uncommitted change check
git -C <path> status --porcelain

# Conflict detection — v1.1 only, Git >= 2.38
git merge-tree --write-tree -z --stdin
```

---

## 8. Safety Guards

All guards are internal functions in `src/guards.rs`. They are not part of the public API.

### 8.1 Pre-Create Guards

Run in this exact order inside `Manager::create()`, before `git worktree add` is called. All must pass.

```rust
// 1. Branch not already checked out
// Runs: git worktree list --porcelain
// Scans output for: branch refs/heads/<branch>
// Returns: BranchAlreadyCheckedOut { branch, worktree }
fn check_branch_not_checked_out(repo: &Path, branch: &str, caps: &GitCapabilities) -> Result<(), WorktreeError>;

// 2. Minimum free disk space
// Uses: statvfs() on Unix, GetDiskFreeSpaceEx on Windows
// Returns: DiskSpaceLow { available_mb, required_mb }
fn check_disk_space(target_path: &Path, required_mb: u64) -> Result<(), WorktreeError>;

// 3. Worktree count limit
// Returns: RateLimitExceeded { current, max }
fn check_worktree_count(current: usize, max: usize) -> Result<(), WorktreeError>;

// 4. Target path does not already exist
// Returns: WorktreePathExists
fn check_path_not_exists(path: &Path) -> Result<(), WorktreeError>;

// 5. Target path not nested inside any existing worktree (and vice versa)
// Uses dunce::canonicalize on both paths before starts_with check.
// Checks both directions: candidate inside existing AND existing inside candidate.
// Returns: NestedWorktree { parent }
fn check_not_nested_worktree(candidate: &Path, existing: &[WorktreeHandle]) -> Result<(), WorktreeError>;

// 6. Not a network filesystem (warning, not hard block by default)
// Linux: parse /proc/mounts or use statfs() st_fstype
// macOS: use statfs() f_fstypename
// Windows: use GetDriveTypeW()
// Returns: NetworkFilesystem { mount_point }
fn check_not_network_filesystem(path: &Path) -> Result<(), WorktreeError>;

// 7. Not crossing WSL/Windows filesystem boundary
// Detect WSL via /proc/version containing "Microsoft"
// Returns: WslCrossBoundary if repo on /mnt/* and worktree not, or vice versa
fn check_not_wsl_cross_boundary(repo: &Path, worktree: &Path) -> Result<(), WorktreeError>;

// 8. Bare repository detection
// Runs: git rev-parse --is-bare-repository
// Returns true if bare; caller adjusts path defaults (bare repos are permitted).
fn check_bare_repo(repo: &Path) -> Result<bool, WorktreeError>;

// 9. Submodule context
// Runs: git rev-parse --show-superproject-working-tree
// Returns true if inside a submodule; caller returns SubmoduleContext error.
fn check_submodule_context(repo: &Path) -> Result<bool, WorktreeError>;

// 10. Aggregate disk usage
// Uses jwalk + filesize to walk all worktree directories.
// Skips .git/ (shared across worktrees, not owned by any individual worktree).
// Returns: AggregateDiskLimitExceeded
fn check_total_disk_usage(repo: &Path, limit: Option<u64>) -> Result<(), WorktreeError>;

// 11. Windows: junction target is not a network path (Windows only)
// A network target starts with \\ but not \\?\
// Returns: NetworkJunctionTarget { path }
#[cfg(target_os = "windows")]
fn check_not_network_junction_target(path: &Path) -> Result<(), WorktreeError>;

// 12. git-crypt pre-create check
// See Section 8.3 for full implementation.
fn check_git_crypt_pre_create(repo: &Path) -> Result<GitCryptStatus, WorktreeError>;
```

### 8.2 Pre-Delete Guards

Run in this exact order inside `Manager::delete()` and inside `Manager::gc()` for each worktree to be deleted.

```rust
// 1. Not deleting CWD
// Compares dunce::canonicalize(path) == dunce::canonicalize(current_dir())
// Returns: CannotDeleteCwd
fn check_not_cwd(path: &Path) -> Result<(), WorktreeError>;

// 2. Uncommitted changes check
// Runs: git -C <path> status --porcelain
// Returns list of affected files, or UncommittedChanges if non-empty.
// Skipped if DeleteOptions.force_dirty = true.
fn check_no_uncommitted_changes(path: &Path) -> Result<Vec<String>, WorktreeError>;

// 3. Worktree not locked
// Checks handle.state == WorktreeState::Locked
// Returns: WorktreeLocked { reason }
// NOTE: gc() NEVER touches locked worktrees regardless of the force flag.
fn check_not_locked(handle: &WorktreeHandle) -> Result<(), WorktreeError>;

// 4. Five-step unmerged commit decision tree
// Skipped if DeleteOptions.force = true.
// See Section 8.2.1 for exact implementation.
fn five_step_unmerged_check(branch: &str, repo: &Path) -> Result<(), WorktreeError>;
```

### 8.2.1 Five-Step Unmerged Commit Decision Tree

This replaces the naive single-command check (`git log <branch> --not --remotes`). The old check produced false positives when no remote was configured and false negatives for squash-merged branches.

**Precondition:** Before Step 1, run `git rev-parse --is-shallow-repository`. If the repo is shallow, skip Steps 2–4, go directly to Step 5, and log `WARNING: shallow repo detected — remote ancestor checks skipped`.

**Determine primary branch:** Run `git symbolic-ref refs/remotes/origin/HEAD`. Fall back to `"main"` then `"master"` if that fails.

**Step 1: `git fetch --prune origin`**
- Network error → skip this step; log `WARNING: fetch failed, skipping remote check`. Do NOT block deletion due to fetch failure.

**Step 2: `git merge-base --is-ancestor <branch> <primary_branch>`**
- Exit 0 → branch is fully merged locally → **SAFE TO DELETE** → return `Ok(())`
- Exit 1 → not merged locally → continue to Step 3
- Exit 128 → git error (shallow clone, invalid ref) → log `WARNING`, continue to Step 4

**Step 3: `git merge-base --is-ancestor <branch> origin/<primary_branch>`**
- Exit 0 → branch is fully merged into remote → **SAFE TO DELETE** → return `Ok(())`
- Exit 1 → not merged into remote → continue to Step 4
- Exit 128 → no remote exists → continue to Step 4

**Step 4: `git cherry -v origin/<primary_branch> <branch>`**
- Lines starting with `+` = commits unique to `<branch>` (not yet upstream)
- Lines starting with `-` = patches that exist upstream (patch-ID match; handles squash/rebase)
- No `+` lines → all patches are upstream → **SAFE TO DELETE** → return `Ok(())`
- `+` lines present → unique commits remain → continue to Step 5
- Command fails (no remote) → continue to Step 5

**Step 5: `git log <branch> --not --remotes --oneline`**
- Count output lines = number of unpushed commits
- 0 lines → **SAFE TO DELETE** → return `Ok(())`
- \> 0 lines → return `Err(WorktreeError::UnmergedCommits { branch, commit_count })`

**Edge cases:**
- **Orphan branches** (no commits): `merge-base` returns exit 1. Step 5 returns 0 lines. Orphan branches are safe to delete.
- **No remote configured:** Steps 1, 3, 4 fail gracefully. Step 5 is the final arbiter.

### 8.3 git-crypt Detection Protocol

**Why this matters:** `claude-code#38538` — worktree creation with git-crypt produced commits that deleted all files. The smudge filter had not run, leaving encrypted binary blobs staged as deletions.

**Implementation:**

```rust
pub enum GitCryptStatus {
    NotUsed,
    LockedNoKey,   // Key file absent
    Locked,        // Key exists but files show magic header — smudge filter not run
    Unlocked,      // Key exists and files are decrypted
}

const GIT_CRYPT_MAGIC: &[u8; 10] = b"\x00GITCRYPT\x00";

fn is_git_crypt_encrypted(path: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 10];
    match file.read_exact(&mut header) {
        Ok(_) => Ok(&header == GIT_CRYPT_MAGIC),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}
```

**Four-step detection sequence:**

1. **Parse `.gitattributes`** for `filter=git-crypt` or `filter=git-crypt-<keyname>` lines. If none found, return `GitCryptStatus::NotUsed`.
2. **Check for key file** at `<git_dir>/git-crypt/keys/default` (and named keys). `<git_dir>` from `git rev-parse --git-dir`. If absent, return `GitCryptStatus::LockedNoKey` with a warning.
3. **Check for `.git-crypt/` directory** in worktree root. Confirms git-crypt is configured.
4. **Byte-level inspection** of each file matched by a `filter=git-crypt` `.gitattributes` rule:
   - Read exactly the first 10 bytes.
   - If any file matches `GIT_CRYPT_MAGIC` → files are still encrypted → return `GitCryptStatus::Locked`.
   - If all files differ → return `GitCryptStatus::Unlocked`.

**Post-create action (called after `git worktree add`, before returning `Ok()`):**
1. Run the four-step detection on the new worktree.
2. If result is `Locked`:
   - Run `git worktree remove --force <path>`.
   - Return `WorktreeError::GitCryptLocked`.
   - Do NOT leave the partially-initialized worktree on disk.

### 8.4 Nested Worktree Path Containment

Both the candidate path and all existing worktree paths must be canonicalized before comparison. Raw string comparison fails on case-insensitive filesystems (APFS, NTFS) and with symlinks.

```rust
fn check_not_nested_worktree(
    candidate: &Path,
    existing: &[WorktreeHandle],
) -> Result<(), WorktreeError> {
    let canon_candidate = dunce::canonicalize(candidate)
        .unwrap_or_else(|_| candidate.to_path_buf());

    for wt in existing {
        let canon_existing = dunce::canonicalize(&wt.path)
            .unwrap_or_else(|_| wt.path.clone());

        // Case 1: New worktree would be inside an existing one.
        if canon_candidate.starts_with(&canon_existing) {
            return Err(WorktreeError::NestedWorktree { parent: wt.path.clone() });
        }
        // Case 2: An existing worktree would be inside the new one.
        if canon_existing.starts_with(&canon_candidate) {
            return Err(WorktreeError::NestedWorktree { parent: canon_candidate });
        }
    }
    Ok(())
}
```

> **Note:** `Path::starts_with` checks component boundaries, not string prefixes. `/foobar` does NOT start with `/foo`. This is correct behavior; canonicalization handles case sensitivity and symlinks.

---

## 9. Locking Protocol

### 9.1 Why PID-Only Locks Fail

On Linux, PIDs cycle within ~32,768 increments. In container environments with PID namespacing, a new process can reuse PID 1 within seconds of the previous process dying. A `kill(pid, 0)` check on a reused PID returns success even though the original lock holder is dead.

This library uses a **four-factor check** (PID + process start time + UUID + hostname) modeled after PostgreSQL's `postmaster.pid`. A live process with the same PID but a different start time means the PID was reused and the lock is stale.

### 9.2 Lock File Format

The lock file (`state.lock`) contains a single JSON record:

```json
{
  "pid": 42891,
  "start_time": 1744533600,
  "uuid": "f7a3b9c1-2d4e-4f56-a789-0123456789ab",
  "hostname": "dev-container-7f3a",
  "acquired_at": "2026-04-13T10:00:00Z"
}
```

- `pid` — process ID of the lock holder
- `start_time` — epoch seconds of process start time, from `sysinfo::Process::start_time()`. Linux: `/proc/<pid>/stat` field 22. macOS: `proc_pidinfo`. Windows: `GetProcessTimes`.
- `uuid` — UUID v4 generated at lock acquisition time. Used as correlation handle in logs.
- `hostname` — for diagnosing multi-container environments sharing a filesystem.
- `acquired_at` — human-readable acquisition time for debugging.

### 9.3 Stale Detection Logic (Four-Factor Check)

```
1. Open state.lock.
   If absent → no lock held → proceed to acquisition.

2. Deserialize JSON record.
   If parse fails → lock file is corrupt (crashed mid-write).
   Log WARNING: "Stale lock detected: corrupt JSON, overwriting".
   Delete state.lock → proceed to acquisition.

3. Check kill(pid, 0).
   If ESRCH (no such process) → process is dead.
   Log WARNING: "Stale lock detected: PID {pid} no longer exists".
   Delete state.lock → proceed to acquisition.
   If other error → treat as live (conservative) → enter retry loop.

4. If process is alive, verify start_time via sysinfo.
   If start_time != lock_record.start_time:
     PID was reused.
     Log WARNING: "Stale lock detected: PID reused (start time mismatch)".
     Delete state.lock → proceed to acquisition.
   If start_time matches AND uuid differs from current session:
     Lock is genuinely held → enter Full Jitter retry loop.
```

### 9.4 Full Jitter Backoff

Formula (from AWS analysis of 100 concurrent lock contenders):

```
sleep_ms = random(0, min(cap_ms, base_ms × 2^attempt))
```

Parameters:
- `base_ms` = 10
- `cap_ms` = 2000
- `max_attempts` = 15 (~30s total worst case)

```rust
fn acquire_lock_with_backoff(
    lock_path: &Path,
    timeout_ms: u64,
) -> Result<LockGuard, WorktreeError> {
    let started = std::time::Instant::now();
    let mut attempt = 0u32;
    loop {
        if lock_path.exists() {
            match check_stale(lock_path) {
                StaleResult::Stale => { std::fs::remove_file(lock_path)?; }
                StaleResult::Live  => { /* fall through to retry */ }
            }
        }
        match try_acquire(lock_path) {
            Ok(guard) => {
                write_lock_record(lock_path)?;
                return Ok(guard);
            }
            Err(_) => {
                let elapsed_ms = started.elapsed().as_millis() as u64;
                if elapsed_ms >= timeout_ms {
                    return Err(WorktreeError::StateLockContention { timeout_ms });
                }
                let cap = 2000u64;
                let base = 10u64;
                let window = cap.min(base.saturating_mul(1 << attempt.min(10)));
                let sleep_ms = rand::random::<u64>() % (window + 1);
                std::thread::sleep(Duration::from_millis(sleep_ms));
                attempt += 1;
            }
        }
    }
}
```

### 9.5 Exact Lock Acquisition Sequence

Do not reorder these steps:

1. Run four-factor stale detection on `state.lock`. Delete if stale.
2. Attempt non-blocking exclusive advisory lock via `fd-lock` crate. On failure, enter Full Jitter retry loop. On timeout, return `WorktreeError::StateLockContention`.
3. Write multi-factor JSON record to `state.lock` contents.
4. Read `state.json`.
5. Apply mutation in memory.
6. Write new content to `state.json.tmp` (same directory).
7. `fsync()` the temp file descriptor.
8. `rename(state.json.tmp → state.json)` — atomic on POSIX same-filesystem; also atomic on Windows within the same volume.
9. Drop lock guard (RAII: OS releases `fd-lock` automatically).

**Critical invariants:**
- **Never delete `state.lock` after releasing.** Leave it in place; the next acquisition overwrites the record. Deleting it introduces a race.
- **Never hold `state.lock` across `git worktree add`.** The lock scope is ONLY around the `state.json` read-modify-write cycle. `git worktree add` can take seconds; holding the lock that long blocks all other agents.

### 9.6 Network Filesystem Degradation

`flock()` is unreliable on NFS (was a no-op before Linux kernel 5.5 in some configurations). Detection:

```rust
fn is_network_filesystem(path: &Path) -> bool {
    // Linux: check /proc/mounts for nfs, nfs4, cifs, smbfs
    // macOS: statfs() f_fstypename starts with "nfs", "afp", or "smbfs"
    // Windows: GetDriveTypeW() returns DRIVE_REMOTE
}
```

If network filesystem detected:
- Skip advisory lock acquisition.
- Log `WARNING: "Network filesystem detected — advisory locking skipped. Using atomic rename only."`
- Still perform `rename(tmp → state.json)` for atomicity.
- Concurrent access guarantees are reduced; document this in error messages.

---

## 10. State Persistence

### 10.1 File Locations

| Data | Location | Notes |
|---|---|---|
| Worktree metadata + port leases | `<repo>/.git/worktree-core/state.json` | Safe from `git gc` — custom dirs in `.git/` are never pruned |
| Lock file | `<repo>/.git/worktree-core/state.lock` | Adjacent to state for atomic coordination |
| User preferences | `$XDG_CONFIG_HOME/worktree-core/config.toml` | macOS: `~/Library/Application Support/worktree-core/` |
| Cache | `$XDG_CACHE_HOME/worktree-core/` | Disposable |
| Logs | `$XDG_STATE_HOME/worktree-core/` | Falls back to `$XDG_CACHE_HOME` on macOS/Windows |

`WORKTREE_CORE_HOME` environment variable overrides all computed paths. When set, all state files go under `$WORKTREE_CORE_HOME/`. Use the `directories` crate v6.0.0 via `ProjectDirs::from("", "", "worktree-core")`.

### 10.2 state.json Schema (v2)

```json
{
  "schema_version": 2,
  "repo_id": "<sha256 of absolute canonicalized repo path>",
  "last_modified": "2026-04-13T14:22:00Z",
  "active_worktrees": {
    "feature-auth": {
      "path": "/abs/path/.worktrees/feature-auth",
      "branch": "feature-auth",
      "base_commit": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
      "state": "Active",
      "created_at": "2026-04-10T09:00:00Z",
      "last_activity": "2026-04-13T14:00:00Z",
      "creator_pid": 12345,
      "creator_name": "claude-squad",
      "session_uuid": "f7a3b9c1-2d4e-4f56-a789-0123456789ab",
      "adapter": "shell-command",
      "setup_complete": true,
      "port": 3200
    }
  },
  "stale_worktrees": {
    "old-refactor": {
      "original_path": "/abs/path/.worktrees/old-refactor",
      "branch": "refactor/db-layer",
      "base_commit": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
      "creator_name": "alice",
      "session_uuid": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
      "port": 3100,
      "last_activity": "2026-03-15T16:30:00Z",
      "evicted_at": "2026-04-01T00:00:00Z",
      "eviction_reason": "auto-gc: inactive >7 days",
      "expires_at": "2026-05-01T00:00:00Z"
    }
  },
  "port_leases": {
    "feature-auth": {
      "port": 3200,
      "branch": "feature-auth",
      "session_uuid": "f7a3b9c1-2d4e-4f56-a789-0123456789ab",
      "pid": 12345,
      "created_at": "2026-04-10T09:00:00Z",
      "expires_at": "2026-04-10T17:00:00Z",
      "status": "active"
    }
  },
  "config_snapshot": {
    "max_worktrees": 20,
    "disk_threshold_percent": 90,
    "gc_max_age_days": 7,
    "port_range_start": 3100,
    "port_range_end": 5100,
    "stale_metadata_ttl_days": 30
  },
  "gc_history": [
    {
      "timestamp": "2026-04-11T00:00:00Z",
      "removed": 2,
      "evicted": 1,
      "freed_mb": 1500
    }
  ]
}
```

Unknown fields are preserved via `serde`'s `#[serde(flatten)]` with `HashMap<String, serde_json::Value>` as a catch-all field on all state structs (forward compatibility: a file written by v1.1 can be read by v1.0 without data loss).

### 10.3 Reconciliation Policy

Run on every `Manager::list()` call and at `Manager::new()` startup:

1. Call `git worktree list --porcelain [-z]`.
2. For each entry in `state.json` `active_worktrees`:
   - If path is NOT in git's output:
     - Move to `stale_worktrees`.
     - Set `eviction_reason = "not in git registry"`.
     - Set `expires_at = now + stale_metadata_ttl_days`.
     - Set `port_leases[branch].status = "stale"`.
     - Log `WARNING: "Worktree {branch} missing from git registry, moved to stale"`.
     - **DO NOT silently delete.** Preserve port and identity data.
3. For each entry in git's output:
   - If path IS in `state.json` `active_worktrees`: merge git's state data (git's `locked`/`prunable` flags override `state.json` state field).
   - If path IS NOT in `state.json`: synthesize a minimal `WorktreeHandle` from git output; log `INFO: "Worktree {path} found in git but not in state.json, synthesizing entry"`.
4. Purge `stale_worktrees` entries where `expires_at < now`.
5. Sweep `port_leases`: for `"active"` leases, check `kill(pid, 0)` + `start_time`. If process is dead AND `expires_at < now` → remove lease; release port for reuse.

### 10.4 Port Lease Model

Ports are leased to a `(branch, session_uuid)` tuple — not to an index. This ensures port assignments survive intermediate deletions without causing collisions.

**Assignment algorithm:**
1. Compute preferred port:
   - `hash_input = format!("{repo_id}:{branch}")`
   - `hash_value = sha256(hash_input)[0..4] as u32`
   - `preferred = port_range_start + (hash_value % (port_range_end - port_range_start))`
2. If preferred port is not in `port_leases` (or its lease has expired): assign it.
3. If preferred port is taken: probe sequentially (`preferred+1`, `preferred+2`, ...). Wrap around at `port_range_end` back to `port_range_start`. If no free port found after full scan: return an error.

**Lease lifecycle:**
- TTL: 8 hours. Renewed every TTL/3 (~2.5 hours) during active use.
- When a worktree moves to `stale_worktrees`, its lease moves to `status: "stale"` and the port remains reserved until the stale entry's `expires_at`.
- Recovery: `wt attach` on a path matching a stale entry reclaims the original port.

### 10.5 Schema Migration

```rust
fn migrate(raw: serde_json::Value) -> Result<StateV2, WorktreeError> {
    let version = raw["schema_version"].as_u64().unwrap_or(1);
    match version {
        1 => migrate_v1_to_v2(raw),
        2 => serde_json::from_value(raw).map_err(|e| WorktreeError::StateCorrupted {
            reason: e.to_string()
        }),
        v => Err(WorktreeError::StateCorrupted {
            reason: format!("unknown schema version {v}"),
        }),
    }
}

fn migrate_v1_to_v2(raw: serde_json::Value) -> Result<StateV2, WorktreeError> {
    // v1: { "version": 1, "worktrees": { ... } }
    // v2: { "schema_version": 2, "active_worktrees": { ... },
    //        "stale_worktrees": {}, "port_leases": {} }
    let old_worktrees = raw["worktrees"].clone();
    // ... transform and return StateV2
}
```

---

## 11. Cross-Platform Implementation

### 11.1 macOS

- **File locking:** `flock()` — reliable on local APFS and HFS+. Use `fd-lock` crate.
- **Copy-on-Write:** `clonefile(2)` (macOS 10.12+). A directory clone is near-instant (metadata-only). The `reflink-copy` crate handles this automatically.
  - `CLONE_NOFOLLOW` flag prevents following symlinks.
  - Cross-volume returns `EXDEV` — block sharing impossible across volume boundaries.
  - Warn users for >10 GB worktrees: `clonefile()` on very large hierarchies blocks concurrent modifications to the source tree.
- **Path limits:** `PATH_MAX = 1024` bytes (the most restrictive platform). Use short directory names. Warn (do not error) when absolute path exceeds ~900 bytes — this is a real failure mode for deep `node_modules` trees.
- **Symlinks:** No privileges required. `std::os::unix::fs::symlink()` is sufficient.
- **Disk usage:** Use `jwalk` with `preload_metadata(true)` and Rayon parallelism. On APFS, CoW-reflinked files show full `st_blocks` — no portable way to detect shared extents. Don't use `diskutil apfs list` in the hot path.

### 11.2 Linux

- **File locking:** `flock()` on local filesystems. On NFS: see Section 9.6.
- **Copy-on-Write:** `FICLONE` ioctl (Linux 4.5+). Supported on Btrfs (with `reflink=1`, default since xfsprogs 4.19.0), XFS (with `reflink=1`), and OpenZFS 2.2.2+ (with `zfs_bclone_enabled=1`). Returns `EOPNOTSUPP` on ext4 or tmpfs — `reflink-copy` falls back gracefully.
- **Path limits:** `PATH_MAX = 4096` bytes (4095 usable). Not a practical constraint in most cases.
- **Symlinks:** No privileges required on local filesystems. Use relative symlinks on NFS to avoid mount-point mismatch.
- **Disk usage:** `jwalk` with `parallelism(RayonNewPool(num_cpus::get()))` + `preload_metadata(true)`. Deduplicate hardlinks via `HashSet<(dev_t, ino_t)>`.

### 11.3 Windows

Windows support ships after macOS and Linux. The implementation is designed for correctness from the start; platform stubs compile in v1.0.

- **File locking:** `LockFileEx` — mandatory semantics (blocks all other readers/writers). Never lock `state.json` directly with `LockFileEx`. Use `fd-lock` crate which locks `state.lock` (a sentinel file) and leaves `state.json` unlocked.
- **Symlinks:** Require `SeCreateSymbolicLinkPrivilege` (admin or Developer Mode since Windows 10 Build 1703). Enterprise environments commonly disable Developer Mode via Group Policy. Do not require symlinks; use junctions instead.
- **Junctions:**
  - No privileges required. Use the `junction` crate v1.4.2.
  - **Junctions CAN span drive volumes.** (Cross-volume restriction applies to hard links, not junctions.) Do NOT add a cross-volume restriction for junctions.
  - Junctions CANNOT target network shares (`\\server\share`). Enforce with `check_not_network_junction_target`.
  - `Remove-Item -Recurse -Force` in PowerShell follows junctions and deletes target contents — document this danger prominently.
- **Path lengths:** Rust 1.58+ automatically prepends `\\?\` to paths via `std::fs` operations, bypassing MAX_PATH. Use `dunce` crate when passing paths to external tools (including git) to strip the `\\?\` prefix.
- **Copy-on-Write:** `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (Windows Server 2016+, ReFS only). Not available on consumer NTFS. `ReflinkMode::Preferred` will fall back to standard copy.
- **Disk usage:** `FindFirstFileEx` with `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH` for enumeration; `GetCompressedFileSizeW()` per file for actual disk usage. The `filesize` crate handles this correctly.
- **git on Windows:**
  - `core.symlinks = false` is the default.
  - `core.longpaths = true` required in gitconfig for deep paths.
  - Always strip `\\?\` prefixes via `dunce` before passing paths to git.

### 11.4 Universal: Disk Usage Calculation

```rust
fn calculate_worktree_disk_usage(path: &Path) -> Result<u64, WorktreeError> {
    use jwalk::WalkDir;
    use filesize::PathExt;
    use std::collections::HashSet;

    let mut total_bytes: u64 = 0;
    let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();

    for entry in WalkDir::new(path)
        .skip_hidden(false)
        .process_read_dir(|_, _, _, children| {
            // Skip .git directory (shared across worktrees via gitdir link)
            children.retain(|e| {
                e.as_ref().map(|e| e.file_name() != ".git").unwrap_or(true)
            });
        })
    {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let key = (metadata.dev(), metadata.ino());
                if !seen_inodes.insert(key) { continue; }  // deduplicate hardlinks
            }
            total_bytes += entry.path().size_on_disk_fast(&metadata)
                .unwrap_or(metadata.len());
        }
    }
    Ok(total_bytes)
}
```

**Target performance:** <200ms for 50K files on a local filesystem.

**Caching:** Cache results with a timestamp. Invalidate by comparing the worktree root directory's `mtime` against the cached time. Do NOT use filesystem watchers (`inotify`/`FSEvents`/`ReadDirectoryChangesW`) — they add complexity with no benefit when a full walk completes in <200ms.

---

## 12. CLI and MCP Surface

### 12.1 CLI Commands

```bash
# Create a new worktree
wt create <branch> [path] \
    [--base <ref>]                          # default: HEAD
    [--setup]                               # run ecosystem adapter
    [--lock [--reason <reason>]]            # lock immediately after creation
    [--reflink=<required|preferred|disabled>]
    [--port]                                # allocate a port lease

# Delete a worktree
wt delete <branch|path> [--force] [--force-dirty]

# Register an existing worktree under management
wt attach <path> [--setup]

# List all worktrees
wt list [--json] [--porcelain]

# Show worktree status and aggregate disk usage
wt status

# Run garbage collection
wt gc [--dry-run] [--confirm] [--max-age <days>] [--force]

# Claude Code WorktreeCreate hook integration
wt hook [--stdin-format claude-code] [--setup]

# Conflict detection — reserved, not implemented in v1.0
wt check
```

### 12.2 `wt hook` — Claude Code Integration Contract

This subcommand is the integration point for Claude Code's `WorktreeCreate` hook. It has a strict stdin/stdout contract that is incompatible with `wt create`'s interactive output.

**Critical constraint:** Claude Code sends JSON on stdin and expects **only the absolute path** on stdout. Any extra stdout causes Claude Code to hang silently. (Confirmed bug `claude-code#27467` — cannot be worked around; the library must comply.)

**Protocol:**

```json
// Input on stdin:
{
  "session_id": "abc123",
  "cwd": "/path/to/repo",
  "hook_event_name": "WorktreeCreate",
  "name": "feature-auth"
}
```

```
// Output — stdout (ONLY this, nothing else):
/absolute/path/to/created/worktree

// Output — stderr (all other output):
[worktree-core] Creating worktree for branch 'feature-auth'...
[worktree-core] Running adapter setup...
[worktree-core] Done. Worktree created at /abs/path/to/worktree
```

**Implementation requirements:**
1. Read JSON from stdin.
2. Extract `name` field (used as branch name as-is — pass-through).
3. Create worktree with `Manager::create()`.
4. If `--setup` flag: run adapter.
5. Print only the absolute worktree path to stdout.
6. All subprocess output, git progress, and log messages go to stderr.
7. Exit 0 on success, non-zero on failure (Claude Code logs stderr on failure).

**Claude Code integration config (`~/.claude.json` or `.mcp.json`):**
```json
{
  "hooks": {
    "WorktreeCreate": "wt hook --stdin-format claude-code --setup"
  }
}
```

### 12.3 MCP Server

The `worktree-core-mcp` binary runs as a stdio MCP server. Transport: stdio only in v1.0 (HTTP transport deferred to v1.1).

**Tool schema:**

| Tool | readOnlyHint | destructiveHint | idempotentHint | v1.0 |
|---|---|---|---|---|
| `worktree_list` | true | false | true | ✅ |
| `worktree_status` | true | false | true | ✅ |
| `conflict_check` | true | false | true | Returns `not_implemented` |
| `worktree_create` | false | false | false | ✅ |
| `worktree_delete` | false | true | false | ✅ |
| `worktree_gc` | false | true | false | ✅ |

Tool annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`) are required per MCP spec 2025-03-26+. Read-only tools will not prompt for approval in Claude Code and Cursor.

**MCP client config locations:**

| Client | Config File | Root Key |
|---|---|---|
| Claude Code | `~/.claude.json` or `.mcp.json` | `mcpServers` |
| Cursor | `.cursor/mcp.json` | `mcpServers` |
| VS Code Copilot | `.vscode/mcp.json` | `servers` ⚠️ |
| OpenCode | `opencode.jsonc` | `mcp` |

> ⚠️ VS Code uses `"servers"`, not `"mcpServers"`. The README must include config snippets for all formats.

**Claude Code installation one-liner:**
```bash
claude mcp add worktree-core -- worktree-core-mcp
```

**Config snippets (include all three in README):**

```json
// Claude Code / Cursor
{
  "mcpServers": {
    "worktree-core": { "command": "worktree-core-mcp", "args": [] }
  }
}

// VS Code
{
  "servers": {
    "worktree-core": { "command": "worktree-core-mcp", "args": [] }
  }
}

// OpenCode
{
  "mcp": {
    "worktree-core": { "command": "worktree-core-mcp", "args": [] }
  }
}
```

---

## 13. Architectural RFC Process

### 13.1 When to Open an RFC

Open an RFC **before** proceeding if:
- A borrow-checker constraint cannot be resolved with a targeted local fix (≤50 added lines).
- A design decision affects two or more public API surfaces simultaneously.
- An `unsafe` block is required.
- A public type or function signature needs to change.

Do NOT open an RFC for: implementation details inside a single private function, dependency version bumps, or bug fixes.

### 13.2 Escalation Hierarchy

Exhaust these options before opening an RFC:
1. **Restructure ownership** — split structs, use indices instead of references, clone to break shared ownership.
2. **Interior mutability** — `RefCell<T>` (single-threaded), `Cell<T>` (Copy types), `OnceCell<T>` (lazy init).
3. **Smart pointers** — `Rc<RefCell<T>>` (single-threaded), `Arc<Mutex<T>>` or `Arc<RwLock<T>>` (multi-threaded).
4. **`unsafe`** — last resort; requires a `// SAFETY:` comment documenting the invariant.

### 13.3 RFC Template

File as `docs/decisions/rfc-NNN.md`:

```markdown
# RFC-NNN: <Short Title>

**Date:** YYYY-MM-DD
**Status:** Draft | Accepted | Implemented | Superseded by RFC-XXX
**Affects:** <list public types or functions>

## Problem
[1–3 sentences: what constraint or conflict exists?]

## Current behavior / compile error
[Code showing the issue]

## Proposed solution
[Code showing the new design]

## Escape hatch used
- [ ] RefCell<T>
- [ ] Arc<Mutex<T>>
- [ ] unsafe // SAFETY: <invariant>
- [ ] None — pure safe Rust redesign

## Alternatives considered
[Why each was rejected]

## Trade-offs
Improves: ...
Costs: ...
Breaking change: yes/no; migration path: ...
```

Reference accepted RFCs in code: `// DESIGN DECISION (RFC-003): Using RefCell here because...`

---

## 14. Crate Dependencies

| Crate | Version | Justification |
|---|---|---|
| `fd-lock` | 4 | Cross-platform advisory file locking. `flock()` on Unix, `LockFileEx` on Windows. RAII guards auto-release on drop. 33M downloads. |
| `sysinfo` | 0.37 | Cross-platform process `start_time` for PID reuse detection. Linux: `/proc/<pid>/stat` field 22. macOS: `proc_pidinfo`. Windows: `GetProcessTimes`. |
| `uuid` | 1 (v4 feature) | UUID v4 generation for multi-factor lock identity and session tracking. |
| `reflink-copy` | 0.1 | CoW file operations. `clonefile()` on macOS, `FICLONE` on Linux, `FSCTL_DUPLICATE_EXTENTS_TO_FILE` on Windows. Falls back to `std::fs::copy()` when unsupported. |
| `junction` | 1 | Windows NTFS junction creation without admin privileges. |
| `jwalk` | 0.8 | Rayon-based parallel directory walking. <200ms for 50K files. Use `parallelism(RayonNewPool(num_cpus))` + `preload_metadata(true)`. |
| `filesize` | 0.2 | Cross-platform disk usage. `st_blocks * 512` on Unix, `GetCompressedFileSizeW()` on Windows. |
| `directories` | 6 | XDG-compliant platform-appropriate paths via `ProjectDirs::from("", "", "worktree-core")`. |
| `dunce` | 1 | Strips `\\?\` prefix when passing paths to external tools on Windows. |
| `thiserror` | 2 | Structured error types with `#[error]` derive. |
| `serde` + `serde_json` | 1 | `state.json` serialization with `#[serde(flatten)]` for forward compatibility. |
| `chrono` | 0.4 | DateTime handling for lease expiry, timestamps, TTL calculations. |
| `rand` | 0.8 | Full Jitter sleep calculation: `rand::random::<u64>() % window`. |
| `sha2` | 0.10 | SHA-256 of repo path for `repo_id` field and deterministic port hash assignment. |

**Excluded from core library:** `gix` and `git2` are reserved for v1.1 conflict detection (`gix::Repository::merge_trees()`) and must be feature-flagged.

---

## 15. Implementation Milestones

### Milestone 1 — Foundation (Weeks 1–6)

**Deliverable:** `worktree-core` crate on crates.io, `wt` CLI binary, `worktree-core-mcp` binary with 6 tools.

**Scope:**
- All types from Section 4 implemented and compiling.
- `Manager::new()` with git version detection and capability map.
- `Manager::create()` with all pre-create guards and post-create git-crypt check.
- `Manager::delete()` with five-step unmerged commit decision tree.
- `Manager::list()` with porcelain parser (newline and NUL-delimited).
- `Manager::gc()` with dry-run default; locked worktree protection.
- `Manager::attach()`.
- `state.json` v2 schema, locking protocol, migration from v1.
- Full Jitter backoff.
- Multi-factor lock identity (PID + `start_time` + UUID + hostname).
- Port lease model.
- `stale_worktrees` eviction on reconciliation (not silent drop).
- `ReflinkMode` tristate in `CreateOptions`.
- `wt hook --stdin-format claude-code` subcommand.
- MCP server with 6 tools; `conflict_check` returns `not_implemented`.
- macOS and Linux platforms. Windows stubs (compile only).

**Ship criteria (all must pass):**
- `cargo clippy -- -D warnings` clean.
- `cargo test` passes on macOS and Linux.
- Zero data loss in stress test: 100 create/delete cycles with simulated crash injection (SIGKILL at random points).
- `wt gc` successfully cleans orphaned worktrees from a simulated OpenCode failure (1000 orphans, varying ages).
- `wt hook --stdin-format claude-code` produces exactly one line on stdout (the absolute path), nothing else.
- MCP server responds correctly to `worktree_list`, `worktree_create`, `worktree_delete`, `worktree_gc`.
- Crates published: `worktree-core`, `worktree-core-cli`, `worktree-core-mcp`.

### Milestone 2 — Environment Lifecycle (Weeks 7–10)

**Deliverable:** `DefaultAdapter`, `ShellCommandAdapter`, port allocation CLI, `wt attach` stability, cross-platform cleanup.

**Scope:**
- `DefaultAdapter` — file copy from configurable list.
- `ShellCommandAdapter` — arbitrary shell commands with `WORKTREE_CORE_*` env vars.
- Port allocation exposed in `wt create --port` and `wt status`.
- macOS `.DS_Store` handling in `wt delete` and `wt gc` (`.DS_Store` blocks `git worktree remove` — remove it first).
- Windows MAX_PATH workarounds via `dunce`.
- Retry logic for locked files on Windows.
- `wt attach` recovery of port lease and `session_uuid` from `stale_worktrees`.
- MCP server documented with config snippets for all clients.

**Ship criteria:**
- `wt create --setup` bootstraps a Node.js project using `ShellCommandAdapter` with `npm install`.
- `wt create --setup` copies `.env` using `DefaultAdapter`.
- Port allocation assigns unique ports to 20 simultaneous worktrees with no collision.
- `wt attach` on a path matching a `stale_worktrees` entry correctly recovers the original port.
- macOS `.DS_Store` test: `wt delete` succeeds even when `.DS_Store` is present in worktree root.

### Milestone 3 — Ecosystem Integration (Weeks 11–16)

**Deliverable:** External integrations, conflict detection MVP, HTTP MCP transport.

**Scope:**
- Claude Squad worktree setup hook integration (after PR #268/#270 merge — coordinate with maintainers).
- `workmux` crate integration PR — `worktree-core` as optional dependency behind feature flag.
- Conflict detection MVP: `wt check` subcommand using `git merge-tree --write-tree -z` (Git ≥ 2.38 required; graceful degradation message on older git).
- MCP `conflict_check` tool implementation (replaces `not_implemented` stub).
- HTTP transport for MCP server (for Cursor remote, VS Code Dev Containers, SSH setups).
- Windows platform: full implementation (replace stubs with actual platform code).

**Ship criteria:**
- At least one external project consuming `worktree-core` as a library dependency.
- `wt check` correctly identifies conflicts for a test corpus of 20 merge scenarios.
- MCP HTTP transport responds correctly in a VS Code Dev Container environment.
- Windows CI passing (`cargo test` on Windows Server 2019 runner).

### Milestone 4 — Hardening (Weeks 17–20)

**Deliverable:** Ecosystem-specific adapters, language bindings, worktree pooling.

**Scope:**
- **pnpm adapter:** leverage `enableGlobalVirtualStore: true`. pnpm now has official multi-agent worktree documentation.
- **uv adapter:** per-worktree venvs. `uv venv && uv pip install -r requirements.txt` takes seconds vs. minutes.
- **Cargo adapter:** use per-worktree `target` directories. Do NOT share `CARGO_TARGET_DIR` across worktrees — cargo has a bug with path deps of the same name from different worktrees.
- `gix::Repository::merge_trees()` integration replacing CLI fallback for conflict detection. Feature-complete as of November 2024; GitButler PR #5722 replaced all `git2::merge_trees()` calls.
- Node.js bindings via `napi-rs` (generates TypeScript types automatically).
- Worktree pooling: pre-create N worktrees for instant checkout. Pool size configurable; automatic replenishment.

**Ship criteria:**
- pnpm adapter: 5 worktrees share a single virtual store. `du -sh node_modules` in each shows <1 MB (symlinks only).
- uv adapter: worktree with `requirements.txt` fully installed in <10 seconds.
- Node.js package published to npm as `@worktree-core/node`.
- Worktree pool of 5 worktrees available in <1 second (vs. ~5 seconds for on-demand creation).

---

## 16. Known Failure Modes and Recovery

| Failure | Detection | Recovery |
|---|---|---|
| git command timeout | Process exceeds 30s timeout | Kill process; retry once with exponential delay; return `GitCommandFailed` with diagnostic |
| `state.json` corrupt | JSON parse fails at startup | Rebuild from `git worktree list` output; log `WARNING: State rebuilt from git` |
| `state.lock` stale | Four-factor stale check (Section 9.3) | Delete stale lock; log warning with recovered PID and hostname; acquire fresh lock |
| Lock contention timeout | `fd-lock` returns after 15 Full Jitter attempts (~30s) | Return `StateLockContention`; caller must retry or surface to user |
| Worktree path vanished from disk | `stat()` fails | Move to `stale_worktrees`; offer `wt gc` cleanup |
| Branch deleted under active worktree | `git rev-parse refs/heads/<branch>` fails | Mark `WorktreeState::Broken`; warn user; block further operations on this handle |
| git not installed | `git --version` fails at `Manager::new()` | Return `GitNotFound` |
| Unmerged commits | Five-step decision tree (Section 8.2.1) | Return `UnmergedCommits`; user must pass `--force` or merge first |
| git-crypt files encrypted post-create | Magic header byte check (Section 8.3) | Auto-remove worktree via `git worktree remove --force`; return `GitCryptLocked` |
| PID reuse in stale lock | `start_time` mismatch via `sysinfo` | Delete stale lock; log warning; acquire fresh lock |
| Stale port lease | `kill(pid, 0)` returns ESRCH AND `expires_at < now` | Release port; re-assign on next `create()` |
| `ReflinkMode::Required` on ext4 | `EOPNOTSUPP` from `FICLONE` ioctl | Return `ReflinkNotSupported` immediately; no fallback |
| `git worktree add` partial failure | Non-zero exit from git | Run `rm -rf <path>`; return `GitCommandFailed` |
| Nested worktree creation | `dunce::canonicalize` + `starts_with` | Return `NestedWorktree { parent }` before git is called |
| Network filesystem | `statfs()` type detection | Log warning; degrade lock to atomic-rename-only mode |
| Windows junction to network path | UNC path prefix check | Return `NetworkJunctionTarget` before junction creation |
| Circuit breaker open | 3 consecutive git failures | Return `CircuitBreakerOpen`; all operations blocked until reset |
| Shallow clone merge check | `git rev-parse --is-shallow-repository` | Skip Steps 2–4 of unmerged check; go directly to Step 5; log warning |

---

## 17. Git Version Capability Matrix

**Minimum supported version: 2.20.** At startup, `Manager::new()` parses `git --version` and builds a `GitCapabilities` struct. All feature gates use this struct.

| Feature | Min Version | Behavior on Older Git |
|---|---|---|
| `worktree add/list/prune` | 2.15 | N/A — 2.20 is hard minimum |
| `worktree list --porcelain` | 2.7 | N/A — 2.20 is hard minimum |
| `worktree lock/unlock` | 2.14 | N/A — 2.20 is hard minimum |
| `worktree move/remove` | 2.18 | N/A — 2.20 is hard minimum |
| `worktree list --porcelain -z` | 2.36 | Fall back to newline-delimited. Paths with newlines fail silently — log warning. |
| `worktree repair` | 2.30 | Skip repair step; log warning. `wt attach` may produce broken gitdir links. |
| `worktree add --orphan` | 2.42 | Orphan branch worktrees not supported; return descriptive error. |
| `worktree.useRelativePaths` | 2.48 | Skip; use absolute paths (default). |
| `git merge-tree --write-tree` | 2.38 | Conflict detection unavailable; `wt check` returns error with upgrade instructions. |
| `locked`/`prunable` in list output | 2.31 | Parse without these fields; assume not locked, not prunable. |
| `--lock` flag on `worktree add` | 2.17 | Fall back to separate `worktree add` + `worktree lock` (race window exists; document). |

**Unknown subcommand detection:**
```bash
git worktree repair 2>&1 | grep "unknown subcommand"
git worktree list -z 2>&1 | grep "unknown option"
```

---

## 18. Integration Targets

| Target | Stars | Language | Integration Path | Bugs Fixed by worktree-core |
|---|---|---|---|---|
| Claude Code | ~112K | TypeScript | MCP (Week 1) + `wt hook` (Week 2) | `#38287`, `#41010`, `#38538`, `#27881`, `#43730`, `#33045` |
| OpenCode | ~140K | TypeScript/Bun | MCP only | `#14648`, `#9290` |
| Gas Town | ~13K | Go | MCP — deprioritize (complex Dolt-backed tracking) | Ephemeral session management |
| Claude Squad | ~6.8K | Go | CLI direct + PR hooks (#268/#270) | `#260` (no env setup), stale worktree errors |
| Cursor | — | TypeScript | MCP (Week 1) | Force-deleted branches, git stash contamination |
| VS Code Copilot | — | TypeScript | MCP (Week 1) | `#296194` (runaway loop), `#289973` (premature cleanup) |
| workmux | ~871 | Rust | Rust crate (preferred) | Port collisions, no env bootstrapping |
| worktrunk | ~3.7K | Rust | Rust crate (discuss with maintainer) | CLI-first; library API not stable per maintainer |

### 18.1 Branch Naming by Target

The library **never** transforms branch names. Each target uses a different convention; accept any string as-is and let git validate it.

| Target | Branch Pattern |
|---|---|
| Claude Code | `worktree-<name>` |
| Claude Squad | `<prefix>_<name>_<timestamp>` |
| OpenCode | `opencode-<random-adjective-noun>` (e.g., `brave-cabin`) |
| Gas Town | `polecat/<name>-<timestamp>` |
| workmux | User-specified, no prefix |
| worktrunk | User-specified, no prefix |

---

## Appendix A: Non-Negotiable Invariants

The following rules are authoritative. Any implementation that violates them is incorrect.

1. **Shell out to git CLI.** Never use `git2` or `gix` for worktree CRUD.
2. **`git worktree list --porcelain` is the source of truth.** `state.json` is a supplementary cache.
3. **Never write to `.git/worktrees/` directly.**
4. **Never call `git gc` or `git prune`.**
5. **All deletion paths run the five-step unmerged commit check unless `force = true`.**
6. **On failure after `git worktree add` succeeds, run `git worktree remove --force` before returning error.**
7. **The `state.lock` scope is ONLY around `state.json` read-modify-write.** Never hold it across `git worktree add`.
8. **Entries evicted from `active_worktrees` go to `stale_worktrees`** — never silently deleted.
9. **Windows junctions CAN span volumes.** Do NOT add a cross-volume restriction for junctions.
10. **Worktree paths with newlines are unparseable without `-z` (Git 2.36+).** Log a warning; do not crash.
11. **Branch names are never transformed by the core library.** Accept any string as-is.
12. **All public structs are `#[non_exhaustive]`.** Do not remove this attribute.
13. **`gc()` never touches locked worktrees regardless of the `force` flag.**
14. **Never use `git branch --merged` as the sole safe-to-delete check.** It misses squash-merged branches.

---

## Appendix B: Data Loss Incidents Referenced

| Incident | Source | Consequence |
|---|---|---|
| Cleanup deleted branches with unmerged commits — no warning | `claude-code#38287` (`data-loss`) | Developers lost days of work |
| Sub-agent cleanup deleted parent session's CWD | `claude-code#41010` (`data-loss`) | Entire session rendered unusable |
| Three agents reported success; all work lost | `claude-code#29110` | Significant token burn, zero output |
| git-crypt worktree committed all files as deletions | `claude-code#38538` | Repository corruption |
| Nested worktree created inside worktree after context compaction | `claude-code#27881` | Unpredictable git state |
| Background worker cleaned worktree with uncommitted changes | `vscode#289973` | Multiple days of work lost |
| Runaway `git worktree add` loop: 1,526 worktrees | `vscode#296194` | Disk exhaustion |
| 9.82 GB consumed in 20-minute session (2 GB repo) | Cursor forum | Developer unaware until disk full |
| 5 worktrees × 2 GB `node_modules` = 10 GB wasted | `claude-squad#260` | Disk pressure, slow operations |
| Each retry creates orphan: hundreds of MB per attempt | `opencode#14648` | Unbounded disk growth |

---

## 19. Open Questions

The following ambiguities were identified during the v1.4 → v1.5 rewrite. They should be resolved before the corresponding milestone begins.

1. **Port lease renewal mechanism (Section 10.4).** The spec states leases are renewed "every TTL/3 (~2.5 hours) during active use" but does not define what constitutes "active use" or who triggers renewal. Is renewal the responsibility of the `Manager` (background timer) or the caller? If background, what thread/task model is expected?

2. **`check_not_network_filesystem` as warning vs. error (Section 8.1, guard #6).** The spec says "warning, not hard block by default" but does not expose a `Config` field to escalate it to an error. Should a `Config::deny_network_filesystem: bool` field be added?

3. **`wt attach` for bare repos (Sections 5.3, 11.x).** `BareRepositoryUnsupported` was removed in v1.5 (bare repos permitted with explicit paths), but `Manager::attach()` does not specify behavior when attaching a worktree into a bare repo. Clarify: is `attach()` permitted on bare repos?

4. **Circuit breaker reset mechanism (Section 4.4, `Config::circuit_breaker_threshold`).** The `CircuitBreakerOpen` error is documented, but no reset mechanism is specified. Is the circuit breaker reset automatically after a timeout, manually via a `Manager` method, or on `Manager` reconstruction?

5. **`git worktree add` base for bare repos (Section 2, "bare repos supported").** When the primary repo is bare, `git worktree add` is called from the bare repo root. Confirm that `git worktree add` from a bare repo root works as expected in the minimum supported Git version (2.20) and document the exact command form.

6. **`wt gc` concurrency with active agents (Section 5.3).** If an agent is actively using a worktree but its process is not the one holding `state.lock`, `gc()` could evict it (if it appears orphaned). Is there a mechanism to mark a worktree as "in use" beyond the `Locked` state (which requires an explicit `git worktree lock` call)?

---

*End of ISO_PRD-v1.5*
