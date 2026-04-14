# Story 3.1: git merge-tree --write-tree -z Output Parser

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As a library consumer, I want a robust parser for `git merge-tree --write-tree -z` output so that conflict detection accurately identifies all conflict types from git's NUL-delimited output.

## Description
Implement a parser for the NUL-delimited output of `git merge-tree --write-tree -z`. The parser must handle all token types in git's merge-tree output: conflicted file paths, conflict markers, rename/rename conflicts, modify/delete conflicts, directory/file conflicts, submodule conflicts, and binary file conflicts. Unknown or unrecognized conflict types must map to `ConflictType::Unknown` for forward compatibility with future git versions. The parser must also handle LFS pointer files, which can produce false-positive conflicts when the actual content is stored in LFS.

## Acceptance Criteria
- [ ] Parser correctly splits NUL-delimited output into individual conflict records
- [ ] All `ConflictType` variants are mapped from git output tokens
- [ ] Unknown conflict type strings map to `ConflictType::Unknown(String)` without panicking
- [ ] Parser handles empty output (no conflicts) and returns an empty conflict list
- [ ] Parser handles the tree SHA output line (first line of `--write-tree` output)
- [ ] LFS pointer files (content starts with `version https://git-lfs.github.com/spec/v1`) are flagged for filtering
- [ ] Parser handles malformed output gracefully (returns parse error, never panics)
- [ ] Parser handles multi-file conflicts (rename/rename where both sides rename differently)
- [ ] Performance: parser processes 1000 conflict records in under 100ms

## Tasks
- [ ] Create `src/conflict/parser.rs` module
- [ ] Define internal parse types for raw merge-tree output tokens
- [ ] Implement NUL-delimited tokenizer splitting on `\0`
- [ ] Implement token-to-ConflictType mapping for all known conflict categories
- [ ] Implement `ConflictType::Unknown(String)` fallback for unrecognized tokens
- [ ] Implement LFS pointer detection (check file content prefix)
- [ ] Handle the leading tree SHA line from `--write-tree` output
- [ ] Write unit tests with sample git merge-tree output for each conflict type
- [ ] Write unit test for unknown conflict type forward compatibility
- [ ] Write unit test for empty output (no conflicts)
- [ ] Write unit test for malformed/truncated output
- [ ] Write benchmark for 1000-record parsing performance

## Technical Notes
- `git merge-tree --write-tree -z` requires Git >= 2.38 (PRD Section 17, `GitVersion::HAS_MERGE_TREE_WRITE`).
- The `-z` flag makes output NUL-delimited, which is critical for paths containing spaces or special characters.
- PRD Section 7.2 lists the exact command: `git merge-tree --write-tree -z --stdin`.
- The `--write-tree` flag writes a result tree and outputs conflict information. The first line is the resulting tree SHA (or a special marker if conflicts exist).
- LFS pointer files are typically small (~130 bytes) text files starting with `version https://git-lfs.github.com/spec/v1`. When both sides modify LFS-tracked files, the conflict is on the pointer, not the actual content.
- Forward compatibility via `#[non_exhaustive]` on `ConflictType` and the `Unknown` variant ensures new git conflict types do not break existing consumers.

## Test Hints
- Capture real `git merge-tree --write-tree -z` output from a test repo with known conflicts and use as test fixtures
- Test rename/rename: branch A renames `foo.rs` to `bar.rs`, branch B renames `foo.rs` to `baz.rs`
- Test modify/delete: branch A modifies `config.yaml`, branch B deletes it
- Test content conflict: both branches modify the same lines of `main.rs`
- Test unknown type: inject a fake conflict type string, verify `ConflictType::Unknown` is returned
- Test LFS: create a `.gitattributes` with `*.bin filter=lfs`, create conflicting `.bin` changes

## Dependencies
- ISO-1.3 (Git Version Detection -- GitCapabilities.has_merge_tree_write)

## Estimated Effort
L

## Priority
P2

## Traceability
- PRD: Section 7.2 (Exact git Commands -- git merge-tree)
- PRD: Section 17 (Git Version Matrix -- merge-tree --write-tree at 2.38+)
- FR: FR-P2-001
- QA ref: QA-3.1-001 through QA-3.1-006
