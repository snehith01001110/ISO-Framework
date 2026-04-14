# Story 4.9: Documentation Site

## Status
Draft

## Epic
Epic 4: Ecosystem Integration

## User Story
As a developer evaluating or integrating worktree-core, I want a comprehensive documentation site so that I can understand the API, configure MCP clients, and set up ecosystem adapters without reading raw source code.

## Description
Build a documentation site using mdBook that covers the complete worktree-core API, integration guides for each supported MCP client, ecosystem adapter configuration, configuration reference, and troubleshooting. The site complements the auto-generated `docs.rs` API documentation with narrative guides, architecture diagrams, and worked examples. It is published via GitHub Pages.

## Acceptance Criteria
- [ ] mdBook site builds successfully and renders all pages
- [ ] API documentation section covering all public types and methods with examples
- [ ] Integration guide for each MCP client: Claude Code, Cursor, VS Code Copilot, OpenCode
- [ ] Ecosystem adapter guide: DefaultAdapter, ShellCommandAdapter, pnpm, uv, Cargo
- [ ] Configuration reference: `config.toml` format with all fields documented
- [ ] Troubleshooting section covering common errors and their solutions
- [ ] Architecture overview with crate structure diagram
- [ ] Quick-start guide: install + create first worktree in < 5 minutes
- [ ] Config snippet reference: copy-paste ready JSON for all 4 MCP clients
- [ ] Site deployed to GitHub Pages via CI
- [ ] All code examples compile (tested via `mdbook test` or extracted and compiled)

## Tasks
- [ ] Initialize mdBook project in `docs/` directory
- [ ] Write `SUMMARY.md` with page hierarchy
- [ ] Write quick-start guide
- [ ] Write API documentation for Manager, Config, CreateOptions, etc.
- [ ] Write MCP integration guide with per-client config snippets
- [ ] Write ecosystem adapter configuration guide
- [ ] Write configuration reference (all `config.toml` fields)
- [ ] Write troubleshooting guide (common errors and solutions)
- [ ] Write architecture overview with crate structure diagram
- [ ] Configure `mdbook test` to compile embedded Rust code examples
- [ ] Add GitHub Actions workflow for mdBook build and GitHub Pages deploy
- [ ] Add link to docs site from crate-level README and `docs.rs` metadata

## Technical Notes
- mdBook is the standard documentation tool for Rust projects. It is used by the Rust Book, Tokio, and many other crate docs.
- `mdbook test` extracts Rust code blocks and compiles them, catching documentation rot.
- GitHub Pages deployment is free for public repositories.
- The docs site should link to `docs.rs/worktree-core` for auto-generated API reference and focus on narrative guides.
- Config snippet reference must match PRD Section 12.3 exactly, especially the VS Code `"servers"` vs. `"mcpServers"` distinction.
- Architecture diagrams can use Mermaid (supported by mdBook via the mermaid preprocessor plugin).

## Test Hints
- `mdbook build` should produce no errors or warnings
- `mdbook test` should compile all Rust code examples
- Verify all internal links resolve (no broken anchors)
- Verify config snippets are valid JSON (parse with `serde_json`)
- Spot-check that API docs match current public API (no stale signatures)

## Dependencies
- ISO-2.9 (MCP Server Documentation -- config snippets to incorporate)
- ISO-2.1 (EcosystemAdapter trait -- adapter docs)
- ISO-4.1 (pnpm Adapter -- adapter guide content)
- ISO-4.2 (uv Adapter -- adapter guide content)
- ISO-4.3 (Cargo Adapter -- adapter guide content)

## Estimated Effort
L

## Priority
P3

## Traceability
- PRD: Section 15 M4 (implicit -- documentation is part of hardening)
- FR: FR-P3-009
- QA ref: QA-4.9-001 through QA-4.9-003
