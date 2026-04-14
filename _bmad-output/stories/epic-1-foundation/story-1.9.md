# Story 1.9: Manager::attach()

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want to register an existing worktree (created outside worktree-core) under management so that it gains port leases, state tracking, and lifecycle protection without being recreated.

## Description
Implement `Manager::attach()` from PRD Section 5.3. This method registers a worktree that already exists in git's registry without calling `git worktree add`. It synthesizes a `WorktreeHandle` from git's porcelain output, optionally runs `EcosystemAdapter::setup()`, and writes the entry to `state.json`. If a `stale_worktrees` entry exists for this path, the original port lease and session_uuid are recovered. Bare repo behavior is permitted per OQ-3 resolution (callers supply explicit absolute paths).

## Acceptance Criteria
- [ ] The worktree path must already appear in `git worktree list --porcelain` output; error if absent
- [ ] `git worktree add` is NOT called -- attach only registers, it does not create
- [ ] A `WorktreeHandle` is synthesized with fields populated from git porcelain output and state metadata
- [ ] `session_uuid` is either recovered from `stale_worktrees` or generated fresh (UUID v4)
- [ ] Port lease is recovered from `stale_worktrees` entry if the path matches
- [ ] If no stale entry exists and port allocation is desired, a new port is allocated
- [ ] `EcosystemAdapter::setup()` runs if `setup = true`
- [ ] State is written to `state.json` under `active_worktrees`
- [ ] Bare repos are permitted -- no error when attaching a worktree into a bare repo
- [ ] Attaching an already-managed worktree (present in `active_worktrees`) is idempotent and returns the existing handle
- [ ] `stale_worktrees` entry is removed after successful recovery

## Tasks
- [ ] Implement `attach()` method on `Manager` in `manager.rs`
- [ ] Verify worktree exists in git registry: call `git worktree list --porcelain`, search for path
- [ ] Extract branch, HEAD SHA, and state annotations from porcelain output for the matched worktree
- [ ] Check `stale_worktrees` for matching path: recover `session_uuid`, `port`, and other metadata
- [ ] If stale match found: remove from `stale_worktrees`, restore port lease to `status: "active"`
- [ ] If no stale match: generate new `session_uuid`, leave port as None (unless explicitly allocated)
- [ ] Handle bare repo: `check_bare_repo()` returns true, adjust path defaults but do not error
- [ ] Handle idempotent attach: if path already in `active_worktrees`, return existing handle
- [ ] Run `EcosystemAdapter::setup()` if `setup = true`
- [ ] Write handle to `state.json` `active_worktrees` with lock
- [ ] Write integration tests for attach with and without stale recovery

## Technical Notes
- PRD Section 5.3: attach preconditions -- "The worktree must already exist in git's registry. Does NOT call git worktree add."
- PRD Section 5.3: "If a stale_worktrees entry exists for this path, recovers its port lease and session_uuid."
- OQ-3: bare repos are permitted; `BareRepositoryUnsupported` was removed in v1.5
- PRD Section 10.4: recovered port lease transitions from `status: "stale"` back to `status: "active"`
- The `stale_worktrees` lookup should match on `original_path` field
- Attach does not run pre-create guards (the worktree already exists)
- If `git worktree repair` is available (Git >= 2.30, `has_repair`), consider running it on the attached worktree to fix broken gitdir links

## Test Hints
- QA-I-001: attach a worktree created manually via `git worktree add`, verify it appears in `Manager::list()`
- QA-I-002: create and delete a worktree (moves to stale), then `git worktree add` again at same path, then attach -- verify port and UUID are recovered
- Integration test: attach on a non-existent path returns error
- Integration test: attach on a path not in git registry returns error
- Integration test: idempotent attach returns same handle
- Unit test: bare repo attach does not error

## Dependencies
ISO-1.2, ISO-1.3, ISO-1.4

## Estimated Effort
M

## Priority
P0

## Traceability
- PRD: Section 5.3 (Manager::attach), Section 10.4 (Port Lease recovery)
- FR: FR-P0-001 (attach lifecycle)
- Appendix A invariant: Rule 2 (porcelain is source of truth), Rule 8 (stale recovery)
- Bug regression: N/A
- QA ref: QA-I-001+
