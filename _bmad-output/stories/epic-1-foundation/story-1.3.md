# Story 1.3: Git Version Detection

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want `Manager::new()` to detect the installed git version and build a capability map so that the library can gate features by git version and reject unsupported git installations at startup.

## Description
Implement the first two steps of the `Manager::new()` startup sequence (PRD Section 5.2): run `git --version`, parse the output into a `GitVersion` struct, and build a `GitCapabilities` struct. Return `WorktreeError::GitNotFound` if git is not in PATH, or `WorktreeError::GitVersionTooOld` if the version is below 2.20 (PRD Section 17 hard minimum). The capability map drives all downstream feature gates for the Manager's lifetime.

## Acceptance Criteria
- [ ] `git --version` is invoked via `std::process::Command`
- [ ] Output like `"git version 2.43.0"` is parsed into `GitVersion { major: 2, minor: 43, patch: 0 }`
- [ ] Apple-style output `"git version 2.39.3 (Apple Git-146)"` parses correctly, ignoring the suffix
- [ ] `GitCapabilities.has_list_nul` is true when version >= 2.36
- [ ] `GitCapabilities.has_repair` is true when version >= 2.30
- [ ] `GitCapabilities.has_orphan` is true when version >= 2.42
- [ ] `GitCapabilities.has_relative_paths` is true when version >= 2.48
- [ ] `GitCapabilities.has_merge_tree_write` is true when version >= 2.38
- [ ] `WorktreeError::GitNotFound` is returned when `git --version` fails (not in PATH)
- [ ] `WorktreeError::GitVersionTooOld` is returned when version < 2.20, with `required` and `found` fields populated
- [ ] The parsed version is stored in `GitCapabilities.version`

## Tasks
- [ ] Create version parsing function in `worktree-core/src/git.rs`: `fn parse_git_version(output: &str) -> Result<GitVersion, WorktreeError>`
- [ ] Handle format variations: `"git version X.Y.Z"`, `"git version X.Y.Z (Apple Git-NNN)"`, `"git version X.Y.Z.windows.N"`
- [ ] Create capability detection function: `fn detect_capabilities(version: &GitVersion) -> GitCapabilities`
- [ ] Compare version against `GitVersion::MINIMUM` (2.20.0); return `GitVersionTooOld` if less
- [ ] Wire into `Manager::new()` as steps 1 and 4 of the startup sequence
- [ ] Expose `Manager::git_capabilities()` returning `&GitCapabilities`
- [ ] Write unit tests for version parsing across all known format variations
- [ ] Write unit tests for capability thresholds at boundary versions (2.19 vs 2.20, 2.29 vs 2.30, 2.35 vs 2.36, etc.)

## Technical Notes
- PRD Section 7.2: exact command is `git --version`
- PRD Section 4.8: `GitVersion` implements `PartialOrd` and `Ord` for version comparison
- PRD Section 17: minimum supported version is 2.20; hard error below this
- Windows git output may include `.windows.N` suffix (e.g., `2.43.0.windows.1`)
- The `Manager` struct stores `capabilities: GitCapabilities` as a private field (PRD Section 5.1)
- Use `Appendix A rule 1`: always shell out via `std::process::Command`, never `git2`/`gix`

## Test Hints
- Unit test: parse `"git version 2.43.0"` -> `GitVersion { 2, 43, 0 }`
- Unit test: parse `"git version 2.39.3 (Apple Git-146)"` -> `GitVersion { 2, 39, 3 }`
- Unit test: parse `"git version 2.43.0.windows.1"` -> `GitVersion { 2, 43, 0 }`
- Unit test: version 2.19.9 returns `GitVersionTooOld`
- Unit test: version 2.20.0 returns Ok
- Unit test: capabilities at version 2.35 has `has_list_nul = false`, `has_repair = true`
- Unit test: capabilities at version 2.36 has `has_list_nul = true`
- Integration test: actual `git --version` invocation on CI runner

## Dependencies
ISO-1.1, ISO-1.2

## Estimated Effort
M

## Priority
P0

## Traceability
- PRD: Section 4.8 (GitCapabilities), Section 5.2 (Constructor), Section 17 (Git Version Capability Matrix)
- FR: FR-P0-001 (GitCapabilities type)
- Appendix A invariant: Rule 1 (shell out to git CLI)
- Bug regression: N/A
- QA ref: QA-V-001+
