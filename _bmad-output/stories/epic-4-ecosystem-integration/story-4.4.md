# Story 4.4: gix Conflict Detection

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a library consumer, I want conflict detection to use `gix` for in-process merge-tree computation so that I get faster results without spawning git subprocesses, while keeping the CLI fallback available.

## Description
Integrate `gix::Repository::merge_trees()` as an alternative conflict detection backend, behind the feature flag `conflict-detection-gix`. This replaces the `git merge-tree` CLI path (ISO-3.1 through ISO-3.4) with a pure-Rust in-process implementation. The gix path is faster (no process spawn overhead) and produces the same `ConflictReport` types. The CLI fallback remains the default; the gix backend is opt-in via feature flag. This follows the pattern established by GitButler PR #5722 which replaced all `git2::merge_trees()` calls with gix.

## Acceptance Criteria
- [ ] `gix` integration behind `features = ["conflict-detection-gix"]` in `Cargo.toml`
- [ ] `gix::Repository::merge_trees()` used for conflict detection when feature is enabled
- [ ] Returns the same `ConflictReport` and `ConflictType` types as the CLI path
- [ ] Falls back to CLI path if gix merge fails (e.g., unsupported repository format)
- [ ] Does NOT use gix for worktree CRUD operations (Appendix A rule 1: shell out to git CLI)
- [ ] Feature flag does not affect builds that do not opt in (no gix dependency by default)
- [ ] `Manager::check_conflicts()` transparently selects backend based on feature flag
- [ ] `Manager::conflict_matrix()` uses gix for all pairs when feature is enabled
- [ ] Performance: gix path is measurably faster than CLI path for 20-pair matrix

## Tasks
- [ ] Add `conflict-detection-gix` feature flag to `iso-code/Cargo.toml`
- [ ] Add `gix` crate dependency gated behind the feature flag
- [ ] Create `src/conflict/gix_backend.rs` module
- [ ] Implement `check_conflicts_gix()` using `gix::Repository::merge_trees()`
- [ ] Map gix conflict output to `ConflictType` variants
- [ ] Implement fallback: on gix error, log warning and retry with CLI backend
- [ ] Wire feature-flag detection into `Manager::check_conflicts()` and `Manager::conflict_matrix()`
- [ ] Write test: gix backend produces same results as CLI backend for identical inputs
- [ ] Write test: gix backend falls back to CLI on error
- [ ] Write benchmark: compare gix vs CLI performance for 20-pair matrix
- [ ] Write compile test: building without feature flag does not include gix dependency

## Technical Notes
- PRD Section 14: "gix and git2 are reserved for v1.1 conflict detection (gix::Repository::merge_trees()) and must be feature-flagged."
- PRD Section 15 M4: "gix::Repository::merge_trees() integration replacing CLI fallback for conflict detection."
- Appendix A rule 1: "Shell out to git CLI. Never use git2 or gix for worktree CRUD." This rule applies to worktree lifecycle (add/remove/list), NOT to conflict detection.
- GitButler PR #5722 demonstrates the viability of `gix::merge_trees()` as a replacement for `git2::merge_trees()`.
- The feature flag approach means iso-code's default dependency footprint stays lean (no C dependencies, no gix).
- `gix` is a pure-Rust git implementation. `gix::Repository::merge_trees()` was feature-complete as of November 2024.

## Test Hints
- Create a test repo with known conflicts, run both backends, compare `ConflictReport` output field by field
- Benchmark: run 20-pair matrix with both backends, assert gix is faster (no process spawn)
- Test fallback: mock a gix error (e.g., corrupt repo), verify CLI backend is used with a warning log
- Compile test: `cargo check --no-default-features` should succeed without gix

## Dependencies
- ISO-3.1 (git merge-tree Output Parser -- CLI backend to fall back to)
- ISO-3.2 (ConflictReport and ConflictType Types -- shared output types)
- ISO-3.3 (Single-Pair Conflict Check -- API to extend with gix backend)
- ISO-3.4 (Conflict Matrix -- batch API to extend)

## Estimated Effort
L

## Priority
P3

## Traceability
- PRD: Section 14 (Crate Dependencies -- gix feature-flagged)
- PRD: Section 15 M4 (Scope -- gix integration)
- FR: FR-P3-004
- Appendix A invariant: Rule 1 (shell out to git CLI for worktree CRUD -- gix only for conflict detection)
- QA ref: QA-4.4-001 through QA-4.4-004
