# Story 1.13: MCP Server Skeleton

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a Claude Code / Cursor / VS Code Copilot user, I want an MCP server exposing worktree operations as tools so that AI agents can manage worktrees through the standard MCP protocol without custom integration code.

## Description
Implement the `worktree-core-mcp` binary as a stdio MCP server with 6 tools defined in PRD Section 12.3. Each tool must have correct MCP annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`) per the MCP spec 2025-03-26+. The `conflict_check` tool returns `not_implemented` in v1.0. Transport is stdio only; HTTP transport is deferred to M3.

## Acceptance Criteria
- [ ] `worktree-core-mcp` binary starts and communicates via stdio (stdin/stdout JSON-RPC)
- [ ] 6 tools are registered: `worktree_list`, `worktree_status`, `conflict_check`, `worktree_create`, `worktree_delete`, `worktree_gc`
- [ ] `worktree_list` has annotations: `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true`
- [ ] `worktree_status` has annotations: `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true`
- [ ] `conflict_check` has annotations: `readOnlyHint: true`, `destructiveHint: false`, `idempotentHint: true`; returns `not_implemented` response
- [ ] `worktree_create` has annotations: `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: false`
- [ ] `worktree_delete` has annotations: `readOnlyHint: false`, `destructiveHint: true`, `idempotentHint: false`
- [ ] `worktree_gc` has annotations: `readOnlyHint: false`, `destructiveHint: true`, `idempotentHint: false`
- [ ] `worktree_list` calls `Manager::list()` and returns the result as JSON
- [ ] `worktree_create` accepts branch, path, and options; calls `Manager::create()`
- [ ] `worktree_delete` accepts branch or path and options; calls `Manager::delete()`
- [ ] `worktree_gc` accepts options; calls `Manager::gc()`
- [ ] Read-only tools do not prompt for approval in Claude Code and Cursor (correct annotations)
- [ ] MCP server handles `initialize`, `tools/list`, and `tools/call` JSON-RPC methods

## Tasks
- [ ] Set up `worktree-core-mcp` crate with MCP SDK dependency (or implement stdio JSON-RPC protocol directly)
- [ ] Define tool schemas for all 6 tools with input parameters matching the `Manager` API
- [ ] Implement `worktree_list` tool handler: instantiate Manager, call `list()`, serialize result
- [ ] Implement `worktree_status` tool handler: call `list()` + aggregate disk usage, format status report
- [ ] Implement `conflict_check` tool handler: return JSON `{"status": "not_implemented", "message": "Conflict detection is available in v1.1"}`
- [ ] Implement `worktree_create` tool handler: parse input, construct `CreateOptions`, call `create()`
- [ ] Implement `worktree_delete` tool handler: parse input, construct `DeleteOptions`, call `delete()`
- [ ] Implement `worktree_gc` tool handler: parse input, construct `GcOptions`, call `gc()`
- [ ] Set correct MCP tool annotations per PRD Section 12.3 table
- [ ] Implement stdio transport: read JSON-RPC from stdin, write responses to stdout
- [ ] Handle `initialize` method: return server info and capabilities
- [ ] Handle `tools/list` method: return all 6 tool definitions with annotations
- [ ] Handle `tools/call` method: dispatch to appropriate handler
- [ ] Write MCP contract tests: send JSON-RPC requests, verify response structure

## Technical Notes
- PRD Section 12.3: tool annotations are required per MCP spec 2025-03-26+
- PRD Section 12.3: read-only tools (`worktree_list`, `worktree_status`, `conflict_check`) will not prompt for approval in Claude Code and Cursor
- PRD Section 12.3: transport is stdio only in v1.0; HTTP deferred to M3
- PRD Section 12.3: `conflict_check` is a stub returning `not_implemented`
- PRD Section 12.3: config locations vary by client -- Claude Code uses `mcpServers`, VS Code uses `servers`
- The MCP server binary should be self-contained: it creates its own `Manager` instance based on the current working directory or a `repo_path` parameter
- JSON-RPC 2.0 protocol: requests have `jsonrpc`, `method`, `params`, `id` fields; responses have `jsonrpc`, `result`, `id` (or `error`)
- Consider using the `rmcp` crate or implementing the minimal JSON-RPC protocol directly for v1.0

## Test Hints
- MCP contract test: send `tools/list` request, verify 6 tools returned with correct annotations
- MCP contract test: send `worktree_list` via `tools/call`, verify JSON response structure
- MCP contract test: send `conflict_check` via `tools/call`, verify `not_implemented` response
- MCP contract test: send `worktree_create` via `tools/call` with valid params, verify worktree created
- Integration test: pipe JSON-RPC to the binary's stdin, read response from stdout
- Annotation test: verify `readOnlyHint` is correct for each tool (read-only tools must be true)

## Dependencies
ISO-1.2, ISO-1.5, ISO-1.6, ISO-1.7, ISO-1.8

## Estimated Effort
L

## Priority
P0

## Traceability
- PRD: Section 12.3 (MCP Server)
- FR: FR-P0-001 through FR-P0-004 (MCP surface for all operations)
- Appendix A invariant: N/A (MCP is a transport layer)
- Bug regression: N/A
- QA ref: MCP contract tests
