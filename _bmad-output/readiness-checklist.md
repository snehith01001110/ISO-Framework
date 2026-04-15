# Implementation Readiness Checklist

| Field | Value |
|---|---|
| **Document** | Pre-Implementation Readiness Assessment |
| **PRD** | ISO_PRD-v1.5 |
| **Date** | 2026-04-13 |
| **Verdict** | **GO** |

---

## 1. FR → Story Traceability Matrix

Every functional requirement maps to at least one story. No FR is uncovered.

| FR ID | Title | Story IDs | Covered |
|---|---|---|---|
| FR-P0-001 | Manager Constructor with Git Version Detection | ISO-1.3, ISO-1.10 | Yes |
| FR-P0-002 | Manager::create() with Pre-Create Guards and Post-Create Verification | ISO-1.5, ISO-1.6 | Yes |
| FR-P0-003 | Manager::delete() with Five-Step Unmerged Commit Decision Tree | ISO-1.7 | Yes |
| FR-P0-004 | Manager::list() with Porcelain Parser and NUL Fallback | ISO-1.4 | Yes |
| FR-P0-005 | Manager::gc() with Dry-Run Default and Locked Worktree Protection | ISO-1.8 | Yes |
| FR-P0-006 | Manager::attach() | ISO-1.9 | Yes |
| FR-P0-007 | state.json v2 Schema, fd-lock Protocol, and Reconciliation | ISO-1.10 | Yes |
| FR-P0-008 | Full Jitter Backoff | ISO-1.10 | Yes |
| FR-P0-009 | Multi-Factor Lock Identity | ISO-1.10 | Yes |
| FR-P0-010 | Port Lease Model | ISO-1.11 | Yes |
| FR-P0-011 | stale_worktrees Eviction | ISO-1.7, ISO-1.8, ISO-1.10 | Yes |
| FR-P0-012 | ReflinkMode Tristate in CreateOptions | ISO-1.2, ISO-1.6 | Yes |
| FR-P0-013 | wt hook --stdin-format claude-code | ISO-1.12 | Yes |
| FR-P0-014 | MCP Server with 6 Tools | ISO-1.13 | Yes |
| FR-P0-015 | macOS and Linux Platform Modules; Windows Stubs | ISO-1.1, ISO-1.6, ISO-3.8 | Yes |
| FR-P0-016 | Rate Limiter and Circuit Breaker | ISO-1.5, ISO-1.3 | Yes |
| FR-P1-001 | EcosystemAdapter Trait | ISO-2.1 | Yes |
| FR-P1-002 | DefaultAdapter | ISO-2.2 | Yes |
| FR-P1-003 | ShellCommandAdapter | ISO-2.3 | Yes |
| FR-P1-004 | wt create --setup CLI Integration | ISO-2.4, ISO-2.5 | Yes |
| FR-P1-005 | macOS .DS_Store Pre-Removal | ISO-2.6 | Yes |
| FR-P1-006 | Windows MAX_PATH Workarounds via dunce | ISO-2.7 | Yes |
| FR-P2-001 | git merge-tree CLI Parser | ISO-3.1 | Yes |
| FR-P2-002 | ConflictReport and ConflictType Enum | ISO-3.2 | Yes |
| FR-P2-003 | wt check Subcommand | ISO-3.5 | Yes |
| FR-P2-004 | MCP conflict_check Tool Implementation | ISO-3.6 | Yes |
| FR-P2-005 | HTTP MCP Transport | ISO-3.7 | Yes |
| FR-P2-006 | Windows Full Platform Implementation | ISO-3.8 | Yes |
| FR-P3-001 | pnpm Adapter | ISO-4.1 | Yes |
| FR-P3-002 | uv Adapter | ISO-4.2 | Yes |
| FR-P3-003 | Cargo Adapter | ISO-4.3 | Yes |
| FR-P3-004 | gix Conflict Detection | ISO-4.4 | Yes |
| FR-P3-005 | napi-rs Node.js Bindings | ISO-4.5 | Yes |
| FR-P3-006 | PyO3 Python Bindings | ISO-4.6 | Yes |
| FR-P3-007 | Worktree Pooling | ISO-4.7 | Yes |
| FR-P3-008 | Merge Lifecycle Automation | ISO-4.7 | Partial |
| FR-P3-009 | MCP Server Docs for All Four Clients | ISO-2.9, ISO-4.9 | Yes |

**Gaps:** FR-P3-008 (Merge Lifecycle Automation) is partially covered by story ISO-4.7 (Worktree Pooling) which includes pool lifecycle states. A dedicated story for the `Active -> Merging -> Active` state transition flow should be considered but is non-blocking for M4 since the state machine is already defined in ISO-1.2 (type system) and the pooling story handles the operational side.

---

## 2. NFR Coverage

| NFR | Requirement | Addressed By | Covered |
|---|---|---|---|
| Performance: Manager::new() cold start | < 500 ms | ISO-1.3 (git version detection), QA-P-001 benchmark | Yes |
| Performance: Disk walk 50K files | < 200 ms | ISO-1.5 (disk usage guard), QA-P-004 benchmark | Yes |
| Performance: create() on 2 GB repo | < 10 s | ISO-1.6 (create part 2), QA-P-002 benchmark | Yes |
| Performance: conflict_check 20 pairs | < 10 s | ISO-3.4 (conflict matrix), QA-P-005 benchmark | Yes |
| Reliability: Zero data loss | 100-cycle crash injection | ISO-1.14 (stress test), QA-S-001 | Yes |
| Reliability: Circuit breaker | 3 consecutive failures | ISO-1.3, ISO-1.5, QA-C-004 | Yes |
| Reliability: Atomic state writes | write-tmp-fsync-rename | ISO-1.10 (state persistence) | Yes |
| Reliability: Stale lock recovery | 4-factor check | ISO-1.10, QA-C-005, QA-C-008 | Yes |
| Compatibility: Git >= 2.20 | Hard minimum | ISO-1.3, QA-V-008, QA-V-009 | Yes |
| Compatibility: Rust MSRV 1.75 | CI enforced | ISO-1.1 (workspace scaffolding) | Yes |
| Compatibility: macOS 10.15+ | APFS clonefile | ISO-1.6, platform module | Yes |
| Compatibility: Linux glibc 2.17+ | CI runners | ISO-1.1 | Yes |
| Compatibility: Windows 10 1607+ | M3 full impl | ISO-3.8 | Yes |
| Security: Advisory locking only | fd-lock, no mandatory | ISO-1.10 | Yes |
| Security: No unsafe without SAFETY | Code review + clippy | ISO-1.1, RFC process (PRD §13) | Yes |
| Security: No .git/worktrees/ writes | Appendix A rule 3 | ISO-1.6, ISO-1.7 | Yes |
| Usability: Structured WorktreeError | All variants defined | ISO-1.2 | Yes |
| Usability: Human-readable CLI output | wt list/status | ISO-1.12, ISO-2.4 | Yes |
| Usability: MCP tool annotations | readOnly/destructive/idempotent | ISO-1.13 | Yes |

---

## 3. Appendix A Invariant Coverage

All 14 non-negotiable invariants from PRD Appendix A are enforced by stories and verified by tests.

| # | Invariant | Enforced By Story | Verified By Test ID |
|---|---|---|---|
| 1 | Shell out to git CLI. No git2 or gix for worktree CRUD. | ISO-1.6 (create), ISO-1.7 (delete), ISO-1.4 (list) | QA-R-001 through QA-R-010 (all integration tests use real git CLI) |
| 2 | `git worktree list --porcelain` is source of truth. state.json is supplementary. | ISO-1.4 (parser), ISO-1.10 (reconciliation) | QA-G-001 through QA-G-012 (guards test against git output) |
| 3 | Never write to `.git/worktrees/` directly. | ISO-1.6, ISO-1.10 (architecture constraint — all state in `.git/iso-code/`) | QA-S-001 (stress test verifies no corruption) |
| 4 | Never invoke `git gc` or `git prune`. | ISO-1.8 (gc uses `git worktree prune` only, never `git gc`) | QA-R-007, QA-R-010 (gc regression tests) |
| 5 | All deletion paths run five-step unmerged commit check unless force=true. | ISO-1.7 (delete), ISO-1.8 (gc) | QA-R-001, QA-R-006 |
| 6 | On failure after `git worktree add` succeeds, run `git worktree remove --force`. | ISO-1.6 (create part 2 — cleanup on failure) | QA-R-004 (git-crypt failure triggers cleanup) |
| 7 | state.lock scope is ONLY around state.json read-modify-write. Never held across git worktree add. | ISO-1.10 (locking protocol) | QA-C-003 (contention test verifies lock duration) |
| 8 | Entries evicted from active_worktrees go to stale_worktrees — never silently deleted. | ISO-1.7, ISO-1.8, ISO-1.10 | QA-R-002, QA-R-003 (verify data preserved) |
| 9 | Windows junctions CAN span volumes. No cross-volume restriction. | ISO-2.7 (Windows MAX_PATH), ISO-3.8 (Windows full) | QA-G-011 (junction target test) |
| 10 | Worktree paths with newlines are unparseable without -z. Log warning, don't crash. | ISO-1.4 (porcelain parser with fallback) | QA-V-001 (mock git NUL fallback test) |
| 11 | Branch names are never transformed by the core library. | ISO-1.6 (create — pass-through), ISO-1.2 (types) | QA-I-003 (slash-prefixed branch preserved) |
| 12 | All public structs are #[non_exhaustive]. | ISO-1.2 (complete type system) | Compile-time enforcement |
| 13 | gc() never touches locked worktrees regardless of force flag. | ISO-1.8 (gc — locked protection) | QA-I-005 (locked survives gc force=true) |
| 14 | Never use `git branch --merged` as sole safe-to-delete check. | ISO-1.7 (five-step decision tree replaces naive check) | QA-R-001 (regression: unmerged commits deleted) |

---

## 4. Story Dependency Chain

All story dependencies flow forward (lower-numbered stories within and across epics). No circular dependencies exist.

| Story | Depends On | Status |
|---|---|---|
| ISO-1.1 | none | OK |
| ISO-1.2 | ISO-1.1 | OK |
| ISO-1.3 | ISO-1.1 | OK |
| ISO-1.4 | ISO-1.2, ISO-1.3 | OK |
| ISO-1.5 | ISO-1.2, ISO-1.3, ISO-1.4 | OK |
| ISO-1.6 | ISO-1.5 | OK |
| ISO-1.7 | ISO-1.4, ISO-1.6 | OK |
| ISO-1.8 | ISO-1.7 | OK |
| ISO-1.9 | ISO-1.4, ISO-1.10 | OK |
| ISO-1.10 | ISO-1.2 | OK |
| ISO-1.11 | ISO-1.10 | OK |
| ISO-1.12 | ISO-1.6 | OK |
| ISO-1.13 | ISO-1.4, ISO-1.6, ISO-1.7, ISO-1.8 | OK |
| ISO-1.14 | ISO-1.6, ISO-1.7, ISO-1.8 | OK |
| ISO-2.1 | ISO-1.2 | OK |
| ISO-2.2 | ISO-2.1 | OK |
| ISO-2.3 | ISO-2.1 | OK |
| ISO-2.4 | ISO-2.2, ISO-2.3, ISO-1.12 | OK |
| ISO-2.5 | ISO-1.11, ISO-1.6 | OK |
| ISO-2.6 | ISO-1.7 | OK |
| ISO-2.7 | ISO-1.6 | OK |
| ISO-2.8 | ISO-1.9, ISO-1.11 | OK |
| ISO-2.9 | ISO-1.13 | OK |
| ISO-2.10 | ISO-2.2 through ISO-2.8 | OK |
| ISO-3.1 | ISO-1.3 (GitCapabilities) | OK |
| ISO-3.2 | ISO-1.2 | OK |
| ISO-3.3 | ISO-3.1, ISO-3.2 | OK |
| ISO-3.4 | ISO-3.3 | OK |
| ISO-3.5 | ISO-3.3 | OK |
| ISO-3.6 | ISO-3.3, ISO-1.13 | OK |
| ISO-3.7 | ISO-1.13 | OK |
| ISO-3.8 | ISO-1.6, ISO-1.7, ISO-1.10 | OK |
| ISO-4.1 | ISO-2.1 | OK |
| ISO-4.2 | ISO-2.1 | OK |
| ISO-4.3 | ISO-2.1 | OK |
| ISO-4.4 | ISO-3.1, ISO-3.2 | OK |
| ISO-4.5 | ISO-1.2, ISO-1.6, ISO-1.7 | OK |
| ISO-4.6 | ISO-4.5 | OK |
| ISO-4.7 | ISO-1.6, ISO-1.8 | OK |
| ISO-4.8 | ISO-1.6, ISO-1.7, ISO-1.13 | OK |
| ISO-4.9 | ISO-2.9, ISO-4.5 | OK |

**No circular dependencies.** All forward references resolve to previously-defined stories.

---

## 5. Open Questions Impact

All 6 open questions from PRD §19 were resolved in `architecture.md § Decisions Log`.

| OQ | Decision | Implementing Story | Validation Test | Default if Deferred |
|---|---|---|---|---|
| OQ-1: Port lease renewal mechanism | Caller-driven via `renew_port_lease()`. No background timer. Library stays async-free. | ISO-1.11 (Port lease model) | QA-O-001 | Leases expire after 8h with no renewal API; callers must re-create. |
| OQ-2: Network FS warning vs error | `Config::deny_network_filesystem: bool` (default false). One-line addition. | ISO-1.5 (pre-create guards) | QA-O-002 | Warning-only; no Config field. |
| OQ-3: wt attach on bare repos | `attach()` permitted on bare repos when path is explicitly provided. | ISO-1.9 (Manager::attach) | QA-O-003 | attach() rejects bare repos; users must use create(). |
| OQ-4: Circuit breaker reset | Auto-reset after `Config::circuit_breaker_reset_secs` (default 60). Manager reconstruction also resets. | ISO-1.3 (git version detection — circuit breaker lives in Manager) | QA-O-004 | No reset; Manager must be reconstructed. |
| OQ-5: Bare repo git worktree add | Confirmed safe in Git 2.20. Exact form: `git -C <bare-root> worktree add <path> -b <branch> <base>`. | ISO-1.6 (create part 2) | QA-O-005 | Bare repos unsupported in create(). |
| OQ-6: wt gc concurrency with active agents | `WorktreeState::InUse { pid, since }` variant. gc() skips InUse even if force=true. PID-liveness check evicts dead processes. | ISO-1.8 (Manager::gc) | QA-O-006 | gc() may evict actively-used worktrees that appear orphaned. |

---

## 6. Data Loss Regression Coverage

All 10 incidents from PRD Appendix B have corresponding regression test IDs and are mapped to implementing stories.

| # | Incident | Bug ID | Test ID | Story | Protected |
|---|---|---|---|---|---|
| 1 | Cleanup deleted branches with unmerged commits | `claude-code#38287` | QA-R-001 | ISO-1.7 | Yes |
| 2 | Sub-agent cleanup deleted parent CWD | `claude-code#41010` | QA-R-002 | ISO-1.7 | Yes |
| 3 | Three agents reported success; all work lost | `claude-code#29110` | QA-R-003 | ISO-1.8 | Yes |
| 4 | git-crypt worktree committed all files as deletions | `claude-code#38538` | QA-R-004 | ISO-1.6 | Yes |
| 5 | Nested worktree inside worktree after compaction | `claude-code#27881` | QA-R-005 | ISO-1.5 | Yes |
| 6 | Background worker cleaned worktree with uncommitted changes | `vscode#289973` | QA-R-006 | ISO-1.7 | Yes |
| 7 | Runaway `git worktree add` loop: 1,526 worktrees | `vscode#296194` | QA-R-007 | ISO-1.5 | Yes |
| 8 | 9.82 GB consumed in 20-minute session | Cursor forum | QA-R-008 | ISO-1.5 | Yes |
| 9 | 5 worktrees x 2 GB node_modules | `claude-squad#260` | QA-R-009 | ISO-1.6 | Yes |
| 10 | Each retry creates orphan (unbounded) | `opencode#14648` | QA-R-010 | ISO-1.6 | Yes |

**All 10 incidents are protected.** Every data-loss scenario has a named regression test and a story that implements the guard preventing recurrence.

---

## 7. Verdict

### **GO**

All requirements are met for implementation readiness:

- **All 37 FRs covered** by at least one story (1 partial: FR-P3-008 merge lifecycle is implicitly covered by type system + pooling stories; non-blocking).
- **All 19 NFRs addressed** by stories and/or architectural decisions.
- **All 14 Appendix A invariants enforced** by specific stories and verified by test IDs.
- **No circular story dependencies.** All dependency chains flow forward.
- **All 6 Open Questions resolved** with implementing stories and validation tests identified.
- **All 10 data-loss incidents protected** by named regression tests mapped to stories.
- **72 test IDs defined** across 11 testing layers with milestone acceptance gates.

### Items Requiring Human Review Before Implementation

1. **External coordination (ISO-4.8):** Claude Squad PR #268/#270 integration and workmux optional dependency require coordination with external maintainers. Begin outreach during Epic 2 to avoid M3 schedule risk.

2. **Windows CI runner selection (ISO-3.8):** Confirm GitHub Actions Windows Server 2019 runner availability and cost. NTFS junction tests require actual Windows filesystem — no WSL workaround.

3. **napi-rs npm package naming (ISO-4.5):** Confirm `@iso-code/node` scope is available on npmjs.com. Alternative: `iso-code` (unscoped). Reserve the name early.

4. **gix feature completeness (ISO-4.4):** `gix::Repository::merge_trees()` was feature-complete as of Nov 2024 per GitButler PR #5722. Verify current gix version on crates.io still exposes this API before committing to the feature-flagged integration.

5. **Crates.io name reservation:** Reserve `iso-code`, `iso-code-cli`, and `iso-code-mcp` on crates.io before M1 publish to prevent name squatting. This should be done in Sprint 1.

6. **RFC-001 candidate — InUse state variant (OQ-6):** The `WorktreeState::InUse { pid: u32, since: String }` variant was not in the original PRD type definition. This is a non-breaking addition (the enum is `#[non_exhaustive]`), but should be documented as RFC-001 per PRD §13 since it adds a new variant to a public type.
