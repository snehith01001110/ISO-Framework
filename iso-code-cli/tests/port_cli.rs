use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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

fn make_repo() -> (tempfile::TempDir, PathBuf) {
    let outer = tempfile::TempDir::new().unwrap();
    let repo = outer.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    run_git(&repo, &["init", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "test@example.com"]);
    run_git(&repo, &["config", "user.name", "Test"]);
    run_git(&repo, &["commit", "--allow-empty", "-m", "initial"]);
    (outer, repo)
}

fn wt(repo: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("wt").expect("wt binary");
    cmd.current_dir(repo);
    cmd
}

fn state_json(repo: &Path) -> Value {
    let raw = std::fs::read_to_string(repo.join(".git/iso-code/state.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn parse_allocated_port(stderr: &[u8]) -> u16 {
    let stderr = String::from_utf8_lossy(stderr);
    let marker = "Port allocated: ";
    let line = stderr
        .lines()
        .find(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("missing port allocation line in stderr:\n{stderr}"));
    line.split(marker).nth(1).unwrap().trim().parse().unwrap()
}

#[test]
fn create_port_allocates_and_records_port() {
    let (outer, repo) = make_repo();
    let wt_path = outer.path().join("port-wt");

    let output = wt(&repo)
        .args(["create", "port-branch", wt_path.to_str().unwrap(), "--port"])
        .output()
        .expect("spawn wt");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let port = parse_allocated_port(&output.stderr);
    assert!((3100..5100).contains(&port), "port out of range: {port}");

    let state = state_json(&repo);
    assert_eq!(
        state["active_worktrees"]["port-branch"]["port"].as_u64(),
        Some(u64::from(port))
    );
    assert_eq!(
        state["port_leases"]["port-branch"]["port"].as_u64(),
        Some(u64::from(port))
    );
}

#[test]
fn status_and_list_json_include_port() {
    let (outer, repo) = make_repo();
    let wt_path = outer.path().join("status-wt");

    let output = wt(&repo)
        .args([
            "create",
            "status-branch",
            wt_path.to_str().unwrap(),
            "--port",
        ])
        .output()
        .expect("spawn wt");
    assert!(output.status.success());
    let port = parse_allocated_port(&output.stderr);

    let status = wt(&repo)
        .args(["status", "--json"])
        .output()
        .expect("spawn wt status");
    assert!(status.status.success());
    let rows: Value = serde_json::from_slice(&status.stdout).unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["branch"] == "status-branch")
        .unwrap();
    assert_eq!(row["port"].as_u64(), Some(u64::from(port)));

    let table = wt(&repo)
        .args(["status"])
        .output()
        .expect("spawn wt status");
    assert!(table.status.success());
    let table_stdout = String::from_utf8_lossy(&table.stdout);
    assert!(table_stdout.contains("PORT"), "{table_stdout}");
    assert!(table_stdout.contains(&port.to_string()), "{table_stdout}");

    let list = wt(&repo)
        .args(["list", "--json"])
        .output()
        .expect("spawn wt list");
    assert!(list.status.success());
    let rows: Value = serde_json::from_slice(&list.stdout).unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["branch"] == "status-branch")
        .unwrap();
    assert_eq!(row["port"].as_u64(), Some(u64::from(port)));
}

#[test]
fn delete_releases_port_lease() {
    let (outer, repo) = make_repo();
    let wt_path = outer.path().join("delete-port-wt");

    let output = wt(&repo)
        .args([
            "create",
            "delete-port-branch",
            wt_path.to_str().unwrap(),
            "--port",
        ])
        .output()
        .expect("spawn wt");
    assert!(output.status.success());

    let output = wt(&repo)
        .args(["delete", wt_path.to_str().unwrap()])
        .output()
        .expect("spawn wt delete");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let state = state_json(&repo);
    assert!(state["port_leases"]["delete-port-branch"].is_null());
}

#[test]
fn setup_adapter_receives_iso_code_port() {
    let (outer, repo) = make_repo();
    let wt_path = outer.path().join("adapter-port-wt");

    std::fs::write(
        repo.join(".iso-code.toml"),
        "[adapter]\ntype = \"shell-command\"\npost_create = \"echo $ISO_CODE_PORT > .port\"\n",
    )
    .unwrap();

    let output = wt(&repo)
        .args([
            "create",
            "adapter-port-branch",
            wt_path.to_str().unwrap(),
            "--setup",
            "--port",
        ])
        .output()
        .expect("spawn wt");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let port = parse_allocated_port(&output.stderr);
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".port"))
            .unwrap()
            .trim(),
        port.to_string()
    );
}

#[test]
fn twenty_worktrees_created_with_port_get_unique_ports() {
    let (outer, repo) = make_repo();
    let mut ports = HashSet::new();

    for i in 0..20 {
        let branch = format!("port-{i}");
        let wt_path = outer.path().join(format!("port-wt-{i}"));
        let output = wt(&repo)
            .args(["create", &branch, wt_path.to_str().unwrap(), "--port"])
            .output()
            .expect("spawn wt");
        assert!(
            output.status.success(),
            "create {i} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let port = parse_allocated_port(&output.stderr);
        assert!(ports.insert(port), "duplicate port allocated: {port}");
    }

    assert_eq!(ports.len(), 20);
}

#[test]
fn exhausted_port_range_returns_clear_error() {
    let (outer, repo) = make_repo();
    let wt_path = outer.path().join("exhausted-wt");

    let _ = wt(&repo).args(["list"]).output().expect("spawn wt list");

    let state_path = repo.join(".git/iso-code/state.json");
    let mut state = state_json(&repo);
    let leases = state["port_leases"].as_object_mut().unwrap();
    for port in 3100..5100u16 {
        leases.insert(
            format!("lease-{port}"),
            serde_json::json!({
                "port": port,
                "branch": format!("lease-{port}"),
                "session_uuid": format!("uuid-{port}"),
                "pid": 12345,
                "created_at": "2099-01-01T00:00:00Z",
                "expires_at": "2099-01-01T08:00:00Z",
                "status": "active"
            }),
        );
    }
    std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap()).unwrap();

    let output = wt(&repo)
        .args([
            "create",
            "exhausted-branch",
            wt_path.to_str().unwrap(),
            "--port",
        ])
        .output()
        .expect("spawn wt");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rate limit exceeded") || stderr.contains("maximum"),
        "stderr: {stderr}"
    );
}
