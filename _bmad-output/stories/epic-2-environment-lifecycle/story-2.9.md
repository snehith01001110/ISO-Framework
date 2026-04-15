# Story 2.9: MCP Server Documentation

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a developer integrating iso-code with my AI coding assistant, I want correct MCP configuration snippets for my specific client so that I can set up the integration without trial and error.

## Description
Write comprehensive MCP server documentation in the project README with configuration snippets for all four supported clients: Claude Code, Cursor, VS Code Copilot, and OpenCode. Each client uses a different config file and a different root key name, and getting these wrong causes silent failures. The documentation must include the one-liner installation command for Claude Code and complete JSON config blocks for all clients.

## Acceptance Criteria
- [ ] README includes config snippets for all 4 MCP clients with correct file paths and root key names
- [ ] Claude Code snippet uses `mcpServers` key in `~/.claude.json` or `.mcp.json`
- [ ] Cursor snippet uses `mcpServers` key in `.cursor/mcp.json`
- [ ] VS Code Copilot snippet uses `servers` key (NOT `mcpServers`) in `.vscode/mcp.json`
- [ ] OpenCode snippet uses `mcp` key in `opencode.jsonc`
- [ ] Claude Code one-liner included: `claude mcp add iso-code -- iso-code-mcp`
- [ ] Each snippet is copy-paste ready with correct JSON syntax
- [ ] Tool annotations table included: `readOnlyHint`, `destructiveHint`, `idempotentHint` for all 6 tools
- [ ] Warning callout about VS Code using `servers` not `mcpServers` is visually prominent
- [ ] Claude Code WorktreeCreate hook config example included
- [ ] All JSON snippets validated (no trailing commas, correct quoting)

## Tasks
- [ ] Write Claude Code MCP config snippet with `mcpServers` key
- [ ] Write Cursor MCP config snippet with `mcpServers` key
- [ ] Write VS Code Copilot config snippet with `servers` key and warning callout
- [ ] Write OpenCode config snippet with `mcp` key
- [ ] Write Claude Code one-liner installation command
- [ ] Write Claude Code `WorktreeCreate` hook config example
- [ ] Write tool annotations reference table
- [ ] Add troubleshooting section for common MCP setup issues
- [ ] Validate all JSON snippets with a JSON linter
- [ ] Review against PRD Section 12.3 for completeness

## Technical Notes
- PRD Section 12.3 is the authoritative reference for MCP config formats.
- VS Code uses `"servers"`, not `"mcpServers"` -- this is a known source of confusion and must be called out prominently.
- Tool annotations are required per MCP spec 2025-03-26+. Read-only tools (`worktree_list`, `worktree_status`, `conflict_check`) do not prompt for approval.
- The `conflict_check` tool returns `not_implemented` in v1.0 but should still be documented with its future behavior.
- Claude Code hook config: `{ "hooks": { "WorktreeCreate": "wt hook --stdin-format claude-code --setup" } }`.

## Test Hints
- JSON lint all config snippets (use `python3 -m json.tool` or `jq .`)
- Verify each snippet uses the correct root key for its client
- Cross-reference tool names against PRD Section 12.3 table
- Verify the one-liner `claude mcp add` command syntax is correct

## Dependencies
- ISO-1.13 (MCP Server Skeleton -- tool names and annotations)

## Estimated Effort
S

## Priority
P1

## Traceability
- PRD: Section 12.3 (MCP Server -- client config locations)
- FR: FR-P1-014
- QA ref: QA-2.9-001 through QA-2.9-003
