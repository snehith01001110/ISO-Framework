# Story 3.6: MCP conflict_check Tool

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As an AI coding agent using the MCP protocol, I want a `conflict_check` tool that returns structured conflict information so that I can automatically detect and report potential merge conflicts to the user.

## Description
Replace the `not_implemented` stub for the `conflict_check` MCP tool with a working implementation that returns structured JSON conflict results. The tool should accept optional branch parameters for single-pair checking or operate in matrix mode when no branches are specified. The response must be well-structured JSON that agents can parse programmatically. Tool annotations (`readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true`) ensure the tool does not prompt for user approval in Claude Code or Cursor.

## Acceptance Criteria
- [ ] `conflict_check` MCP tool returns structured JSON instead of `not_implemented` stub
- [ ] Tool accepts optional `branch_a` and `branch_b` parameters for single-pair mode
- [ ] When no branches specified, tool runs in matrix mode (all active worktree pairs)
- [ ] JSON response includes `conflicts` array with `path`, `conflict_type`, and `is_lfs_pointer` for each conflict
- [ ] JSON response includes `has_conflicts` boolean for quick checking
- [ ] JSON response includes `branch_a` and `branch_b` fields identifying the pair
- [ ] Tool annotations remain `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true`
- [ ] When Git < 2.38, tool returns a structured error with `upgrade_required: true` and version info
- [ ] Tool works correctly via stdio MCP transport

## Tasks
- [ ] Remove `not_implemented` stub from `conflict_check` tool handler
- [ ] Implement single-pair mode: parse `branch_a` and `branch_b` from tool arguments
- [ ] Implement matrix mode: call `Manager::conflict_matrix()` when no branches specified
- [ ] Serialize `ConflictReport` to JSON for the tool response
- [ ] Handle Git version error: return structured error JSON with upgrade guidance
- [ ] Verify tool annotations are correct in tool schema
- [ ] Write test: MCP request with branch_a and branch_b returns single-pair result
- [ ] Write test: MCP request without branches returns matrix result
- [ ] Write test: MCP request on Git < 2.38 returns structured error
- [ ] Write integration test: full MCP stdio roundtrip with conflict_check

## Technical Notes
- PRD Section 12.3: `conflict_check` is listed with `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true`.
- The tool was a stub returning `not_implemented` in M1 (ISO-1.13). This story replaces the stub with a working implementation.
- MCP tool responses are JSON objects. The response schema should match the `ConflictReport` serde serialization.
- Per MCP spec 2025-03-26+, read-only tools do not prompt for approval in Claude Code and Cursor, making conflict checking frictionless for agents.
- The structured error for Git < 2.38 should include `min_version: "2.38"`, `current_version: "<detected>"`, and `message: "..."`.

## Test Hints
- Send a valid MCP JSON-RPC request for `conflict_check` via stdio, verify response is valid JSON with expected fields
- Test with conflicting branches: verify `has_conflicts: true` and `conflicts` array is non-empty
- Test with clean branches: verify `has_conflicts: false` and `conflicts` array is empty
- Test version error: mock Git 2.37, verify `upgrade_required: true` in response

## Dependencies
- ISO-3.3 (Single-Pair Conflict Check)
- ISO-3.4 (Conflict Matrix)
- ISO-1.13 (MCP Server Skeleton -- stub to replace)

## Estimated Effort
M

## Priority
P2

## Traceability
- PRD: Section 12.3 (MCP Server -- conflict_check tool)
- FR: FR-P2-006
- M3 ship criterion: implicit -- MCP tool must work for agent integration validation
- QA ref: QA-3.6-001 through QA-3.6-004
