# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-04-18

## [0.1.0] - 2026-04-16

### Added

- Initial release.
- Core `Manager` API for worktree lifecycle management (create, delete, list, attach, gc).
- 7 pre-create safety guards (branch checkout, disk space, worktree limit, nesting, network FS, git-crypt, aggregate disk).
- 5-step unmerged commit check on delete (local, remote, merge-base, merge-tree, squash-merge detection).
- Crash-safe state persistence via atomic write (tmp + fsync + rename).
- Advisory file locking with 4-factor stale detection.
- Port lease allocation with deterministic hashing.
- Cross-platform file copying with reflink/CoW support.
- `iso-code-cli` (`wt`) binary with Claude Code hook integration.
- `iso-code-mcp` stdio JSON-RPC 2.0 server with annotated MCP tool definitions.
- CI with clippy enforcement and nightly stress test gates.

[Unreleased]: https://github.com/snehith01001110/ISO-Framework/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/snehith01001110/ISO-Framework/releases/tag/v0.1.1
[0.1.0]: https://github.com/snehith01001110/ISO-Framework/releases/tag/v0.1.0
