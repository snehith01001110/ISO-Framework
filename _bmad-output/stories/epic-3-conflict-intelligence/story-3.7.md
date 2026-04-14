# Story 3.7: HTTP MCP Transport

## Status
Draft

## Epic
Epic 3: Conflict Intelligence

## User Story
As a developer using VS Code Dev Containers or SSH remote environments, I want an HTTP transport for the MCP server so that my AI coding assistant can communicate with worktree-core across network boundaries where stdio is not available.

## Description
Implement an HTTP-based MCP transport using the Axum web framework, behind a feature flag `mcp-http`. The HTTP server listens on a configurable port and exposes all MCP tools over HTTP POST. This enables remote development scenarios where the MCP client (e.g., VS Code Copilot in a Dev Container) cannot use stdio to communicate with the server. The transport must handle concurrent requests, CORS headers for web-based clients, and graceful shutdown.

## Acceptance Criteria
- [ ] HTTP MCP transport implemented behind `features = ["mcp-http"]` feature flag in `Cargo.toml`
- [ ] Server starts with `worktree-core-mcp --transport http --port <port>` command
- [ ] Default port is 8080 if not specified
- [ ] All 6 MCP tools accessible via HTTP POST to `/mcp` endpoint
- [ ] Request/response format follows MCP HTTP transport specification (JSON-RPC over HTTP)
- [ ] CORS headers set to allow requests from `localhost` origins (for Dev Container scenarios)
- [ ] Server handles concurrent requests without blocking
- [ ] Graceful shutdown on SIGTERM/SIGINT (completes in-flight requests before stopping)
- [ ] Health check endpoint at `GET /health` returns 200 OK
- [ ] Integration test passes in a simulated Dev Container environment
- [ ] Feature flag `mcp-http` does not add Axum dependency to builds that do not opt in

## Tasks
- [ ] Add `mcp-http` feature flag to `worktree-core-mcp/Cargo.toml` with Axum dependency
- [ ] Implement `HttpTransport` struct with `axum::Router` setup
- [ ] Implement `/mcp` POST handler that dispatches JSON-RPC requests to tool handlers
- [ ] Implement `/health` GET handler returning 200 OK
- [ ] Add CORS middleware via `tower-http` crate
- [ ] Add `--transport` CLI argument (`stdio` or `http`) and `--port` argument
- [ ] Implement graceful shutdown via `tokio::signal::ctrl_c()`
- [ ] Write test: HTTP POST to `/mcp` with `worktree_list` returns valid JSON-RPC response
- [ ] Write test: concurrent requests are handled correctly
- [ ] Write test: CORS preflight request returns correct headers
- [ ] Write Dev Container integration test (docker-compose with VS Code Dev Container config)

## Technical Notes
- PRD Section 12.3: "HTTP transport for MCP server (for Cursor remote, VS Code Dev Containers, SSH setups)" is M3 scope.
- Axum is chosen for its async performance, tower middleware ecosystem, and type-safe extractors.
- The `mcp-http` feature flag ensures the Axum + tokio dependency tree is not pulled in for stdio-only users.
- MCP HTTP transport spec (as of 2025-03-26): JSON-RPC 2.0 over HTTP POST with `Content-Type: application/json`.
- CORS must allow `http://localhost:*` origins for Dev Container port forwarding.
- The Dev Container test should use a `devcontainer.json` with port forwarding and verify MCP tool calls work through the forwarded port.

## Test Hints
- Start server in background, send HTTP POST with `curl` or `reqwest`, verify JSON-RPC response
- Test concurrent requests: spawn 10 parallel `worktree_list` requests, verify all return valid responses
- Test CORS: send OPTIONS request with `Origin: http://localhost:3000`, verify `Access-Control-Allow-Origin` header
- Dev Container test: use `docker-compose` to spin up a container, start MCP server, send request from host

## Dependencies
- ISO-1.13 (MCP Server Skeleton -- tool handler dispatch logic to reuse)
- ISO-3.6 (MCP conflict_check Tool -- all tools must work over HTTP)

## Estimated Effort
L

## Priority
P2

## Traceability
- PRD: Section 12.3 (MCP Server -- HTTP transport deferred to v1.1)
- PRD: Section 15 M3 (Ship criteria -- MCP HTTP transport in Dev Container)
- FR: FR-P2-007
- M3 ship criterion: "MCP HTTP transport responds correctly in VS Code Dev Container"
- QA ref: QA-3.7-001 through QA-3.7-004
