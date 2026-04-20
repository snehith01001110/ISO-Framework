use std::path::PathBuf;

#[doc(inline)]
pub use crate::adapter::EcosystemAdapter;

// ── 4.1 WorktreeHandle ──────────────────────────────────────────────────

/// A handle to a managed git worktree, containing all metadata tracked by iso-code.
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

impl WorktreeHandle {
    /// Create a new WorktreeHandle. Used internally by Manager.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: PathBuf,
        branch: String,
        base_commit: String,
        state: WorktreeState,
        created_at: String,
        creator_pid: u32,
        creator_name: String,
        adapter: Option<String>,
        setup_complete: bool,
        port: Option<u16>,
        session_uuid: String,
    ) -> Self {
        Self {
            path,
            branch,
            base_commit,
            state,
            created_at,
            creator_pid,
            creator_name,
            adapter,
            setup_complete,
            port,
            session_uuid,
        }
    }
}

// ── 4.2 WorktreeState ───────────────────────────────────────────────────

/// Lifecycle state of a managed worktree.
///
/// Deserialization is lenient: unknown variant names (written by a newer
/// version of iso-code) are mapped to `Broken` rather than failing, so old
/// readers don't reject an otherwise-valid state file. This keeps the enum
/// forward-compatible despite `#[non_exhaustive]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
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

impl<'de> serde::Deserialize<'de> for WorktreeState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "Pending" => Self::Pending,
            "Creating" => Self::Creating,
            "Active" => Self::Active,
            "Merging" => Self::Merging,
            "Deleting" => Self::Deleting,
            "Deleted" => Self::Deleted,
            "Orphaned" => Self::Orphaned,
            "Broken" => Self::Broken,
            "Locked" => Self::Locked,
            // Unknown variant from a newer writer — degrade rather than fail.
            _ => Self::Broken,
        })
    }
}

// ── 4.3 ReflinkMode & CopyOutcome ──────────────────────────────────────

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

// ── 4.4 Config ──────────────────────────────────────────────────────────

/// Configuration for a [`Manager`](crate::Manager) instance.
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
    /// Mirrors the ISO_CODE_HOME environment variable.
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
    /// Skip network operations (e.g. `git fetch` in the five-step unmerged
    /// commit check). Set to true when running offline or in CI to avoid
    /// network latency per delete / per-candidate during `gc()`.
    /// Default: false.
    pub offline: bool,
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
            creator_name: "iso-code".to_string(),
            offline: false,
        }
    }
}

// ── 4.5 CreateOptions ───────────────────────────────────────────────────

/// Options for [`Manager::create()`](crate::Manager::create).
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

// ── 4.5b AttachOptions ──────────────────────────────────────────────────

/// Options for [`Manager::attach()`](crate::Manager::attach).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct AttachOptions {
    /// Run the registered EcosystemAdapter after attaching. Default: false.
    pub setup: bool,
    /// Allocate a port lease for this worktree. Default: false.
    pub allocate_port: bool,
}

// ── 4.6 DeleteOptions ───────────────────────────────────────────────────

/// Options for [`Manager::delete()`](crate::Manager::delete).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DeleteOptions {
    /// Skip the five-step unmerged commits check.
    /// WARNING: Can cause data loss. Requires explicit opt-in. Default: false.
    pub force: bool,
    /// Skip the uncommitted changes check.
    /// WARNING: Destroys uncommitted work. Default: false.
    pub force_dirty: bool,
    /// Delete even when the worktree is locked. Bypasses `check_not_locked`
    /// and calls `git worktree remove --force` instead of the plain remove.
    /// WARNING: Locks exist for a reason — only set when the caller owns the lock. Default: false.
    pub force_locked: bool,
}

// ── 4.7 GcOptions & GcReport ────────────────────────────────────────────

/// Options for [`Manager::gc()`](crate::Manager::gc).
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
        Self {
            dry_run: true,
            max_age_days: None,
            force: false,
        }
    }
}

/// Report returned by [`Manager::gc()`](crate::Manager::gc).
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

impl GcReport {
    pub fn new(
        orphans: Vec<PathBuf>,
        removed: Vec<PathBuf>,
        evicted: Vec<PathBuf>,
        freed_bytes: u64,
        dry_run: bool,
    ) -> Self {
        Self { orphans, removed, evicted, freed_bytes, dry_run }
    }
}

// ── 4.8 GitCapabilities & GitVersion ────────────────────────────────────

/// Feature flags derived from the detected git version.
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

impl GitCapabilities {
    pub fn new(
        version: GitVersion,
        has_list_nul: bool,
        has_repair: bool,
        has_orphan: bool,
        has_relative_paths: bool,
        has_merge_tree_write: bool,
    ) -> Self {
        Self { version, has_list_nul, has_repair, has_orphan, has_relative_paths, has_merge_tree_write }
    }
}

/// Parsed semantic version of the git binary.
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

// ── 4.9 PortLease ───────────────────────────────────────────────────────

/// A port allocated to a worktree, with an 8-hour TTL.
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

// ── 8.3 GitCryptStatus ──────────────────────────────────────────────────

/// Status of git-crypt in a repository or worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCryptStatus {
    NotUsed,
    /// Key file absent.
    LockedNoKey,
    /// Key exists but files show magic header — smudge filter not run.
    Locked,
    /// Key exists and files are decrypted.
    Unlocked,
}

// ── 6. EcosystemAdapter Trait ───────────────────────────────────────────
// Defined in `crate::adapter`; re-exported at the top of this file for
// backward compatibility with `crate::types::EcosystemAdapter` imports.
