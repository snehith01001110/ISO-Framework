# Story 1.2: Complete Type System

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a library consumer, I want all public types from the PRD to be defined and compiling so that I can write code against stable type signatures before any logic is implemented.

## Description
Implement every public type defined in PRD Section 4: `WorktreeHandle`, `WorktreeState`, `ReflinkMode`, `CopyOutcome`, `Config`, `CreateOptions`, `DeleteOptions`, `GcOptions`, `GcReport`, `GitCapabilities`, `GitVersion`, `PortLease`, and `WorktreeError`. All structs must use `#[non_exhaustive]` (Appendix A rule 12). All enum variants and field names must match the PRD exactly. No business logic -- types only.

## Acceptance Criteria
- [ ] All 13 types from PRD Section 4 compile without errors
- [ ] All structs are marked `#[non_exhaustive]`
- [ ] `WorktreeHandle` has all 12 fields with correct Rust types as specified in PRD Section 4.1
- [ ] `WorktreeState` has all 9 variants: `Pending`, `Creating`, `Active`, `Merging`, `Deleting`, `Deleted`, `Orphaned`, `Broken`, `Locked`
- [ ] `WorktreeError` has all 22 variants with correct field types matching PRD Section 4.10
- [ ] `Config::default()` returns values matching PRD Section 4.4 defaults
- [ ] `GcOptions::default()` returns `dry_run: true` per PRD Section 4.7
- [ ] `GitVersion::MINIMUM`, `GitVersion::HAS_LIST_NUL`, `GitVersion::HAS_REPAIR`, `GitVersion::HAS_MERGE_TREE_WRITE` constants match PRD Section 4.8
- [ ] `ReflinkMode::default()` returns `Preferred`
- [ ] Derive macros match PRD: `Debug`, `Clone` on all types; `PartialEq`, `Eq`, `Hash` on `WorktreeState`; `serde::Serialize`, `serde::Deserialize` on `PortLease`; `PartialOrd`, `Ord` on `GitVersion`
- [ ] `cargo clippy -- -D warnings` passes after type definitions

## Tasks
- [ ] Create `worktree-core/src/types.rs` with `WorktreeHandle`, `WorktreeState`, `ReflinkMode`, `CopyOutcome`, `Config`, `CreateOptions`, `DeleteOptions`, `GcOptions`, `GcReport`, `GitCapabilities`, `GitVersion`, `PortLease`
- [ ] Create `worktree-core/src/error.rs` with `WorktreeError` enum using `thiserror` derives
- [ ] Implement `Default` for `Config` with all values from PRD Section 4.4
- [ ] Implement `Default` for `GcOptions` with `dry_run: true`
- [ ] Implement `Default` for `CreateOptions` and `DeleteOptions`
- [ ] Implement `Default` for `ReflinkMode` returning `Preferred`
- [ ] Add `GitVersion` associated constants: `MINIMUM = (2,20,0)`, `HAS_LIST_NUL = (2,36,0)`, `HAS_REPAIR = (2,30,0)`, `HAS_MERGE_TREE_WRITE = (2,38,0)`
- [ ] Add `GitCryptStatus` enum from PRD Section 8.3: `NotUsed`, `LockedNoKey`, `Locked`, `Unlocked`
- [ ] Re-export all types from `lib.rs`
- [ ] Write compile-only test: `cargo test` confirming all types instantiate

## Technical Notes
- PRD Section 4.10: `WorktreeError` uses `thiserror = "2"` for `#[error(...)]` derive
- PRD Section 4.9: `PortLease` uses `chrono::DateTime<chrono::Utc>` for timestamps and `serde` derives
- `#[non_exhaustive]` is on all public structs AND `WorktreeState` and `WorktreeError` enums per Appendix A rule 12
- Do not rename any variant or field; PRD Section "Rules for implementers" forbids it
- `EcosystemAdapter` trait definition belongs here but has no implementors yet

## Test Hints
- Compile-only tests: instantiate each type with dummy values and assert it compiles
- Test `Config::default()` field values match PRD spec
- Test `GitVersion` ordering: `MINIMUM < HAS_REPAIR < HAS_LIST_NUL < HAS_MERGE_TREE_WRITE`

## Dependencies
ISO-1.1

## Estimated Effort
L

## Priority
P0

## Traceability
- PRD: Section 4 (Complete Type System), Section 6 (EcosystemAdapter Trait)
- FR: FR-P0-001 (WorktreeHandle), FR-P0-002 (WorktreeState), FR-P0-003 (Config), FR-P0-004 (WorktreeError)
- Appendix A invariant: Rule 12 (all public structs `#[non_exhaustive]`)
- Bug regression: N/A
- QA ref: Compile-only
