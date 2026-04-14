# Story 1.7: Manager::delete()

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want `Manager::delete()` to safely remove worktrees only after verifying there are no unmerged commits or uncommitted changes so that I never lose work through cleanup operations.

## Description
Implement the full `Manager::delete()` sequence from PRD Section 5.3: check_not_cwd, check_no_uncommitted_changes (skippable via `force_dirty`), five_step_unmerged_check (skippable via `force`), check_not_locked, state transition to Deleting, `git worktree remove`, state transition to Deleted, port lease release, final state write. The five-step unmerged commit decision tree (PRD Section 8.2.1) replaces the naive single-command check that caused data loss in claude-code#38287.

## Acceptance Criteria
- [ ] `check_not_cwd()` prevents deleting the current working directory, returning `CannotDeleteCwd`
- [ ] `check_no_uncommitted_changes()` runs `git -C <path> status --porcelain` and returns `UncommittedChanges` if output is non-empty
- [ ] `check_no_uncommitted_changes()` is skipped when `DeleteOptions.force_dirty = true`
- [ ] Five-step unmerged commit check runs in order: fetch, merge-base local, merge-base remote, cherry, log
- [ ] Shallow repos skip Steps 2-4 and go directly to Step 5 with a logged warning
- [ ] Primary branch is detected via `git symbolic-ref refs/remotes/origin/HEAD`, falling back to "main" then "master"
- [ ] Network failure on `git fetch` logs a warning and continues (does not block deletion)
- [ ] `git cherry -v` detects squash-merged branches (lines starting with `-` are upstream patches)
- [ ] Five-step check is skipped when `DeleteOptions.force = true`
- [ ] `check_not_locked()` returns `WorktreeLocked` if the worktree state is `Locked`
- [ ] `git worktree remove <path>` is invoked after all checks pass
- [ ] State transitions: Active -> Deleting -> Deleted
- [ ] Port lease is released after successful deletion
- [ ] `stale_worktrees` eviction rule applies: metadata moves to `stale_worktrees`, not silently deleted

## Tasks
- [ ] Implement `check_not_cwd()` in `guards.rs`: compare `dunce::canonicalize(path)` with `dunce::canonicalize(std::env::current_dir())`
- [ ] Implement `check_no_uncommitted_changes()` in `guards.rs`: run `git -C <path> status --porcelain`, return file list
- [ ] Implement `detect_primary_branch()`: run `git symbolic-ref refs/remotes/origin/HEAD`, strip `refs/remotes/origin/`, fallback to "main" then "master"
- [ ] Implement `five_step_unmerged_check()` in `guards.rs` per PRD Section 8.2.1 exactly
- [ ] Step 1: `git fetch --prune origin` -- handle network failure gracefully
- [ ] Step 2: `git merge-base --is-ancestor <branch> <primary>` -- exit 0 = safe, exit 1 = continue, exit 128 = warn + skip
- [ ] Step 3: `git merge-base --is-ancestor <branch> origin/<primary>` -- same exit code handling
- [ ] Step 4: `git cherry -v origin/<primary> <branch>` -- count `+` lines vs `-` lines
- [ ] Step 5: `git log <branch> --not --remotes --oneline` -- count lines = unpushed commits
- [ ] Handle shallow repo precondition: `git rev-parse --is-shallow-repository`, skip Steps 2-4 if true
- [ ] Handle orphan branches: merge-base returns exit 1, Step 5 returns 0 lines = safe to delete
- [ ] Implement `check_not_locked()` in `guards.rs`
- [ ] Implement `git worktree remove <path>` invocation in `git.rs`
- [ ] Wire the full delete sequence in `manager.rs` with state transitions and port release
- [ ] Write comprehensive tests for each step of the decision tree

## Technical Notes
- PRD Section 8.2.1: the five-step check replaces `git branch --merged` which misses squash-merged branches (Appendix A rule 14)
- PRD Section 7.2: exact git commands for each step are specified
- PRD Section 5.3: delete sequence is steps 1-9, must not be reordered
- Appendix A rule 5: "All deletion paths run the five-step unmerged commit check unless force = true"
- Appendix A rule 8: "Entries evicted from active_worktrees go to stale_worktrees -- never silently deleted"
- Edge case: orphan branches (no commits) -- merge-base returns exit 1, Step 5 returns 0 lines, safe to delete
- Edge case: no remote configured -- Steps 1, 3, 4 fail gracefully, Step 5 is final arbiter

## Test Hints
- QA-R-001: delete with unmerged commits returns `UnmergedCommits` error with correct `commit_count`
- QA-R-002: delete with `force = true` skips unmerged check and succeeds
- QA-R-003: delete of squash-merged branch (cherry shows `-` lines only) is safe
- QA-R-006: delete of CWD returns `CannotDeleteCwd`
- Unit test: shallow repo skips Steps 2-4, logs warning
- Unit test: orphan branch is safe to delete
- Unit test: network failure on fetch logs warning, does not block
- Unit test: locked worktree returns `WorktreeLocked`
- Integration test: create then delete a worktree, verify path removed and `git worktree list` no longer shows it

## Dependencies
ISO-1.2, ISO-1.3, ISO-1.4

## Estimated Effort
XL

## Priority
P0

## Traceability
- PRD: Section 5.3 (Manager::delete sequence), Section 8.2 (Pre-Delete Guards), Section 8.2.1 (Five-Step Decision Tree)
- FR: FR-P0-002 (delete lifecycle)
- Appendix A invariant: Rule 5 (five-step check on all deletes), Rule 8 (evict to stale), Rule 14 (never use git branch --merged alone)
- Bug regression: claude-code#38287 (unmerged commits deleted without warning), claude-code#41010 (sub-agent cleanup deleted parent CWD)
- QA ref: QA-R-001, QA-R-002, QA-R-003, QA-R-006
