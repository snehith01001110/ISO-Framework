//! State persistence: state.json v2 read/write/migrate.
//!
//! State lives at `<repo>/.git/iso-code/state.json` and is rewritten via a
//! write-temp → fsync → rename sequence for crash safety. The file lock is
//! scoped strictly around each read-modify-write. Unknown fields are preserved
//! through a `#[serde(flatten)]` catch-all to keep newer writers forward
//! compatible with older readers.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::WorktreeError;
use crate::types::{PortLease, WorktreeState};

// ── state.json v2 schema ─────────────────────────────────────────────────

/// Top-level state file (v2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StateV2 {
    pub schema_version: u64,
    pub repo_id: String,
    pub last_modified: DateTime<Utc>,
    #[serde(default)]
    pub active_worktrees: HashMap<String, ActiveWorktreeEntry>,
    #[serde(default)]
    pub stale_worktrees: HashMap<String, StaleWorktreeEntry>,
    #[serde(default)]
    pub port_leases: HashMap<String, PortLease>,
    #[serde(default)]
    pub config_snapshot: Option<ConfigSnapshot>,
    #[serde(default)]
    pub gc_history: Vec<GcHistoryEntry>,
    /// Catch-all for forward compatibility.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// An active worktree entry in state.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ActiveWorktreeEntry {
    pub path: String,
    pub branch: String,
    pub base_commit: String,
    pub state: WorktreeState,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_activity: Option<DateTime<Utc>>,
    pub creator_pid: u32,
    #[serde(default)]
    pub creator_name: String,
    pub session_uuid: String,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub setup_complete: bool,
    #[serde(default)]
    pub port: Option<u16>,
    /// Catch-all for forward compatibility.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// A stale (evicted) worktree entry in state.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StaleWorktreeEntry {
    pub original_path: String,
    pub branch: String,
    pub base_commit: String,
    #[serde(default)]
    pub creator_name: String,
    pub session_uuid: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub last_activity: Option<DateTime<Utc>>,
    pub evicted_at: DateTime<Utc>,
    #[serde(default)]
    pub eviction_reason: String,
    pub expires_at: DateTime<Utc>,
    /// Catch-all for forward compatibility.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Snapshot of config written into state.json for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ConfigSnapshot {
    #[serde(default = "default_max_worktrees")]
    pub max_worktrees: usize,
    #[serde(default = "default_disk_threshold")]
    pub disk_threshold_percent: u8,
    #[serde(default = "default_gc_max_age")]
    pub gc_max_age_days: u32,
    #[serde(default = "default_port_start")]
    pub port_range_start: u16,
    #[serde(default = "default_port_end")]
    pub port_range_end: u16,
    #[serde(default = "default_stale_ttl")]
    pub stale_metadata_ttl_days: u32,
    /// Catch-all for forward compatibility.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

fn default_max_worktrees() -> usize { 20 }
fn default_disk_threshold() -> u8 { 90 }
fn default_gc_max_age() -> u32 { 7 }
fn default_port_start() -> u16 { 3100 }
fn default_port_end() -> u16 { 5100 }
fn default_stale_ttl() -> u32 { 30 }

/// A single GC history record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GcHistoryEntry {
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub removed: u32,
    #[serde(default)]
    pub evicted: u32,
    #[serde(default)]
    pub freed_mb: u64,
    /// Catch-all for forward compatibility.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

// ── Constructors ─────────────────────────────────────────────────────────

impl StateV2 {
    /// Create a fresh empty state for the given repo.
    pub fn new_empty(repo_id: String) -> Self {
        Self {
            schema_version: 2,
            repo_id,
            last_modified: Utc::now(),
            active_worktrees: HashMap::new(),
            stale_worktrees: HashMap::new(),
            port_leases: HashMap::new(),
            config_snapshot: None,
            gc_history: Vec::new(),
            extra: HashMap::new(),
        }
    }
}

// ── Path helpers ─────────────────────────────────────────────────────────

/// Compute the repo_id: sha256 hex of the absolute canonicalized repo path.
pub fn compute_repo_id(repo_root: &Path) -> String {
    use sha2::{Digest, Sha256};
    let canonical = dunce::canonicalize(repo_root)
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Return the state directory: `<repo>/.git/iso-code/`.
/// Respects `ISO_CODE_HOME` env var override and `Config.home_override`.
pub fn state_dir(repo_root: &Path, home_override: Option<&Path>) -> PathBuf {
    // Check config override first, then env var
    if let Some(home) = home_override {
        return home.to_path_buf();
    }
    if let Ok(home) = std::env::var("ISO_CODE_HOME") {
        return PathBuf::from(home);
    }
    repo_root.join(".git").join("iso-code")
}

/// Return the state.json path.
pub fn state_json_path(repo_root: &Path, home_override: Option<&Path>) -> PathBuf {
    state_dir(repo_root, home_override).join("state.json")
}

/// Return the state.lock path.
pub fn state_lock_path(repo_root: &Path, home_override: Option<&Path>) -> PathBuf {
    state_dir(repo_root, home_override).join("state.lock")
}

/// Ensure the state directory exists. Called from Manager::new().
pub fn ensure_state_dir(repo_root: &Path, home_override: Option<&Path>) -> Result<(), WorktreeError> {
    let dir = state_dir(repo_root, home_override);
    fs::create_dir_all(&dir)?;
    Ok(())
}

// ── Read / Write / Migrate ───────────────────────────────────────────────

/// Read and parse state.json, migrating from v1 if needed.
/// If the file is missing, returns a fresh empty state.
///
/// If the file exists but cannot be parsed as JSON, the corrupt file is
/// renamed to `state.json.corrupt.<timestamp>` and a fresh empty state is
/// returned. The next `list()` will repopulate active worktrees from
/// `git worktree list`. A migration failure (unknown schema version) is
/// surfaced as `StateCorrupted` — we don't clobber data we can't interpret.
pub fn read_state(repo_root: &Path, home_override: Option<&Path>) -> Result<StateV2, WorktreeError> {
    let path = state_json_path(repo_root, home_override);

    if !path.exists() {
        let repo_id = compute_repo_id(repo_root);
        return Ok(StateV2::new_empty(repo_id));
    }

    let raw_bytes = fs::read(&path)?;
    let raw: Value = match serde_json::from_slice(&raw_bytes) {
        Ok(v) => v,
        Err(e) => {
            let ts = chrono::Utc::now().timestamp();
            let backup = path.with_extension(format!("json.corrupt.{ts}"));
            let rename_result = fs::rename(&path, &backup);
            eprintln!(
                "[iso-code] WARNING: state.json is corrupt ({e}); {} rebuilding from git",
                match &rename_result {
                    Ok(_) => format!("moved to {}", backup.display()),
                    Err(re) => format!("could not back up ({re});"),
                }
            );
            let repo_id = compute_repo_id(repo_root);
            return Ok(StateV2::new_empty(repo_id));
        }
    };

    migrate(raw)
}

/// Atomically write state.json: write tmp -> fsync -> rename.
pub fn write_state(
    repo_root: &Path,
    home_override: Option<&Path>,
    state: &mut StateV2,
) -> Result<(), WorktreeError> {
    let dir = state_dir(repo_root, home_override);
    fs::create_dir_all(&dir)?;

    let final_path = dir.join("state.json");
    let tmp_path = dir.join("state.json.tmp");

    // Update last_modified timestamp
    state.last_modified = Utc::now();

    let json = serde_json::to_string_pretty(state).map_err(|e| {
        WorktreeError::StateCorrupted {
            reason: format!("serialization failed: {e}"),
        }
    })?;

    // Write to tmp file
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?; // fsync
    }

    // Atomic rename
    fs::rename(&tmp_path, &final_path)?;

    Ok(())
}

/// Schema migration dispatcher. Upgrades older state files to the current
/// schema version in-place before they are returned to callers.
pub fn migrate(raw: Value) -> Result<StateV2, WorktreeError> {
    let version = raw.get("schema_version")
        .and_then(|v| v.as_u64())
        .or_else(|| raw.get("version").and_then(|v| v.as_u64()))
        .unwrap_or(1);

    match version {
        1 => migrate_v1_to_v2(raw),
        2 => serde_json::from_value(raw).map_err(|e| WorktreeError::StateCorrupted {
            reason: e.to_string(),
        }),
        v => Err(WorktreeError::StateCorrupted {
            reason: format!("unknown schema version {v}"),
        }),
    }
}

/// Migrate a v1 state file to the v2 schema.
///
/// v1 format: `{ "version": 1, "worktrees": { ... } }`.
fn migrate_v1_to_v2(raw: Value) -> Result<StateV2, WorktreeError> {
    let repo_id = raw.get("repo_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let now = Utc::now();

    // Convert v1 worktrees map to v2 active_worktrees
    let mut active_worktrees = HashMap::new();
    if let Some(wts) = raw.get("worktrees").and_then(|v| v.as_object()) {
        for (key, val) in wts {
            let entry: ActiveWorktreeEntry = serde_json::from_value(val.clone())
                .map_err(|e| WorktreeError::StateCorrupted {
                    reason: format!("v1 worktree entry '{key}' invalid: {e}"),
                })?;
            active_worktrees.insert(key.clone(), entry);
        }
    }

    Ok(StateV2 {
        schema_version: 2,
        repo_id,
        last_modified: now,
        active_worktrees,
        stale_worktrees: HashMap::new(),
        port_leases: HashMap::new(),
        config_snapshot: None,
        gc_history: Vec::new(),
        extra: HashMap::new(),
    })
}

/// Default lock acquisition timeout when the caller doesn't supply one.
/// Tests and internal callers without a `Config` in hand use this.
const DEFAULT_LOCK_TIMEOUT_MS: u64 = 30_000;

/// Read-modify-write helper: acquires state.lock, reads state, applies the
/// closure, then writes back. The lock is released as soon as this function
/// returns — callers must not perform long-running work inside the closure.
///
/// Uses `DEFAULT_LOCK_TIMEOUT_MS`. Prefer [`with_state_timeout`] from inside a
/// Manager so `Config.lock_timeout_ms` is honored.
pub fn with_state<F>(
    repo_root: &Path,
    home_override: Option<&Path>,
    f: F,
) -> Result<StateV2, WorktreeError>
where
    F: FnOnce(&mut StateV2) -> Result<(), WorktreeError>,
{
    with_state_timeout(repo_root, home_override, DEFAULT_LOCK_TIMEOUT_MS, f)
}

/// Like [`with_state`] but takes an explicit lock acquisition timeout.
pub fn with_state_timeout<F>(
    repo_root: &Path,
    home_override: Option<&Path>,
    lock_timeout_ms: u64,
    f: F,
) -> Result<StateV2, WorktreeError>
where
    F: FnOnce(&mut StateV2) -> Result<(), WorktreeError>,
{
    let lock_path = state_lock_path(repo_root, home_override);
    let _lock = crate::lock::StateLock::acquire(&lock_path, lock_timeout_ms)?;

    let mut state = read_state(repo_root, home_override)?;
    f(&mut state)?;
    write_state(repo_root, home_override, &mut state)?;
    Ok(state)
    // _lock dropped here → flock released
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        // Create .git directory to mimic a repo
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        dir
    }

    #[test]
    fn test_compute_repo_id_deterministic() {
        let dir = setup_repo();
        let id1 = compute_repo_id(dir.path());
        let id2 = compute_repo_id(dir.path());
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 64); // sha256 hex = 64 chars
    }

    #[test]
    fn test_state_dir_default() {
        let dir = setup_repo();
        let sd = state_dir(dir.path(), None);
        assert!(sd.ends_with("iso-code"));
        assert!(sd.to_string_lossy().contains(".git"));
    }

    #[test]
    fn test_state_dir_home_override() {
        let dir = setup_repo();
        let override_path = dir.path().join("custom");
        let sd = state_dir(dir.path(), Some(&override_path));
        assert_eq!(sd, override_path);
    }

    #[test]
    fn test_ensure_state_dir_creates_directory() {
        let dir = setup_repo();
        let sd = state_dir(dir.path(), None);
        assert!(!sd.exists());
        ensure_state_dir(dir.path(), None).unwrap();
        assert!(sd.exists());
    }

    #[test]
    fn test_new_empty_state() {
        let state = StateV2::new_empty("test-repo-id".to_string());
        assert_eq!(state.schema_version, 2);
        assert_eq!(state.repo_id, "test-repo-id");
        assert!(state.active_worktrees.is_empty());
        assert!(state.stale_worktrees.is_empty());
        assert!(state.port_leases.is_empty());
        assert!(state.gc_history.is_empty());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut state = StateV2::new_empty("roundtrip-test".to_string());

        // Add an active worktree
        state.active_worktrees.insert(
            "feature-auth".to_string(),
            ActiveWorktreeEntry {
                path: "/tmp/worktrees/feature-auth".to_string(),
                branch: "feature-auth".to_string(),
                base_commit: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
                state: WorktreeState::Active,
                created_at: Utc::now(),
                last_activity: Some(Utc::now()),
                creator_pid: 12345,
                creator_name: "test".to_string(),
                session_uuid: "f7a3b9c1-2d4e-4f56-a789-0123456789ab".to_string(),
                adapter: None,
                setup_complete: true,
                port: Some(3200),
                extra: HashMap::new(),
            },
        );

        // Add a stale worktree
        state.stale_worktrees.insert(
            "old-refactor".to_string(),
            StaleWorktreeEntry {
                original_path: "/tmp/worktrees/old-refactor".to_string(),
                branch: "refactor/db-layer".to_string(),
                base_commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
                creator_name: "alice".to_string(),
                session_uuid: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
                port: Some(3100),
                last_activity: Some(Utc::now()),
                evicted_at: Utc::now(),
                eviction_reason: "auto-gc: inactive >7 days".to_string(),
                expires_at: Utc::now(),
                extra: HashMap::new(),
            },
        );

        // Serialize
        let json = serde_json::to_string_pretty(&state).unwrap();

        // Deserialize
        let parsed: StateV2 = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.repo_id, "roundtrip-test");
        assert_eq!(parsed.active_worktrees.len(), 1);
        assert_eq!(parsed.stale_worktrees.len(), 1);

        let active = parsed.active_worktrees.get("feature-auth").unwrap();
        assert_eq!(active.branch, "feature-auth");
        assert_eq!(active.port, Some(3200));
        assert!(active.setup_complete);

        let stale = parsed.stale_worktrees.get("old-refactor").unwrap();
        assert_eq!(stale.branch, "refactor/db-layer");
        assert_eq!(stale.eviction_reason, "auto-gc: inactive >7 days");
    }

    #[test]
    fn test_forward_compatibility_unknown_fields_preserved() {
        let json = r#"{
            "schema_version": 2,
            "repo_id": "test",
            "last_modified": "2026-04-13T14:22:00Z",
            "active_worktrees": {},
            "stale_worktrees": {},
            "port_leases": {},
            "gc_history": [],
            "future_field": "hello from the future",
            "another_unknown": 42
        }"#;

        let state: StateV2 = serde_json::from_str(json).unwrap();
        assert_eq!(state.extra.get("future_field").unwrap(), "hello from the future");
        assert_eq!(state.extra.get("another_unknown").unwrap(), 42);

        // Re-serialize and verify unknown fields survive
        let reserialized = serde_json::to_string(&state).unwrap();
        assert!(reserialized.contains("future_field"));
        assert!(reserialized.contains("another_unknown"));
    }

    #[test]
    fn test_active_worktree_entry_unknown_fields() {
        let json = r#"{
            "path": "/tmp/wt",
            "branch": "feat",
            "base_commit": "abc123",
            "state": "Active",
            "created_at": "2026-04-10T09:00:00Z",
            "creator_pid": 100,
            "creator_name": "test",
            "session_uuid": "uuid-1",
            "new_v3_field": true
        }"#;

        let entry: ActiveWorktreeEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.branch, "feat");
        assert!(entry.extra.contains_key("new_v3_field"));

        let reserialized = serde_json::to_string(&entry).unwrap();
        assert!(reserialized.contains("new_v3_field"));
    }

    #[test]
    fn test_read_state_missing_file_returns_empty() {
        let dir = setup_repo();
        let state = read_state(dir.path(), None).unwrap();
        assert_eq!(state.schema_version, 2);
        assert!(state.active_worktrees.is_empty());
    }

    #[test]
    fn test_write_and_read_state_roundtrip() {
        let dir = setup_repo();
        let mut state = StateV2::new_empty(compute_repo_id(dir.path()));
        state.active_worktrees.insert(
            "test-branch".to_string(),
            ActiveWorktreeEntry {
                path: "/tmp/test".to_string(),
                branch: "test-branch".to_string(),
                base_commit: "abc123".to_string(),
                state: WorktreeState::Active,
                created_at: Utc::now(),
                last_activity: None,
                creator_pid: std::process::id(),
                creator_name: "test".to_string(),
                session_uuid: uuid::Uuid::new_v4().to_string(),
                adapter: None,
                setup_complete: false,
                port: None,
                extra: HashMap::new(),
            },
        );

        write_state(dir.path(), None, &mut state).unwrap();

        let read_back = read_state(dir.path(), None).unwrap();
        assert_eq!(read_back.active_worktrees.len(), 1);
        assert!(read_back.active_worktrees.contains_key("test-branch"));
    }

    #[test]
    fn test_atomic_write_creates_no_tmp_file() {
        let dir = setup_repo();
        let mut state = StateV2::new_empty("test".to_string());
        write_state(dir.path(), None, &mut state).unwrap();

        let sd = state_dir(dir.path(), None);
        assert!(sd.join("state.json").exists());
        assert!(!sd.join("state.json.tmp").exists());
    }

    #[test]
    fn test_migrate_v2_passthrough() {
        let json = serde_json::json!({
            "schema_version": 2,
            "repo_id": "test",
            "last_modified": "2026-04-13T14:22:00Z",
            "active_worktrees": {},
            "stale_worktrees": {},
            "port_leases": {},
            "gc_history": []
        });

        let state = migrate(json).unwrap();
        assert_eq!(state.schema_version, 2);
        assert_eq!(state.repo_id, "test");
    }

    #[test]
    fn test_migrate_v1_to_v2() {
        let json = serde_json::json!({
            "version": 1,
            "repo_id": "legacy-repo",
            "worktrees": {
                "feat-x": {
                    "path": "/tmp/feat-x",
                    "branch": "feat-x",
                    "base_commit": "abc123",
                    "state": "Active",
                    "created_at": "2026-01-01T00:00:00Z",
                    "creator_pid": 999,
                    "creator_name": "old-tool",
                    "session_uuid": "uuid-old"
                }
            }
        });

        let state = migrate(json).unwrap();
        assert_eq!(state.schema_version, 2);
        assert_eq!(state.repo_id, "legacy-repo");
        assert_eq!(state.active_worktrees.len(), 1);
        assert!(state.active_worktrees.contains_key("feat-x"));
        assert!(state.stale_worktrees.is_empty());

        let entry = state.active_worktrees.get("feat-x").unwrap();
        assert_eq!(entry.branch, "feat-x");
        assert_eq!(entry.creator_pid, 999);
    }

    #[test]
    fn test_migrate_unknown_version_returns_error() {
        let json = serde_json::json!({
            "schema_version": 99,
            "repo_id": "future"
        });

        let result = migrate(json);
        assert!(result.is_err());
        match result.unwrap_err() {
            WorktreeError::StateCorrupted { reason } => {
                assert!(reason.contains("unknown schema version 99"));
            }
            other => panic!("expected StateCorrupted, got: {other:?}"),
        }
    }

    #[test]
    fn test_corrupt_json_rebuilds_empty_and_backs_up() {
        let dir = setup_repo();
        ensure_state_dir(dir.path(), None).unwrap();
        let path = state_json_path(dir.path(), None);
        fs::write(&path, "this is not valid json {{{").unwrap();

        let state = read_state(dir.path(), None).expect("corrupt JSON should rebuild empty");
        assert!(state.active_worktrees.is_empty());
        assert_eq!(state.schema_version, 2);

        // Corrupt file moved aside with a .corrupt.<ts> suffix.
        let sd = state_dir(dir.path(), None);
        let moved: Vec<_> = fs::read_dir(&sd)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("state.json.corrupt.")
            })
            .collect();
        assert_eq!(moved.len(), 1, "corrupt backup should exist");
    }

    #[test]
    fn test_unknown_schema_version_still_errors() {
        // Migration failures don't wipe data — we return StateCorrupted
        // so an operator can inspect.
        let dir = setup_repo();
        ensure_state_dir(dir.path(), None).unwrap();
        let path = state_json_path(dir.path(), None);
        fs::write(&path, r#"{"schema_version": 99, "repo_id": "x"}"#).unwrap();
        let result = read_state(dir.path(), None);
        assert!(matches!(result, Err(WorktreeError::StateCorrupted { .. })));
    }

    #[test]
    fn test_with_state_read_modify_write() {
        let dir = setup_repo();

        // First write
        let state = with_state(dir.path(), None, |s| {
            s.active_worktrees.insert(
                "branch-a".to_string(),
                ActiveWorktreeEntry {
                    path: "/tmp/a".to_string(),
                    branch: "branch-a".to_string(),
                    base_commit: "aaa".to_string(),
                    state: WorktreeState::Active,
                    created_at: Utc::now(),
                    last_activity: None,
                    creator_pid: 1,
                    creator_name: "test".to_string(),
                    session_uuid: "uuid-a".to_string(),
                    adapter: None,
                    setup_complete: false,
                    port: None,
                    extra: HashMap::new(),
                },
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(state.active_worktrees.len(), 1);

        // Second modify
        let state = with_state(dir.path(), None, |s| {
            s.active_worktrees.insert(
                "branch-b".to_string(),
                ActiveWorktreeEntry {
                    path: "/tmp/b".to_string(),
                    branch: "branch-b".to_string(),
                    base_commit: "bbb".to_string(),
                    state: WorktreeState::Active,
                    created_at: Utc::now(),
                    last_activity: None,
                    creator_pid: 2,
                    creator_name: "test".to_string(),
                    session_uuid: "uuid-b".to_string(),
                    adapter: None,
                    setup_complete: false,
                    port: None,
                    extra: HashMap::new(),
                },
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(state.active_worktrees.len(), 2);

        // Verify on disk
        let final_state = read_state(dir.path(), None).unwrap();
        assert_eq!(final_state.active_worktrees.len(), 2);
    }

    #[test]
    fn test_config_snapshot_roundtrip() {
        let snap = ConfigSnapshot {
            max_worktrees: 20,
            disk_threshold_percent: 90,
            gc_max_age_days: 7,
            port_range_start: 3100,
            port_range_end: 5100,
            stale_metadata_ttl_days: 30,
            extra: HashMap::new(),
        };

        let json = serde_json::to_string(&snap).unwrap();
        let parsed: ConfigSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_worktrees, 20);
        assert_eq!(parsed.port_range_start, 3100);
    }

    #[test]
    fn test_gc_history_entry_roundtrip() {
        let entry = GcHistoryEntry {
            timestamp: Utc::now(),
            removed: 2,
            evicted: 1,
            freed_mb: 1500,
            extra: HashMap::new(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: GcHistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.removed, 2);
        assert_eq!(parsed.freed_mb, 1500);
    }

    #[test]
    fn test_stale_worktree_entry_roundtrip() {
        let entry = StaleWorktreeEntry {
            original_path: "/tmp/old".to_string(),
            branch: "old-branch".to_string(),
            base_commit: "dead".to_string(),
            creator_name: "bob".to_string(),
            session_uuid: "uuid-stale".to_string(),
            port: Some(3100),
            last_activity: Some(Utc::now()),
            evicted_at: Utc::now(),
            eviction_reason: "gc".to_string(),
            expires_at: Utc::now(),
            extra: HashMap::new(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: StaleWorktreeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.branch, "old-branch");
        assert_eq!(parsed.port, Some(3100));
    }

    #[test]
    fn test_iso_code_home_env_override() {
        let dir = setup_repo();
        let custom = dir.path().join("custom-home");

        // home_override takes precedence
        let sd = state_dir(dir.path(), Some(&custom));
        assert_eq!(sd, custom);
    }

    #[test]
    fn test_full_prd_example_deserializes() {
        // Canonical v2 payload covering every documented field.
        let json = r#"{
            "schema_version": 2,
            "repo_id": "abc123hash",
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
        }"#;

        let state: StateV2 = serde_json::from_str(json).unwrap();
        assert_eq!(state.schema_version, 2);
        assert_eq!(state.active_worktrees.len(), 1);
        assert_eq!(state.stale_worktrees.len(), 1);
        assert_eq!(state.port_leases.len(), 1);
        assert!(state.config_snapshot.is_some());
        assert_eq!(state.gc_history.len(), 1);

        let cs = state.config_snapshot.unwrap();
        assert_eq!(cs.max_worktrees, 20);
        assert_eq!(cs.port_range_start, 3100);
    }
}
