# iso-code-mcp

**MCP server for safe git worktree management in AI coding agents.**

Model Context Protocol server exposing the [`iso-code`](https://crates.io/crates/iso-code)
worktree management library to Claude Code, Cursor, VS Code Copilot, OpenCode,
and any other MCP-capable client.

## Install

```bash
cargo install iso-code-mcp
```

The binary is named `iso-code-mcp` and speaks MCP over stdio.

## Tools exposed

| Tool              | Description                                            |
|-------------------|--------------------------------------------------------|
| `worktree_list`   | List all tracked worktrees for the current repo        |
| `worktree_create` | Create a worktree (with pre-create safety guards)      |
| `worktree_delete` | Delete a worktree (5-step unmerged-commit check)       |
| `worktree_gc`     | Garbage-collect orphaned worktrees                     |
| `worktree_attach` | Attach an existing worktree path to iso-code state     |
| `conflict_check`  | Returns `not_implemented` until milestone 3            |

## Client configuration

### Claude Code (`~/.claude/claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "iso-code": {
      "command": "iso-code-mcp",
      "args": []
    }
  }
}
```

### Cursor (`.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "iso-code": {
      "command": "iso-code-mcp"
    }
  }
}
```

### VS Code Copilot (`.vscode/mcp.json`)

```json
{
  "servers": {
    "iso-code": {
      "type": "stdio",
      "command": "iso-code-mcp"
    }
  }
}
```

### OpenCode (`opencode.json`)

```json
{
  "mcp": {
    "servers": {
      "iso-code": {
        "type": "local",
        "command": ["iso-code-mcp"]
      }
    }
  }
}
```

## Status

Milestone 1 (Foundation). HTTP transport and full `conflict_check`
implementation ship in milestone 3. See the
[PRD](https://github.com/snehith01001110/ISO-Framework/blob/main/ISO_PRD-v1.5.md)
for the roadmap.

## License

Licensed under either of Apache License 2.0 or MIT License at your option.
