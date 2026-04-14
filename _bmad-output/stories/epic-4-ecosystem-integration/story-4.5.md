# Story 4.5: napi-rs Node.js Binding

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a Node.js/TypeScript developer, I want to use worktree-core as an npm package with full TypeScript type definitions so that I can integrate worktree management into my JavaScript-based tools without shelling out to the CLI.

## Description
Build a Node.js native addon using napi-rs that wraps the worktree-core Rust library. napi-rs generates TypeScript type definitions automatically from the Rust types, providing a first-class TypeScript API. The package is published to npm as `@worktree-core/node` and tested on Node.js 18+. This enables tools like Claude Code (TypeScript), OpenCode (TypeScript/Bun), and Cursor (TypeScript) to consume worktree-core as a native library dependency instead of through the MCP protocol.

## Acceptance Criteria
- [ ] npm package `@worktree-core/node` builds and publishes successfully
- [ ] TypeScript type definitions generated automatically by napi-rs
- [ ] All public `Manager` methods exposed: `create()`, `delete()`, `list()`, `gc()`, `attach()`, `checkConflicts()`
- [ ] `Config`, `CreateOptions`, `DeleteOptions`, `GcOptions` types available in TypeScript
- [ ] `WorktreeHandle` returned as a plain JavaScript object with correct field types
- [ ] Works on Node.js 18, 20, and 22
- [ ] Native binary pre-built for macOS (x64, arm64), Linux (x64, arm64), and Windows (x64)
- [ ] `npm install @worktree-core/node` works without requiring Rust toolchain on consumer's machine
- [ ] Error types map to JavaScript `Error` subclasses with descriptive messages
- [ ] Package size < 10 MB (native binary + TypeScript definitions)

## Tasks
- [ ] Create `worktree-core-node/` crate with napi-rs scaffolding
- [ ] Define `#[napi]` attributes on public API functions
- [ ] Map Rust types to napi-rs TypeScript equivalents
- [ ] Map `WorktreeError` variants to JavaScript error classes
- [ ] Configure napi-rs build matrix for macOS, Linux, Windows (x64 + arm64)
- [ ] Set up GitHub Actions workflow for prebuilt binary generation
- [ ] Configure npm publish workflow
- [ ] Write TypeScript test: `create()`, `list()`, `delete()` lifecycle
- [ ] Write TypeScript test: `checkConflicts()` returns correct types
- [ ] Write TypeScript test: error handling with try/catch
- [ ] Test on Node.js 18, 20, and 22
- [ ] Write README with API documentation and usage examples

## Technical Notes
- PRD Section 15 M4: "Node.js bindings via napi-rs (generates TypeScript types automatically)."
- PRD Section 15 M4 ship criterion: "Node.js package published to npm as `@worktree-core/node`."
- napi-rs generates `.d.ts` files from `#[napi]` annotated Rust structs and functions. This eliminates manual type maintenance.
- Pre-built binaries via `napi-rs`'s `@napi-rs/cli` support: the npm package includes platform-specific optional dependencies (`@worktree-core/node-darwin-arm64`, etc.) that resolve at install time.
- The `@worktree-core/node` scope uses npm org-style scoping. This requires an npm organization or user scope.
- napi-rs supports async Rust functions mapped to JavaScript Promises.

## Test Hints
- TypeScript test: `const manager = new Manager('/path/to/repo', {}); const list = await manager.list();`
- Verify TypeScript types match Rust types: `WorktreeHandle.path` is `string`, `WorktreeHandle.port` is `number | null`
- Test error handling: attempt to create a worktree with an invalid path, catch the error, verify message
- Verify prebuilt binaries: `npm install` on a clean machine without Rust toolchain should succeed
- Run tests on Node.js 18 and 22 to verify compatibility range

## Dependencies
- ISO-1.2 (Complete Type System -- all public types to expose)
- ISO-3.3 (Single-Pair Conflict Check -- checkConflicts API)
- ISO-3.4 (Conflict Matrix -- conflict_matrix API)

## Estimated Effort
XL

## Priority
P3

## Traceability
- PRD: Section 15 M4 (Scope -- Node.js bindings via napi-rs)
- PRD: Section 18 (Integration Targets -- Claude Code, OpenCode, Cursor are TypeScript)
- FR: FR-P3-005
- M4 ship criterion: "Node.js package published to npm as @worktree-core/node"
- QA ref: QA-4.5-001 through QA-4.5-005
