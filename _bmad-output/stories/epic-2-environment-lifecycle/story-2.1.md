# Story 2.1: EcosystemAdapter Trait

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a library consumer, I want a stable trait defining how ecosystem adapters hook into worktree lifecycle events so that I can implement custom adapters for my project's toolchain.

## Description
Define the `EcosystemAdapter` trait exactly as specified in PRD Section 6. The trait has four methods: `name()`, `detect()`, `setup()`, and `teardown()`, plus a default `branch_name()` identity method. The trait must be `Send + Sync` so adapters can be shared across threads. The `setup()` method receives both the new worktree path and the source worktree path, and the library must inject all `WORKTREE_CORE_*` environment variables before calling it. Define `AdapterError` variants for setup failure, teardown failure, timeout, and missing dependency.

## Acceptance Criteria
- [ ] `EcosystemAdapter` trait defined in `src/adapter.rs` with `name()`, `detect()`, `setup()`, `teardown()`, and default `branch_name()` methods matching PRD Section 6 signatures exactly
- [ ] Trait is `Send + Sync`
- [ ] `setup()` receives `worktree_path: &Path` and `source_worktree: &Path` parameters
- [ ] All six `WORKTREE_CORE_*` environment variables documented in trait doc comments: `WORKTREE_CORE_PATH`, `WORKTREE_CORE_BRANCH`, `WORKTREE_CORE_REPO`, `WORKTREE_CORE_NAME`, `WORKTREE_CORE_PORT`, `WORKTREE_CORE_UUID`
- [ ] CCManager and workmux compatibility env vars documented: `CCMANAGER_WORKTREE_PATH`, `CCMANAGER_BRANCH_NAME`, `CCMANAGER_GIT_ROOT`, `WM_WORKTREE_PATH`, `WM_PROJECT_ROOT`
- [ ] `WorktreeError` extended with adapter-specific variants (setup failed, teardown failed, timeout, missing dependency)
- [ ] `Manager::create()` calls `adapter.setup()` when `CreateOptions::setup = true`
- [ ] `Manager::delete()` calls `adapter.teardown()` before `git worktree remove`
- [ ] On `setup()` failure after `git worktree add` succeeds, `git worktree remove --force` is called before returning error (PRD Section 7, invariant 6)

## Tasks
- [ ] Create `src/adapter.rs` with the `EcosystemAdapter` trait definition
- [ ] Add adapter-specific error variants to `WorktreeError` in `src/error.rs`
- [ ] Implement environment variable injection helper that sets all `WORKTREE_CORE_*` and compatibility variables before calling `setup()`
- [ ] Wire `adapter.setup()` into `Manager::create()` at step 6 (after post-create verification, before state transition to Active)
- [ ] Wire `adapter.teardown()` into `Manager::delete()` before `git worktree remove`
- [ ] Add `adapter` field to `Manager` struct (type: `Option<Box<dyn EcosystemAdapter>>`)
- [ ] Write unit tests for env var injection with mock adapter
- [ ] Write test verifying cleanup on setup failure (git worktree remove --force called)

## Technical Notes
- PRD Section 6 is the authoritative reference for the trait signature. Do not add or rename methods.
- The `branch_name()` default method returns `input.to_string()` -- the core library never calls this internally. Only adapters that opt in use it.
- Environment variables must be set via `std::process::Command::envs()` when shelling out for `ShellCommandAdapter`, and via `std::env::set_var()` for in-process adapters.
- The adapter field on `Manager` should be `Option<Box<dyn EcosystemAdapter>>` to allow runtime registration.

## Test Hints
- Mock adapter that records calls to `detect()`, `setup()`, `teardown()` with assertions on call order
- Test that `setup()` failure triggers `git worktree remove --force` cleanup
- Test that all 11 environment variables are present during `setup()` call
- Test that `teardown()` is called before `git worktree remove` during delete

## Dependencies
- ISO-1.2 (Complete Type System -- WorktreeError enum)
- ISO-1.6 (Manager::create() -- step 6 adapter hook point)
- ISO-1.7 (Manager::delete() -- teardown hook point)

## Estimated Effort
M

## Priority
P1

## Traceability
- PRD: Section 6 (EcosystemAdapter Trait)
- FR: FR-P1-006
- Appendix A invariant: Rule 6 (on failure after git worktree add, run git worktree remove --force)
- QA ref: QA-2.1-001 through QA-2.1-004
