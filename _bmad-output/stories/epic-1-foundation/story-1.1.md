# Story 1.1: Workspace Scaffolding

## Status
Done

## Epic
Epic 1: Foundation

## User Story
As a library contributor, I want a properly structured Cargo workspace with CI, linting, and crate feature flags so that all subsequent stories build on a reliable, reproducible foundation.

## Description
Initialize the three-crate Cargo workspace defined in PRD Section 3: `iso-code` (library), `iso-code-cli` (binary `wt`), and `iso-code-mcp` (binary MCP server). Set up GitHub Actions CI for macOS and Linux runners, enforce `cargo clippy -D warnings` as a gate, and create skeleton `Cargo.toml` files with the dependency list from PRD Section 14. Feature flags for optional platform modules and future `gix`/`git2` conflict detection must be declared but empty.

## Acceptance Criteria
- [ ] `cargo build` succeeds for all three crates on macOS and Linux
- [ ] `cargo clippy -- -D warnings` passes with zero warnings
- [ ] `cargo test` runs (may have zero tests) without error
- [ ] GitHub Actions workflow runs on push and PR for `macos-latest` and `ubuntu-latest`
- [ ] Workspace root `Cargo.toml` declares all three crate members
- [ ] `iso-code/Cargo.toml` lists all 14 dependencies from PRD Section 14 with exact version constraints
- [ ] Feature flags `conflict-detection` (for future `gix`/`git2`) and `windows` (platform module) exist but are empty
- [ ] `iso-code-cli` and `iso-code-mcp` depend on `iso-code` as a path dependency
- [ ] Rust MSRV set to 1.75 in all `Cargo.toml` files via `rust-version` field
- [ ] `.gitignore` excludes `target/` and platform artifacts

## Tasks
- [ ] Create repo root `Cargo.toml` with `[workspace]` containing members `["iso-code", "iso-code-cli", "iso-code-mcp"]`
- [ ] Create `iso-code/Cargo.toml` with all dependencies: `fd-lock = "4"`, `sysinfo = "0.37"`, `uuid = { version = "1", features = ["v4"] }`, `reflink-copy = "0.1"`, `junction = "1"`, `jwalk = "0.8"`, `filesize = "0.2"`, `directories = "6"`, `dunce = "1"`, `thiserror = "2"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `chrono = { version = "0.4", features = ["serde"] }`, `rand = "0.8"`, `sha2 = "0.10"`
- [ ] Create `iso-code/src/lib.rs` with placeholder module declarations matching PRD Section 3 file layout
- [ ] Create empty module files: `manager.rs`, `types.rs`, `error.rs`, `git.rs`, `guards.rs`, `lock.rs`, `state.rs`, `ports.rs`, `platform/mod.rs`, `platform/macos.rs`, `platform/linux.rs`, `platform/windows.rs`
- [ ] Create `iso-code-cli/Cargo.toml` and `iso-code-cli/src/main.rs` with minimal `fn main()`
- [ ] Create `iso-code-mcp/Cargo.toml` and `iso-code-mcp/src/main.rs` with minimal `fn main()`
- [ ] Add `.github/workflows/ci.yml` with matrix strategy for `macos-latest` and `ubuntu-latest`, running `cargo build`, `cargo clippy -- -D warnings`, and `cargo test`
- [ ] Add `.gitignore` with `target/`, `*.swp`, `.DS_Store`
- [ ] Declare feature flags in `iso-code/Cargo.toml`: `conflict-detection = []`, `windows = []`
- [ ] Set `rust-version = "1.75"` in all three `Cargo.toml` files
- [ ] Verify `cargo build --workspace` succeeds locally

## Technical Notes
- Crate structure matches PRD Section 3 exactly: `iso-code/`, `iso-code-cli/`, `iso-code-mcp/`
- `junction` crate is `#[cfg(target_os = "windows")]` only; mark it as an optional dependency gated on the `windows` feature flag
- `gix` and `git2` are explicitly excluded from v1.0 per PRD Section 14 ("reserved for v1.1 conflict detection")
- MSRV 1.75 per PRD header table
- CI must use `actions/checkout@v4` and `dtolnay/rust-toolchain@stable`

## Test Hints
- This story has no functional tests; it enables all subsequent test infrastructure
- Verify the CI pipeline runs green on a trivial commit
- `cargo clippy -- -D warnings` must be the gate, not just advisory

## Dependencies
None

## Estimated Effort
M

## Priority
P0

## Traceability
- PRD: Section 3 (Crate Structure), Section 14 (Crate Dependencies)
- FR: N/A (infrastructure)
- Appendix A invariant: N/A
- Bug regression: N/A
- QA ref: N/A -- this enables all other tests
