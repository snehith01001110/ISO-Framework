# Story 3.8: Windows Full Platform

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As a Windows developer, I want worktree-core to fully support my platform so that I can use it with my Windows-based AI coding setup without encountering platform-specific failures.

## Description
Replace all Windows platform stubs (created in M1 as compile-only placeholders) with full implementations. This covers file locking via `LockFileEx` through the `fd-lock` crate, NTFS junction creation via the `junction` crate, disk usage calculation via `GetCompressedFileSizeW`, path handling with `dunce`, `core.longpaths = true` detection, and network drive detection via `GetDriveTypeW`. Establish Windows CI on GitHub Actions with `cargo test` running on a Windows Server 2019 runner.

## Acceptance Criteria
- [ ] `src/platform/windows.rs` fully implemented (no more `todo!()` or `unimplemented!()` stubs)
- [ ] File locking uses `fd-lock` crate with `LockFileEx` backend on Windows
- [ ] Junction creation uses `junction` crate v1.4.2 -- no admin privileges required
- [ ] Junctions permitted across drive volumes (PRD Appendix A rule 9)
- [ ] Network junction targets blocked: `check_not_network_junction_target()` rejects UNC paths (`\\server\share`)
- [ ] Disk usage calculation uses `GetCompressedFileSizeW` via `filesize` crate
- [ ] All paths passed to git use `dunce::simplified()` (no `\\?\` prefix)
- [ ] `core.longpaths` detection: warn if not set and worktree path > 200 chars
- [ ] Network filesystem detection uses `GetDriveTypeW` returning `DRIVE_REMOTE`
- [ ] GitHub Actions Windows CI job configured and passing
- [ ] `cargo test` passes on Windows Server 2019 runner
- [ ] `cargo clippy -- -D warnings` clean on Windows target

## Tasks
- [ ] Implement `is_network_filesystem()` in `src/platform/windows.rs` using `GetDriveTypeW`
- [ ] Implement `check_not_network_junction_target()` -- reject paths starting with `\\` (UNC)
- [ ] Implement `calculate_worktree_disk_usage()` using `filesize` crate with `GetCompressedFileSizeW`
- [ ] Implement retry logic for locked files: retry with backoff when `ERROR_SHARING_VIOLATION` is returned
- [ ] Implement `core.longpaths` detection via `git config --get core.longpaths`
- [ ] Remove all `todo!()`, `unimplemented!()`, and `unreachable!()` stubs from `windows.rs`
- [ ] Add GitHub Actions workflow job: `windows-ci` with `runs-on: windows-2019`
- [ ] Install git on Windows runner and configure `core.longpaths = true`
- [ ] Run `cargo test` and `cargo clippy` on Windows runner
- [ ] Write test: junction creation across drive volumes succeeds
- [ ] Write test: junction to UNC path is blocked
- [ ] Write test: locked file retry logic works
- [ ] Write test: disk usage calculation returns valid size on NTFS

## Technical Notes
- PRD Section 11.3 is the authoritative reference for all Windows platform behavior.
- `junction` crate v1.4.2 creates junctions without admin privileges. Junctions CAN span volumes (Appendix A rule 9). Junctions CANNOT target network shares.
- `fd-lock` uses `LockFileEx` on Windows which provides mandatory locking (blocks all other readers/writers). The lock target is `state.lock`, not `state.json` directly.
- `GetDriveTypeW` returns `DRIVE_REMOTE` (value 4) for network drives.
- Windows `ERROR_SHARING_VIOLATION` (error code 32) is the equivalent of Unix `EACCES` for locked files. Retry with exponential backoff.
- `core.symlinks = false` is the Windows git default. Do not attempt symlink creation on Windows.
- GitHub Actions `windows-2019` runner includes git, Rust toolchain, and MSVC build tools.

## Test Hints
- Test junction creation: create a junction from `C:\test_junction` to `C:\test_target`, verify it resolves correctly
- Test UNC rejection: attempt junction to `\\localhost\share`, verify `NetworkJunctionTarget` error
- Test locked file retry: lock a file with `LockFileEx`, attempt to acquire in another thread, verify retry logic
- Test disk usage: create a known-size file, verify `calculate_worktree_disk_usage()` returns correct size
- CI test: verify `cargo test` passes on `windows-2019` runner in GitHub Actions

## Dependencies
- ISO-1.1 (Workspace Scaffolding -- Windows stubs created)
- ISO-2.7 (Windows MAX_PATH -- dunce integration)
- ISO-1.10 (State Persistence -- fd-lock locking protocol)

## Estimated Effort
XL

## Priority
P2

## Traceability
- PRD: Section 11.3 (Windows platform implementation)
- PRD: Section 14 (Crate Dependencies -- junction, fd-lock, filesize, dunce)
- FR: FR-P2-008
- Appendix A invariant: Rule 9 (Windows junctions CAN span volumes)
- M3 ship criterion: "Windows CI passing (cargo test on Windows Server 2019 runner)"
- QA ref: QA-3.8-001 through QA-3.8-008
