# Story 4.8: External Integration

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a maintainer of worktree-core, I want at least one external project (Claude Squad or workmux) consuming worktree-core as a library dependency so that I can validate the API design with real-world usage and demonstrate ecosystem value.

## Description
Integrate worktree-core into at least one external project as a library dependency. The primary target is Claude Squad (Go, using worktree-core via CLI or MCP) or workmux (Rust, using worktree-core as a crate dependency). This validates that the public API is ergonomic, the error types are actionable, and the library solves real problems that these tools have documented (e.g., `claude-squad#260` for environment setup, workmux port collisions). The integration should be contributed upstream as a PR.

## Acceptance Criteria
- [ ] At least one external project has a working integration with worktree-core
- [ ] Integration is functional (not just a compile-time dependency -- actual worktree operations work)
- [ ] Integration submitted as a PR to the external project's repository
- [ ] PR demonstrates solving a documented problem (e.g., `claude-squad#260`, workmux port collisions)
- [ ] Integration uses worktree-core's public API correctly (no private API access)
- [ ] Integration tests pass in the external project's CI
- [ ] README or documentation in the external project references worktree-core

## Tasks
- [ ] Evaluate Claude Squad integration path: CLI wrapper or MCP client
- [ ] Evaluate workmux integration path: Rust crate dependency with feature flag
- [ ] Choose primary integration target based on maintainer responsiveness and technical fit
- [ ] Implement integration: replace ad-hoc worktree management with worktree-core calls
- [ ] Add worktree-core as a dependency in the external project's build system
- [ ] Write integration tests that exercise the worktree-core API in the external project's context
- [ ] Submit PR to the external project with clear description of benefits
- [ ] Address PR review feedback
- [ ] Document the integration pattern for other projects

## Technical Notes
- PRD Section 15 M3 ship criterion: "At least one external project consuming worktree-core as a library dependency."
- PRD Section 18 lists integration targets with their languages and integration paths:
  - Claude Squad (Go): CLI direct + PR hooks (#268/#270). Would use `wt` CLI or MCP.
  - workmux (Rust): Rust crate (preferred). Ideal for library-level integration.
- Claude Squad PRs #268 and #270 are worktree setup hook PRs -- coordinate with maintainers.
- workmux is Rust-native, making it the most natural integration target for a Rust library.
- The integration should be behind an optional feature flag in the external project to avoid forcing the dependency on all users.

## Test Hints
- For workmux: replace workmux's internal worktree creation with `worktree_core::Manager::create()`, run workmux's test suite
- For Claude Squad: configure `wt hook` as the WorktreeCreate hook, verify worktrees are created correctly
- Verify the integration solves a documented bug: create a scenario that triggers the original bug, verify worktree-core prevents it
- Run the external project's existing test suite with the integration enabled

## Dependencies
- ISO-2.1 (EcosystemAdapter trait -- for adapter integration)
- ISO-2.4 (wt create --setup -- for CLI-based integration)
- ISO-1.13 (MCP Server -- for MCP-based integration)
- ISO-3.3 (Single-Pair Conflict Check -- if conflict detection is part of integration)

## Estimated Effort
M

## Priority
P3

## Traceability
- PRD: Section 15 M3 (Ship criteria -- external project consuming library)
- PRD: Section 18 (Integration Targets)
- FR: FR-P3-008
- Bug regression: claude-squad#260 (environment setup), workmux port collisions
- QA ref: QA-4.8-001 through QA-4.8-003
