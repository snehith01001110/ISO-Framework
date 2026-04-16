//! Pre-create and pre-delete safety guards.
//!
//! Internal functions — not part of the public API. The pre-create guards
//! are ordered: callers must invoke them via `run_pre_create_guards` so the
//! cheap checks short-circuit before any expensive filesystem probes run.

use std::path::Path;
use std::process::Command;

use crate::error::WorktreeError;
use crate::git;
use crate::types::{GitCapabilities, GitCryptStatus, WorktreeHandle, WorktreeState};

/// Guard 1: Branch not already checked out in any worktree.
/// Runs `git worktree list --porcelain` and scans for the branch.
pub(crate) fn check_branch_not_checked_out(
    repo: &Path,
    branch: &str,
    caps: &GitCapabilities,
) -> Result<(), WorktreeError> {
    let worktrees = git::run_worktree_list(repo, caps)?;
    for wt in &worktrees {
        if wt.branch == branch {
            return Err(WorktreeError::BranchAlreadyCheckedOut {
                branch: branch.to_string(),
                worktree: wt.path.clone(),
            });
        }
    }
    Ok(())
}

/// Guard 2: Minimum free disk space.
/// Uses sysinfo disk info on the target path's mount point.
pub(crate) fn check_disk_space(target_path: &Path, required_mb: u64) -> Result<(), WorktreeError> {
    // Use sysinfo to check available disk space
    use sysinfo::Disks;

    let check_path = if target_path.exists() {
        target_path.to_path_buf()
    } else {
        // If target doesn't exist yet, check parent
        target_path
            .parent()
            .unwrap_or(Path::new("/"))
            .to_path_buf()
    };

    let disks = Disks::new_with_refreshed_list();

    // Find the disk containing the path by longest mount point prefix match
    let mut best_match: Option<&sysinfo::Disk> = None;
    let mut best_len = 0;

    for disk in disks.list() {
        let mount = disk.mount_point();
        if check_path.starts_with(mount) {
            let len = mount.as_os_str().len();
            if len > best_len {
                best_len = len;
                best_match = Some(disk);
            }
        }
    }

    if let Some(disk) = best_match {
        let available_mb = disk.available_space() / (1024 * 1024);
        if available_mb < required_mb {
            return Err(WorktreeError::DiskSpaceLow {
                available_mb,
                required_mb,
            });
        }
    }
    // If we can't determine disk space, don't block — be permissive

    Ok(())
}

/// Guard 3: Worktree count limit.
pub(crate) fn check_worktree_count(current: usize, max: usize) -> Result<(), WorktreeError> {
    if current >= max {
        return Err(WorktreeError::RateLimitExceeded { current, max });
    }
    Ok(())
}

/// Guard 4: Target path does not already exist on disk.
pub(crate) fn check_path_not_exists(path: &Path) -> Result<(), WorktreeError> {
    if path.exists() {
        return Err(WorktreeError::WorktreePathExists(path.to_path_buf()));
    }
    Ok(())
}

/// Reject targets that nest inside an existing worktree (or would contain one).
///
/// Uses `dunce::canonicalize` and [`Path::starts_with`] so the comparison is
/// bounded by full path components rather than raw string prefixes. The
/// primary worktree (repo root) is excluded because every new worktree is by
/// definition "inside" it.
pub(crate) fn check_not_nested_worktree(
    candidate: &Path,
    repo_root: &Path,
    existing: &[WorktreeHandle],
) -> Result<(), WorktreeError> {
    // The candidate path doesn't exist yet (guard 4 verified this), so canonicalize
    // will fail. Instead, canonicalize the nearest existing ancestor and re-append
    // the remaining components to get a reliable absolute path for starts_with checks.
    let canon_candidate = if candidate.exists() {
        dunce::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf())
    } else if let Some(parent) = candidate.parent() {
        let canon_parent = dunce::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        if let Some(file_name) = candidate.file_name() {
            canon_parent.join(file_name)
        } else {
            canon_parent
        }
    } else {
        candidate.to_path_buf()
    };

    let canon_repo = dunce::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());

    for wt in existing {
        let canon_existing = dunce::canonicalize(&wt.path).unwrap_or_else(|_| wt.path.clone());

        // Skip the primary worktree (the repo root itself). The primary worktree is
        // always at the repo root and all worktree paths are naturally "inside" it.
        if canon_existing == canon_repo {
            continue;
        }

        // Case 1: New worktree would be inside an existing one.
        if canon_candidate.starts_with(&canon_existing) {
            return Err(WorktreeError::NestedWorktree {
                parent: wt.path.clone(),
            });
        }
        // Case 2: An existing worktree would be inside the new one.
        if canon_existing.starts_with(&canon_candidate) {
            return Err(WorktreeError::NestedWorktree {
                parent: canon_candidate,
            });
        }
    }
    Ok(())
}

/// Guard 6: Not a network filesystem (warning-level, not hard block).
/// On macOS: uses statfs() f_fstypename.
/// On Linux: parses /proc/mounts or uses statfs.
pub(crate) fn check_not_network_filesystem(path: &Path) -> Result<(), WorktreeError> {
    // Platform-specific detection
    #[cfg(target_os = "macos")]
    {
        let path_cstr = std::ffi::CString::new(
            path.to_str().unwrap_or("/"),
        )
        .unwrap_or_else(|_| std::ffi::CString::new("/").unwrap());

        unsafe {
            let mut stat: libc::statfs = std::mem::zeroed();
            if libc::statfs(path_cstr.as_ptr(), &mut stat) == 0 {
                let fstype = std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr())
                    .to_string_lossy();
                let network_types = ["nfs", "smbfs", "afpfs", "cifs", "webdav"];
                if network_types.iter().any(|t| fstype.eq_ignore_ascii_case(t)) {
                    return Err(WorktreeError::NetworkFilesystem {
                        mount_point: path.to_path_buf(),
                    });
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Parse /proc/mounts for network filesystem types
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            let path_str = path.to_string_lossy();
            let network_types = ["nfs", "nfs4", "cifs", "smbfs", "fuse.sshfs", "9p"];
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let mount_point = parts[1];
                    let fs_type = parts[2];
                    if path_str.starts_with(mount_point)
                        && network_types.contains(&fs_type)
                    {
                        return Err(WorktreeError::NetworkFilesystem {
                            mount_point: std::path::PathBuf::from(mount_point),
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

/// Guard 7: Not crossing WSL/Windows filesystem boundary.
/// Detects WSL via /proc/version containing "Microsoft".
pub(crate) fn check_not_wsl_cross_boundary(
    repo: &Path,
    worktree: &Path,
) -> Result<(), WorktreeError> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(version) = std::fs::read_to_string("/proc/version") {
            if version.contains("Microsoft") || version.contains("microsoft") {
                let repo_on_mnt = repo.starts_with("/mnt/");
                let wt_on_mnt = worktree.starts_with("/mnt/");
                if repo_on_mnt != wt_on_mnt {
                    return Err(WorktreeError::WslCrossBoundary);
                }
            }
        }
    }

    // Not WSL on non-Linux platforms
    let _ = (repo, worktree);
    Ok(())
}

/// Guard 8: Bare repository detection.
/// Runs `git rev-parse --is-bare-repository`.
/// Returns true if bare; caller adjusts path defaults.
pub(crate) fn check_bare_repo(repo: &Path) -> Result<bool, WorktreeError> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-bare-repository"])
        .current_dir(repo)
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "true")
}

/// Guard 9: Submodule context detection.
/// Runs `git rev-parse --show-superproject-working-tree`.
/// Returns true if inside a submodule.
pub(crate) fn check_submodule_context(repo: &Path) -> Result<bool, WorktreeError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-superproject-working-tree"])
        .current_dir(repo)
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Ok(false);
    }

    // If output is non-empty, we're inside a submodule
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

/// Guard 10: Aggregate disk usage check.
///
/// Evaluates up to two independent limits:
///   * `max_bytes` — hard cap on aggregate worktree bytes (Config.max_total_disk_bytes).
///   * `threshold_percent` — refuse if aggregate usage exceeds this percentage
///     of the filesystem capacity hosting `target_path` (Config.disk_threshold_percent).
pub(crate) fn check_total_disk_usage(
    worktrees: &[WorktreeHandle],
    target_path: &Path,
    max_bytes: Option<u64>,
    threshold_percent: Option<u8>,
) -> Result<(), WorktreeError> {
    if max_bytes.is_none() && threshold_percent.is_none() {
        return Ok(());
    }

    let total_bytes = crate::util::dir_size_skipping_git(worktrees.iter().map(|wt| wt.path.as_path()));

    if let Some(limit) = max_bytes {
        if total_bytes > limit {
            return Err(WorktreeError::AggregateDiskLimitExceeded);
        }
    }

    if let Some(pct) = threshold_percent {
        if let Some(capacity) = crate::util::filesystem_capacity_bytes(target_path) {
            if capacity > 0 {
                let limit = capacity.saturating_mul(u64::from(pct)) / 100;
                if total_bytes > limit {
                    return Err(WorktreeError::AggregateDiskLimitExceeded);
                }
            }
        }
    }

    Ok(())
}

/// Guard 11 (Windows only): Junction target is not a network path.
#[cfg(target_os = "windows")]
pub(crate) fn check_not_network_junction_target(path: &Path) -> Result<(), WorktreeError> {
    let path_str = path.to_string_lossy();
    // Network paths start with \\ but not \\?\
    if path_str.starts_with("\\\\") && !path_str.starts_with("\\\\?\\") {
        return Err(WorktreeError::NetworkJunctionTarget {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// Guard 12: git-crypt pre-create check.
/// Parses .gitattributes for `filter=git-crypt` patterns.
pub(crate) fn check_git_crypt_pre_create(repo: &Path) -> Result<GitCryptStatus, WorktreeError> {
    let gitattributes = repo.join(".gitattributes");
    if !gitattributes.exists() {
        return Ok(GitCryptStatus::NotUsed);
    }

    let content = std::fs::read_to_string(&gitattributes).map_err(WorktreeError::Io)?;

    let has_git_crypt = content
        .lines()
        .any(|line| line.contains("filter=git-crypt"));

    if !has_git_crypt {
        return Ok(GitCryptStatus::NotUsed);
    }

    // Check for key file
    let git_dir_output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(repo)
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    let git_dir = repo.join(String::from_utf8_lossy(&git_dir_output.stdout).trim());
    let key_file = git_dir.join("git-crypt").join("keys").join("default");

    if !key_file.exists() {
        return Ok(GitCryptStatus::LockedNoKey);
    }

    // Check if any git-crypt files are still encrypted by reading their headers
    const GIT_CRYPT_MAGIC: &[u8; 10] = b"\x00GITCRYPT\x00";

    // Find files with git-crypt filter
    for line in content.lines() {
        if !line.contains("filter=git-crypt") {
            continue;
        }
        // Extract the pattern (first field before any attributes)
        let pattern = line.split_whitespace().next().unwrap_or("");
        if pattern.is_empty() {
            continue;
        }

        // Use git ls-files to find matching files
        let ls_output = Command::new("git")
            .args(["ls-files", "--", pattern])
            .current_dir(repo)
            .output();

        if let Ok(ls) = ls_output {
            for file_path in String::from_utf8_lossy(&ls.stdout).lines() {
                let full_path = repo.join(file_path);
                if full_path.exists() {
                    if let Ok(true) = git::is_encrypted(&full_path, GIT_CRYPT_MAGIC) {
                        return Ok(GitCryptStatus::Locked);
                    }
                }
            }
        }
    }

    Ok(GitCryptStatus::Unlocked)
}


/// Arguments for `run_pre_create_guards`. Keeps the guard runner readable
/// and maps directly onto Config / CreateOptions fields so wiring new knobs
/// doesn't require touching every call site.
pub(crate) struct PreCreateArgs<'a> {
    pub repo: &'a Path,
    pub branch: &'a str,
    pub target_path: &'a Path,
    pub caps: &'a GitCapabilities,
    pub existing_worktrees: &'a [WorktreeHandle],
    pub max_worktrees: usize,
    pub min_free_disk_mb: u64,
    pub max_total_disk_bytes: Option<u64>,
    /// When true, Guard 10 (aggregate disk limits) is skipped entirely.
    pub ignore_disk_limit: bool,
    /// When set, Guard 10 additionally enforces a percentage-of-filesystem cap.
    pub disk_threshold_percent: Option<u8>,
}

/// Run every pre-create guard in order. The ordering is load-bearing: later
/// guards assume invariants established by earlier ones. Returns the detected
/// [`GitCryptStatus`] so callers can decide whether to proceed.
pub(crate) fn run_pre_create_guards(args: PreCreateArgs<'_>) -> Result<GitCryptStatus, WorktreeError> {
    // 1. Branch not already checked out
    check_branch_not_checked_out(args.repo, args.branch, args.caps)?;

    // 2. Minimum free disk space
    check_disk_space(args.target_path, args.min_free_disk_mb)?;

    // 3. Worktree count limit.
    // Count every worktree that still occupies a slot on disk or in git's
    // registry. Locked worktrees absolutely count (they hold resources and
    // can't be evicted by gc). Orphaned/Broken/Deleted are about to be
    // reaped or are already gone, so they don't block new creation.
    let active_count = args
        .existing_worktrees
        .iter()
        .filter(|wt| {
            !matches!(
                wt.state,
                WorktreeState::Orphaned | WorktreeState::Broken | WorktreeState::Deleted
            )
        })
        .count();
    check_worktree_count(active_count, args.max_worktrees)?;

    // 4. Target path does not already exist
    check_path_not_exists(args.target_path)?;

    // 5. Not nested inside existing worktree (bidirectional)
    check_not_nested_worktree(args.target_path, args.repo, args.existing_worktrees)?;

    // 6. Not a network filesystem (warning-level)
    if let Err(e) = check_not_network_filesystem(args.target_path) {
        eprintln!("WARNING: {e}");
        // Network-FS placement is discouraged but not fatal — lock semantics
        // and rename atomicity may degrade, so we warn and continue.
    }

    // 7. Not crossing WSL/Windows boundary
    check_not_wsl_cross_boundary(args.repo, args.target_path)?;

    // 8. Bare repo detection (adjusts behavior, doesn't block)
    let _is_bare = check_bare_repo(args.repo)?;

    // 9. Submodule context
    if check_submodule_context(args.repo)? {
        return Err(WorktreeError::SubmoduleContext);
    }

    // 10. Aggregate disk usage (opt-out via CreateOptions.ignore_disk_limit)
    if !args.ignore_disk_limit {
        check_total_disk_usage(
            args.existing_worktrees,
            args.target_path,
            args.max_total_disk_bytes,
            args.disk_threshold_percent,
        )?;
    }

    // 11. Windows junction target check (Windows only)
    #[cfg(target_os = "windows")]
    check_not_network_junction_target(args.target_path)?;

    // 12. git-crypt pre-create check
    let crypt_status = check_git_crypt_pre_create(args.repo)?;

    Ok(crypt_status)
}

// ── Pre-Delete Guards ──────────────────────────────────────────────────

/// Reject deletions that would unmount the caller's current working directory
/// or any of its ancestors — doing so would leave the calling shell stranded
/// in a directory that no longer exists.
pub(crate) fn check_not_cwd(path: &Path) -> Result<(), WorktreeError> {
    if let Ok(cwd) = std::env::current_dir() {
        let canon_path = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let canon_cwd = dunce::canonicalize(&cwd).unwrap_or(cwd);
        // Block if path IS the CWD, or if CWD is inside the path (path is a parent of CWD).
        if canon_cwd.starts_with(&canon_path) {
            return Err(WorktreeError::CannotDeleteCwd);
        }
    }
    Ok(())
}

/// Pre-delete guard 2: No uncommitted changes.
/// Runs `git -C <path> status --porcelain`.
pub(crate) fn check_no_uncommitted_changes(path: &Path) -> Result<(), WorktreeError> {
    let output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "status", "--porcelain"])
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Ok(()); // If status fails, don't block
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    if !files.is_empty() {
        return Err(WorktreeError::UncommittedChanges { files });
    }

    Ok(())
}

/// Detect the primary branch name.
/// Runs `git symbolic-ref refs/remotes/origin/HEAD`, strips prefix.
/// Falls back to "main" then "master".
fn detect_primary_branch(repo: &Path) -> String {
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(repo)
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let trimmed = stdout.trim();
            // Strip refs/remotes/origin/ prefix
            if let Some(branch) = trimmed.strip_prefix("refs/remotes/origin/") {
                return branch.to_string();
            }
        }
    }

    // Fallback: check if "main" exists as a local branch, else "master"
    let check_main = Command::new("git")
        .args(["rev-parse", "--verify", "refs/heads/main"])
        .current_dir(repo)
        .output();
    if let Ok(out) = check_main {
        if out.status.success() {
            return "main".to_string();
        }
    }

    "master".to_string()
}

/// Check if the repository is shallow.
fn is_shallow_repo(repo: &Path) -> bool {
    let output = Command::new("git")
        .args(["rev-parse", "--is-shallow-repository"])
        .current_dir(repo)
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim() == "true";
        }
    }
    false
}

/// Five-step unmerged-commit decision tree used before delete.
///
/// Classifies the branch against its upstream and integration branches to
/// decide whether the worktree holds work that would be lost by deletion.
/// Skipped entirely when `DeleteOptions::force` is true.
///
/// When `offline` is true, step 1 (`git fetch --prune origin`) is skipped
/// so deletes and gc don't stall on network I/O. Steps 2–5 still run against
/// whatever refs are already local.
pub(crate) fn five_step_unmerged_check(
    branch: &str,
    repo: &Path,
    offline: bool,
) -> Result<(), WorktreeError> {
    let shallow = is_shallow_repo(repo);
    let primary = detect_primary_branch(repo);

    // Step 1: git fetch --prune origin (skipped when offline).
    if !offline {
        let fetch_result = Command::new("git")
            .args(["fetch", "--prune", "origin"])
            .current_dir(repo)
            .output();
        match fetch_result {
            Ok(out) if !out.status.success() => {
                eprintln!("WARNING: fetch failed, continuing with local refs only");
            }
            Err(_) => {
                eprintln!("WARNING: fetch failed, continuing with local refs only");
            }
            _ => {}
        }
    }

    if shallow {
        eprintln!("WARNING: shallow repo detected — remote ancestor checks skipped");
        // Skip Steps 2-4, go directly to Step 5
    } else {
        // Step 2: git merge-base --is-ancestor <branch> <primary_branch>
        let step2 = Command::new("git")
            .args(["merge-base", "--is-ancestor", branch, &primary])
            .current_dir(repo)
            .output();
        if let Ok(out) = step2 {
            match out.status.code() {
                Some(0) => return Ok(()), // SAFE TO DELETE
                Some(1) => {}             // not merged locally, continue
                _ => {
                    eprintln!("WARNING: merge-base local check returned unexpected exit code");
                }
            }
        }

        // Step 3: git merge-base --is-ancestor <branch> origin/<primary_branch>
        let remote_primary = format!("origin/{primary}");
        let step3 = Command::new("git")
            .args(["merge-base", "--is-ancestor", branch, &remote_primary])
            .current_dir(repo)
            .output();
        if let Ok(out) = step3 {
            match out.status.code() {
                Some(0) => return Ok(()), // SAFE TO DELETE
                Some(1) => {}             // not merged into remote, continue
                _ => {}                   // no remote exists, continue
            }
        }

        // Step 4: git cherry -v origin/<primary_branch> <branch>
        let step4 = Command::new("git")
            .args(["cherry", "-v", &remote_primary, branch])
            .current_dir(repo)
            .output();
        if let Ok(out) = step4 {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let has_plus_lines = stdout.lines().any(|l| l.starts_with("+ ") || l.starts_with('+'));
                if !has_plus_lines {
                    // All patches are upstream (only '-' lines or empty)
                    return Ok(());
                }
                // '+' lines present → unique commits remain → continue to Step 5
            }
            // Command fails (no remote) → continue to Step 5
        }
    }

    // Step 5: git log <branch> --not --remotes --oneline
    let step5 = Command::new("git")
        .args(["log", branch, "--not", "--remotes", "--oneline"])
        .current_dir(repo)
        .output();

    if let Ok(out) = step5 {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let commit_count = stdout.lines().filter(|l| !l.is_empty()).count();
            if commit_count == 0 {
                return Ok(()); // SAFE TO DELETE
            }
            return Err(WorktreeError::UnmergedCommits {
                branch: branch.to_string(),
                commit_count,
            });
        }
    }

    // If step 5 fails entirely, don't block deletion
    Ok(())
}

/// Pre-delete guard 3: Worktree not locked.
pub(crate) fn check_not_locked(handle: &WorktreeHandle) -> Result<(), WorktreeError> {
    if handle.state == WorktreeState::Locked {
        return Err(WorktreeError::WorktreeLocked { reason: None });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_check_worktree_count_under_limit() {
        assert!(check_worktree_count(5, 20).is_ok());
    }

    #[test]
    fn test_check_worktree_count_at_limit() {
        let result = check_worktree_count(20, 20);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorktreeError::RateLimitExceeded { current: 20, max: 20 }
        ));
    }

    #[test]
    fn test_check_path_not_exists_ok() {
        let path = PathBuf::from("/tmp/definitely_not_exists_iso_test_1234567890");
        assert!(check_path_not_exists(&path).is_ok());
    }

    #[test]
    fn test_check_path_not_exists_fails() {
        let path = PathBuf::from("/tmp");
        let result = check_path_not_exists(&path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorktreeError::WorktreePathExists(_)));
    }

    #[test]
    fn test_check_not_nested_no_worktrees() {
        let result = check_not_nested_worktree(Path::new("/tmp/test"), Path::new("/some/repo"), &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_not_nested_candidate_inside_existing() {
        // Use a real existing directory as the "existing worktree"
        let base = dunce::canonicalize(std::env::temp_dir()).unwrap();
        let existing = vec![WorktreeHandle::new(
            base.clone(),
            "main".to_string(),
            String::new(),
            WorktreeState::Active,
            String::new(),
            0,
            String::new(),
            None,
            false,
            None,
            String::new(),
        )];
        let candidate = base.join("nested").join("wt");
        // Use a repo_root that differs from the existing worktree so it's not skipped
        let result = check_not_nested_worktree(&candidate, Path::new("/some/other/repo"), &existing);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorktreeError::NestedWorktree { .. }
        ));
    }

    #[test]
    fn test_check_bare_repo_not_bare() {
        // Run against this project's repo — it's not bare
        let result = check_bare_repo(Path::new("."));
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_check_submodule_not_submodule() {
        // This project is not a submodule
        let result = check_submodule_context(Path::new("."));
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_check_disk_space_permissive() {
        // Should pass for reasonable amounts on /tmp
        let result = check_disk_space(Path::new("/tmp"), 1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_disk_space_huge_requirement() {
        // 999 TB requirement should fail
        let result = check_disk_space(Path::new("/tmp"), 999_000_000);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorktreeError::DiskSpaceLow { .. }));
    }

    #[test]
    fn test_check_git_crypt_not_used() {
        // This repo doesn't use git-crypt
        let result = check_git_crypt_pre_create(Path::new("."));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), GitCryptStatus::NotUsed);
    }

    #[test]
    fn test_check_not_cwd_different_path() {
        let result = check_not_cwd(Path::new("/tmp/definitely_not_cwd_12345"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_not_locked_active() {
        let handle = WorktreeHandle::new(
            PathBuf::from("/tmp/wt"),
            "test".to_string(),
            String::new(),
            WorktreeState::Active,
            String::new(),
            0,
            String::new(),
            None,
            false,
            None,
            String::new(),
        );
        assert!(check_not_locked(&handle).is_ok());
    }

    #[test]
    fn test_check_not_locked_locked() {
        let handle = WorktreeHandle::new(
            PathBuf::from("/tmp/wt"),
            "test".to_string(),
            String::new(),
            WorktreeState::Locked,
            String::new(),
            0,
            String::new(),
            None,
            false,
            None,
            String::new(),
        );
        let result = check_not_locked(&handle);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorktreeError::WorktreeLocked { .. }));
    }

    #[test]
    fn test_check_total_disk_usage_no_limit() {
        let result = check_total_disk_usage(&[], Path::new("/tmp"), None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_branch_not_checked_out_ok() {
        // A branch that definitely doesn't exist
        let caps = git::detect_git_version().unwrap();
        let result = check_branch_not_checked_out(
            Path::new("."),
            "definitely-nonexistent-branch-xyz-123",
            &caps,
        );
        assert!(result.is_ok());
    }
}
