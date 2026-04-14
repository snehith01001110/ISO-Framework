# Story 2.8: wt attach Port Recovery

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a developer, I want `wt attach` to recover the original port assignment and session UUID when re-attaching a previously managed worktree so that my development environment stays consistent after detach/reattach cycles.

## Description
When a worktree is evicted from `active_worktrees` to `stale_worktrees` (e.g., via `wt gc` or reconciliation), its port lease and session UUID are preserved in the stale entry. When `wt attach` is called on a path that matches a `stale_worktrees` entry, the original port and session_uuid are recovered instead of generating new ones. This ensures port stability across worktree lifecycle events -- a dev server configured for port 3142 continues to use port 3142 after reattachment.

## Acceptance Criteria
- [ ] `wt attach <path>` on a path matching a `stale_worktrees` entry recovers the original `port` value
- [ ] `wt attach <path>` on a path matching a `stale_worktrees` entry recovers the original `session_uuid`
- [ ] The recovered port lease is moved from stale status back to active status in `port_leases`
- [ ] The `stale_worktrees` entry is removed after successful recovery
- [ ] The recovered worktree entry is added to `active_worktrees` with the original metadata
- [ ] If the original port is now taken by another worktree, a new port is allocated and a warning is logged
- [ ] `wt attach` on a path with no stale entry works normally (allocates fresh UUID, no port unless `--setup` requests it)
- [ ] Port recovery is reflected in `wt status` output after attach

## Tasks
- [ ] In `Manager::attach()`, look up `stale_worktrees` by path before creating new metadata
- [ ] If stale entry found, recover `port` and `session_uuid` fields
- [ ] Move the port lease from `status: "stale"` to `status: "active"` in `port_leases`
- [ ] Remove the stale entry from `stale_worktrees` after successful recovery
- [ ] Handle port conflict: if recovered port is now taken, allocate a new port with `allocate_port()`
- [ ] Write test: evict worktree via gc, re-attach, verify port matches original
- [ ] Write test: evict worktree, assign its port to another worktree, re-attach, verify new port allocated with warning
- [ ] Write test: attach with no stale entry generates fresh UUID

## Technical Notes
- PRD Section 5.3: `Manager::attach()` -- "If a stale_worktrees entry exists for this path, recovers its port lease and session_uuid."
- PRD Section 10.4: Port lease lifecycle -- "Recovery: `wt attach` on a path matching a stale entry reclaims the original port."
- PRD Section 10.3: Reconciliation policy moves evicted entries to `stale_worktrees` with `expires_at`.
- The stale entry lookup should match on `original_path` field after `dunce::canonicalize()` normalization.
- This is an M2 ship criterion (PRD Section 15).

## Test Hints
- Full lifecycle test: create with port -> gc (evicts to stale) -> attach -> verify port matches
- Port conflict test: create A with port 3142, gc A, create B with port 3142, attach A -> verify A gets new port
- Verify `session_uuid` is recovered (not regenerated) by comparing before-eviction and after-attach values
- Verify stale entry is removed from `state.json` after attach

## Dependencies
- ISO-1.9 (Manager::attach() -- base implementation)
- ISO-1.11 (Port Lease Model -- stale lease handling)
- ISO-1.10 (State Persistence -- stale_worktrees schema)

## Estimated Effort
S

## Priority
P1

## Traceability
- PRD: Section 5.3 (Manager::attach())
- PRD: Section 10.4 (Port Lease Model -- Recovery)
- FR: FR-P1-013
- M2 ship criterion: "wt attach on stale_worktrees entry recovers original port"
- QA ref: QA-2.8-001 through QA-2.8-003
