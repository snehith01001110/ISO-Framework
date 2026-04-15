# Project Brief: iso-code

**The canonical safe worktree lifecycle library for AI coding orchestrators.**

---

## 1. Problem Statement

Every major AI coding orchestrator in 2026 -- Claude Code, Claude Squad, Cursor, OpenCode, VS Code Copilot, Gas Town -- uses git worktrees to isolate parallel agent sessions. None share any worktree management code. Each has independently implemented creation, deletion, and cleanup, and each has shipped critical, user-facing bugs as a direct consequence.

These are not hypothetical risks. They are filed bugs on public repositories, and they are all symptoms of the same root cause: **there is no shared, reusable worktree lifecycle library in any language.**

| Bug | Tool | Symptom |
|---|---|---|
| `claude-code#38538` | Claude Code | Worktree creation in git-crypt repos produced commits deleting all files. The smudge filter never ran, leaving encrypted blobs staged as deletions. No pre-create or post-create check existed. |
| `claude-code#41010` | Claude Code | Sub-agent cleanup deleted the parent session's working directory due to agent ID collision. No CWD guard, no identity isolation. |
| `claude-code#38287` | Claude Code | Cleanup deleted branches with unmerged commits silently. No multi-step merge check; developers lost days of work. |
| `claude-code#29110` | Claude Code | Three agents reported successful task completion; all work was discovered lost at verification. No persistence or verification layer. |
| `vscode#289973` | VS Code Copilot | Background worker cleaned a worktree containing uncommitted changes. No dirty-state guard before deletion. |
| `vscode#296194` | VS Code Copilot | A logic flaw called `git worktree add` on every diff paste, reaching 1,526 worktrees with no circuit breaker or rate limit. |
| `opencode#14648` | OpenCode | Each retry on failure generated a new random worktree name with no cleanup. A 2 GB repo accumulated hundreds of MB per retry with no upper bound. |
| `claude-squad#260` | Claude Squad | 5 worktrees each duplicated a 2 GB `node_modules` directory -- 10 GB wasted. No environment setup, no shared dependency stores, no disk budget. |

Every one of these bugs would be prevented by a shared library that enforces pre-delete merge checks, CWD guards, rate limits, disk budgets, git-crypt detection, and proper state reconciliation. `iso-code` is that library.

---

## 2. Target Users

- **Orchestrator developer** (primary): Maintainers of tools like Claude Squad, workmux, OpenCode, or custom agent harnesses who need a reliable worktree primitive and are currently writing fragile, one-off implementations from scratch.

- **Solo power developer**: An individual running multiple AI agents in parallel (via Claude Code, Cursor, or the `wt` CLI directly) who is losing disk space to orphans and risking data loss from unguarded cleanup.

- **Team / enterprise**: Organizations running shared CI machines or developer containers where multiple engineers (or their agents) operate on the same repository concurrently, and orphaned worktrees accumulate without any centralized lifecycle management.

- **CI/CD integrator**: Build system maintainers who need isolated, reproducible worktree environments for parallel test or build pipelines, with deterministic cleanup and port allocation that does not collide across jobs.

---

## 3. Vision

At the completion of Milestone 4, `iso-code` is the standard worktree lifecycle layer across the AI coding ecosystem. The Rust library crate is published on crates.io with a stable public API. Node.js bindings are published to npm. At least one major external orchestrator consumes it as a dependency. The `wt` CLI is the human-facing interface for worktree management, and the `iso-code-mcp` server is installed across Claude Code, Cursor, VS Code Copilot, and OpenCode via a single config line. Conflict detection via `git merge-tree` is operational. Ecosystem-specific adapters (pnpm, uv, cargo) eliminate the multi-gigabyte dependency duplication problem. Worktree pooling provides sub-second checkout for interactive workflows. The data-loss bugs catalogued in Section 1 are structurally impossible for any consumer of the library.

---

## 4. Scope Boundaries

### In Scope

- **`iso-code` library crate** -- the Rust library with `Manager`, all safety guards, state persistence, locking protocol, and port lease model.
- **`wt` CLI** -- thin binary wrapping the library for human use, shell hooks, and CI scripts. Includes `wt hook --stdin-format claude-code` for direct Claude Code integration.
- **`iso-code-mcp` MCP server** -- stdio transport (v1.0), HTTP transport (v1.1), exposing 6 tools (`worktree_list`, `worktree_status`, `worktree_create`, `worktree_delete`, `worktree_gc`, `conflict_check`).
- **`DefaultAdapter`** -- copies configurable files (`.env`, `.env.local`, etc.) into new worktrees.
- **`ShellCommandAdapter`** -- runs arbitrary shell commands at create/delete time with `ISO_CODE_*` environment variables.
- **napi-rs Node.js bindings** (M4) and PyO3 Python bindings (M4).
- **Conflict detection** via `git merge-tree --write-tree` (Git 2.38+), with `gix::merge_trees()` as an optional compiled-in backend in M4.

### Out of Scope

- **Full agent orchestrator** -- iso-code manages worktree lifecycle, not agent coordination, task scheduling, or prompt routing.
- **GUI** -- no graphical interface; CLI and MCP only.
- **Hosted service** -- single-machine operation only; no SaaS, no cloud API.
- **Distributed / network coordination in v1.0** -- no multi-machine lock coordination. Network filesystem support is degraded-mode only (advisory locking skipped; atomic rename only).

---

## 5. Key Constraints

1. **Rust-only core.** The library, CLI, and MCP server are written in Rust. Language bindings (napi-rs, PyO3) wrap the Rust crate via FFI.

2. **Shell out for all worktree CRUD.** All git operations use `std::process::Command` against the user's installed git binary. No `libgit2`. No `gix` for worktree add/remove/list. (`gix` is reserved for M4 conflict detection behind a feature flag.)

3. **`git worktree list --porcelain` is always authoritative.** `state.json` is a supplementary cache. If they disagree, git wins. Reconciliation runs at every `Manager::list()` call and at startup.

4. **Minimum Git version: 2.20.** Capability detection at startup gates features for older git versions (e.g., NUL-delimited output requires 2.36; `git worktree repair` requires 2.30).

5. **Rust MSRV: 1.75.** No nightly features required.

6. **Cross-platform.** macOS is the primary development and test platform. Linux is secondary (CI). Windows ships as compile-only stubs in M1, with full platform implementation in M3.

7. **Solo maintainer.** The project is maintained by a single developer. Milestones are scoped to 1-2 week sprints. Community-contributed ecosystem adapters (pnpm, npm, uv, cargo) are accepted but not authored in-house before M4.

---

## 6. Competitive Landscape

The current ecosystem is fragmented. At least ten tools manage git worktrees for AI coding workflows:

| Tool | Language | Approach |
|---|---|---|
| Claude Squad | Go | CLI orchestrator; manages worktrees + tmux sessions |
| Cursor | TypeScript | Built-in worktree isolation; no public API |
| Claude Code | TypeScript | `EnterWorktree` / `ExitWorktree` internal commands |
| OpenCode | TypeScript/Bun | Random worktree naming; no cleanup |
| Gas Town | Go | Dolt-backed tracking; ephemeral sessions |
| git-worktree-runner | Various | Thin wrappers; no safety guards |
| worktrunk | Rust | CLI-first; library API not stable per maintainer |
| gwq | Various | Workspace manager; different abstraction level |
| agentree | Various | Agent-oriented; tightly coupled to specific orchestrator |
| workmux | Rust | tmux integration; port collisions, no env bootstrapping |

**The gap:** Not a single one of these tools exposes a reusable, importable library in any language. Every tool has reimplemented worktree creation, deletion, and cleanup from scratch, and every tool has shipped the same categories of bugs: data loss from unguarded deletion, orphan accumulation from missing cleanup, disk exhaustion from missing rate limits, and environment breakage from missing setup hooks.

**iso-code's positioning:** It is the shared foundation layer -- the "testcontainers for worktrees." It does not compete with orchestrators; it makes them reliable. Any tool that manages worktrees can depend on this crate and immediately inherit safe deletion guards, orphan GC, disk budgets, port allocation, git-crypt detection, and state persistence. The library is intentionally unopinionated about branch naming, orchestration strategy, or UI.

---

## 7. Top 5 Risks and Mitigations

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | **`state.json` corruption under concurrent access** -- multiple agents writing simultaneously corrupt the state file, causing orphaned worktrees or lost metadata. | High | High | Four-factor lock identity (PID + process start time + UUID + hostname), Full Jitter exponential backoff, atomic rename via temp file + `fsync`, and automatic state rebuild from `git worktree list --porcelain` if JSON parse fails. The lock is never held across `git worktree add`. |
| 2 | **GC races with active agents** -- `wt gc` evicts a worktree that an agent is actively using but whose process is not the lock holder and has not called `git worktree lock`. | Medium | High | GC never touches locked worktrees. GC defaults to `dry_run = true`. Open question (PRD S19 Q6) to be resolved before M1: add an "in-use" heartbeat or require agents to lock worktrees during active sessions. |
| 3 | **Port lease renewal undefined** -- the spec defines 8-hour TTL with renewal every ~2.5 hours but does not specify who triggers renewal or what "active use" means, risking port collisions after lease expiry. | Medium | Medium | Open question (PRD S19 Q1) to be resolved before M1. Likely resolution: caller-driven renewal via explicit `Manager` method, not background timer, to avoid hidden thread/async dependencies in a library crate. |
| 4 | **Circuit breaker with no reset path** -- 3 consecutive git failures open the circuit breaker and block all operations, but no reset mechanism is specified, potentially bricking a session until the `Manager` is reconstructed. | Medium | Medium | Open question (PRD S19 Q4). Likely resolution: automatic reset after a configurable cooldown period (e.g., 60 seconds), plus a `Manager::reset_circuit_breaker()` escape hatch for callers. |
| 5 | **Adoption failure -- external projects do not consume the crate** -- if no orchestrator integrates `iso-code` by M3, the library becomes a standalone tool rather than a shared foundation, undermining its core value proposition. | Medium | High | Early MCP server delivery (M1) provides zero-integration-cost adoption for Claude Code, Cursor, and VS Code. Claude Squad PR integration and workmux crate dependency are planned for M3 with explicit maintainer coordination. The `wt hook` subcommand provides a Claude Code integration path that requires only a one-line config change. |

---

## 8. Success Criteria

These are the measurable, non-negotiable ship gates:

1. **Zero data loss in crash-injection stress test.** 100 create/delete cycles with simulated crash injection (SIGKILL at random points during create, delete, and GC operations) must produce zero data loss. This is the M1 ship criterion from PRD S15.

2. **`wt gc` cleans 1,000 orphans in a single run.** Simulating an OpenCode-style failure (1,000 orphaned worktrees of varying ages), `wt gc --confirm` must identify and remove all eligible orphans in one invocation without manual intervention.

3. **At least one external project consuming the crate as a library by M3.** Either Claude Squad (via PR hooks) or workmux (as a Cargo dependency) must ship a release that depends on `iso-code`.

4. **`wt hook --stdin-format claude-code` produces exactly one line on stdout.** The absolute worktree path and nothing else -- all other output goes to stderr. This is a hard contract with Claude Code's hook protocol.

5. **Port allocation assigns unique ports to 20 simultaneous worktrees with zero collisions.** Deterministic hash-based assignment with sequential probe fallback must handle the maximum default worktree count without conflict.

---

*This brief covers iso-code (codename: ISO), PRD v1.5, April 2026. Solo maintainer: Snehith.*
