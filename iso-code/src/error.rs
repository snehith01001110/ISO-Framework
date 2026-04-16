use std::path::PathBuf;
use crate::types::WorktreeState;

/// Errors returned by iso-code operations.
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
    InvalidStateTransition {
        from: WorktreeState,
        to: WorktreeState,
    },

    #[error("worktree path not found in git registry: {0}")]
    WorktreeNotInGitRegistry(PathBuf),

    #[error("branch '{branch}' already exists at {branch_commit} but base was explicitly set to '{requested_base}' ({requested_commit}) — reset the branch or omit base")]
    BranchExistsWithDifferentBase {
        branch: String,
        branch_commit: String,
        requested_base: String,
        requested_commit: String,
    },

    #[error("setup = true was requested but no EcosystemAdapter is registered on this Manager — use Manager::with_adapter()")]
    SetupRequestedWithoutAdapter,

    #[error("git command failed\n  command: {command}\n  stderr: {stderr}\n  exit: {exit_code}")]
    GitCommandFailed {
        command: String,
        stderr: String,
        exit_code: i32,
    },

    #[error("state file corrupted: {reason} — rebuild from git worktree list")]
    StateCorrupted { reason: String },

    #[error("circuit breaker open: {consecutive_failures} consecutive git failures")]
    CircuitBreakerOpen { consecutive_failures: u32 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
