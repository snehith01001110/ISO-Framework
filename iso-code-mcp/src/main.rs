//! iso-code MCP server.
//!
//! Exposes the worktree manager over the Model Context Protocol via a stdio
//! JSON-RPC 2.0 transport, with correctly annotated tool definitions for
//! compliant MCP clients.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use iso_code::{Config, CreateOptions, DeleteOptions, GcOptions, Manager};

// ── JSON-RPC 2.0 types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Request {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Value::is_null")]
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl Response {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ── Tool definitions ─────────────────────────────────────────────────

fn tool_list() -> Value {
    json!([
        {
            "name": "worktree_list",
            "description": "List all managed worktrees in the repository.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo_path": {
                        "type": "string",
                        "description": "Absolute path to the repository root. Defaults to current working directory."
                    }
                }
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "worktree_status",
            "description": "Get status information for all worktrees (path, branch, state, disk usage).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo_path": { "type": "string" }
                }
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "conflict_check",
            "description": "Check for merge conflicts between worktrees. [Not implemented in v1.0]",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo_path": { "type": "string" },
                    "branch_a": { "type": "string" },
                    "branch_b": { "type": "string" }
                }
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "worktree_create",
            "description": "Create a new managed worktree.",
            "inputSchema": {
                "type": "object",
                "required": ["branch", "path"],
                "properties": {
                    "repo_path": { "type": "string" },
                    "branch": { "type": "string", "description": "Branch name (never transformed)." },
                    "path": { "type": "string", "description": "Absolute path for the new worktree." },
                    "base": { "type": "string", "description": "Base ref (default: HEAD)." },
                    "lock": { "type": "boolean", "default": false },
                    "allocate_port": { "type": "boolean", "default": false }
                }
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": false
            }
        },
        {
            "name": "worktree_delete",
            "description": "Delete a managed worktree. Runs unmerged commit check by default.",
            "inputSchema": {
                "type": "object",
                "required": ["path"],
                "properties": {
                    "repo_path": { "type": "string" },
                    "path": { "type": "string", "description": "Absolute path to the worktree." },
                    "force": { "type": "boolean", "default": false },
                    "force_dirty": { "type": "boolean", "default": false }
                }
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": false
            }
        },
        {
            "name": "worktree_gc",
            "description": "Garbage collect orphaned and stale worktrees.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo_path": { "type": "string" },
                    "dry_run": { "type": "boolean", "default": true },
                    "max_age_days": { "type": "integer" },
                    "force": { "type": "boolean", "default": false }
                }
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": false
            }
        }
    ])
}

// ── Tool handlers ────────────────────────────────────────────────────

fn get_manager(params: &Value) -> Result<Manager, String> {
    let repo_path = params
        .get("repo_path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });

    Manager::new(&repo_path, Config::default())
        .map_err(|e| format!("Failed to initialize Manager: {e}"))
}

fn handle_worktree_list(params: &Value) -> Result<Value, String> {
    let mgr = get_manager(params)?;
    let worktrees = mgr.list().map_err(|e| e.to_string())?;
    let result: Vec<Value> = worktrees
        .iter()
        .map(|wt| {
            json!({
                "path": wt.path.to_string_lossy(),
                "branch": wt.branch,
                "state": format!("{:?}", wt.state),
                "base_commit": wt.base_commit,
                "created_at": wt.created_at,
                "creator_pid": wt.creator_pid,
                "creator_name": wt.creator_name,
                "session_uuid": wt.session_uuid,
                "port": wt.port
            })
        })
        .collect();
    Ok(json!({ "worktrees": result }))
}

fn handle_worktree_status(params: &Value) -> Result<Value, String> {
    let mgr = get_manager(params)?;
    let worktrees = mgr.list().map_err(|e| e.to_string())?;
    let result: Vec<Value> = worktrees
        .iter()
        .map(|wt| {
            let disk_bytes = mgr.disk_usage(&wt.path);
            json!({
                "path": wt.path.to_string_lossy(),
                "branch": wt.branch,
                "state": format!("{:?}", wt.state),
                "disk_usage_bytes": disk_bytes,
                "disk_usage_mb": disk_bytes / (1024 * 1024),
                "port": wt.port,
                "adapter": wt.adapter,
                "setup_complete": wt.setup_complete
            })
        })
        .collect();
    Ok(json!({ "worktrees": result, "count": worktrees.len() }))
}

fn handle_conflict_check(_params: &Value) -> Result<Value, String> {
    // v1.0 stub — conflict detection is in v1.1
    Ok(json!({
        "status": "not_implemented",
        "message": "Conflict detection is available in v1.1"
    }))
}

fn handle_worktree_create(params: &Value) -> Result<Value, String> {
    let mgr = get_manager(params)?;

    let branch = params
        .get("branch")
        .and_then(|v| v.as_str())
        .ok_or("'branch' parameter is required")?;
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("'path' parameter is required")?;

    let mut opts = CreateOptions::default();
    if let Some(base) = params.get("base").and_then(|v| v.as_str()) {
        opts.base = Some(base.to_string());
    }
    if let Some(lock) = params.get("lock").and_then(|v| v.as_bool()) {
        opts.lock = lock;
    }
    if let Some(alloc) = params.get("allocate_port").and_then(|v| v.as_bool()) {
        opts.allocate_port = alloc;
    }

    let (handle, outcome) = mgr
        .create(branch, std::path::Path::new(path), opts)
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "path": handle.path.to_string_lossy(),
        "branch": handle.branch,
        "state": format!("{:?}", handle.state),
        "base_commit": handle.base_commit,
        "session_uuid": handle.session_uuid,
        "port": handle.port,
        "copy_outcome": format!("{:?}", outcome)
    }))
}

fn handle_worktree_delete(params: &Value) -> Result<Value, String> {
    let mgr = get_manager(params)?;

    let path_str = params
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("'path' parameter is required")?;
    let path = std::path::PathBuf::from(path_str);

    let worktrees = mgr.list().map_err(|e| e.to_string())?;
    let canon_path = dunce::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let handle = worktrees
        .iter()
        .find(|wt| {
            dunce::canonicalize(&wt.path)
                .map(|p| p == canon_path)
                .unwrap_or(wt.path == path)
        })
        .ok_or_else(|| format!("Worktree not found: {path_str}"))?
        .clone();

    let mut opts = DeleteOptions::default();
    if let Some(force) = params.get("force").and_then(|v| v.as_bool()) {
        opts.force = force;
    }
    if let Some(fd) = params.get("force_dirty").and_then(|v| v.as_bool()) {
        opts.force_dirty = fd;
    }

    mgr.delete(&handle, opts).map_err(|e| e.to_string())?;

    Ok(json!({ "deleted": path_str }))
}

fn handle_worktree_gc(params: &Value) -> Result<Value, String> {
    let mgr = get_manager(params)?;

    let mut opts = GcOptions::default();
    if let Some(dry_run) = params.get("dry_run").and_then(|v| v.as_bool()) {
        opts.dry_run = dry_run;
    }
    if let Some(days) = params.get("max_age_days").and_then(|v| v.as_u64()) {
        opts.max_age_days = Some(days as u32);
    }
    if let Some(force) = params.get("force").and_then(|v| v.as_bool()) {
        opts.force = force;
    }

    let report = mgr.gc(opts).map_err(|e| e.to_string())?;

    Ok(json!({
        "orphans": report.orphans.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "removed": report.removed.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "evicted": report.evicted.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "freed_bytes": report.freed_bytes,
        "dry_run": report.dry_run
    }))
}

// ── Request dispatch ─────────────────────────────────────────────────

fn dispatch(method: &str, params: &Value) -> Result<Value, (i32, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "iso-code",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        })),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => {
            let tool_name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| (-32602, "Missing 'name' in tools/call params".to_string()))?;
            let tool_params = params.get("arguments").unwrap_or(&Value::Null);

            let result = match tool_name {
                "worktree_list" => handle_worktree_list(tool_params),
                "worktree_status" => handle_worktree_status(tool_params),
                "conflict_check" => handle_conflict_check(tool_params),
                "worktree_create" => handle_worktree_create(tool_params),
                "worktree_delete" => handle_worktree_delete(tool_params),
                "worktree_gc" => handle_worktree_gc(tool_params),
                _ => Err(format!("Unknown tool: {tool_name}")),
            };

            result
                .map(|r| json!({ "content": [{ "type": "text", "text": r.to_string() }] }))
                .map_err(|e| (-32000, e))
        }
        "ping" => Ok(json!({})),
        "notifications/initialized" | "notifications/cancelled" => Ok(Value::Null),
        _ => Err((-32601, format!("Method not found: {method}"))),
    }
}

// ── Main loop ─────────────────────────────────────────────────────────

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[iso-code-mcp] stdin read error: {e}");
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::err(Value::Null, -32700, format!("Parse error: {e}"));
                let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
                let _ = out.flush();
                continue;
            }
        };

        // notifications have no id and don't need a response
        let is_notification = req.id.is_null();

        let resp = match dispatch(&req.method, &req.params) {
            Ok(result) => {
                if is_notification && result.is_null() {
                    continue; // No response for notifications
                }
                Response::ok(req.id, result)
            }
            Err((code, msg)) => Response::err(req.id, code, msg),
        };

        let json = match serde_json::to_string(&resp) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("[iso-code-mcp] serialization error: {e}");
                continue;
            }
        };

        if let Err(e) = writeln!(out, "{json}") {
            eprintln!("[iso-code-mcp] write error: {e}");
            break;
        }
        let _ = out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_has_6_tools() {
        let tools = tool_list();
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 6);
    }

    #[test]
    fn tools_list_has_correct_names() {
        let tools = tool_list();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"worktree_list"));
        assert!(names.contains(&"worktree_status"));
        assert!(names.contains(&"conflict_check"));
        assert!(names.contains(&"worktree_create"));
        assert!(names.contains(&"worktree_delete"));
        assert!(names.contains(&"worktree_gc"));
    }

    #[test]
    fn readonly_tools_have_correct_annotations() {
        let tools = tool_list();
        let arr = tools.as_array().unwrap();
        for tool in arr {
            let name = tool["name"].as_str().unwrap();
            let annotations = &tool["annotations"];
            match name {
                "worktree_list" | "worktree_status" | "conflict_check" => {
                    assert_eq!(annotations["readOnlyHint"], true, "{name} readOnlyHint");
                    assert_eq!(
                        annotations["destructiveHint"], false,
                        "{name} destructiveHint"
                    );
                    assert_eq!(annotations["idempotentHint"], true, "{name} idempotentHint");
                }
                "worktree_create" => {
                    assert_eq!(annotations["readOnlyHint"], false, "{name} readOnlyHint");
                    assert_eq!(
                        annotations["destructiveHint"], false,
                        "{name} destructiveHint"
                    );
                    assert_eq!(
                        annotations["idempotentHint"], false,
                        "{name} idempotentHint"
                    );
                }
                "worktree_delete" | "worktree_gc" => {
                    assert_eq!(annotations["readOnlyHint"], false, "{name} readOnlyHint");
                    assert_eq!(
                        annotations["destructiveHint"], true,
                        "{name} destructiveHint"
                    );
                    assert_eq!(
                        annotations["idempotentHint"], false,
                        "{name} idempotentHint"
                    );
                }
                _ => panic!("unexpected tool: {name}"),
            }
        }
    }

    #[test]
    fn conflict_check_returns_not_implemented() {
        let result = handle_conflict_check(&json!({})).unwrap();
        assert_eq!(result["status"], "not_implemented");
    }

    #[test]
    fn dispatch_initialize() {
        let result = dispatch("initialize", &json!({})).unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["serverInfo"]["name"].as_str().unwrap() == "iso-code");
    }

    #[test]
    fn dispatch_tools_list() {
        let result = dispatch("tools/list", &json!({})).unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);
    }

    #[test]
    fn dispatch_unknown_method_returns_error() {
        let result = dispatch("unknown/method", &json!({}));
        assert!(result.is_err());
        let (code, _) = result.unwrap_err();
        assert_eq!(code, -32601);
    }

    #[test]
    fn dispatch_ping() {
        let result = dispatch("ping", &json!({})).unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn dispatch_tools_call_unknown_tool_returns_error() {
        let params = json!({ "name": "nonexistent_tool", "arguments": {} });
        let result = dispatch("tools/call", &params);
        assert!(result.is_err());
    }
}
