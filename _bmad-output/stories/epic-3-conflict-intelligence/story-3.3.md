# Story 3.3: Single-Pair Conflict Check

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As a developer, I want to check whether two branches would conflict if merged so that I can avoid starting work on a branch that will be difficult to merge.

## Description
Implement `manager.check_conflicts(repo, branch_a, branch_b)` which runs `git merge-tree --write-tree` for a single branch pair and returns a `ConflictReport`. This is the foundational conflict check method that the batch matrix and CLI both delegate to. The method requires Git >= 2.38 and returns a clear error on older versions.

## Acceptance Criteria
- [ ] `Manager::check_conflicts(&self, branch_a: &str, branch_b: &str) -> Result<ConflictReport, WorktreeError>` method implemented
- [ ] Method invokes `git merge-tree --write-tree -z <branch_a> <branch_b>` via `std::process::Command`
- [ ] Output is parsed using the parser from ISO-3.1
- [ ] Returns `ConflictReport` with all detected conflicts
- [ ] Returns `ConflictReport { has_conflicts: false, conflicts: vec![] }` when branches merge cleanly
- [ ] Returns an error if Git version < 2.38 (checked via `GitCapabilities.has_merge_tree_write`)
- [ ] Returns `GitCommandFailed` if either branch does not exist
- [ ] Non-existent branch error message includes the branch name
- [ ] Method works correctly when called from any working directory (uses repo root)

## Tasks
- [ ] Add `check_conflicts()` method signature to `Manager` impl block
- [ ] Implement git command construction: `git -C <repo> merge-tree --write-tree -z <branch_a> <branch_b>`
- [ ] Gate on `self.capabilities.has_merge_tree_write` -- return descriptive error if false
- [ ] Parse stdout through the merge-tree parser (ISO-3.1)
- [ ] Handle exit code: 0 = clean merge, 1 = conflicts found (both are valid, not errors)
- [ ] Handle exit code 128 = invalid ref (return `GitCommandFailed`)
- [ ] Write test: two branches with no conflicts returns empty report
- [ ] Write test: two branches with content conflict returns correct ConflictReport
- [ ] Write test: non-existent branch returns error
- [ ] Write test: Git < 2.38 returns version error

## Technical Notes
- PRD Section 7.2: `git merge-tree --write-tree -z --stdin` is listed for batch mode. For single-pair, omit `--stdin` and pass branches as positional arguments.
- `git merge-tree --write-tree` exit codes: 0 = clean merge (result tree written), 1 = conflicts present (conflict info on stdout), 128 = fatal error.
- PRD Section 17: `has_merge_tree_write` capability requires Git >= 2.38.
- The `merge-tree` command does NOT modify the working tree or index -- it is a read-only operation that computes a hypothetical merge result.
- All git operations use `std::process::Command` per Appendix A rule 1.

## Test Hints
- Create a test repo, make conflicting changes on two branches, run `check_conflicts()`, verify `ConflictReport` lists the conflicting files
- Create a test repo with two branches that merge cleanly, verify `has_conflicts == false`
- Test with a branch name that does not exist, verify error contains the branch name
- Mock `GitCapabilities { has_merge_tree_write: false }` to test version check
- Test a rename conflict: rename same file differently on each branch

## Dependencies
- ISO-3.1 (git merge-tree Output Parser)
- ISO-3.2 (ConflictReport and ConflictType Types)
- ISO-1.3 (Git Version Detection -- GitCapabilities)

## Estimated Effort
M

## Priority
P2

## Traceability
- PRD: Section 7.2 (Exact git Commands -- git merge-tree)
- PRD: Section 17 (Git Version Matrix -- 2.38+)
- FR: FR-P2-003
- Appendix A invariant: Rule 1 (shell out to git CLI)
- QA ref: QA-3.3-001 through QA-3.3-004
