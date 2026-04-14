# Story 3.5: wt check CLI Subcommand

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As a developer, I want a `wt check` command that shows me which branches would conflict if merged so that I can coordinate work across parallel branches before merge time.

## Description
Implement the `wt check` CLI subcommand that exposes conflict detection to users. The command requires Git >= 2.38 and provides a clear, actionable error message with upgrade instructions when run on older git versions. It supports both single-pair checking (`wt check <branch_a> <branch_b>`) and full matrix mode (`wt check --all`). Output defaults to a human-readable table and supports `--json` for machine consumption.

## Acceptance Criteria
- [ ] `wt check <branch_a> <branch_b>` checks a single pair and displays conflicts
- [ ] `wt check --all` runs the full conflict matrix across all active worktrees
- [ ] Default output is a human-readable table showing conflicting files per pair
- [ ] `--json` flag outputs structured JSON matching the `ConflictReport` serialization
- [ ] When Git < 2.38, command exits with a clear error: "Conflict detection requires Git 2.38 or later. Current version: X.Y.Z. Upgrade instructions: ..."
- [ ] Exit code 0 when no conflicts found, exit code 1 when conflicts detected, exit code 2 on error
- [ ] Clean merge pairs are shown with a green checkmark in default output
- [ ] Conflicting pairs show affected file paths and conflict types
- [ ] LFS pointer conflicts are annotated with "[LFS]" in default output

## Tasks
- [ ] Add `check` subcommand to clap argument parser with `branch_a` and `branch_b` positional args
- [ ] Add `--all` flag for full matrix mode
- [ ] Add `--json` flag for structured output
- [ ] Implement Git version check with upgrade instructions in error message
- [ ] Implement single-pair output formatting (table with file paths and conflict types)
- [ ] Implement matrix output formatting (table with branch pairs and conflict counts)
- [ ] Set exit codes: 0 = clean, 1 = conflicts, 2 = error
- [ ] Write test: single pair with no conflicts shows clean result
- [ ] Write test: single pair with conflicts shows file paths
- [ ] Write test: `--all` with 3 branches shows matrix
- [ ] Write test: Git < 2.38 shows upgrade instructions

## Technical Notes
- PRD Section 12.1: `wt check` is listed as "reserved, not implemented in v1.0" but is implemented in M3.
- PRD Section 17: `has_merge_tree_write` capability at Git >= 2.38. The graceful degradation message is an M3 ship criterion.
- Exit code convention: 0 = success (no conflicts), 1 = conflicts found (not an error, but actionable), 2 = actual error (git too old, invalid branch, etc.).
- The `--json` output format should match the `ConflictReport` serde serialization to ensure consistency with the MCP tool.

## Test Hints
- Test exit codes by running `wt check` as a subprocess and checking `ExitStatus::code()`
- Test human-readable output contains file paths and conflict type descriptions
- Test `--json` output is valid JSON and deserializes to `ConflictReport`
- Mock `GitCapabilities { has_merge_tree_write: false }` to test the degradation message

## Dependencies
- ISO-3.3 (Single-Pair Conflict Check)
- ISO-3.4 (Conflict Matrix -- for --all mode)

## Estimated Effort
S

## Priority
P2

## Traceability
- PRD: Section 12.1 (CLI Commands -- wt check)
- PRD: Section 17 (Git Version Matrix -- graceful degradation)
- FR: FR-P2-005
- M3 ship criterion: "wt check correctly identifies conflicts for 20 merge scenarios"
- QA ref: QA-3.5-001 through QA-3.5-004
