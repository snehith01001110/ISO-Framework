# Story 4.7: Worktree Pooling

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As an agent orchestrator, I want a pool of pre-created worktrees available for instant checkout so that new agent sessions start in under 1 second instead of waiting 5+ seconds for on-demand worktree creation.

## Description
Implement a worktree pool that pre-creates N worktrees from a base branch, keeping them ready for instant assignment. When a consumer calls `acquire_from_pool()`, a pre-created worktree is assigned and returned immediately. When the consumer is done, `release_to_pool()` resets the worktree and returns it to the pool for reuse. The pool automatically replenishes to maintain the configured size. This dramatically reduces latency for agent orchestrators that create many short-lived worktrees.

## Acceptance Criteria
- [ ] `PoolConfig` struct with `size: usize` and `base_branch: String` fields
- [ ] `Manager::create_pool(config: PoolConfig) -> Result<(), WorktreeError>` pre-creates N worktrees
- [ ] `Manager::acquire_from_pool() -> Result<WorktreeHandle, WorktreeError>` returns a pre-created worktree in < 1 second
- [ ] `Manager::release_to_pool(handle: &WorktreeHandle) -> Result<(), WorktreeError>` resets and returns worktree to pool
- [ ] Pool of 5 worktrees available in < 1 second (M4 ship criterion)
- [ ] Pool auto-replenishes when pool size drops below configured minimum
- [ ] Released worktrees are reset via `git checkout -- .` and `git clean -fd` before returning to pool
- [ ] Pool worktrees are marked with a special state in `state.json` (e.g., `"pooled"` in metadata)
- [ ] Pool worktrees are excluded from `wt gc` (treated similarly to locked worktrees)
- [ ] `wt status` shows pool size and available count

## Tasks
- [ ] Define `PoolConfig` struct with `size` and `base_branch` fields
- [ ] Add `pool` section to `state.json` schema for pool metadata
- [ ] Implement `Manager::create_pool()` that creates N worktrees with unique branch names
- [ ] Implement `Manager::acquire_from_pool()` that pops a worktree from the available pool
- [ ] Implement `Manager::release_to_pool()` that resets and returns a worktree
- [ ] Implement reset logic: `git checkout -- .` + `git clean -fd` in the worktree directory
- [ ] Implement auto-replenishment: after acquire, check pool size, replenish if below threshold
- [ ] Exclude pooled worktrees from `gc()` operations
- [ ] Add pool info to `wt status` output
- [ ] Write test: pool of 5, acquire takes < 1s
- [ ] Write test: acquire -> release -> acquire reuses the same worktree
- [ ] Write test: pool auto-replenishes after acquire
- [ ] Write test: pooled worktrees not touched by gc

## Technical Notes
- PRD Section 15 M4: "Worktree pooling: pre-create N worktrees for instant checkout. Pool size configurable; automatic replenishment."
- PRD Section 15 M4 ship criterion: "Worktree pool of 5 worktrees available in < 1 second (vs. ~5 seconds for on-demand creation)."
- Pool branch naming convention: `pool/<base_branch>/0`, `pool/<base_branch>/1`, etc.
- Reset via `git checkout -- .` restores tracked files. `git clean -fd` removes untracked files and directories. Together they return the worktree to a clean state.
- Auto-replenishment should be asynchronous or deferred to avoid blocking the `acquire_from_pool()` caller.
- Pooled worktrees should be excluded from the `max_worktrees` count in Config to avoid pool creation hitting the rate limit.

## Test Hints
- Timing test: `let start = Instant::now(); manager.acquire_from_pool()?; assert!(start.elapsed() < Duration::from_secs(1));`
- Reuse test: acquire A, release A, acquire B -> assert A.path == B.path
- Replenishment test: create pool of 3, acquire 2, check pool metadata shows 1 available + 2 replenishing
- GC test: create pool, run gc with aggressive max_age, verify pool worktrees are untouched

## Dependencies
- ISO-1.6 (Manager::create() -- worktree creation mechanics)
- ISO-1.7 (Manager::delete() -- deletion for pool cleanup)
- ISO-1.8 (Manager::gc() -- gc exclusion logic for pooled worktrees)

## Estimated Effort
L

## Priority
P3

## Traceability
- PRD: Section 15 M4 (Scope -- Worktree pooling)
- FR: FR-P3-007
- M4 ship criterion: "Worktree pool of 5 worktrees available in < 1 s"
- QA ref: QA-4.7-001 through QA-4.7-004
