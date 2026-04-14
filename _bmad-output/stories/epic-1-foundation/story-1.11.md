# Story 1.11: Port Lease Model

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want deterministic, collision-free port allocation for worktrees so that each worktree can run a dev server on a unique port without conflicting with other worktrees or manual processes.

## Description
Implement the port lease model from PRD Section 10.4: deterministic port assignment via SHA-256 hash of `(repo_id, branch)`, sequential probing on collision, 8-hour TTL with renewal every TTL/3, stale lease cleanup, and recovery of port leases from `stale_worktrees` during `attach()`. Ports are leased to a `(branch, session_uuid)` tuple, not to an index, ensuring assignments survive intermediate deletions.

## Acceptance Criteria
- [ ] Preferred port is calculated as `port_range_start + (sha256(repo_id:branch)[0..4] as u32 % range_size)`
- [ ] If preferred port is free (not in `port_leases` or lease expired), it is assigned
- [ ] If preferred port is taken, sequential probing wraps around at `port_range_end` back to `port_range_start`
- [ ] If no free port after full scan, an error is returned
- [ ] Port lease TTL is 8 hours; lease is renewable via `renew_port_lease()` (OQ-1)
- [ ] Expired leases are cleaned up during `Manager::new()` startup sweep
- [ ] When a worktree is deleted, its port lease is released
- [ ] When a worktree is GC'd, its port lease transitions to `status: "stale"`
- [ ] `Manager::attach()` recovers the original port from a matching `stale_worktrees` entry
- [ ] `Manager::allocate_port()` allocates a port without creating a worktree
- [ ] `Manager::release_port()` explicitly releases a port lease
- [ ] `Manager::port_lease()` returns the active lease for a branch
- [ ] Port leases are persisted in `state.json` under `port_leases`
- [ ] `PortLease` struct matches PRD Section 4.9 exactly

## Tasks
- [ ] Implement `compute_preferred_port()` in `ports.rs`: SHA-256 of `format!("{repo_id}:{branch}")`, take first 4 bytes as u32, modulo range size, add to `port_range_start`
- [ ] Implement `allocate_port()`: check preferred port availability, probe sequentially on collision, wrap around
- [ ] Implement `release_port()`: remove lease from `port_leases` in `state.json`
- [ ] Implement `renew_port_lease()`: update `expires_at` to `now + 8 hours`
- [ ] Implement stale lease sweep at startup: for each `"active"` lease, check `kill(pid, 0)` + `start_time`; if dead AND expired, remove
- [ ] Implement port recovery in `attach()`: match `stale_worktrees` entry by path, reclaim port
- [ ] Implement port release in `delete()`: release lease after `git worktree remove` succeeds
- [ ] Implement port transition in `gc()`: move lease to `status: "stale"` when worktree is evicted
- [ ] Wire `CreateOptions.allocate_port` to trigger allocation during `Manager::create()`
- [ ] Store allocated port in `WorktreeHandle.port` field
- [ ] Write collision tests: multiple branches hashing to the same preferred port

## Technical Notes
- PRD Section 10.4: ports are keyed by `(branch, session_uuid)` tuple, not by index
- PRD Section 10.4: hash uses `sha2` crate, exact formula: `sha256(format!("{repo_id}:{branch}"))[0..4]` reinterpreted as `u32` (big-endian)
- PRD Section 10.4: lease lifecycle: 8h TTL, renewed every TTL/3 (~2.5h) during active use
- OQ-1: renewal mechanism -- the PRD does not specify who triggers renewal; implement as an explicit `renew_port_lease()` method that callers invoke
- PRD Section 4.9: `PortLease` has `port`, `branch`, `session_uuid`, `pid`, `created_at`, `expires_at`, `status` fields
- Default port range: 3100-5100 (2000 ports), configurable via `Config`
- Port collision test: create 20 worktrees with branches that hash to the same preferred port, verify all get unique ports

## Test Hints
- Port collision test: 20 worktrees in range 3100-3120 (20-port range) all get unique ports
- Unit test: preferred port calculation is deterministic for same repo_id + branch
- Unit test: different branches get different preferred ports (statistical -- not guaranteed but extremely likely)
- Unit test: expired lease frees the port for reuse
- Unit test: stale lease from evicted worktree is recovered by attach
- Unit test: full range exhaustion returns error
- Integration test: create 5 worktrees with `allocate_port = true`, verify no port collisions

## Dependencies
ISO-1.2, ISO-1.10

## Estimated Effort
L

## Priority
P0

## Traceability
- PRD: Section 10.4 (Port Lease Model), Section 4.9 (PortLease type)
- FR: FR-P0-004 (port allocation)
- Appendix A invariant: Rule 8 (stale recovery preserves port)
- Bug regression: N/A (preventive -- addresses port collision class of bugs)
- QA ref: Port collision test
