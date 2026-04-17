//! MCP Contract Tests — QA-M-001 through QA-M-006.
//!
//! Spawns `iso-code-mcp` as a subprocess over stdio, sends a JSON-RPC 2.0
//! request, and asserts the response + tool annotations match the PRD
//! Section 12.3 contract.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use assert_cmd::prelude::*;
use serde_json::{json, Value};

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?}: {e}"));
    if !out.status.success() {
        panic!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn create_test_repo() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    run_git(dir.path(), &["init", "-b", "main"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test"]);
    run_git(dir.path(), &["commit", "--allow-empty", "-m", "initial"]);
    dir
}

struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl McpSession {
    fn spawn(home_override: &Path) -> Self {
        let mut cmd = Command::cargo_bin("iso-code-mcp").expect("iso-code-mcp binary");
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .env("ISO_CODE_HOME", home_override);
        let mut child = cmd.spawn().expect("spawn iso-code-mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self { child, stdin, stdout }
    }

    fn request(&mut self, method: &str, params: Value, id: i64) -> Value {
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });
        writeln!(self.stdin, "{req}").expect("write mcp request");
        self.stdin.flush().expect("flush mcp stdin");

        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .expect("read mcp response");
        assert!(!line.trim().is_empty(), "empty mcp response");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad JSON-RPC response {line:?}: {e}"))
    }

    fn tools_call(&mut self, tool: &str, args: Value, id: i64) -> Value {
        self.request(
            "tools/call",
            json!({ "name": tool, "arguments": args }),
            id,
        )
    }

    fn shutdown(mut self) {
        drop(self.stdin);
        let _ = self.child.wait();
    }
}

/// Pull the JSON payload out of the MCP content-wrapper:
/// `{"result":{"content":[{"type":"text","text":"<json>"}]}}`.
fn content_payload(resp: &Value) -> Value {
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("response has no content.text: {resp}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("content.text not JSON: {text}: {e}"))
}

/// Shared fixture: find the tool-list definition for a given tool name.
fn tool_definition(session: &mut McpSession, tool: &str) -> Value {
    let resp = session.request("tools/list", json!({}), 999);
    resp["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .find(|t| t["name"] == tool)
        .cloned()
        .unwrap_or_else(|| panic!("tool {tool} not advertised"))
}

/// QA-M-001: `worktree_list` — readOnly/idempotent, returns an array of
/// worktree objects with path/branch/state fields.
#[test]
fn qa_m_001_worktree_list() {
    let repo = create_test_repo();
    let home = tempfile::TempDir::new().unwrap();
    let mut s = McpSession::spawn(home.path());

    let tool = tool_definition(&mut s, "worktree_list");
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["annotations"]["idempotentHint"], true);

    let resp = s.tools_call(
        "worktree_list",
        json!({ "repo_path": repo.path().to_string_lossy() }),
        1,
    );
    let payload = content_payload(&resp);
    let entries = payload["worktrees"].as_array().expect("worktrees array");
    assert!(
        !entries.is_empty(),
        "primary worktree should be listed at minimum"
    );
    for e in entries {
        assert!(e.get("path").is_some());
        assert!(e.get("branch").is_some());
        assert!(e.get("state").is_some());
    }

    s.shutdown();
}

/// QA-M-002: `worktree_status` — readOnly/idempotent, returns status objects
/// with disk usage.
#[test]
fn qa_m_002_worktree_status() {
    let repo = create_test_repo();
    let home = tempfile::TempDir::new().unwrap();
    let mut s = McpSession::spawn(home.path());

    let tool = tool_definition(&mut s, "worktree_status");
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["annotations"]["idempotentHint"], true);

    let resp = s.tools_call(
        "worktree_status",
        json!({ "repo_path": repo.path().to_string_lossy() }),
        2,
    );
    let payload = content_payload(&resp);
    assert!(payload["count"].as_u64().is_some(), "count field required");
    let worktrees = payload["worktrees"].as_array().expect("worktrees array");
    for wt in worktrees {
        assert!(wt.get("disk_usage_bytes").is_some());
    }

    s.shutdown();
}

/// QA-M-003: `conflict_check` — v1.0 stub returns `not_implemented` without
/// erroring.
#[test]
fn qa_m_003_conflict_check_returns_not_implemented() {
    let repo = create_test_repo();
    let home = tempfile::TempDir::new().unwrap();
    let mut s = McpSession::spawn(home.path());

    let tool = tool_definition(&mut s, "conflict_check");
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["annotations"]["idempotentHint"], true);

    let resp = s.tools_call(
        "conflict_check",
        json!({ "repo_path": repo.path().to_string_lossy() }),
        3,
    );
    assert!(
        resp["error"].is_null(),
        "conflict_check must not error in v1.0: {resp}"
    );
    let payload = content_payload(&resp);
    assert_eq!(payload["status"], "not_implemented");

    s.shutdown();
}

/// QA-M-004: `worktree_create` — non-readonly/non-idempotent, returns the
/// new worktree's path/branch/state.
#[test]
fn qa_m_004_worktree_create() {
    let repo = create_test_repo();
    let home = tempfile::TempDir::new().unwrap();
    let mut s = McpSession::spawn(home.path());

    let tool = tool_definition(&mut s, "worktree_create");
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["annotations"]["idempotentHint"], false);

    let wt_path = repo.path().join("mcp-create-wt");
    let resp = s.tools_call(
        "worktree_create",
        json!({
            "repo_path": repo.path().to_string_lossy(),
            "branch": "mcp-create-branch",
            "path": wt_path.to_string_lossy(),
        }),
        4,
    );
    let payload = content_payload(&resp);
    assert_eq!(payload["branch"], "mcp-create-branch");
    assert_eq!(payload["state"], "Active");
    assert!(payload["session_uuid"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(wt_path.exists(), "worktree dir must be created on disk");

    s.shutdown();
}

/// QA-M-005: `worktree_delete` — destructive/non-idempotent, removes the
/// worktree from git's registry.
#[test]
fn qa_m_005_worktree_delete() {
    let repo = create_test_repo();
    let home = tempfile::TempDir::new().unwrap();
    let mut s = McpSession::spawn(home.path());

    let tool = tool_definition(&mut s, "worktree_delete");
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], true);
    assert_eq!(tool["annotations"]["idempotentHint"], false);

    let wt_path = repo.path().join("mcp-delete-wt");
    let create = s.tools_call(
        "worktree_create",
        json!({
            "repo_path": repo.path().to_string_lossy(),
            "branch": "mcp-delete-branch",
            "path": wt_path.to_string_lossy(),
        }),
        5,
    );
    assert!(create["error"].is_null(), "create failed: {create}");

    let resp = s.tools_call(
        "worktree_delete",
        json!({
            "repo_path": repo.path().to_string_lossy(),
            "path": wt_path.to_string_lossy(),
            "force": true,
        }),
        6,
    );
    assert!(resp["error"].is_null(), "delete errored: {resp}");
    assert!(!wt_path.exists(), "worktree dir must be removed");

    // Confirm git's registry no longer contains it.
    let list = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo.path())
        .output()
        .unwrap()
        .stdout;
    let list = String::from_utf8_lossy(&list);
    assert!(
        !list.contains(wt_path.to_string_lossy().as_ref()),
        "git worktree list still contains deleted path"
    );

    s.shutdown();
}

/// QA-M-006: `worktree_gc` — destructive/non-idempotent, returns a report
/// with `removed`, `evicted`, `orphans`, `freed_bytes`, `dry_run` fields.
#[test]
fn qa_m_006_worktree_gc_report_shape() {
    let repo = create_test_repo();
    let home = tempfile::TempDir::new().unwrap();
    let mut s = McpSession::spawn(home.path());

    let tool = tool_definition(&mut s, "worktree_gc");
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], true);
    assert_eq!(tool["annotations"]["idempotentHint"], false);

    let resp = s.tools_call(
        "worktree_gc",
        json!({
            "repo_path": repo.path().to_string_lossy(),
            "dry_run": true,
        }),
        7,
    );
    let payload = content_payload(&resp);
    assert!(payload["orphans"].is_array());
    assert!(payload["removed"].is_array());
    assert!(payload["evicted"].is_array());
    assert!(payload["freed_bytes"].as_u64().is_some());
    assert_eq!(payload["dry_run"], true);

    s.shutdown();
}

/// Companion check: the advertised tool list has all 6 tools with the
/// documented names. Failures here catch drops in schema wiring before
/// each per-tool test is even reached.
#[test]
fn mcp_tools_list_advertises_six_tools() {
    let home = tempfile::TempDir::new().unwrap();
    let mut s = McpSession::spawn(home.path());
    let resp = s.request("tools/list", json!({}), 1);
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "worktree_list",
        "worktree_status",
        "conflict_check",
        "worktree_create",
        "worktree_delete",
        "worktree_gc",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    s.shutdown();
}
