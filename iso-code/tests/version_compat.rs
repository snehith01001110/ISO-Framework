//! Git Version Compatibility Matrix — QA-V-001 through QA-V-009 (pure
//! capability tests).
//!
//! Capability detection is a pure function of the reported `git --version`
//! string, so this file tests `parse_git_version` + `detect_capabilities`
//! directly. The subprocess variants of QA-V-008 and QA-V-009 (that invoke
//! `wt list` with a tweaked `PATH` pointing at `tests/fixtures/mock-git/`)
//! live in `iso-code-cli/tests/version_compat_subprocess.rs` because the
//! `wt` binary is defined there and `assert_cmd::Command::cargo_bin("wt")`
//! only sees binaries declared in the test's own crate.

use iso_code::git::{detect_capabilities, parse_git_version};
use iso_code::types::GitVersion;

/// QA-V-001: git worktree list --porcelain -z (NUL-delimited).
/// Simulated version 2.35 — below the 2.36 threshold. Capability map must
/// report `has_list_nul = false` so the parser falls back to newline mode.
#[test]
fn qa_v_001_list_nul_falls_back_below_236() {
    let v = parse_git_version("git version 2.35.1\n").unwrap();
    let caps = detect_capabilities(&v);
    assert!(!caps.has_list_nul, "2.35 must not advertise -z support");
}

/// QA-V-002: git worktree repair — below 2.30 threshold skips repair.
#[test]
fn qa_v_002_repair_skipped_below_230() {
    let v229 = parse_git_version("git version 2.29.0\n").unwrap();
    assert!(!detect_capabilities(&v229).has_repair);
    let v230 = parse_git_version("git version 2.30.0\n").unwrap();
    assert!(detect_capabilities(&v230).has_repair);
}

/// QA-V-003: git worktree add --orphan — below 2.42 threshold disables
/// orphan branch creation.
#[test]
fn qa_v_003_orphan_branch_requires_242() {
    let v241 = parse_git_version("git version 2.41.0\n").unwrap();
    assert!(!detect_capabilities(&v241).has_orphan);
    let v242 = parse_git_version("git version 2.42.0\n").unwrap();
    assert!(detect_capabilities(&v242).has_orphan);
}

/// QA-V-004: worktree.useRelativePaths — below 2.48 threshold falls back
/// to absolute paths.
#[test]
fn qa_v_004_relative_paths_requires_248() {
    let v247 = parse_git_version("git version 2.47.2\n").unwrap();
    assert!(!detect_capabilities(&v247).has_relative_paths);
    let v248 = parse_git_version("git version 2.48.0\n").unwrap();
    assert!(detect_capabilities(&v248).has_relative_paths);
}

/// QA-V-005: git merge-tree --write-tree requires 2.38+. Conflict detection
/// degrades gracefully when missing.
#[test]
fn qa_v_005_merge_tree_write_requires_238() {
    let v237 = parse_git_version("git version 2.37.4\n").unwrap();
    assert!(!detect_capabilities(&v237).has_merge_tree_write);
    let v238 = parse_git_version("git version 2.38.0\n").unwrap();
    assert!(detect_capabilities(&v238).has_merge_tree_write);
    // Live stub check: `conflict_check` is advertised as `not_implemented`
    // in v1.0 (QA-M-003). That is a stronger contract than "fallback below
    // 2.38" — it holds at every supported git version.
}

/// QA-V-006: locked/prunable fields in porcelain output — below 2.31 the
/// parser simply doesn't see those fields and classifies the worktree as
/// Active, which is the correct fallback.
#[test]
fn qa_v_006_locked_prunable_absent_below_231() {
    // Simulated pre-2.31 porcelain output has no `locked`/`prunable` lines.
    let output = b"worktree /tmp/wt\nHEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nbranch refs/heads/main\n\n";
    let handles = iso_code::git::parse_worktree_list_porcelain(output, false).unwrap();
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].state, iso_code::WorktreeState::Active);
}

/// QA-V-007: `--lock` flag on `worktree add` — semantically, when
/// unavailable the library must fall back to `worktree add` + `worktree
/// lock` as two commands. At the capability level there's no feature flag
/// for this (the flag was added in 2.17 — one minor below our 2.20
/// minimum, so `Manager::new()` wouldn't even start on a git old enough to
/// lack it). We assert the invariant: all supported versions ≥ 2.20
/// include the flag.
#[test]
fn qa_v_007_lock_flag_is_always_present_on_supported_versions() {
    let minimum = GitVersion::MINIMUM;
    let flag_introduced = GitVersion { major: 2, minor: 17, patch: 0 };
    assert!(minimum >= flag_introduced, "--lock flag must be present at our hard minimum");
}

/// QA-V-008: Hard minimum version check — git 2.19 must be refused with a
/// `GitVersionTooOld` error. Tested at the parse/compare layer so we don't
/// need to swap the process's real git.
#[test]
fn qa_v_008_hard_minimum_rejects_219() {
    let v = parse_git_version("git version 2.19.0\n").unwrap();
    assert!(v < GitVersion::MINIMUM);
}

/// QA-V-009 contract shape: `GitNotFound` is a documented error variant
/// that `Manager::new()` returns when `git` is unavailable. Subprocess
/// variant in `iso-code-cli/tests/version_compat_subprocess.rs` exercises
/// the full PATH-manipulation flow.
#[test]
fn qa_v_009_git_not_found_error_variant_exists() {
    let e = iso_code::WorktreeError::GitNotFound;
    let s = format!("{e}");
    assert!(s.contains("git"), "Display: {s}");
}

/// Sanity check: every mock git fixture we ship parses to the version it
/// claims. This guards against fixture drift — if someone edits a script
/// wrong, tests depending on it would lie about the simulated version.
#[cfg(unix)]
#[test]
fn mock_git_fixtures_report_their_tagged_versions() {
    use std::process::Command;
    for (tag, expected) in [
        ("2.19", GitVersion { major: 2, minor: 19, patch: 0 }),
        ("2.20", GitVersion { major: 2, minor: 20, patch: 0 }),
        ("2.30", GitVersion { major: 2, minor: 30, patch: 0 }),
        ("2.35", GitVersion { major: 2, minor: 35, patch: 1 }),
        ("2.37", GitVersion { major: 2, minor: 37, patch: 4 }),
        ("2.41", GitVersion { major: 2, minor: 41, patch: 0 }),
        ("2.47", GitVersion { major: 2, minor: 47, patch: 2 }),
    ] {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mock-git")
            .join(format!("git-{tag}"));
        let real_path = std::env::var("PATH").unwrap_or_default();
        let out = Command::new(&script)
            .arg("--version")
            .env("MOCK_REAL_PATH", &real_path)
            .output()
            .unwrap_or_else(|e| panic!("spawn mock git-{tag}: {e}"));
        assert!(out.status.success(), "mock git-{tag} --version failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let parsed = parse_git_version(&stdout).unwrap();
        assert_eq!(
            parsed, expected,
            "mock git-{tag} reports {parsed:?}, expected {expected:?}"
        );
    }
}
