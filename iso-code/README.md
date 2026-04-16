# iso-code

**Safe git worktree lifecycle management for AI coding agents.**

iso-code is a Rust library + CLI + MCP server that solves documented data-loss bugs
in Claude Code, Cursor, Claude Squad, OpenCode, and VS Code Copilot by providing a
shared, battle-tested worktree management foundation.

## Problem

Every major AI coding orchestrator independently implements worktree management and
each has critical bugs:
- Silent data loss (unmerged commits deleted without warning)
- Unbounded resource consumption (hundreds of orphaned worktrees)
- Nested worktree creation after context compaction
- git-crypt corruption on worktree creation

iso-code fixes all of these with a single shared library.

## Installation

```toml
[dependencies]
iso-code = "0.1"
```

CLI:
```text
cargo install iso-code-cli
```

## Basic Usage

```rust,no_run
use iso_code::{Manager, Config, CreateOptions, DeleteOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mgr = Manager::new("/path/to/repo", Config::default())?;

    // Create a worktree
    let (handle, _) = mgr.create("feature/my-branch", "/path/to/worktree", CreateOptions::default())?;

    // List all worktrees
    let worktrees = mgr.list()?;

    // Delete safely (runs 5-step unmerged commit check)
    mgr.delete(&handle, DeleteOptions::default())?;

    // GC orphaned worktrees (dry_run = true by default)
    let report = mgr.gc(Default::default())?;
    Ok(())
}
```

## CLI

```bash
wt list
wt create feature/my-branch /path/to/worktree
wt delete /path/to/worktree
wt hook --stdin-format claude-code   # Claude Code hook integration
```

## MCP Server Configuration

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

## Claude Code Hook Integration

Add to your Claude Code config:

```json
{
  "hooks": {
    "WorktreeCreate": "wt hook --stdin-format claude-code --setup"
  }
}
```

## Safety Guarantees

- Never deletes branches with unmerged commits (5-step check)
- Never leaves partial worktrees on disk (cleanup-on-failure)
- Never corrupts git-crypt repos (post-create verification)
- Never creates nested worktrees (bidirectional path check)
- Never evicts locked worktrees (unconditional protection)
- State.json is crash-safe (atomic write via tmp + fsync + rename)

## License

Licensed under either of Apache License 2.0 or MIT License at your option.
