# Story 2.10: M2 Integration Test Suite

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a maintainer, I want an integration test suite that validates all M2 ship criteria end-to-end so that I can confidently declare the milestone complete.

## Description
Build a comprehensive integration test suite that exercises all M2 ship criteria from PRD Section 15. The tests cover: `wt create --setup` with `ShellCommandAdapter` running `npm install` on a real Node.js project, `wt create --setup` with `DefaultAdapter` copying `.env`, `.DS_Store` deletion before `git worktree remove` on macOS, `wt attach` port recovery from stale entries, and the 20-worktree port collision test. These tests run in CI and serve as the go/no-go gate for M2 release.

## Acceptance Criteria
- [ ] Integration test: `wt create --setup` with ShellCommandAdapter runs `npm install` and produces a working `node_modules` directory
- [ ] Integration test: `wt create --setup` with DefaultAdapter copies `.env` from source worktree to new worktree
- [ ] Integration test: on macOS, `wt delete` succeeds when `.DS_Store` is present in worktree root
- [ ] Integration test: `wt attach` on a stale entry recovers the original port number
- [ ] Integration test: 20 worktrees created with `--port` all receive unique port numbers
- [ ] All tests run in CI (GitHub Actions) on both macOS and Linux runners
- [ ] macOS-specific tests are gated with `#[cfg(target_os = "macos")]`
- [ ] Test suite completes in under 5 minutes on CI
- [ ] Each test cleans up all created worktrees and temp directories on completion (even on failure)
- [ ] Test failures include diagnostic output (git version, OS info, state.json contents)

## Tasks
- [ ] Create `tests/m2_integration.rs` with all M2 ship criteria tests
- [ ] Implement test fixture: init a real git repo with a `package.json` for npm install test
- [ ] Implement test fixture: init a real git repo with `.env` for DefaultAdapter test
- [ ] Implement test fixture: create 20 worktrees and collect port assignments
- [ ] Implement macOS-gated test: create `.DS_Store` in worktree, run delete, assert success
- [ ] Implement attach recovery test: create -> gc -> attach -> verify port
- [ ] Add cleanup guard (Drop impl) that removes all test worktrees even on panic
- [ ] Add diagnostic output on test failure (git version, state.json dump)
- [ ] Configure GitHub Actions to run M2 tests on macOS and Linux
- [ ] Add test timeout of 5 minutes to prevent CI hangs

## Technical Notes
- These tests require a real git binary (not mocked) and real filesystem operations.
- The `npm install` test requires Node.js and npm to be available on the CI runner. Use a minimal `package.json` with zero dependencies to keep it fast.
- The 20-worktree test should use a dedicated port range to avoid conflicts with other tests running in parallel.
- Use `tempfile::TempDir` for test repos to ensure cleanup.
- The `.DS_Store` test only runs on macOS (`#[cfg(target_os = "macos")]`).
- PRD Section 15 M2 ship criteria are the authoritative acceptance criteria.

## Test Hints
- npm install test: create `package.json` with `{ "name": "test", "version": "1.0.0" }`, verify `node_modules` directory exists after setup
- .env test: write `.env` with `DB_HOST=localhost`, verify copied file has identical content
- Port collision test: `assert_eq!(ports.into_iter().collect::<HashSet<_>>().len(), 20)`
- Attach recovery: create with port 3142, run gc to evict, attach same path, assert port == 3142
- .DS_Store test: `std::fs::write(worktree_path.join(".DS_Store"), &[0u8; 100]).unwrap()`

## Dependencies
- ISO-2.1 (EcosystemAdapter trait)
- ISO-2.2 (DefaultAdapter)
- ISO-2.3 (ShellCommandAdapter)
- ISO-2.4 (wt create --setup CLI)
- ISO-2.5 (Port Allocation CLI)
- ISO-2.6 (macOS .DS_Store Handling)
- ISO-2.8 (wt attach Port Recovery)

## Estimated Effort
L

## Priority
P1

## Traceability
- PRD: Section 15 M2 (Ship Criteria)
- FR: FR-P1-015
- M2 ship criterion: all five criteria from PRD Section 15 M2
- QA ref: QA-2.10-001 through QA-2.10-005
