# Epic 3: Conflict Intelligence

## Summary
Implement conflict detection using `git merge-tree --write-tree -z`, expose it through the `wt check` CLI subcommand and the MCP `conflict_check` tool, add an HTTP transport for the MCP server (for VS Code Dev Containers and SSH setups), and bring Windows to full platform parity by replacing all stubs with real implementations and establishing Windows CI on GitHub Actions.

## Goals
- NUL-delimited output parser for `git merge-tree --write-tree -z` covering all `ConflictType` variants including `Unknown` for forward compatibility.
- `ConflictReport` and `ConflictType` types with `#[non_exhaustive]` and LFS pointer false-positive handling.
- `manager.check_conflicts(repo, branch_a, branch_b)` for single-pair conflict checks.
- `manager.conflict_matrix(repo)` for batch conflict checking using `git merge-tree --stdin`; completes in under 10 seconds for 20 pairs.
- `wt check` CLI subcommand requiring Git >= 2.38 with graceful degradation on older git.
- MCP `conflict_check` tool replacing the `not_implemented` stub with structured JSON results.
- Axum-based HTTP MCP transport behind `feature = ["mcp-http"]` flag; tested in Dev Container.
- Windows platform: all stubs replaced with real implementations; `cargo test` passing on Windows Server 2019 runner in GitHub Actions CI.

## Dependencies
Epic 2: Environment Lifecycle (all stories ISO-2.1 through ISO-2.10)

## Ship Criteria
- At least one external project consuming `iso-code` as a library dependency.
- `wt check` correctly identifies conflicts for 20 merge scenarios.
- MCP HTTP transport responds correctly in VS Code Dev Container.
- Windows CI passing (`cargo test` on Windows Server 2019 runner).

## Stories
- ISO-3.1: git merge-tree Output Parser
- ISO-3.2: ConflictReport and ConflictType Types
- ISO-3.3: Single-Pair Conflict Check
- ISO-3.4: Conflict Matrix (Batch)
- ISO-3.5: wt check CLI Subcommand
- ISO-3.6: MCP conflict_check Tool
- ISO-3.7: HTTP MCP Transport
- ISO-3.8: Windows Full Platform

## Duration
Weeks 11-16
