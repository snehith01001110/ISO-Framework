# Epic 4: Ecosystem Integration

## Summary
Deliver ecosystem-specific adapters (pnpm, uv, Cargo), replace the CLI-based conflict detection path with `gix::Repository::merge_trees()` behind a feature flag, provide language bindings for Node.js (napi-rs) and Python (PyO3, stretch goal), implement worktree pooling for instant checkout, validate external integration by having at least one third-party project (Claude Squad or workmux) consume `iso-code` as a library, and publish a documentation site with mdBook.

## Goals
- pnpm adapter leveraging `enableGlobalVirtualStore: true` so 5 worktrees share a single virtual store with < 1 MB `node_modules` each (symlinks only).
- uv adapter running `uv venv && uv pip install -r requirements.txt` for per-worktree Python venvs in under 10 seconds.
- Cargo adapter setting per-worktree `target` directories; explicitly not sharing `CARGO_TARGET_DIR` across worktrees.
- `gix::Repository::merge_trees()` integration behind `features = ["conflict-detection-gix"]` replacing the CLI fallback path.
- napi-rs Node.js binding generating TypeScript types; published to npm as `@iso-code/node`; tested on Node 18+.
- PyO3 Python binding (stretch goal) installable via `pip install iso-code`; tested on Python 3.9+.
- Worktree pooling with `PoolConfig { size, base_branch }` and `acquire_from_pool()` / `release_to_pool()` APIs; pool of 5 available in < 1 second.
- At least one external project (Claude Squad or workmux) consuming `iso-code` as a library dependency.
- mdBook documentation site with API docs, integration guides, and config snippet reference.

## Dependencies
Epic 3: Conflict Intelligence (all stories ISO-3.1 through ISO-3.8)

## Ship Criteria
- pnpm adapter: 5 worktrees share single virtual store, `du -sh node_modules` < 1 MB each.
- uv adapter: worktree with `requirements.txt` fully installed in < 10 s.
- Node.js package published to npm as `@iso-code/node`.
- Worktree pool of 5 worktrees available in < 1 s.

## Stories
- ISO-4.1: pnpm Adapter
- ISO-4.2: uv Adapter
- ISO-4.3: Cargo Adapter
- ISO-4.4: gix Conflict Detection
- ISO-4.5: napi-rs Node.js Binding
- ISO-4.6: PyO3 Python Binding (Stretch)
- ISO-4.7: Worktree Pooling
- ISO-4.8: External Integration
- ISO-4.9: Documentation Site

## Duration
Weeks 17-20
