//! Type-system smoke tests.
//!
//! No QA-* IDs map here directly — these tests guard the PRD Section 4
//! public types (WorktreeHandle, WorktreeState, Config, GcOptions, etc.)
//! against accidental API-level regressions. They're a prerequisite for
//! every other test group in `_bmad-output/qa/test-strategy.md`.

use iso_code::*;
use std::path::PathBuf;

#[test]
fn worktree_handle_instantiates() {
    let _handle = WorktreeHandle::new(
        PathBuf::from("/tmp/test"),
        "feature/test".to_string(),
        "a".repeat(40),
        WorktreeState::Active,
        "2026-01-01T00:00:00Z".to_string(),
        1234,
        "test".to_string(),
        None,
        false,
        None,
        "test-uuid".to_string(),
    );
}

#[test]
fn worktree_state_all_variants() {
    let variants = vec![
        WorktreeState::Pending,
        WorktreeState::Creating,
        WorktreeState::Active,
        WorktreeState::Merging,
        WorktreeState::Deleting,
        WorktreeState::Deleted,
        WorktreeState::Orphaned,
        WorktreeState::Broken,
        WorktreeState::Locked,
    ];
    assert_eq!(variants.len(), 9);
}

#[test]
fn worktree_state_eq_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(WorktreeState::Active);
    set.insert(WorktreeState::Active);
    assert_eq!(set.len(), 1);
    assert_eq!(WorktreeState::Active, WorktreeState::Active);
    assert_ne!(WorktreeState::Active, WorktreeState::Pending);
}

#[test]
fn reflink_mode_default_is_preferred() {
    assert_eq!(ReflinkMode::default(), ReflinkMode::Preferred);
}

#[test]
fn config_defaults_match_prd() {
    let c = Config::default();
    assert_eq!(c.max_worktrees, 20);
    assert_eq!(c.disk_threshold_percent, 90);
    assert_eq!(c.gc_max_age_days, 7);
    assert_eq!(c.port_range_start, 3100);
    assert_eq!(c.port_range_end, 5100);
    assert_eq!(c.min_free_disk_mb, 500);
    assert!(c.home_override.is_none());
    assert!(c.max_total_disk_bytes.is_none());
    assert_eq!(c.circuit_breaker_threshold, 3);
    assert_eq!(c.stale_metadata_ttl_days, 30);
    assert_eq!(c.lock_timeout_ms, 30_000);
    assert_eq!(c.creator_name, "iso-code");
}

#[test]
fn create_options_default() {
    let o = CreateOptions::default();
    assert!(o.base.is_none());
    assert!(!o.setup);
    assert!(!o.ignore_disk_limit);
    assert!(!o.lock);
    assert!(o.lock_reason.is_none());
    assert_eq!(o.reflink_mode, ReflinkMode::Preferred);
    assert!(!o.allocate_port);
}

#[test]
fn delete_options_default() {
    let o = DeleteOptions::default();
    assert!(!o.force);
    assert!(!o.force_dirty);
}

#[test]
fn gc_options_default_dry_run_true() {
    let o = GcOptions::default();
    assert!(o.dry_run);
    assert!(o.max_age_days.is_none());
    assert!(!o.force);
}

#[test]
fn gc_report_instantiates() {
    let _r = GcReport::new(vec![], vec![], vec![], 0, true);
}

#[test]
fn git_version_constants() {
    assert_eq!(GitVersion::MINIMUM, GitVersion { major: 2, minor: 20, patch: 0 });
    assert_eq!(GitVersion::HAS_LIST_NUL, GitVersion { major: 2, minor: 36, patch: 0 });
    assert_eq!(GitVersion::HAS_REPAIR, GitVersion { major: 2, minor: 30, patch: 0 });
    assert_eq!(GitVersion::HAS_MERGE_TREE_WRITE, GitVersion { major: 2, minor: 38, patch: 0 });
}

#[test]
fn git_version_ordering() {
    assert!(GitVersion::MINIMUM < GitVersion::HAS_REPAIR);
    assert!(GitVersion::HAS_REPAIR < GitVersion::HAS_LIST_NUL);
    assert!(GitVersion::HAS_LIST_NUL < GitVersion::HAS_MERGE_TREE_WRITE);
}

#[test]
fn git_capabilities_instantiates() {
    let _c = GitCapabilities::new(
        GitVersion::MINIMUM,
        false,
        false,
        false,
        false,
        false,
    );
}

#[test]
fn port_lease_serde_roundtrip() {
    let lease = PortLease {
        port: 3100,
        branch: "main".to_string(),
        session_uuid: "uuid-1".to_string(),
        pid: 999,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now(),
        status: "active".to_string(),
    };
    let json = serde_json::to_string(&lease).unwrap();
    let _roundtrip: PortLease = serde_json::from_str(&json).unwrap();
}

#[test]
fn copy_outcome_variants() {
    let _a = CopyOutcome::Reflinked;
    let _b = CopyOutcome::StandardCopy { bytes_written: 42 };
    let _c = CopyOutcome::None;
}

#[test]
fn git_crypt_status_variants() {
    let _a = GitCryptStatus::NotUsed;
    let _b = GitCryptStatus::LockedNoKey;
    let _c = GitCryptStatus::Locked;
    let _d = GitCryptStatus::Unlocked;
}

#[test]
fn worktree_error_display() {
    let e = WorktreeError::GitNotFound;
    assert!(format!("{e}").contains("git not found"));

    let e = WorktreeError::GitVersionTooOld {
        required: "2.20".to_string(),
        found: "2.10".to_string(),
    };
    assert!(format!("{e}").contains("2.20"));
}
