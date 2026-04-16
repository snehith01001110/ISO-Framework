# iso-code-cli

**`wt` — a safe git worktree CLI for AI coding agents.**

Command-line interface for the [`iso-code`](https://crates.io/crates/iso-code) worktree
management library. Drop-in replacement for `git worktree` with safety guarantees
that prevent the data-loss bugs documented in Claude Code, Cursor, Claude Squad,
OpenCode, and VS Code Copilot.

## Install

```bash
cargo install iso-code-cli
```

The binary is named `wt`.

## Usage

```bash
# List all worktrees tracked for this repo
wt list

# Create a worktree for a branch
wt create feature/my-branch ../my-branch-worktree

# Delete a worktree (runs 5-step unmerged-commit check)
wt delete ../my-branch-worktree

# Garbage-collect orphaned worktrees (dry run by default)
wt gc
wt gc --execute

# Attach an existing worktree path to iso-code's state
wt attach ../existing-worktree

# Claude Code hook integration — prints the new worktree path on stdout
wt hook --stdin-format claude-code
```

## Safety guarantees

- Never deletes branches with unmerged commits (5-step check)
- Never leaves partial worktrees on disk (cleanup on failure)
- Never corrupts git-crypt repos (post-create verification)
- Never creates nested worktrees (bidirectional path check)
- Never evicts locked worktrees
- `state.json` is crash-safe (atomic write via tmp + fsync + rename)

## Status

Milestone 1 (Foundation). `--setup` adapter flags will ship in milestone 2.
See the [PRD](https://github.com/snehith01001110/ISO-Framework/blob/main/ISO_PRD-v1.5.md)
for the roadmap.

## License

Licensed under either of Apache License 2.0 or MIT License at your option.
