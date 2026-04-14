# Story 1.6: Manager::create() Part 2 -- git worktree add and Cleanup

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want `Manager::create()` to safely invoke `git worktree add`, verify the result with post-create checks, and clean up on any failure so that I never end up with a partially-created or corrupted worktree on disk.

## Description
Implement steps 4-8 of the `Manager::create()` sequence from PRD Section 5.3: invoke `git worktree add`, run post-create git-crypt verification (PRD Section 8.3), optionally run `EcosystemAdapter::setup()`, transition state to Active, and write final state. On any failure after `git worktree add` succeeds, run `git worktree remove --force` before returning the error (Appendix A rule 6). This story connects to the pre-create guards from Story 1.5 and the state persistence from Story 1.10.

## Acceptance Criteria
- [ ] `git worktree add <path> -b <branch> [<base>]` is invoked for new branches
- [ ] `git worktree add <path> <branch>` is invoked for existing branches
- [ ] `--lock` flag is included when `CreateOptions.lock = true` (with optional `--reason`)
- [ ] Post-create git-crypt check runs the four-step detection sequence on the new worktree
- [ ] If git-crypt files are encrypted (magic header present), `git worktree remove --force` is called and `GitCryptLocked` is returned
- [ ] On any failure after `git worktree add` succeeds, `git worktree remove --force <path>` is called before returning error
- [ ] If `git worktree add` returns non-zero, `rm -rf <path>` is called (not `git worktree remove`) per PRD Section 7 rule 7
- [ ] State transitions follow: Pending -> Creating -> Active (or Creating -> Broken on git-crypt failure)
- [ ] `CopyOutcome` is returned alongside `WorktreeHandle`
- [ ] `WorktreeHandle.base_commit` contains the resolved 40-char SHA of the base ref
- [ ] `WorktreeHandle.session_uuid` is a new UUID v4 generated at creation time
- [ ] `WorktreeHandle.creator_pid` is the current process PID
- [ ] `WorktreeHandle.created_at` is an ISO 8601 UTC timestamp

## Tasks
- [ ] Implement git worktree add invocation in `git.rs`: construct command with correct flags based on `CreateOptions`
- [ ] Implement `--lock` and `--reason` flag handling
- [ ] Implement base ref resolution: `git rev-parse <base>` to get the 40-char SHA
- [ ] Implement post-create git-crypt verification per PRD Section 8.3: parse `.gitattributes`, check each `filter=git-crypt` file for `GIT_CRYPT_MAGIC` header
- [ ] Implement cleanup-on-failure: if `git worktree add` fails, call `rm -rf`; if post-create check fails, call `git worktree remove --force`
- [ ] Wire state transitions into the create sequence: write Pending, transition to Creating, then Active
- [ ] Generate `session_uuid` via `uuid::Uuid::new_v4()`
- [ ] Capture `creator_pid` via `std::process::id()`
- [ ] Capture `created_at` via `chrono::Utc::now().to_rfc3339()`
- [ ] Return `(WorktreeHandle, CopyOutcome)` tuple on success
- [ ] Write integration test: create a worktree in a test git repo, verify it appears in `git worktree list`
- [ ] Write test: simulate git-crypt locked state, verify cleanup occurs and `GitCryptLocked` is returned

## Technical Notes
- PRD Section 7.2: exact git commands are `git worktree add <path> -b <branch> [<base>]` (new branch) and `git worktree add <path> <branch>` (existing branch)
- PRD Section 7.2: `git worktree add --lock [--reason <reason>]` for atomic lock (no race window)
- PRD Section 5.3: "On any failure after step 4 (git worktree add) succeeds: Run `git worktree remove --force <path>` before returning error. Never leave a partial worktree on disk."
- PRD Section 7 rule 7: "On git worktree add failure, clean up immediately. Delete any partially-created directory with `rm -rf` (not `git worktree remove`, which may also fail)."
- PRD Section 8.3: `GIT_CRYPT_MAGIC = b"\x00GITCRYPT\x00"` -- first 10 bytes of encrypted files
- Appendix A rule 6: cleanup is mandatory; this is the root fix for claude-code#38538
- State persistence (Story 1.10) must be available for state writes; if not yet implemented, use in-memory state as placeholder

## Test Hints
- QA-R-004: git-crypt repo creates and then removes worktree when files are encrypted
- QA-R-005: partial `git worktree add` failure triggers `rm -rf` cleanup, not `git worktree remove`
- Integration test: create worktree, verify path exists and branch is correct
- Integration test: create worktree with `--lock`, verify it appears as locked in `git worktree list`
- Edge case: base ref that does not exist returns `GitCommandFailed`

## Dependencies
ISO-1.2, ISO-1.3, ISO-1.4, ISO-1.5

## Estimated Effort
XL

## Priority
P0

## Traceability
- PRD: Section 5.3 (Manager::create sequence), Section 7 (Git Interaction Rules), Section 8.3 (git-crypt Detection)
- FR: FR-P0-001 (create lifecycle)
- Appendix A invariant: Rule 1 (shell out), Rule 6 (cleanup on failure), Rule 8 (evict to stale, not silent drop)
- Bug regression: claude-code#38538 (git-crypt corruption)
- QA ref: QA-R-004, QA-R-005
