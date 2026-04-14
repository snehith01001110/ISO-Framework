# Story 1.8: Manager::gc()

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want `Manager::gc()` to safely identify and remove orphaned and stale worktrees while protecting locked worktrees and active sessions so that disk space is recovered without risking data loss.

## Description
Implement `Manager::gc()` from PRD Section 5.3 with dry-run as the default behavior. GC must reconcile `state.json` against `git worktree list --porcelain`, identify orphans, apply the age-based eviction policy, protect locked worktrees unconditionally (Appendix A rule 13), and perform PID-liveness checks to avoid evicting worktrees in active use (OQ-6 resolution). The `stale_worktrees` mechanism preserves metadata for recovery rather than silently deleting.

## Acceptance Criteria
- [ ] `GcOptions::default()` has `dry_run = true` -- GC never modifies by default
- [ ] Dry-run mode returns a `GcReport` listing what would happen without performing any deletions
- [ ] Locked worktrees are never touched regardless of `GcOptions.force` (Appendix A rule 13)
- [ ] Orphaned worktrees (present in state.json but absent from `git worktree list`) are identified
- [ ] Age-based eviction: worktrees older than `gc_max_age_days` (default 7) are candidates for removal
- [ ] `GcOptions.max_age_days` overrides `Config.gc_max_age_days` for this run
- [ ] PID-liveness check: for worktrees in `Active` state, verify `creator_pid` is alive via `kill(pid, 0)` before evicting
- [ ] Five-step unmerged commit check runs before each deletion unless `GcOptions.force = true`
- [ ] Evicted worktrees move to `stale_worktrees` in state.json (not silently deleted)
- [ ] `GcReport` contains `orphans`, `removed`, `evicted`, `freed_bytes`, and `dry_run` fields
- [ ] `git worktree prune` is called to clean stale git metadata
- [ ] `freed_bytes` is calculated from actual disk usage of removed worktrees

## Tasks
- [ ] Implement `gc()` method on `Manager` in `manager.rs`
- [ ] Run `git worktree list --porcelain` and reconcile against `state.json` `active_worktrees`
- [ ] Identify orphans: entries in `state.json` but not in git output, or on disk but not in either
- [ ] Implement age-based filtering: compare `created_at` or `last_activity` against `gc_max_age_days`
- [ ] Implement locked worktree protection: skip any worktree with `state == Locked`, never override
- [ ] Implement PID-liveness check: `kill(pid, 0)` on Unix for `creator_pid`; if alive, skip eviction
- [ ] Run `five_step_unmerged_check()` (from Story 1.7) for each deletion candidate unless `force = true`
- [ ] Calculate disk usage of each worktree to be removed using `jwalk` + `filesize`
- [ ] Move evicted entries to `stale_worktrees` with `eviction_reason`, `evicted_at`, `expires_at` fields
- [ ] Call `git worktree remove` for each confirmed removal (when `dry_run = false`)
- [ ] Call `git worktree prune` to clean stale git worktree metadata
- [ ] Purge `stale_worktrees` entries where `expires_at < now`
- [ ] Build and return `GcReport`
- [ ] Write tests for dry-run vs live behavior

## Technical Notes
- PRD Section 5.3: GC rules specify dry_run default, locked protection, five-step check before deletion
- PRD Section 10.3: reconciliation policy defines how state.json is updated against git output
- Appendix A rule 13: "gc() never touches locked worktrees regardless of the force flag" -- this is absolute
- Appendix A rule 8: "Entries evicted from active_worktrees go to stale_worktrees -- never silently deleted"
- OQ-6 resolution: PID-liveness check prevents evicting worktrees in active use by another process; use `libc::kill(pid, 0)` on Unix, `OpenProcess` on Windows
- PRD Section 7.2: `git worktree prune` is the exact command for cleaning stale metadata
- `stale_worktrees` entries have `expires_at = now + Config.stale_metadata_ttl_days` (default 30 days)
- Port leases for evicted worktrees transition to `status: "stale"` per PRD Section 10.4

## Test Hints
- QA-R-007: GC of 1000 orphans completes without error and reports correct counts
- QA-R-010: GC with simulated OpenCode failure (orphans at varying ages) correctly filters by age
- QA-C-002: concurrent GC and create operations do not corrupt state.json
- Unit test: dry_run returns report without modifying any files
- Unit test: locked worktree survives GC even with `force = true`
- Unit test: active worktree with live PID is not evicted
- Unit test: stale worktree with dead PID is evicted
- Unit test: evicted worktree appears in `stale_worktrees` with correct metadata
- Integration test: create 5 worktrees, delete 3, run GC, verify report

## Dependencies
ISO-1.2, ISO-1.3, ISO-1.4, ISO-1.7

## Estimated Effort
L

## Priority
P0

## Traceability
- PRD: Section 5.3 (Manager::gc), Section 10.3 (Reconciliation Policy)
- FR: FR-P0-003 (gc lifecycle)
- Appendix A invariant: Rule 8 (evict to stale), Rule 13 (gc never touches locked)
- Bug regression: opencode#14648 (unbounded orphan accumulation), vscode#296194 (runaway worktree loop)
- QA ref: QA-R-007, QA-R-010, QA-C-002
