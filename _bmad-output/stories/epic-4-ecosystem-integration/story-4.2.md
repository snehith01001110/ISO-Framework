# Story 4.2: uv Adapter

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a Python developer, I want worktree creation to automatically set up a virtual environment and install dependencies so that each worktree has an isolated, ready-to-use Python environment.

## Description
Implement a uv-specific `EcosystemAdapter` that detects Python projects (via `requirements.txt`, `pyproject.toml`, or `setup.py`) and creates a per-worktree virtual environment using `uv venv` followed by `uv pip install -r requirements.txt`. The `uv` tool is dramatically faster than pip (seconds vs. minutes for typical projects), making per-worktree venvs practical. Each worktree gets its own isolated venv to prevent package version conflicts.

## Acceptance Criteria
- [ ] `UvAdapter` implements `EcosystemAdapter` trait
- [ ] `detect()` returns `true` when `requirements.txt`, `pyproject.toml`, or `setup.py` exists
- [ ] `setup()` runs `uv venv` to create a virtual environment in the worktree
- [ ] `setup()` runs `uv pip install -r requirements.txt` if `requirements.txt` exists
- [ ] `setup()` runs `uv pip install -e .` if `pyproject.toml` with `[project]` section exists
- [ ] Virtual environment is created at `<worktree>/.venv/` (standard convention)
- [ ] Full setup completes in under 10 seconds for a typical project (M4 ship criterion)
- [ ] `teardown()` removes the `.venv/` directory
- [ ] `name()` returns `"uv"`
- [ ] If `uv` is not installed, `setup()` returns error with installation instructions
- [ ] `ISO_CODE_*` environment variables are available during setup commands

## Tasks
- [ ] Create `src/adapters/uv.rs` with `UvAdapter` struct
- [ ] Implement `detect()` checking for `requirements.txt`, `pyproject.toml`, or `setup.py`
- [ ] Implement `setup()` with `uv venv .venv` command
- [ ] Implement dependency installation: `uv pip install -r requirements.txt`
- [ ] Implement editable install for pyproject.toml projects: `uv pip install -e .`
- [ ] Detect `uv` availability via `uv --version`
- [ ] Implement `teardown()` removing `.venv/` directory
- [ ] Write test: full setup with `requirements.txt` completes in < 10s
- [ ] Write test: `.venv/` directory exists after setup
- [ ] Write test: `detect()` returns false for non-Python projects
- [ ] Write test: `uv` not installed returns clear error

## Technical Notes
- PRD Section 15 M4: "uv adapter: per-worktree venvs. `uv venv && uv pip install -r requirements.txt` takes seconds vs. minutes."
- PRD Section 15 M4 ship criterion: "worktree with `requirements.txt` fully installed in < 10 seconds."
- `uv` is the Astral-sh Python package installer. It is 10-100x faster than pip for package installation.
- Virtual environments must be per-worktree (not shared) to avoid conflicts between branches that may have different dependency versions.
- The `.venv/` path is the Python convention used by VS Code, PyCharm, and other IDEs for auto-detection.
- `uv venv` creates a venv in the current directory. `uv pip install` installs into the active venv.

## Test Hints
- Create a `requirements.txt` with a few small packages (e.g., `click`, `rich`), time the full setup, assert < 10s
- Verify `.venv/bin/python` exists after setup (Unix) or `.venv/Scripts/python.exe` (Windows)
- Verify installed packages: `.venv/bin/pip list` should show the installed packages
- Test without `uv`: mock `uv` as missing, verify error message includes install instructions
- Test `teardown()`: verify `.venv/` is removed

## Dependencies
- ISO-2.1 (EcosystemAdapter trait)
- ISO-2.3 (ShellCommandAdapter -- pattern for shell command execution)

## Estimated Effort
M

## Priority
P3

## Traceability
- PRD: Section 15 M4 (Ship criteria -- uv adapter)
- FR: FR-P3-002
- M4 ship criterion: "worktree with requirements.txt fully installed in < 10 s"
- QA ref: QA-4.2-001 through QA-4.2-004
