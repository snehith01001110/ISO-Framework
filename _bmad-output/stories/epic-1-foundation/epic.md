# Epic 1: Foundation

## Summary
Establish the `iso-code` Rust workspace, implement all public types (PRD Section 4), the full Manager lifecycle (`create`, `delete`, `list`, `gc`, `attach`), state persistence with hardened locking, port lease allocation, the `wt hook` Claude Code integration, and a skeleton MCP server with 6 tools. This epic delivers a publishable crate on crates.io that prevents the data-loss and orphan-accumulation bugs documented in PRD Section 1.2.

## Goals
- Cargo workspace compiling on macOS and Linux with CI (GitHub Actions), clippy clean.
- All public types from PRD Section 4 defined with correct derives and `#[non_exhaustive]`.
- Git version detection and capability map (`GitCapabilities`) built at `Manager::new()`.
- Porcelain parser for `git worktree list` supporting both NUL-delimited (`-z`, Git 2.36+) and newline-delimited fallback.
- `Manager::create()` with all 12 pre-create safety guards (PRD Section 8.1) and post-create git-crypt check (Section 8.3).
- `Manager::delete()` with the five-step unmerged commit decision tree (PRD Section 8.2.1) and force bypass.
- `Manager::gc()` with dry-run default, locked worktree protection (Appendix A rule 13), and PID-liveness InUse check.
- `Manager::attach()` for registering external worktrees with stale recovery.
- `state.json` v2 schema with fd-lock protocol, Full Jitter backoff, multi-factor identity, and atomic write (tmp -> fsync -> rename).
- Port lease model: allocation, release, renewal, stale cleanup.
- `wt hook --stdin-format claude-code` with exact stdin JSON / stdout one-line path contract.
- MCP server skeleton with 6 tools and correct annotations; `conflict_check` returns `not_implemented`.
- Stress test: 100 create/delete cycles with SIGKILL injection; 1,000-orphan GC test.
- README, LICENSE, docs.rs metadata for crates.io publish.

## Dependencies
None -- this is the first epic.

## Ship Criteria
- `cargo clippy -- -D warnings` clean.
- `cargo test` passes on macOS and Linux.
- Zero data loss in stress test: 100 create/delete cycles with simulated crash injection (SIGKILL at random points).
- `wt gc` successfully cleans orphaned worktrees from a simulated OpenCode failure (1000 orphans, varying ages).
- `wt hook --stdin-format claude-code` produces exactly one line on stdout (the absolute path), nothing else.
- MCP server responds correctly to `worktree_list`, `worktree_create`, `worktree_delete`, `worktree_gc`.
- Crates published: `iso-code`, `iso-code-cli`, `iso-code-mcp`.

## Stories
- ISO-1.1: Workspace Scaffolding
- ISO-1.2: Complete Type System
- ISO-1.3: Git Version Detection
- ISO-1.4: git worktree list Parser
- ISO-1.5: Manager::create() Part 1 -- Pre-Create Guards
- ISO-1.6: Manager::create() Part 2 -- git worktree add and Cleanup
- ISO-1.7: Manager::delete()
- ISO-1.8: Manager::gc()
- ISO-1.9: Manager::attach()
- ISO-1.10: State Persistence
- ISO-1.11: Port Lease Model
- ISO-1.12: wt hook --stdin-format claude-code
- ISO-1.13: MCP Server Skeleton
- ISO-1.14: M1 Stress Test and crates.io Publish Prep

## Duration
Weeks 1-6
