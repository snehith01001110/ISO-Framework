# Story 1.4: git worktree list Parser

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want the porcelain output of `git worktree list` to be reliably parsed into `WorktreeHandle` structs so that the library always reflects the true state of git's worktree registry.

## Description
Implement the parser for `git worktree list --porcelain` output as defined in PRD Section 7.1. Support both NUL-delimited output (`-z` flag, Git >= 2.36) and newline-delimited fallback for older versions. Handle all annotations: `bare`, `detached`, `locked` (with optional reason), and `prunable` (with optional reason). This parser is the foundation for Appendix A rule 2 ("git worktree list --porcelain is the source of truth").

## Acceptance Criteria
- [ ] NUL-delimited parsing (`--porcelain -z`) works for Git >= 2.36 (gated by `GitCapabilities.has_list_nul`)
- [ ] Newline-delimited parsing (`--porcelain`) works for Git < 2.36
- [ ] Each worktree block produces a `WorktreeHandle` with `path`, `branch`, and `state` fields
- [ ] `HEAD` SHA is extracted as a 40-character string
- [ ] `branch refs/heads/<name>` extracts the branch name without the `refs/heads/` prefix
- [ ] `detached` worktrees are represented (branch field empty or sentinel)
- [ ] `bare` worktrees are represented correctly
- [ ] `locked` annotation sets `WorktreeState::Locked`; optional reason string is captured
- [ ] `locked <reason>` with a reason string is parsed correctly
- [ ] `prunable` annotation sets `WorktreeState::Orphaned`; reason string is captured
- [ ] A warning is logged when `-z` is not available and a path appears to contain newlines
- [ ] Empty output (no worktrees) returns an empty `Vec<WorktreeHandle>`

## Tasks
- [ ] Implement `fn parse_worktree_list_porcelain(output: &[u8], nul_delimited: bool) -> Result<Vec<WorktreeHandle>, WorktreeError>` in `worktree-core/src/git.rs`
- [ ] Split on blank lines (newline mode) or NUL bytes (NUL mode) to separate worktree blocks
- [ ] For each block, parse key-value lines: `worktree`, `HEAD`, `branch`/`detached`/`bare`, `locked`, `prunable`
- [ ] Strip `refs/heads/` prefix from branch field
- [ ] Handle `locked` (no reason) and `locked <reason>` (with reason) variants
- [ ] Handle `prunable <reason>` variant
- [ ] Add newline-in-path detection heuristic: if not NUL-delimited and a `worktree` line's path contains a newline character, log warning per PRD Section 7.1
- [ ] Implement `fn run_worktree_list(repo: &Path, caps: &GitCapabilities) -> Result<Vec<WorktreeHandle>, WorktreeError>` that invokes `git worktree list --porcelain [-z]` and calls the parser
- [ ] Write property-based tests using `proptest` or manual fuzzing for path edge cases

## Technical Notes
- PRD Section 7.1: exact output format is defined; parse `worktree <path>` as always first, always present
- PRD Section 7.2: commands are `git worktree list --porcelain` and `git worktree list --porcelain -z` (Git >= 2.36)
- Appendix A rule 2: this parser output is authoritative; `state.json` must reconcile against it
- Appendix A rule 6: use `--porcelain` output only, never human-readable format
- Appendix A rule 10: paths with newlines are unparseable without `-z`; log warning, do not crash
- `locked` and `prunable` fields only appear in Git >= 2.31 per PRD Section 17; on older git, assume not locked/not prunable
- The parser operates on raw bytes (`&[u8]`) to handle NUL delimiters correctly

## Test Hints
- Property-based test: generate random paths (including spaces, unicode, special chars) and roundtrip through the parser
- Unit test: parse a multi-block porcelain output with main worktree, feature branch, detached HEAD, and bare repo
- Unit test: parse a locked worktree with reason string
- Unit test: parse a prunable worktree
- Unit test: newline-delimited fallback correctly separates blocks
- Regression test: path with embedded newline in NUL-delimited mode parses correctly
- Regression test: path with embedded newline in newline-delimited mode triggers warning (Appendix A rule 10)
- Unit test: empty output returns empty Vec

## Dependencies
ISO-1.2, ISO-1.3

## Estimated Effort
L

## Priority
P0

## Traceability
- PRD: Section 7.1 (Porcelain Output Format), Section 7.2 (Exact git Commands)
- FR: FR-P0-001 (WorktreeHandle construction from git output)
- Appendix A invariant: Rule 2 (porcelain is source of truth), Rule 6 (parse porcelain only), Rule 10 (newline paths)
- Bug regression: Paths with newlines (Appendix A rule 10)
- QA ref: Property-based tests
