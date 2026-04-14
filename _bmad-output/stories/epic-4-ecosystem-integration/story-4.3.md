# Story 4.3: Cargo Adapter

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a Rust developer, I want each worktree to use its own `target` directory so that concurrent builds across worktrees do not interfere with each other.

## Description
Implement a Cargo-specific `EcosystemAdapter` that detects Rust projects (via `Cargo.toml`) and configures per-worktree target directories. The adapter explicitly does NOT share `CARGO_TARGET_DIR` across worktrees because Cargo has a known bug with path dependencies of the same name from different worktrees. Each worktree gets its own `target/` directory via a `.cargo/config.toml` override.

## Acceptance Criteria
- [ ] `CargoAdapter` implements `EcosystemAdapter` trait
- [ ] `detect()` returns `true` when `Cargo.toml` exists in the worktree
- [ ] `setup()` creates `.cargo/config.toml` in the worktree with `target-dir` pointing to a worktree-local path
- [ ] Target directory is set to `<worktree>/target` (the default, but made explicit to prevent `CARGO_TARGET_DIR` override)
- [ ] `CARGO_TARGET_DIR` environment variable is NOT set (explicitly unset if inherited)
- [ ] `teardown()` removes the `target/` directory to free disk space
- [ ] `name()` returns `"cargo"`
- [ ] Warning logged if `CARGO_TARGET_DIR` is set in the environment (potential conflict)
- [ ] `.cargo/config.toml` is created only if it does not already exist (do not overwrite user config)

## Tasks
- [ ] Create `src/adapters/cargo.rs` with `CargoAdapter` struct
- [ ] Implement `detect()` checking for `Cargo.toml`
- [ ] Implement `setup()` creating `.cargo/config.toml` with `[build] target-dir = "target"`
- [ ] Check for existing `.cargo/config.toml` and skip creation if present
- [ ] Unset `CARGO_TARGET_DIR` in the subprocess environment during setup
- [ ] Log warning if `CARGO_TARGET_DIR` is set in the inherited environment
- [ ] Implement `teardown()` removing the `target/` directory
- [ ] Write test: `.cargo/config.toml` created with correct content
- [ ] Write test: existing `.cargo/config.toml` is not overwritten
- [ ] Write test: `CARGO_TARGET_DIR` is unset during setup
- [ ] Write test: `detect()` returns false when no `Cargo.toml`

## Technical Notes
- PRD Section 15 M4: "Cargo adapter: use per-worktree `target` directories. Do NOT share `CARGO_TARGET_DIR` across worktrees -- cargo has a bug with path deps of the same name from different worktrees."
- The Cargo bug: when two worktrees share a `CARGO_TARGET_DIR` and have path dependencies with the same crate name but different source paths, Cargo can link the wrong dependency.
- `.cargo/config.toml` takes precedence over environment variables for `target-dir`.
- The `teardown()` removing `target/` can free significant disk space (often 1-10 GB for Rust projects).
- This adapter is deliberately simple -- no `cargo build` or `cargo fetch` during setup. The goal is configuration, not compilation.

## Test Hints
- Create a temp Rust project with `Cargo.toml`, run setup, verify `.cargo/config.toml` contains `target-dir = "target"`
- Set `CARGO_TARGET_DIR=/shared/target` in env, run setup, verify it is unset in the worktree context
- Create `.cargo/config.toml` before setup with custom content, verify it is preserved
- Test teardown: create a `target/` directory with some files, run teardown, verify it is removed

## Dependencies
- ISO-2.1 (EcosystemAdapter trait)

## Estimated Effort
M

## Priority
P3

## Traceability
- PRD: Section 15 M4 (Scope -- Cargo adapter)
- FR: FR-P3-003
- QA ref: QA-4.3-001 through QA-4.3-004
