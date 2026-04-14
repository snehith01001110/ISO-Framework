# Epic 2: Environment Lifecycle

## Summary
Implement the `EcosystemAdapter` trait and its two built-in adapters (`DefaultAdapter`, `ShellCommandAdapter`), expose port allocation through the CLI, harden `wt attach` recovery from stale worktrees, and resolve cross-platform cleanup issues (macOS `.DS_Store`, Windows `MAX_PATH`). This epic turns `worktree-core` from a bare lifecycle manager into a tool that bootstraps fully working development environments on creation and tears them down cleanly on deletion.

## Goals
- `EcosystemAdapter` trait defined exactly per PRD Section 6 with `detect()`, `setup()`, `teardown()`, and default `branch_name()`.
- `DefaultAdapter` copies files from a configurable list (`.env`, `.env.local`, `config/local.toml`) into new worktrees.
- `ShellCommandAdapter` executes arbitrary shell commands at create/delete time with all `WORKTREE_CORE_*` environment variables injected.
- `wt create --setup` integrates adapter selection from config; all adapter output goes to stderr only.
- `wt create --port` allocates a port lease; `wt status` displays active port assignments.
- 20 simultaneous worktrees receive unique ports with zero collision.
- macOS `.DS_Store` files detected and removed before `git worktree remove` to prevent deletion failures.
- Windows `MAX_PATH` handled via `dunce::simplified()` before passing paths to external tools.
- `wt attach` on a path matching a `stale_worktrees` entry recovers the original port and `session_uuid`.
- MCP server README includes correct config snippets for Claude Code, Cursor, VS Code Copilot, and OpenCode.
- M2 integration test suite covering the full adapter-to-cleanup pipeline.

## Dependencies
Epic 1: Foundation (all stories ISO-1.1 through ISO-1.14)

## Ship Criteria
- `wt create --setup` bootstraps a Node.js project using `ShellCommandAdapter` with `npm install`.
- `wt create --setup` copies `.env` using `DefaultAdapter`.
- Port allocation assigns unique ports to 20 simultaneous worktrees with no collision.
- `wt attach` on a path matching a `stale_worktrees` entry correctly recovers the original port.
- macOS `.DS_Store` test: `wt delete` succeeds even when `.DS_Store` is present in worktree root.

## Stories
- ISO-2.1: EcosystemAdapter Trait
- ISO-2.2: DefaultAdapter
- ISO-2.3: ShellCommandAdapter
- ISO-2.4: wt create --setup CLI Integration
- ISO-2.5: Port Allocation CLI
- ISO-2.6: macOS .DS_Store Handling
- ISO-2.7: Windows MAX_PATH
- ISO-2.8: wt attach Port Recovery
- ISO-2.9: MCP Server Documentation
- ISO-2.10: M2 Integration Test Suite

## Duration
Weeks 7-10
