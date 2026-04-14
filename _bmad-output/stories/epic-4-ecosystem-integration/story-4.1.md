# Story 4.1: pnpm Adapter

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a developer using pnpm, I want worktree creation to leverage pnpm's global virtual store so that multiple worktrees share a single package cache and each worktree's `node_modules` uses only symlinks (< 1 MB).

## Description
Implement a pnpm-specific `EcosystemAdapter` that detects pnpm projects (via `pnpm-lock.yaml`) and configures new worktrees to use pnpm's `enableGlobalVirtualStore: true` feature. This avoids the 2 GB-per-worktree `node_modules` duplication documented in `claude-squad#260`. The adapter runs `pnpm install` in the new worktree, leveraging the shared store so that each worktree's `node_modules` contains only symlinks to the global store, keeping per-worktree disk usage under 1 MB.

## Acceptance Criteria
- [ ] `PnpmAdapter` implements `EcosystemAdapter` trait
- [ ] `detect()` returns `true` when `pnpm-lock.yaml` exists in the worktree
- [ ] `setup()` runs `pnpm install` in the new worktree with `enableGlobalVirtualStore: true`
- [ ] `setup()` creates or updates `.npmrc` in the worktree with `enable-global-virtual-store=true` if not already set
- [ ] 5 worktrees share a single virtual store (verified by `du -sh node_modules` showing < 1 MB each)
- [ ] `teardown()` runs `pnpm prune` or is a no-op (symlinks are cleaned by `git worktree remove`)
- [ ] `name()` returns `"pnpm"`
- [ ] Adapter detects pnpm version and warns if < 9.0 (global virtual store support)
- [ ] `WORKTREE_CORE_*` environment variables are available during `pnpm install`

## Tasks
- [ ] Create `src/adapters/pnpm.rs` with `PnpmAdapter` struct
- [ ] Implement `detect()` checking for `pnpm-lock.yaml`
- [ ] Implement `setup()` that configures `.npmrc` and runs `pnpm install`
- [ ] Detect pnpm version via `pnpm --version` and warn if < 9.0
- [ ] Configure `enable-global-virtual-store=true` in worktree-local `.npmrc`
- [ ] Implement `teardown()` as no-op (symlinks cleaned by directory removal)
- [ ] Write integration test: 5 worktrees with shared virtual store
- [ ] Write test: `du -sh node_modules` < 1 MB per worktree
- [ ] Write test: `detect()` returns false when no `pnpm-lock.yaml`
- [ ] Write test: pnpm not installed returns clear error

## Technical Notes
- PRD Section 15 M4: "pnpm adapter: 5 worktrees share a single virtual store. `du -sh node_modules` in each shows < 1 MB (symlinks only)."
- PRD Section 15 M4 scope: "pnpm adapter: leverage `enableGlobalVirtualStore: true`. pnpm now has official multi-agent worktree documentation."
- `enableGlobalVirtualStore` (pnpm 9.x+) stores all packages in a single global content-addressable store and uses symlinks in `node_modules`. This is the key to avoiding the `claude-squad#260` bug.
- The `.npmrc` file in the worktree root takes precedence over global settings, making this safe for per-worktree configuration.
- Bug regression: `claude-squad#260` -- 5 worktrees x 2 GB `node_modules` = 10 GB wasted.

## Test Hints
- Create a test project with a `package.json` and `pnpm-lock.yaml`, create 5 worktrees with the pnpm adapter, measure `node_modules` size in each
- Verify symlinks: `find node_modules -type l | wc -l` should be > 0, `find node_modules -type f | wc -l` should be minimal
- Test without pnpm installed: verify error message mentions pnpm installation
- Test with pnpm < 9.0: verify warning about version

## Dependencies
- ISO-2.1 (EcosystemAdapter trait)
- ISO-2.3 (ShellCommandAdapter -- pattern for shell command execution)

## Estimated Effort
L

## Priority
P3

## Traceability
- PRD: Section 15 M4 (Ship criteria -- pnpm adapter)
- FR: FR-P3-001
- Bug regression: claude-squad#260 (5 worktrees x 2 GB node_modules)
- M4 ship criterion: "5 worktrees share single virtual store, du -sh node_modules < 1 MB each"
- QA ref: QA-4.1-001 through QA-4.1-004
