# Story 2.2: DefaultAdapter

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a developer, I want worktree creation to automatically copy environment files like `.env` from the main worktree so that new worktrees have the configuration they need to run immediately.

## Description
Implement `DefaultAdapter` as a built-in `EcosystemAdapter` that copies files from a configurable list into the new worktree. The adapter resolves relative paths against `source_worktree` and copies them to the same relative location in the new worktree. Missing source files are logged as warnings but do not fail the setup. The primary use case is copying `.env`, `.env.local`, and similar configuration files that are `.gitignore`d.

## Acceptance Criteria
- [ ] `DefaultAdapter` struct defined with `files_to_copy: Vec<PathBuf>` field matching PRD Section 6.1
- [ ] `detect()` returns `true` if any file in `files_to_copy` exists in `source_worktree`
- [ ] `setup()` copies each file from `source_worktree/<relative>` to `worktree_path/<relative>`
- [ ] Parent directories are created automatically if the target path has intermediate directories (e.g., `config/local.toml`)
- [ ] Missing source files are logged as warnings and skipped -- they do not cause `setup()` to return an error
- [ ] File permissions are preserved on Unix platforms
- [ ] `teardown()` is a no-op (copied files are removed when `git worktree remove` deletes the directory)
- [ ] `name()` returns `"default"`
- [ ] Copy respects `ReflinkMode` from `CreateOptions` (uses `reflink-copy` crate for CoW when available)

## Tasks
- [ ] Implement `DefaultAdapter` struct in `src/adapters/default.rs`
- [ ] Implement `EcosystemAdapter` trait for `DefaultAdapter`
- [ ] Use `reflink_copy::reflink_or_copy()` for file copying to leverage CoW where available
- [ ] Create parent directories with `std::fs::create_dir_all()` before copying
- [ ] Preserve file permissions on Unix via `std::fs::set_permissions()`
- [ ] Add warning log for missing source files using `tracing::warn!`
- [ ] Write unit test: `.env` copied successfully
- [ ] Write unit test: missing source file skipped with warning
- [ ] Write unit test: intermediate directory created for `config/local.toml`
- [ ] Write unit test: `detect()` returns false when no files exist in source

## Technical Notes
- PRD Section 6.1 defines the struct: `pub struct DefaultAdapter { pub files_to_copy: Vec<PathBuf> }`.
- Use the `reflink-copy` crate (PRD Section 14) for CoW-aware copying. On APFS and Btrfs this is nearly instant.
- Paths in `files_to_copy` are relative. Resolve against `source_worktree` for source, against `worktree_path` for destination.
- The `.env` copy use case is an M2 ship criterion (PRD Section 15).

## Test Hints
- Create a temp dir with `.env` and `config/local.toml`, run `setup()`, verify files exist at destination
- Verify missing file produces a warning log entry but `setup()` returns `Ok(())`
- Verify `detect()` returns `true` when `.env` exists, `false` when none of `files_to_copy` exist
- On macOS, verify CoW via `clonefile` is attempted (check `CopyOutcome::Reflinked` if on APFS)

## Dependencies
- ISO-2.1 (EcosystemAdapter trait definition)

## Estimated Effort
M

## Priority
P1

## Traceability
- PRD: Section 6.1 (Built-in Adapters -- DefaultAdapter)
- FR: FR-P1-007
- M2 ship criterion: "wt create --setup copies .env using DefaultAdapter"
- QA ref: QA-2.2-001 through QA-2.2-004
