# Story 3.4: Conflict Matrix (Batch)

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As an agent orchestrator, I want to check all active worktree branches for pairwise conflicts in a single batch operation so that I can identify potential merge issues across the entire workspace efficiently.

## Description
Implement `manager.conflict_matrix(repo)` which checks all pairs of active worktree branches for conflicts using `git merge-tree --stdin` for batch processing. The `--stdin` mode allows feeding multiple branch pairs to a single git process, dramatically reducing overhead compared to spawning one process per pair. The matrix must complete in under 10 seconds for 20 branch pairs on a typical repository.

## Acceptance Criteria
- [ ] `Manager::conflict_matrix(&self) -> Result<Vec<ConflictReport>, WorktreeError>` method implemented
- [ ] Method computes all unique pairs of active worktree branches: N branches produce N*(N-1)/2 pairs
- [ ] Uses `git merge-tree --write-tree -z --stdin` to check all pairs in a single git process
- [ ] Branch pairs are fed to stdin as NUL-delimited input
- [ ] Returns a `Vec<ConflictReport>`, one per branch pair
- [ ] Performance: completes in under 10 seconds for 20 branch pairs
- [ ] Handles repositories with only 0 or 1 active worktrees gracefully (returns empty vec)
- [ ] Returns an error if Git version < 2.38
- [ ] Skips the main worktree branch (bare checkout) in pair generation

## Tasks
- [ ] Add `conflict_matrix()` method signature to `Manager` impl block
- [ ] Implement pair generation: enumerate all unique (branch_a, branch_b) pairs from active worktrees
- [ ] Construct `git merge-tree --write-tree -z --stdin` command
- [ ] Feed branch pairs to stdin in NUL-delimited format
- [ ] Parse batched stdout output, splitting results per pair
- [ ] Gate on `self.capabilities.has_merge_tree_write`
- [ ] Optimize: skip pairs where both branches have the same HEAD commit
- [ ] Write test: 3 branches produce 3 pairs, each checked correctly
- [ ] Write test: performance benchmark with 20 branches completes < 10s
- [ ] Write test: 0 worktrees returns empty vec
- [ ] Write test: 1 worktree returns empty vec

## Technical Notes
- PRD Section 7.2: `git merge-tree --write-tree -z --stdin` is the batch mode command.
- The `--stdin` flag reads branch pairs from stdin, separated by NUL bytes. Output is also NUL-delimited with separators between pair results.
- For 20 branches, there are 20*19/2 = 190 pairs. The 10-second budget gives ~52ms per pair.
- The batch approach spawns one git process instead of 190, saving significant process startup overhead.
- Skip pairs where both branches point to the same commit (trivially no conflict).
- The main worktree (bare checkout / primary branch) should be included in pair generation since agents may need to know if their branch conflicts with main.

## Test Hints
- Create a repo with 3 branches: main, feature-a (conflicts with feature-b), feature-b (conflicts with feature-a), feature-c (clean merge with both). Verify matrix identifies exactly the correct conflicts.
- Performance test: create a repo with 20 branches (can be simple, few files), run `conflict_matrix()`, assert wall-clock time < 10s
- Edge case: identical branches (same HEAD) should report no conflicts
- Edge case: repo with no worktrees returns `Ok(vec![])`

## Dependencies
- ISO-3.1 (git merge-tree Output Parser)
- ISO-3.2 (ConflictReport and ConflictType Types)
- ISO-3.3 (Single-Pair Conflict Check -- reuse of parser logic)

## Estimated Effort
L

## Priority
P2

## Traceability
- PRD: Section 7.2 (Exact git Commands -- git merge-tree --stdin)
- PRD: Section 15 M3 (Ship criteria -- 20 merge scenarios)
- FR: FR-P2-004
- Appendix A invariant: Rule 1 (shell out to git CLI)
- QA ref: QA-3.4-001 through QA-3.4-004
