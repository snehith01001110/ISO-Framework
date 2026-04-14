# Story 2.3: ShellCommandAdapter

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a developer, I want to run arbitrary shell commands when worktrees are created or deleted so that I can automate environment setup tasks like `npm install`, database seeding, or Docker container provisioning.

## Description
Implement `ShellCommandAdapter` as a built-in `EcosystemAdapter` that executes arbitrary shell commands at create and delete time. The adapter has three configurable hook points: `post_create`, `pre_delete`, and `post_delete`. Commands receive all `WORKTREE_CORE_*` environment variables. Command execution has a configurable timeout, captures stderr for error reporting, and runs in the worktree directory as the working directory. This mirrors Cursor's `.cursor/worktrees.json` and workmux's `post_create` hooks.

## Acceptance Criteria
- [ ] `ShellCommandAdapter` struct defined with `post_create: Option<String>`, `pre_delete: Option<String>`, `post_delete: Option<String>` fields matching PRD Section 6.1
- [ ] `setup()` executes `post_create` command if set, with worktree directory as CWD
- [ ] `teardown()` executes `pre_delete` command if set, then `post_delete` after `git worktree remove`
- [ ] All `WORKTREE_CORE_*` environment variables are injected (PATH, BRANCH, REPO, NAME, PORT, UUID)
- [ ] CCManager compatibility variables injected (CCMANAGER_WORKTREE_PATH, CCMANAGER_BRANCH_NAME, CCMANAGER_GIT_ROOT)
- [ ] workmux compatibility variables injected (WM_WORKTREE_PATH, WM_PROJECT_ROOT)
- [ ] Commands have a configurable timeout (default 120 seconds); timeout returns an error
- [ ] stderr output is captured and included in error messages on command failure
- [ ] stdout of commands is suppressed (not forwarded to the caller) -- stderr only for diagnostics
- [ ] Non-zero exit code from a command returns `WorktreeError` with the exit code and stderr
- [ ] `detect()` always returns `false` (ShellCommandAdapter is explicitly configured, not auto-detected)
- [ ] `name()` returns `"shell-command"`

## Tasks
- [ ] Implement `ShellCommandAdapter` struct in `src/adapters/shell_command.rs`
- [ ] Implement `EcosystemAdapter` trait for `ShellCommandAdapter`
- [ ] Use `std::process::Command` with `.current_dir(worktree_path)` and `.envs()` for env vars
- [ ] Implement timeout using `std::process::Child::wait_timeout()` or spawning with `tokio::time::timeout` if async
- [ ] Capture stderr via `Stdio::piped()` and include in error on failure
- [ ] Redirect stdout to `Stdio::null()` to prevent adapter output from leaking to caller
- [ ] Handle `post_delete` correctly -- it runs after `git worktree remove`, so the worktree dir no longer exists; CWD should be repo root
- [ ] Write unit test: post_create with `echo` command succeeds
- [ ] Write unit test: command timeout returns error
- [ ] Write unit test: non-zero exit code returns error with stderr content
- [ ] Write integration test: `npm install` in a Node.js project worktree

## Technical Notes
- PRD Section 6.1: `pub struct ShellCommandAdapter { pub post_create: Option<String>, pub pre_delete: Option<String>, pub post_delete: Option<String> }`.
- Commands are executed via the system shell (`/bin/sh -c` on Unix, `cmd /C` on Windows).
- The `post_delete` hook runs after the worktree is removed, so its CWD must be the repo root, not the (now-deleted) worktree path.
- Timeout default of 120s is generous for `npm install` but prevents runaway processes.
- The M2 ship criterion requires `npm install` via ShellCommandAdapter to succeed.

## Test Hints
- Test `post_create` with a command that writes a marker file; verify the file exists in worktree
- Test timeout by running `sleep 999` with a 1-second timeout; verify error is returned
- Test stderr capture: run a command that writes to stderr and exits non-zero; verify stderr in error message
- Test environment variables: run `env | grep WORKTREE_CORE` and verify all 6 are present
- Integration test: create a minimal `package.json`, run `npm install` via ShellCommandAdapter, verify `node_modules` exists

## Dependencies
- ISO-2.1 (EcosystemAdapter trait definition)

## Estimated Effort
M

## Priority
P1

## Traceability
- PRD: Section 6.1 (Built-in Adapters -- ShellCommandAdapter)
- FR: FR-P1-008
- M2 ship criterion: "wt create --setup bootstraps Node.js project using ShellCommandAdapter with npm install"
- QA ref: QA-2.3-001 through QA-2.3-005
