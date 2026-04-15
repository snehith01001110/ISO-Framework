# Story 1.5: Manager::create() Part 1 -- Pre-Create Guards

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want `Manager::create()` to reject unsafe worktree creation attempts with clear error messages so that I never accidentally create worktrees that would cause data loss, disk exhaustion, or repository corruption.

## Description
Implement all 12 pre-create safety guards from PRD Section 8.1 in `iso-code/src/guards.rs`. These guards run in exact order inside `Manager::create()` before `git worktree add` is called. Each guard returns a specific `WorktreeError` variant. This story covers steps 1-3 of the create sequence (PRD Section 5.3): run guards, write Pending entry, transition to Creating. It does NOT invoke git.

## Acceptance Criteria
- [ ] Guard 1: `check_branch_not_checked_out` returns `BranchAlreadyCheckedOut` when the branch is already in use
- [ ] Guard 2: `check_disk_space` returns `DiskSpaceLow` when free disk is below `Config.min_free_disk_mb`
- [ ] Guard 3: `check_worktree_count` returns `RateLimitExceeded` when count >= `Config.max_worktrees`
- [ ] Guard 4: `check_path_not_exists` returns `WorktreePathExists` when the target path already exists on disk
- [ ] Guard 5: `check_not_nested_worktree` returns `NestedWorktree` when candidate is inside existing worktree or vice versa
- [ ] Guard 6: `check_not_network_filesystem` returns `NetworkFilesystem` (warning-level, not hard block by default)
- [ ] Guard 7: `check_not_wsl_cross_boundary` returns `WslCrossBoundary` when crossing WSL/Windows boundary
- [ ] Guard 8: `check_bare_repo` detects bare repos and adjusts path defaults (does not block)
- [ ] Guard 9: `check_submodule_context` returns `SubmoduleContext` when inside a submodule
- [ ] Guard 10: `check_total_disk_usage` returns `AggregateDiskLimitExceeded` when total usage exceeds `Config.max_total_disk_bytes`
- [ ] Guard 11 (Windows): `check_not_network_junction_target` returns `NetworkJunctionTarget` for UNC paths
- [ ] Guard 12: `check_git_crypt_pre_create` detects git-crypt configuration status
- [ ] Guards execute in the exact order listed in PRD Section 8.1 (1 through 12)
- [ ] Each guard is a standalone function in `guards.rs`, not part of the public API

## Tasks
- [ ] Implement `check_branch_not_checked_out()`: invoke `git worktree list --porcelain`, scan for `branch refs/heads/<branch>`
- [ ] Implement `check_disk_space()`: use `statvfs()` on Unix; stub for Windows
- [ ] Implement `check_worktree_count()`: compare current count against `Config.max_worktrees`
- [ ] Implement `check_path_not_exists()`: `Path::exists()` check on target
- [ ] Implement `check_not_nested_worktree()`: canonicalize via `dunce::canonicalize` then bidirectional `starts_with` check per PRD Section 8.4
- [ ] Implement `check_not_network_filesystem()`: parse `/proc/mounts` on Linux, `statfs()` f_fstypename on macOS
- [ ] Implement `check_not_wsl_cross_boundary()`: detect WSL via `/proc/version`, check `/mnt/*` crossing
- [ ] Implement `check_bare_repo()`: run `git rev-parse --is-bare-repository`
- [ ] Implement `check_submodule_context()`: run `git rev-parse --show-superproject-working-tree`
- [ ] Implement `check_total_disk_usage()`: use `jwalk` + `filesize` to walk all worktree directories, skip `.git/`
- [ ] Implement `check_not_network_junction_target()` gated with `#[cfg(target_os = "windows")]`
- [ ] Implement `check_git_crypt_pre_create()`: parse `.gitattributes` for `filter=git-crypt`, check key file existence
- [ ] Write orchestrating function `run_pre_create_guards()` that calls all guards in order
- [ ] Write unit tests for each guard independently using mock filesystem and git output

## Technical Notes
- PRD Section 8.4: `check_not_nested_worktree` must use `dunce::canonicalize` for case-insensitive filesystem correctness; `Path::starts_with` checks component boundaries, not string prefixes
- PRD Section 8.1: guard 6 is a warning, not a hard error by default (OQ-2 unresolved)
- PRD Section 11.4: disk usage calculation uses `jwalk` with `preload_metadata(true)`, deduplicates hardlinks via `HashSet<(dev_t, ino_t)>` on Unix, skips `.git/` directory
- PRD Section 7.2: `git rev-parse --is-bare-repository` and `git rev-parse --show-superproject-working-tree` are the exact commands
- PRD Section 8.3: `GitCryptStatus` enum with four variants; detection uses `.gitattributes` parsing and `GIT_CRYPT_MAGIC` header byte check
- Appendix A rule 1: all git invocations via `std::process::Command`

## Test Hints
- QA-G-001: branch already checked out -> BranchAlreadyCheckedOut error
- QA-G-002: disk space below threshold -> DiskSpaceLow error
- QA-G-003: worktree count at maximum -> RateLimitExceeded error
- QA-G-004: target path exists -> WorktreePathExists error
- QA-G-005: nested path (candidate inside existing) -> NestedWorktree error
- QA-G-006: nested path (existing inside candidate) -> NestedWorktree error
- QA-G-007: network filesystem -> NetworkFilesystem warning
- QA-G-008: WSL cross-boundary -> WslCrossBoundary error
- QA-G-009: submodule context -> SubmoduleContext error
- QA-G-010: aggregate disk exceeds limit -> AggregateDiskLimitExceeded error
- QA-G-011: bare repo detected -> returns true, no error
- QA-G-012: git-crypt locked -> GitCryptLocked status detected

## Dependencies
ISO-1.2, ISO-1.3, ISO-1.4

## Estimated Effort
XL

## Priority
P0

## Traceability
- PRD: Section 8.1 (Pre-Create Guards), Section 8.3 (git-crypt Detection), Section 8.4 (Nested Worktree)
- FR: FR-P0-001 through FR-P0-004 (safety guards)
- Appendix A invariant: Rule 1 (shell out), Rule 12 (non_exhaustive)
- Bug regression: claude-code#27881 (nested worktree creation)
- QA ref: QA-G-001 through QA-G-012
