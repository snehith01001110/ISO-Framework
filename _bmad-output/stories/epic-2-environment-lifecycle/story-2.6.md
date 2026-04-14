# Story 2.6: macOS .DS_Store Handling

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a macOS user, I want `wt delete` to succeed even when Finder has created `.DS_Store` files in my worktree so that worktree cleanup is not blocked by OS-generated metadata.

## Description
On macOS, Finder automatically creates `.DS_Store` files in any directory it opens. These files cause `git worktree remove` to fail because git considers the worktree directory non-empty. This story adds a pre-deletion step that detects and removes `.DS_Store` files from the worktree root and its subdirectories before invoking `git worktree remove`. The fix applies to both `wt delete` and `wt gc`.

## Acceptance Criteria
- [ ] Before calling `git worktree remove`, the library checks for `.DS_Store` files in the worktree root
- [ ] `.DS_Store` files are removed silently before `git worktree remove` is called
- [ ] Removal handles nested `.DS_Store` files (e.g., `worktree/src/.DS_Store`)
- [ ] `wt delete` succeeds on macOS when `.DS_Store` is present in worktree root
- [ ] `wt gc` also removes `.DS_Store` files before cleanup
- [ ] The fix is `#[cfg(target_os = "macos")]` gated -- no behavior change on Linux or Windows
- [ ] If `.DS_Store` removal fails (permission denied), log a warning and attempt `git worktree remove` anyway
- [ ] Integration test passes on macOS CI runner

## Tasks
- [ ] Add `remove_ds_store()` helper function in `src/platform/macos.rs`
- [ ] Walk worktree directory and remove all `.DS_Store` files
- [ ] Call `remove_ds_store()` in `Manager::delete()` before `git worktree remove` (step 6)
- [ ] Call `remove_ds_store()` in `Manager::gc()` before each worktree removal
- [ ] Gate the call with `#[cfg(target_os = "macos")]`
- [ ] Handle permission errors gracefully (log warning, continue)
- [ ] Write test: create worktree, add `.DS_Store` to root, verify `delete()` succeeds
- [ ] Write test: nested `.DS_Store` in subdirectory also removed
- [ ] Write test: permission error on `.DS_Store` logs warning but does not block deletion

## Technical Notes
- `.DS_Store` is a binary file created by macOS Finder to store custom folder attributes (icon positions, view settings).
- `git worktree remove` fails with "fatal: '<path>' contains modified or untracked files" when `.DS_Store` is present.
- The `jwalk` crate (already a dependency) can walk the directory tree efficiently.
- This is simpler than a full untracked-file cleanup -- `.DS_Store` is the only known macOS system file that causes this issue.
- This is an M2 ship criterion (PRD Section 15).

## Test Hints
- Create a temp git repo, add a worktree, write a `.DS_Store` file to its root, call `Manager::delete()`, assert success
- Test with `.DS_Store` in a nested subdirectory (`worktree/src/.DS_Store`)
- On non-macOS platforms, verify the function is not called (compile test via `#[cfg]`)
- Mock a read-only `.DS_Store` to test the permission error path

## Dependencies
- ISO-1.7 (Manager::delete() -- deletion pipeline)
- ISO-1.8 (Manager::gc() -- gc pipeline)

## Estimated Effort
S

## Priority
P1

## Traceability
- PRD: Section 15 M2 (Ship criteria -- macOS .DS_Store test)
- FR: FR-P1-011
- Bug regression: macOS Finder .DS_Store blocking git worktree remove
- QA ref: QA-2.6-001 through QA-2.6-003
