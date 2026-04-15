# Story 2.7: Windows MAX_PATH

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a Windows user, I want iso-code to handle long file paths correctly so that worktrees with deep directory structures (like `node_modules`) do not fail with path-length errors.

## Description
Windows has a legacy `MAX_PATH` limit of 260 characters. While Rust 1.58+ automatically prepends `\\?\` to paths for `std::fs` operations, external tools like git do not understand `\\?\` prefixes. The `dunce` crate strips these prefixes when paths are passed to external tools. This story integrates `dunce::simplified()` at all points where paths are passed to git or other external processes, and adds a compile test to verify `dunce` is used consistently.

## Acceptance Criteria
- [ ] All paths passed to `std::process::Command` (git invocations) go through `dunce::simplified()` first
- [ ] `dunce::canonicalize()` is used instead of `std::fs::canonicalize()` throughout the codebase
- [ ] `repo_root` in `Manager` struct is canonicalized via `dunce::canonicalize()` at construction
- [ ] Worktree paths in `WorktreeHandle` are stored as `dunce::simplified()` paths
- [ ] Paths in `state.json` are stored without `\\?\` prefix
- [ ] A compile test verifies that `std::fs::canonicalize` is not used directly anywhere in the library crate
- [ ] Windows CI compiles without path-related failures
- [ ] Deep path test: worktree with a 250+ character absolute path is created and deleted successfully

## Tasks
- [ ] Audit all `std::fs::canonicalize()` calls and replace with `dunce::canonicalize()`
- [ ] Audit all `std::process::Command` invocations and ensure path arguments use `dunce::simplified()`
- [ ] Create a path normalization utility function `normalize_path(path: &Path) -> PathBuf` that applies `dunce` on Windows and is identity on Unix
- [ ] Add `dunce` to all path construction in `Manager::new()`, `Manager::create()`, `Manager::delete()`
- [ ] Write compile-time lint or grep test: assert no direct `std::fs::canonicalize` usage in `src/`
- [ ] Write test: roundtrip path through `dunce::simplified()` on Windows produces a valid path without `\\?\`
- [ ] Write test: path > 250 characters works for create/delete on Windows
- [ ] Verify `state.json` paths do not contain `\\?\` prefix

## Technical Notes
- PRD Section 11.3: "Use `dunce` crate when passing paths to external tools (including git) to strip the `\\?\` prefix."
- PRD Section 5.2: `Manager::new()` canonicalizes `repo_root` via `dunce::canonicalize()`.
- PRD Section 14: `dunce` v1 is an explicit dependency.
- On non-Windows platforms, `dunce::simplified()` is identity, so it is safe to call unconditionally.
- The compile test can be a shell script that `grep -r 'std::fs::canonicalize' src/` and fails if matches found.
- `core.longpaths = true` must be set in git config for deep paths on Windows (PRD Section 11.3).

## Test Hints
- Compile test: `grep -rn 'std::fs::canonicalize' src/` should return zero matches
- On Windows CI: create a worktree with path length > 250 characters, verify creation and deletion succeed
- Verify `state.json` does not contain `\\?\` by deserializing and checking path strings
- On Unix: verify `dunce::simplified()` is a no-op (paths unchanged)

## Dependencies
- ISO-1.1 (Workspace Scaffolding -- dunce dependency in Cargo.toml)
- ISO-1.5 (Manager::create() Part 1 -- nested worktree check uses canonicalize)

## Estimated Effort
M

## Priority
P1

## Traceability
- PRD: Section 11.3 (Windows -- Path lengths)
- PRD: Section 14 (Crate Dependencies -- dunce)
- FR: FR-P1-012
- Appendix A invariant: Rule 1 (shell out to git CLI -- paths must be git-compatible)
- QA ref: QA-2.7-001 through QA-2.7-004
