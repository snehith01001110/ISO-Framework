# Story 2.5: Port Allocation CLI

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a developer running multiple worktrees simultaneously, I want each worktree to receive a unique port assignment so that dev servers in different worktrees never collide on the same port.

## Description
Expose the port lease model (implemented in Epic 1) through the CLI. `wt create --port` allocates a port lease during creation. `wt status` displays active port assignments alongside worktree information. A critical test verifies that 20 simultaneous worktrees all receive unique ports with zero collisions, validating the hash-based assignment algorithm with sequential probe fallback from PRD Section 10.4.

## Acceptance Criteria
- [ ] `wt create <branch> --port` allocates a port lease and stores it in `state.json`
- [ ] Allocated port is displayed to stderr during creation: `[iso-code] Port allocated: 3142`
- [ ] `wt status` output includes port column showing assigned ports for each worktree
- [ ] `wt status --json` includes `port` field in each worktree entry (null if no port allocated)
- [ ] 20 simultaneous worktrees created with `--port` all receive unique ports with zero collision
- [ ] Port range defaults to 3100-5100 (Config defaults from PRD Section 4.4)
- [ ] When all ports in range are exhausted, `wt create --port` returns a clear error message
- [ ] Port lease is released when worktree is deleted via `wt delete`
- [ ] `ISO_CODE_PORT` environment variable is set correctly for adapters when `--port` is used

## Tasks
- [ ] Add `--port` flag to `wt create` command in clap argument parser
- [ ] Wire `CreateOptions { allocate_port: true }` when `--port` is present
- [ ] Add port column to `wt status` tabular output
- [ ] Add `port` field to `wt status --json` output
- [ ] Add `port` field to `wt list --json` output
- [ ] Write test: single worktree created with `--port` gets a port in valid range
- [ ] Write test: 20 worktrees created with `--port` all get unique ports
- [ ] Write test: port released on `wt delete`
- [ ] Write test: exhausted port range returns clear error
- [ ] Write test: `ISO_CODE_PORT` env var set correctly during adapter setup

## Technical Notes
- PRD Section 10.4 defines the port assignment algorithm: `sha256(repo_id:branch)[0..4] as u32` for preferred port, with sequential probe on collision.
- PRD Section 4.4 Config defaults: `port_range_start: 3100`, `port_range_end: 5100` (2000 ports available).
- Port leases have an 8-hour TTL with renewal every TTL/3 (~2.5 hours).
- The 20-worktree collision test is an M2 ship criterion (PRD Section 15).
- The port lease model implementation is in ISO-1.11; this story exposes it through CLI.

## Test Hints
- 20-worktree collision test: create 20 worktrees in a loop with `--port`, collect all ports, assert `HashSet::len() == 20`
- Verify port is within configured range `[port_range_start, port_range_end)`
- Test port release: create with `--port`, note port, delete, create another worktree, verify the freed port can be reassigned
- Test `wt status` output format includes port column with correct alignment

## Dependencies
- ISO-1.11 (Port Lease Model -- allocation/release logic)
- ISO-1.6 (Manager::create() -- CreateOptions.allocate_port)

## Estimated Effort
M

## Priority
P1

## Traceability
- PRD: Section 10.4 (Port Lease Model)
- PRD: Section 12.1 (CLI Commands -- wt create --port, wt status)
- FR: FR-P1-010
- M2 ship criterion: "Port allocation assigns unique ports to 20 simultaneous worktrees with no collision"
- QA ref: QA-2.5-001 through QA-2.5-004
