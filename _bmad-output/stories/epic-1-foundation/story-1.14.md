# Story 1.14: M1 Stress Test and crates.io Publish Prep

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library maintainer, I want a comprehensive stress test suite and complete crates.io packaging so that I can ship M1 with confidence that the library handles adversarial conditions and is ready for public consumption.

## Description
Create the M1 ship-gate stress tests defined in PRD Section 15: 100 create/delete cycles with SIGKILL injection at random points (zero data loss requirement), and a 1,000-orphan GC test simulating the OpenCode failure mode. Prepare all three crates for crates.io publishing with README, LICENSE, docs.rs metadata, and correct `Cargo.toml` fields (description, license, repository, keywords, categories).

## Acceptance Criteria
- [ ] Stress test: 100 create/delete cycles complete with zero data loss
- [ ] Stress test: SIGKILL is injected at random points during create/delete; state.json remains consistent after recovery
- [ ] Stress test: after SIGKILL recovery, `Manager::new()` rebuilds state from `git worktree list` without manual intervention
- [ ] GC stress test: 1,000 orphaned worktrees with varying ages (1-30 days) are correctly identified and cleaned
- [ ] GC stress test: locked worktrees within the 1,000 are never touched (Appendix A rule 13)
- [ ] GC stress test: worktrees younger than `gc_max_age_days` are not evicted
- [ ] `cargo publish --dry-run` succeeds for all three crates
- [ ] Each `Cargo.toml` has `description`, `license = "MIT OR Apache-2.0"`, `repository`, `keywords`, `categories`, `documentation`
- [ ] README.md exists at workspace root with: project description, installation, basic usage, MCP config snippets for all 4 clients (Claude Code, Cursor, VS Code, OpenCode)
- [ ] LICENSE-MIT and LICENSE-APACHE files exist at workspace root
- [ ] `cargo doc --no-deps` builds without warnings
- [ ] All doc comments on public API items are present

## Tasks
- [ ] Create `tests/stress_create_delete.rs`: 100 create/delete cycles in a test git repository
- [ ] Implement SIGKILL injection: fork a child process that runs create/delete, send SIGKILL at random delay, verify parent can recover state
- [ ] Verify after each SIGKILL: state.json is either consistent (fsync completed) or rebuildable from `git worktree list`
- [ ] Create `tests/stress_gc_orphans.rs`: create 1,000 worktrees, mark varying proportions as orphaned/stale/locked
- [ ] Verify GC report: correct orphan count, correct removed count, locked worktrees untouched
- [ ] Verify GC performance: 1,000-orphan scan completes within reasonable time (<60s)
- [ ] Add `description` to all three `Cargo.toml` files
- [ ] Add `license = "MIT OR Apache-2.0"` to all three `Cargo.toml` files
- [ ] Add `repository = "https://github.com/<org>/iso-code"` to all three `Cargo.toml` files
- [ ] Add `keywords` and `categories` to `iso-code/Cargo.toml`
- [ ] Create `LICENSE-MIT` and `LICENSE-APACHE` at workspace root
- [ ] Create `README.md` at workspace root with installation instructions and MCP config snippets from PRD Section 12.3
- [ ] Add `#![doc = include_str!("../README.md")]` to `lib.rs` for docs.rs
- [ ] Run `cargo doc --no-deps` and fix any doc warnings
- [ ] Run `cargo publish --dry-run -p iso-code` and fix any issues
- [ ] Run `cargo publish --dry-run -p iso-code-cli` and fix any issues
- [ ] Run `cargo publish --dry-run -p iso-code-mcp` and fix any issues

## Technical Notes
- PRD Section 15 M1 ship criteria: "Zero data loss in stress test: 100 create/delete cycles with simulated crash injection (SIGKILL at random points)"
- PRD Section 15 M1 ship criteria: "wt gc successfully cleans orphaned worktrees from a simulated OpenCode failure (1000 orphans, varying ages)"
- SIGKILL testing: use `fork()` on Unix to create a child process, `kill(child_pid, SIGKILL)` at random intervals, then verify state consistency in the parent
- The atomic write protocol (tmp -> fsync -> rename) from Story 1.10 is what makes SIGKILL recovery possible: either the old or new state.json is intact
- 1,000-orphan test: create worktrees via direct git commands (not Manager) to simulate external tools leaving orphans
- PRD Section 12.3: MCP config snippets for Claude Code, Cursor, VS Code Copilot, and OpenCode must be in README
- crates.io requires `license` or `license-file` field; dual license MIT/Apache-2.0 is standard for Rust ecosystem
- `cargo publish` order matters: `iso-code` first (library), then CLI and MCP (which depend on it)

## Test Hints
- QA-R-008: 100 create/delete cycles with no data loss
- QA-R-009: SIGKILL recovery -- state.json intact or rebuildable
- Stress test: verify `Manager::list()` returns consistent results after each SIGKILL recovery
- GC stress test: create 1000 worktrees with ages uniformly distributed 1-30 days, set `gc_max_age_days = 7`, verify ~767 are candidates (days 8-30) and ~233 survive (days 1-7)
- GC stress test: inject 50 locked worktrees in the 1000, verify all 50 survive regardless of age
- Publish test: `cargo publish --dry-run` for each crate

## Dependencies
ISO-1.1 through ISO-1.13 (all prior stories)

## Estimated Effort
L

## Priority
P0

## Traceability
- PRD: Section 15 (Implementation Milestones, M1 ship criteria)
- FR: N/A (validation, not feature)
- Appendix A invariant: Rule 13 (gc never touches locked), Rule 7 (lock scope)
- Bug regression: N/A (preventive validation)
- QA ref: Stress test, QA-R-008, QA-R-009
