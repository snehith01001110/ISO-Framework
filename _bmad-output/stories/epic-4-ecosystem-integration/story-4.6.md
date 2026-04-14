# Story 4.6: PyO3 Python Binding (Stretch)

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a Python developer, I want to use worktree-core as a pip-installable package so that I can integrate worktree management into Python-based AI agent frameworks.

## Description
Build a Python native extension using PyO3 that wraps the worktree-core Rust library. The package is installable via `pip install worktree-core` and provides a Pythonic API with type hints. This is a stretch goal -- it is only started if the Node.js binding (ISO-4.5) is complete and on schedule. The binding targets Python 3.9+ and uses maturin for build and publish to PyPI.

## Acceptance Criteria
- [ ] `pip install worktree-core` installs successfully on Python 3.9+
- [ ] Python API mirrors the Rust public API: `Manager`, `Config`, `CreateOptions`, etc.
- [ ] Type hints provided via `.pyi` stub files or inline type annotations
- [ ] `Manager.create()`, `Manager.delete()`, `Manager.list()`, `Manager.gc()` all accessible from Python
- [ ] `WorktreeHandle` returned as a Python dataclass or object with typed attributes
- [ ] Error types map to Python exceptions with descriptive messages
- [ ] Pre-built wheels for macOS, Linux, and Windows (x64 + arm64)
- [ ] Tests pass on Python 3.9, 3.10, 3.11, 3.12
- [ ] Package published to PyPI

## Tasks
- [ ] Create `worktree-core-python/` crate with PyO3 + maturin scaffolding
- [ ] Define `#[pyclass]` and `#[pymethods]` for public API types
- [ ] Map Rust types to Python types (PathBuf -> str, Option -> Optional)
- [ ] Map `WorktreeError` variants to Python exception classes
- [ ] Generate `.pyi` stub files for type checker support
- [ ] Configure maturin build matrix for target platforms
- [ ] Set up GitHub Actions workflow for wheel building and PyPI publish
- [ ] Write Python test: full lifecycle (create, list, delete)
- [ ] Write Python test: error handling with try/except
- [ ] Test on Python 3.9 and 3.12
- [ ] Write README with Python API documentation

## Technical Notes
- This is marked as a stretch goal in the PRD. Only start after ISO-4.5 (napi-rs) is complete.
- PyO3 + maturin is the standard approach for Rust-Python bindings. maturin handles wheel building and PyPI publishing.
- Python 3.9 is the minimum because Python 3.8 reached end-of-life in October 2024.
- Type hints via `.pyi` files are important for IDE support (VS Code, PyCharm) and type checkers (mypy, pyright).
- Pre-built wheels eliminate the need for a Rust toolchain on the consumer's machine.
- The `maturin-action` GitHub Action handles cross-platform wheel building.

## Test Hints
- Python test: `manager = Manager('/path/to/repo', Config()); handles = manager.list()`
- Verify type hints work: `reveal_type(manager.list())` should show `list[WorktreeHandle]`
- Test error handling: `with pytest.raises(WorktreeError): manager.create('bad/branch', ...)`
- Verify wheel installs without Rust: `pip install worktree_core-*.whl` on clean Python environment

## Dependencies
- ISO-4.5 (napi-rs Node.js Binding -- stretch goal gated on this being complete)
- ISO-1.2 (Complete Type System -- all public types to expose)

## Estimated Effort
XL

## Priority
P3

## Traceability
- PRD: Section 15 M4 (Scope -- stretch goal)
- FR: FR-P3-006
- QA ref: QA-4.6-001 through QA-4.6-004
