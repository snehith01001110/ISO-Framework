# Product Requirements Document: worktree-core

| Field | Value |
|---|---|
| **Product** | worktree-core |
| **PRD Version** | 1.0 (derived from ISO_PRD v1.5) |
| **Date** | April 2026 |
| **Status** | Implementation-Ready |

---

## 1. Product Overview

### Vision

worktree-core is the canonical shared library for safe, concurrent git worktree lifecycle management. Every major AI coding orchestrator in 2026 -- Claude Code, Claude Squad, Cursor, OpenCode, VS Code Copilot -- uses git worktrees for parallel agent isolation, yet none share any management code. Each has independently reimplemented creation, deletion, and cleanup, producing documented data-loss bugs, unbounded resource consumption, and broken environment isolation. worktree-core eliminates these classes of failure with a single battle-tested Rust library that shells out to the git CLI, treats `git worktree list --porcelain` as the authoritative source of truth, and enforces safety checks on every lifecycle transition.

### Goals

The primary goal is zero data loss: every deletion path checks for unmerged commits via a five-step decision tree, every creation path validates disk space, rate limits, nested paths, and git-crypt state, and orphaned worktrees are never silently dropped. The secondary goal is resource containment: circuit breakers cap runaway git failures, configurable worktree count limits prevent unbounded accumulation, port leases eliminate binding collisions, and garbage collection reclaims orphans with dry-run-by-default semantics. The tertiary goal is broad adoption: the library ships as a Rust crate, a thin CLI (`wt`), and a stdio MCP server consumable by Claude Code, Cursor, VS Code Copilot, and OpenCode with zero configuration beyond a one-liner install.

### Target Audience

The primary audience is AI coding orchestrator developers building tools that spawn parallel agent sessions in git worktrees -- teams behind Claude Code, Claude Squad, OpenCode, Cursor, workmux, and similar projects. The secondary audience is individual developers and DevOps engineers who manage multiple worktrees manually and want safe lifecycle tooling via the `wt` CLI. The tertiary audience is MCP client developers who need worktree operations exposed as tool calls with proper annotations for approval semantics.

---

## 2. Functional Requirements

### P0 -- Epic 1: Foundation

**FR-P0-001: Manager Constructor with Git Version Detection**
Manager::new() constructs a Manager for a given repository root. It runs `git --version`, parses the version, returns GitVersionTooOld if < 2.20, canonicalizes the repo root via dunce, confirms it is a git repository, detects and stores all git capabilities in a GitCapabilities map, creates the `.git/worktree-core/` directory if absent, acquires state.lock, reads or initializes state.json, runs a startup orphan scan, and sweeps expired port leases.
- Acceptance criteria:
  - Returns GitNotFound when git is not in PATH
  - Returns GitVersionTooOld when git < 2.20
  - Populates GitCapabilities with correct boolean flags for the installed git version
  - Cold start completes in < 500 ms
- Priority: P0
- Epic: Foundation

**FR-P0-002: Manager::create() with Pre-Create Guards and Post-Create Verification**
Manager::create() creates a new managed worktree by running all pre-create guards (Section 8.1) in exact order, writing a Pending entry to state.json, transitioning to Creating, running `git worktree add`, executing the post-create git-crypt check (Section 8.3), optionally running EcosystemAdapter::setup(), transitioning to Active, and writing final state. "On any failure after step 4 (git worktree add) succeeds: Run `git worktree remove --force <path>` before returning error. Never leave a partial worktree on disk."
- Acceptance criteria:
  - All 12 pre-create guards execute in specified order and abort on first failure
  - git-crypt detection removes the worktree and returns GitCryptLocked when encrypted files are found post-create
  - Branch names are never transformed -- passed through as-is
  - Returns (WorktreeHandle, CopyOutcome) on success
  - Partial worktrees are never left on disk after any failure
- Priority: P0
- Epic: Foundation

**FR-P0-003: Manager::delete() with Five-Step Unmerged Commit Decision Tree**
Manager::delete() removes a managed worktree by executing pre-delete guards in exact order: check_not_cwd, check_no_uncommitted_changes (skipped if force_dirty), five_step_unmerged_check (skipped if force), check_not_locked, then transitioning through Deleting to Deleted, releasing any port lease, and writing final state. The five-step check replaces the naive `git log --not --remotes` with fetch, local merge-base, remote merge-base, cherry patch-ID matching, and final unpushed commit count.
- Acceptance criteria:
  - Returns CannotDeleteCwd when attempting to delete CWD
  - Returns UncommittedChanges with file list when dirty (unless force_dirty)
  - Returns UnmergedCommits with branch and count when unmerged commits exist (unless force)
  - Returns WorktreeLocked for locked worktrees
  - Handles shallow repos by skipping Steps 2-4 with a warning
  - Handles missing remotes gracefully, falling through to Step 5
- Priority: P0
- Epic: Foundation

**FR-P0-004: Manager::list() with Porcelain Parser and NUL Fallback**
Manager::list() calls `git worktree list --porcelain [-z]` and parses the output. Uses `-z` (NUL-delimited) when Git >= 2.36; falls back to newline-delimited parsing on older versions. Reconciles output with state.json: entries in state.json missing from git output are moved to stale_worktrees, not silently dropped.
- Acceptance criteria:
  - Correctly parses worktree path, HEAD, branch/detached/bare, locked, and prunable fields
  - Uses NUL delimiter on Git >= 2.36, newline on older versions
  - Logs warning for paths that may contain newlines when -z is unavailable
  - Reconciliation moves missing entries to stale_worktrees with eviction_reason
- Priority: P0
- Epic: Foundation

**FR-P0-005: Manager::gc() with Dry-Run Default and Locked Worktree Protection**
Manager::gc() runs garbage collection on orphaned and stale worktrees. Default behavior is dry_run = true. "gc() never touches locked worktrees regardless of the force flag." Runs the five-step unmerged commit check before any deletion unless force = true.
- Acceptance criteria:
  - Defaults to dry_run = true; reports what would happen without acting
  - Never touches locked worktrees even with force = true
  - Returns GcReport with orphans, removed, evicted, freed_bytes, and dry_run flag
  - Cleans 1000 orphans with varying ages in a stress test
- Priority: P0
- Epic: Foundation

**FR-P0-006: Manager::attach()**
Manager::attach() registers an existing worktree (already in git's registry) under worktree-core management without calling `git worktree add`. If a stale_worktrees entry exists for this path, it recovers the port lease and session_uuid.
- Acceptance criteria:
  - Precondition: worktree must already exist in git's registry
  - Recovers port lease and session_uuid from stale_worktrees when a matching entry exists
  - Returns WorktreeHandle with state Active
- Priority: P0
- Epic: Foundation

**FR-P0-007: state.json v2 Schema, fd-lock Protocol, and Reconciliation**
Implements the state.json v2 schema with active_worktrees, stale_worktrees, port_leases, config_snapshot, and gc_history. Uses fd-lock for cross-platform advisory locking with the exact lock acquisition sequence: stale detection, non-blocking exclusive lock, write record, read state.json, mutate, write tmp, fsync, atomic rename, drop guard. "The state.lock scope is ONLY around state.json read-modify-write. Never hold it across git worktree add." Unknown fields are preserved via serde flatten for forward compatibility.
- Acceptance criteria:
  - Schema matches Section 10.2 exactly
  - Lock is never held during git worktree add
  - Atomic writes via tmp + fsync + rename
  - Forward-compatible: v1.1 state files readable by v1.0 without data loss
  - Migrates v1 schema to v2
- Priority: P0
- Epic: Foundation

**FR-P0-008: Full Jitter Backoff**
Implements bounded Full Jitter exponential backoff for lock contention: `sleep_ms = random(0, min(cap_ms, base_ms * 2^attempt))` with base_ms = 10, cap_ms = 2000, max_attempts = 15 (~30s worst case). Returns StateLockContention on timeout.
- Acceptance criteria:
  - Sleep duration follows the Full Jitter formula
  - Total timeout respects Config::lock_timeout_ms (default 30,000 ms)
  - Returns StateLockContention after exhausting retries
- Priority: P0
- Epic: Foundation

**FR-P0-009: Multi-Factor Lock Identity**
Lock identity uses four factors: PID, process start_time (via sysinfo), UUID v4, and hostname. Stale detection checks PID liveness via kill(pid, 0), then verifies start_time to detect PID reuse, following the PostgreSQL postmaster.pid model.
- Acceptance criteria:
  - Detects stale locks when PID no longer exists
  - Detects stale locks when PID was reused (start_time mismatch)
  - Logs warnings with recovered PID and hostname on stale detection
  - Lock file contains pid, start_time, uuid, hostname, and acquired_at
- Priority: P0
- Epic: Foundation

**FR-P0-010: Port Lease Model**
Ports are leased to a (branch, session_uuid) tuple using deterministic hash-based assignment: SHA-256 of `"{repo_id}:{branch}"`, mapped into the configured port range, with sequential probe on collision. Lease TTL is 8 hours, renewed every TTL/3 during active use.
- Acceptance criteria:
  - Deterministic port assignment for same repo+branch combination
  - Sequential probe with wraparound when preferred port is taken
  - Stale leases transition to "stale" status on worktree eviction
  - Expired leases are swept at Manager startup
- Priority: P0
- Epic: Foundation

**FR-P0-011: stale_worktrees Eviction**
"Entries evicted from active_worktrees go to stale_worktrees -- never silently deleted." Evicted entries preserve original_path, branch, base_commit, creator_name, session_uuid, port, last_activity, evicted_at, eviction_reason, and expires_at. Stale entries are purged only after stale_metadata_ttl_days (default 30).
- Acceptance criteria:
  - No active_worktrees entry is ever silently deleted during reconciliation
  - Evicted entries include all specified fields
  - Purge occurs only when expires_at < now
  - Port leases transition to "stale" on eviction, not released
- Priority: P0
- Epic: Foundation

**FR-P0-012: ReflinkMode Tristate in CreateOptions**
CreateOptions includes a ReflinkMode field with three variants: Required (fail if CoW unsupported), Preferred (try CoW, fall back to standard copy -- this is the default), and Disabled (never attempt CoW).
- Acceptance criteria:
  - Required returns ReflinkNotSupported on filesystems without CoW
  - Preferred falls back silently to standard copy
  - Disabled never attempts CoW
  - CopyOutcome reports what actually happened (Reflinked, StandardCopy, or None)
- Priority: P0
- Epic: Foundation

**FR-P0-013: wt hook --stdin-format claude-code**
The `wt hook` subcommand reads JSON from stdin (`session_id`, `cwd`, `hook_event_name`, `name`), creates a worktree using the `name` field as-is, and prints only the absolute worktree path to stdout. "Any extra stdout causes Claude Code to hang silently."
- Acceptance criteria:
  - Reads JSON from stdin and extracts the name field
  - Prints exactly one line to stdout: the absolute path
  - All other output (logs, progress, subprocess output) goes to stderr
  - Exit 0 on success, non-zero on failure
- Priority: P0
- Epic: Foundation

**FR-P0-014: MCP Server with 6 Tools**
The worktree-core-mcp binary runs as a stdio MCP server exposing 6 tools: worktree_list, worktree_status, conflict_check, worktree_create, worktree_delete, worktree_gc. Each tool carries readOnlyHint, destructiveHint, and idempotentHint annotations per MCP spec 2025-03-26+. conflict_check returns `not_implemented` in v1.0.
- Acceptance criteria:
  - All 6 tools are registered with correct annotations
  - Read-only tools (list, status, conflict_check) have readOnlyHint = true
  - Destructive tools (delete, gc) have destructiveHint = true
  - conflict_check returns not_implemented with a descriptive message
  - Responds correctly via stdio transport
- Priority: P0
- Epic: Foundation

**FR-P0-015: macOS and Linux Platform Modules; Windows Stubs**
Full platform implementations for macOS (APFS clonefile, flock, statvfs, statfs f_fstypename) and Linux (FICLONE ioctl, flock, /proc/mounts). Windows stubs that compile but are not functionally complete.
- Acceptance criteria:
  - macOS: CoW via clonefile, correct filesystem type detection, flock-based locking
  - Linux: CoW via FICLONE, /proc/mounts parsing for NFS detection, flock-based locking
  - Windows: compiles without errors; platform-specific operations return appropriate errors
- Priority: P0
- Epic: Foundation

**FR-P0-016: Rate Limiter and Circuit Breaker**
Worktree creation is capped by Config::max_worktrees (default 20). The circuit breaker trips after Config::circuit_breaker_threshold (default 3) consecutive git command failures, returning CircuitBreakerOpen for all subsequent operations until reset.
- Acceptance criteria:
  - Returns RateLimitExceeded when worktree count reaches max_worktrees
  - Returns CircuitBreakerOpen after threshold consecutive git failures
  - Circuit breaker blocks all operations when open
- Priority: P0
- Epic: Foundation

### P1 -- Epic 2: Environment Lifecycle

**FR-P1-001: EcosystemAdapter Trait**
Defines the EcosystemAdapter trait with name(), detect(), setup(), teardown(), and branch_name() methods. Setup receives WORKTREE_CORE_* environment variables and compatibility-mapped CCManager/workmux variables.
- Acceptance criteria:
  - Trait is object-safe (Send + Sync)
  - setup() receives all 6 WORKTREE_CORE_* env vars
  - branch_name() defaults to identity (no transformation)
  - Compatibility env vars set for CCManager and workmux
- Priority: P1
- Epic: Environment Lifecycle

**FR-P1-002: DefaultAdapter**
Copies files from a configurable list (e.g., .env, .env.local) from the source worktree to the new worktree.
- Acceptance criteria:
  - Copies all files in files_to_copy list
  - Skips missing files without error
  - Respects ReflinkMode for copy operations
- Priority: P1
- Epic: Environment Lifecycle

**FR-P1-003: ShellCommandAdapter**
Runs arbitrary shell commands at create/delete time via post_create, pre_delete, and post_delete hooks. Receives all WORKTREE_CORE_* environment variables.
- Acceptance criteria:
  - Executes post_create command after worktree creation
  - Executes pre_delete before and post_delete after worktree removal
  - All WORKTREE_CORE_* env vars available to commands
  - Non-zero exit from post_create is reported but does not roll back worktree creation
- Priority: P1
- Epic: Environment Lifecycle

**FR-P1-004: wt create --setup CLI Integration**
The `wt create` command accepts a `--setup` flag that triggers the registered EcosystemAdapter after worktree creation.
- Acceptance criteria:
  - --setup triggers adapter detection and setup
  - Port allocation exposed via --port flag
  - wt status shows port allocation
- Priority: P1
- Epic: Environment Lifecycle

**FR-P1-005: macOS .DS_Store Pre-Removal**
On macOS, remove .DS_Store files before calling `git worktree remove`, as .DS_Store blocks the removal command.
- Acceptance criteria:
  - wt delete succeeds when .DS_Store is present in worktree root
  - wt gc handles .DS_Store in orphaned worktrees
- Priority: P1
- Epic: Environment Lifecycle

**FR-P1-006: Windows MAX_PATH Workarounds via dunce**
Use the dunce crate to strip `\\?\` prefixes when passing paths to external tools (including git) on Windows. Ensure Rust's automatic `\\?\` prepending does not interfere with git operations.
- Acceptance criteria:
  - Paths passed to git never contain `\\?\` prefix
  - Long paths (> 260 chars) work correctly on Windows
- Priority: P1
- Epic: Environment Lifecycle

### P2 -- Epic 3: Conflict Intelligence

**FR-P2-001: git merge-tree CLI Parser**
Parses output of `git merge-tree --write-tree -z --stdin` to detect merge conflicts between branch pairs. Requires Git >= 2.38.
- Acceptance criteria:
  - Correctly parses NUL-delimited merge-tree output
  - Identifies conflicting files, conflict types, and affected paths
  - Handles batch input of multiple branch pairs via --stdin
- Priority: P2
- Epic: Conflict Intelligence

**FR-P2-002: ConflictReport and ConflictType Enum**
Defines ConflictReport struct and ConflictType enum for representing merge conflict analysis results.
- Acceptance criteria:
  - ConflictType covers content conflicts, add/add, modify/delete, and rename scenarios
  - ConflictReport includes affected files, conflict types, and base/ours/theirs refs
- Priority: P2
- Epic: Conflict Intelligence

**FR-P2-003: wt check Subcommand**
The `wt check` subcommand runs conflict detection using git merge-tree. Requires Git >= 2.38; provides graceful degradation with upgrade instructions on older versions.
- Acceptance criteria:
  - Returns conflict report for specified branch pairs
  - Returns descriptive error with upgrade instructions when Git < 2.38
  - Processes 20 merge pairs via --stdin in < 10 s
- Priority: P2
- Epic: Conflict Intelligence

**FR-P2-004: MCP conflict_check Tool Implementation**
Replaces the not_implemented stub with a working conflict_check tool in the MCP server.
- Acceptance criteria:
  - Returns conflict analysis results via MCP protocol
  - Gracefully degrades on Git < 2.38
- Priority: P2
- Epic: Conflict Intelligence

**FR-P2-005: HTTP MCP Transport**
Adds HTTP transport to the MCP server for Cursor remote, VS Code Dev Containers, and SSH setups.
- Acceptance criteria:
  - MCP server responds correctly over HTTP
  - Functions in VS Code Dev Container environment
- Priority: P2
- Epic: Conflict Intelligence

**FR-P2-006: Windows Full Platform Implementation**
Replaces compile-only stubs with full Windows platform code: LockFileEx via fd-lock, NTFS junctions via the junction crate, GetDiskFreeSpaceEx, GetDriveTypeW, dunce path handling.
- Acceptance criteria:
  - cargo test passes on Windows Server 2019
  - Junction creation works without admin privileges
  - Network junction targets are correctly rejected
- Priority: P2
- Epic: Conflict Intelligence

### P3 -- Epic 4: Ecosystem Integration

**FR-P3-001: pnpm Adapter**
Leverages `enableGlobalVirtualStore: true` so multiple worktrees share a single virtual store.
- Acceptance criteria:
  - 5 worktrees share a single virtual store; du -sh node_modules shows < 1 MB per worktree (symlinks only)
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-002: uv Adapter**
Creates per-worktree virtual environments via `uv venv && uv pip install -r requirements.txt`.
- Acceptance criteria:
  - Worktree with requirements.txt fully installed in < 10 seconds
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-003: Cargo Adapter**
Uses per-worktree target directories. Does not share CARGO_TARGET_DIR across worktrees due to known cargo bug with same-name path deps from different worktrees.
- Acceptance criteria:
  - Each worktree uses an isolated target directory
  - No cross-worktree build interference
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-004: gix Conflict Detection**
Integrates `gix::Repository::merge_trees()` as an alternative to the CLI-based conflict detection, feature-gated behind an optional dependency.
- Acceptance criteria:
  - Feature-flagged; does not affect default compilation
  - Produces equivalent results to CLI-based detection
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-005: napi-rs Node.js Bindings**
Publishes Node.js bindings via napi-rs with auto-generated TypeScript types as `@worktree-core/node`.
- Acceptance criteria:
  - npm package published with TypeScript type definitions
  - Core lifecycle operations (create, delete, list, gc) available from Node.js
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-006: PyO3 Python Bindings**
Provides Python bindings for the core library via PyO3.
- Acceptance criteria:
  - Core lifecycle operations available from Python
  - Package installable via pip
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-007: Worktree Pooling**
Pre-creates N worktrees for instant checkout. Pool size is configurable with automatic replenishment.
- Acceptance criteria:
  - Pool of 5 worktrees available in < 1 second (vs. ~5 seconds for on-demand creation)
  - Pool auto-replenishes after checkout
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-008: Merge Lifecycle Automation**
Manages the Active -> Merging -> Active state transitions for worktrees involved in merge operations.
- Acceptance criteria:
  - State transitions follow the defined state machine
  - Merging state blocks deletion
- Priority: P3
- Epic: Ecosystem Integration

**FR-P3-009: MCP Server Docs for All Four Clients**
README includes config snippets for Claude Code, Cursor, VS Code Copilot, and OpenCode with correct root keys (mcpServers vs. servers vs. mcp).
- Acceptance criteria:
  - Config snippets for all 4 clients included and tested
  - VS Code snippet uses "servers" key (not "mcpServers")
- Priority: P3
- Epic: Ecosystem Integration

---

## 3. Non-Functional Requirements

### Performance

- **Manager::new() cold start:** < 500 ms including git version detection, capability mapping, state.json read, and orphan scan.
- **Disk usage walk:** < 200 ms for 50,000 files on a local filesystem using jwalk with Rayon parallelism and preload_metadata(true).
- **Manager::create() on a 2 GB repository:** < 10 seconds end-to-end including all pre-create guards, git worktree add, and post-create verification.
- **conflict_check for 20 branch pairs via --stdin:** < 10 seconds using git merge-tree --write-tree -z.

### Reliability

- **Zero data loss:** 100 create/delete cycles with simulated crash injection (SIGKILL at random points) must produce no data loss.
- **Circuit breaker:** Trips after 3 consecutive git command failures (configurable via Config::circuit_breaker_threshold), blocking all operations until reset.
- **Atomic state writes:** All state.json mutations use write-to-tmp, fsync, atomic rename. No partial writes are possible.
- **Stale lock recovery:** Four-factor check (PID + start_time + UUID + hostname) detects and recovers from stale locks caused by crashes or PID reuse.

### Compatibility

- **Git:** >= 2.20 (hard minimum). Graceful degradation for features requiring newer versions (see Git Version Capability Matrix).
- **Rust MSRV:** 1.75.
- **macOS:** 10.15+ (Catalina). APFS clonefile for CoW.
- **Linux:** glibc 2.17+. Btrfs/XFS/OpenZFS for CoW via FICLONE ioctl.
- **Windows:** 10 Build 1607+. NTFS junctions (no admin privileges). ReFS for CoW (server only).

### Security

- State files are stored inside `.git/worktree-core/`, safe from `git gc` pruning.
- Lock files use advisory locking with RAII guards; no persistent lock file deletion (prevents races).
- Network filesystem detection degrades locking to atomic-rename-only mode with logged warnings.
- Branch names are never transformed or sanitized by the library; git validates them.

### Usability

- `wt gc` defaults to dry-run; destructive GC requires explicit `--confirm`.
- `wt hook --stdin-format claude-code` produces exactly one line on stdout for integration compatibility.
- All error types are structured with actionable messages (e.g., UnmergedCommits includes branch name and commit count).
- MCP tool annotations enable read-only tools to execute without user approval in Claude Code and Cursor.

---

## 4. Epic Definitions

### Epic 1: Foundation (Milestone 1, Weeks 1-6)

**Deliverable:** worktree-core crate on crates.io, wt CLI binary, worktree-core-mcp binary with 6 tools.

**Ship criteria (all must pass):**
- `cargo clippy -- -D warnings` clean.
- `cargo test` passes on macOS and Linux.
- Zero data loss in stress test: 100 create/delete cycles with simulated crash injection (SIGKILL at random points).
- `wt gc` successfully cleans orphaned worktrees from a simulated OpenCode failure (1000 orphans, varying ages).
- `wt hook --stdin-format claude-code` produces exactly one line on stdout (the absolute path), nothing else.
- MCP server responds correctly to `worktree_list`, `worktree_create`, `worktree_delete`, `worktree_gc`.
- Crates published: `worktree-core`, `worktree-core-cli`, `worktree-core-mcp`.

### Epic 2: Environment Lifecycle (Milestone 2, Weeks 7-10)

**Deliverable:** DefaultAdapter, ShellCommandAdapter, port allocation CLI, wt attach stability, cross-platform cleanup.

**Ship criteria (all must pass):**
- `wt create --setup` bootstraps a Node.js project using `ShellCommandAdapter` with `npm install`.
- `wt create --setup` copies `.env` using `DefaultAdapter`.
- Port allocation assigns unique ports to 20 simultaneous worktrees with no collision.
- `wt attach` on a path matching a `stale_worktrees` entry correctly recovers the original port.
- macOS `.DS_Store` test: `wt delete` succeeds even when `.DS_Store` is present in worktree root.

### Epic 3: Conflict Intelligence (Milestone 3, Weeks 11-16)

**Deliverable:** External integrations, conflict detection MVP, HTTP MCP transport.

**Ship criteria (all must pass):**
- At least one external project consuming `worktree-core` as a library dependency.
- `wt check` correctly identifies conflicts for a test corpus of 20 merge scenarios.
- MCP HTTP transport responds correctly in a VS Code Dev Container environment.
- Windows CI passing (`cargo test` on Windows Server 2019 runner).

### Epic 4: Ecosystem Integration (Milestone 4, Weeks 17-20)

**Deliverable:** Ecosystem-specific adapters, language bindings, worktree pooling.

**Ship criteria (all must pass):**
- pnpm adapter: 5 worktrees share a single virtual store. `du -sh node_modules` in each shows <1 MB (symlinks only).
- uv adapter: worktree with `requirements.txt` fully installed in <10 seconds.
- Node.js package published to npm as `@worktree-core/node`.
- Worktree pool of 5 worktrees available in <1 second (vs. ~5 seconds for on-demand creation).

---

## 5. Data Model

### state.json v2 Schema

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

### Storage Location Table

| Data | Location | Notes |
|---|---|---|
| Worktree metadata + port leases | `<repo>/.git/worktree-core/state.json` | Safe from `git gc` -- custom dirs in `.git/` are never pruned |
| Lock file | `<repo>/.git/worktree-core/state.lock` | Adjacent to state for atomic coordination |
| User preferences | `$XDG_CONFIG_HOME/worktree-core/config.toml` | macOS: `~/Library/Application Support/worktree-core/` |
| Cache | `$XDG_CACHE_HOME/worktree-core/` | Disposable |
| Logs | `$XDG_STATE_HOME/worktree-core/` | Falls back to `$XDG_CACHE_HOME` on macOS/Windows |

`WORKTREE_CORE_HOME` environment variable overrides all computed paths. Uses the `directories` crate v6.0.0 via `ProjectDirs::from("", "", "worktree-core")`.

### Locking Protocol Summary

1. Run four-factor stale detection on state.lock (PID liveness + start_time + UUID + hostname). Delete if stale.
2. Attempt non-blocking exclusive advisory lock via fd-lock. On failure, enter Full Jitter retry loop (base 10ms, cap 2000ms, max 15 attempts). On timeout, return StateLockContention.
3. Write multi-factor JSON record to state.lock.
4. Read state.json.
5. Apply mutation in memory.
6. Write to state.json.tmp.
7. fsync the temp file.
8. Atomic rename state.json.tmp to state.json.
9. Drop lock guard (RAII release).

Critical invariants: "Never delete state.lock after releasing." "Never hold state.lock across git worktree add."

---

## 6. Open Questions

1. **Port lease renewal mechanism (Section 10.4).** The spec states leases are renewed "every TTL/3 (~2.5 hours) during active use" but does not define what constitutes "active use" or who triggers renewal. Is renewal the responsibility of the `Manager` (background timer) or the caller? If background, what thread/task model is expected?

2. **`check_not_network_filesystem` as warning vs. error (Section 8.1, guard #6).** The spec says "warning, not hard block by default" but does not expose a `Config` field to escalate it to an error. Should a `Config::deny_network_filesystem: bool` field be added?

3. **`wt attach` for bare repos (Sections 5.3, 11.x).** `BareRepositoryUnsupported` was removed in v1.5 (bare repos permitted with explicit paths), but `Manager::attach()` does not specify behavior when attaching a worktree into a bare repo. Clarify: is `attach()` permitted on bare repos?

4. **Circuit breaker reset mechanism (Section 4.4, `Config::circuit_breaker_threshold`).** The `CircuitBreakerOpen` error is documented, but no reset mechanism is specified. Is the circuit breaker reset automatically after a timeout, manually via a `Manager` method, or on `Manager` reconstruction?

5. **`git worktree add` base for bare repos (Section 2, "bare repos supported").** When the primary repo is bare, `git worktree add` is called from the bare repo root. Confirm that `git worktree add` from a bare repo root works as expected in the minimum supported Git version (2.20) and document the exact command form.

6. **`wt gc` concurrency with active agents (Section 5.3).** If an agent is actively using a worktree but its process is not the one holding `state.lock`, `gc()` could evict it (if it appears orphaned). Is there a mechanism to mark a worktree as "in use" beyond the `Locked` state (which requires an explicit `git worktree lock` call)?
