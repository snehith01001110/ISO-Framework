# Story 3.2: ConflictReport and ConflictType Types

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As a library consumer, I want well-defined conflict report types so that I can programmatically inspect merge conflicts between worktree branches and present them to users or agents.

## Description
Define the `ConflictReport` and `ConflictType` public types that represent the result of conflict detection. `ConflictType` must be `#[non_exhaustive]` to allow adding new variants without breaking downstream consumers. The types must cover all known git merge conflict categories: content conflicts, rename/rename, modify/delete, directory/file, submodule conflicts, binary file conflicts, and an `Unknown` variant for forward compatibility. `ConflictReport` aggregates conflicts for a branch pair and includes metadata about the merge-tree invocation.

## Acceptance Criteria
- [ ] `ConflictType` enum defined with all variants: `Content`, `RenameRename`, `ModifyDelete`, `DirectoryFile`, `Submodule`, `BinaryFile`, `Unknown(String)`
- [ ] `ConflictType` is `#[non_exhaustive]` per PRD Appendix A rule 12
- [ ] `ConflictReport` struct defined with fields: `branch_a`, `branch_b`, `conflicts: Vec<ConflictEntry>`, `has_conflicts: bool`, `merge_tree_sha: Option<String>`
- [ ] `ConflictEntry` struct with fields: `path`, `conflict_type`, `theirs_path` (for renames), `is_lfs_pointer`
- [ ] `is_lfs_pointer` field allows consumers to filter out LFS false positives
- [ ] All types derive `Debug`, `Clone`, and `serde::Serialize`
- [ ] All types are `#[non_exhaustive]`
- [ ] Types compile with `#[deny(missing_docs)]`

## Tasks
- [ ] Define `ConflictType` enum in `src/conflict/types.rs`
- [ ] Define `ConflictEntry` struct in `src/conflict/types.rs`
- [ ] Define `ConflictReport` struct in `src/conflict/types.rs`
- [ ] Add `#[non_exhaustive]` to all public types
- [ ] Implement `Display` for `ConflictType` (human-readable conflict descriptions)
- [ ] Implement `ConflictReport::has_real_conflicts()` method that filters out LFS false positives
- [ ] Add doc comments to all public fields and variants
- [ ] Write unit test: construct a ConflictReport with each ConflictType variant
- [ ] Write unit test: `has_real_conflicts()` returns false when all conflicts are LFS false positives
- [ ] Write serialization roundtrip test (serialize to JSON, deserialize back, compare)

## Technical Notes
- PRD Appendix A rule 12: "All public structs are `#[non_exhaustive]`. Do not remove this attribute."
- The `Unknown(String)` variant on `ConflictType` preserves the raw git output for conflict types not yet mapped, enabling forward compatibility with newer git versions.
- LFS pointer false positives: when both branches modify an LFS-tracked file, the merge-tree output shows a conflict on the pointer file (a ~130-byte text file), not the actual binary content. The `is_lfs_pointer` flag lets consumers decide how to handle these.
- `serde::Serialize` is needed for the MCP `conflict_check` tool JSON response (ISO-3.6).
- Consider `serde::Deserialize` as well for testing and potential future state persistence.

## Test Hints
- Construct a `ConflictReport` with mix of real conflicts and LFS false positives; verify `has_real_conflicts()` correctly filters
- Verify `ConflictType::Unknown("future-type".into())` round-trips through serde correctly
- Verify JSON serialization produces expected field names for MCP compatibility
- Test `Display` impl produces human-readable output for each variant

## Dependencies
- ISO-3.1 (git merge-tree Output Parser -- parser produces these types)

## Estimated Effort
M

## Priority
P2

## Traceability
- PRD: Section 4 (Complete Type System -- extension point for conflict types)
- FR: FR-P2-002
- Appendix A invariant: Rule 12 (all public structs #[non_exhaustive])
- QA ref: QA-3.2-001 through QA-3.2-004
