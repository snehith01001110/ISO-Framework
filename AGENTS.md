# ISO — worktree-core Project Context

## What this project is
`worktree-core` is a Rust library + CLI + MCP server for safe git worktree
lifecycle management. It solves documented data-loss bugs in Claude Code,
Cursor, Claude Squad, OpenCode, and VS Code Copilot.

## Project structure
```
ISO/
├── ISO_PRD-v1.5.md          ← Single source of truth. Fully locked.
├── AGENTS.md                ← This file
├── _bmad-output/            ← All planning artifacts
│   ├── architecture.md      ← Technical spec + Decisions Log (6 OQ resolutions)
│   ├── prd.md               ← Structured functional requirements (FR-P0-xxx etc.)
│   ├── project-brief.md     ← Analyst brief
│   ├── readiness-checklist.md ← GO/NO-GO verdict
│   ├── qa/
│   │   ├── test-strategy.md ← Full test plan, all QA-* test IDs
│   │   └── test-id-index.md ← Flat lookup: test ID → story → milestone gate
│   └── stories/
│       ├── epic-1-foundation/        ← Weeks 1-6, P0, ~14 stories
│       ├── epic-2-environment-lifecycle/ ← Weeks 7-10, P1
│       ├── epic-3-conflict-intelligence/ ← Weeks 11-16, P2
│       └── epic-4-ecosystem-integration/ ← Weeks 17-20, P3
└── (Rust workspace created by story 1.1)
```

## How to work stories
Stories are executed one at a time in dependency order. The current story
file is the unit of work. Each story file contains:
- Acceptance Criteria (binary pass/fail — all must be checked before Done)
- Tasks (concrete implementation steps)
- Technical Notes (exact Rust types, git commands, crate names from PRD)
- Test Hints (what tests must cover, which QA-* IDs apply)
- QA ref (test IDs from test-id-index.md that must pass before closing)
- Dependencies (which earlier story must be Done first)

## Non-negotiable rules (from PRD Appendix A)
1. Shell out to git CLI for ALL worktree CRUD. Never use git2 or gix for this.
2. `git worktree list --porcelain` is always the source of truth. state.json
   is a cache. If they disagree, git wins.
3. Never write to .git/worktrees/ directly.
4. Never call `git gc` or `git prune`.
5. All deletion paths run the five-step unmerged commit check unless force=true.
6. On failure after `git worktree add` succeeds, run `git worktree remove
   --force` before returning any error.
7. state.lock scope is ONLY around state.json read-modify-write. Never hold
   it across `git worktree add`.
8. Entries evicted from active_worktrees go to stale_worktrees — never
   silently deleted.
9. Windows junctions CAN span volumes. Do NOT add a cross-volume restriction.
10. Worktree paths with newlines are unparseable without -z (Git 2.36+).
    Log a warning; do not crash.
11. Branch names are never transformed. Accept any string as-is.
12. All public structs are #[non_exhaustive]. Do not remove this attribute.
13. gc() never touches locked worktrees regardless of the force flag.
14. Never use `git branch --merged` as the sole safe-to-delete check.
    It misses squash-merged branches.

## Key constraints
- Rust MSRV: 1.75
- Minimum Git version: 2.20
- `cargo clippy -- -D warnings` must stay clean at all times
- `cargo test` must pass on macOS and Linux at all times
- Do not invent types or signatures not defined in ISO_PRD-v1.5.md
- Do not rename variants
- Do not change public function signatures without filing an RFC at
  _bmad-output/docs/decisions/rfc-NNN.md

## Story status lifecycle
Draft → In Progress → Done
A story is Done only when ALL acceptance criteria are checked AND all
QA ref test IDs pass.

## When to stop and ask me
- A story's acceptance criteria cannot be satisfied after reasonable attempts
- A compile error requires changing a public API (file RFC first, then ask)
- A test cannot be written without clarifying a genuine ambiguity in the PRD
- Any git operation would touch production data or remote branches
- The readiness-checklist.md verdict was CONDITIONAL GO and a blocking gap
  has been encountered
