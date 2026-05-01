use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process;

use iso_code::{AttachOptions, Config, CreateOptions, GcOptions, Manager, WorktreeHandle};

mod config;

#[derive(serde::Deserialize)]
struct ClaudeCodeHookPayload {
    #[serde(default)]
    session_id: String,
    cwd: String,
    #[serde(default)]
    hook_event_name: String,
    name: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("[iso-code] Usage: wt <subcommand> [args]");
        eprintln!("[iso-code] Subcommands: hook, list, status, create, delete, attach, gc");
        process::exit(1);
    }

    match args[1].as_str() {
        "hook" => run_hook(&args[2..]),
        "list" => run_list(&args[2..]),
        "status" => run_status(&args[2..]),
        "create" => run_create(&args[2..]),
        "delete" => run_delete(&args[2..]),
        "attach" => run_attach(&args[2..]),
        "gc" => run_gc(&args[2..]),
        unknown => {
            eprintln!("[iso-code] Unknown subcommand: {unknown}");
            process::exit(1);
        }
    }
}

/// wt hook --stdin-format claude-code [--setup]
fn run_hook(args: &[String]) {
    let mut setup = false;
    let mut stdin_format = String::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--stdin-format" => {
                if i + 1 < args.len() {
                    stdin_format = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("[iso-code] --stdin-format requires a value");
                    process::exit(1);
                }
            }
            "--setup" => {
                setup = true;
                i += 1;
            }
            unknown => {
                eprintln!("[iso-code] Unknown flag: {unknown}");
                process::exit(1);
            }
        }
    }

    if stdin_format != "claude-code" {
        eprintln!("[iso-code] Unsupported --stdin-format: {stdin_format}. Only 'claude-code' is supported.");
        process::exit(1);
    }

    // Read JSON from stdin
    let mut raw = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut raw) {
        eprintln!("[iso-code] Failed to read stdin: {e}");
        process::exit(1);
    }

    // Parse JSON
    let payload: ClaudeCodeHookPayload = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[iso-code] Failed to parse stdin JSON: {e}");
            process::exit(1);
        }
    };

    if payload.name.is_empty() {
        eprintln!("[iso-code] 'name' field is required in hook payload");
        process::exit(1);
    }

    if payload.cwd.is_empty() {
        eprintln!("[iso-code] 'cwd' field is required in hook payload");
        process::exit(1);
    }

    let repo_root = PathBuf::from(&payload.cwd);

    eprintln!(
        "[iso-code] hook received: session={} event={} branch={}",
        payload.session_id, payload.hook_event_name, payload.name
    );

    // Reject traversal tokens in the branch before we use it to build a path.
    // Branch names with `..` would otherwise escape the intended parent dir.
    if payload.name.split('/').any(|seg| seg == ".." || seg == ".") {
        eprintln!(
            "[iso-code] branch name contains path traversal: {}",
            payload.name
        );
        process::exit(1);
    }

    let (mgr, setup_enabled) = match manager_for_setup(&repo_root, setup) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Failed to initialize Manager: {e}");
            process::exit(1);
        }
    };

    // Compute worktree path: <repo>/../<branch-slug>. We flatten `/` to `-`
    // in the path segment only — the branch name passed to git stays verbatim
    // per PRD Appendix A rule 11 ("branch names are never transformed").
    // Without this, `feature/auth` would silently create a nested `feature/`
    // directory next to the repo.
    let path_slug = payload.name.replace('/', "-");
    let wt_path = repo_root.parent().unwrap_or(&repo_root).join(&path_slug);

    let mut opts = CreateOptions::default();
    opts.setup = setup_enabled;

    let (handle, _) = match mgr.create(&payload.name, &wt_path, opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[iso-code] Failed to create worktree: {e}");
            process::exit(1);
        }
    };

    // Emit exactly `<absolute-path>\n` on stdout. Shell wrappers pipe this
    // straight into `cd`, so any extra bytes (logging, BOM, stray output)
    // would break composition. `println!` is avoided in favor of `write_all`
    // for precise byte control.
    let path_str = handle.path.to_string_lossy();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if let Err(e) = out
        .write_all(path_str.as_bytes())
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
    {
        eprintln!("[iso-code] Failed to write worktree path to stdout: {e}");
        process::exit(1);
    }
}

/// wt list
fn run_list(args: &[String]) {
    let (repo, json) = parse_repo_and_json_args(args, "wt list [--json] [repo]");

    let mgr = match Manager::new(&repo, Config::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    };

    match mgr.list() {
        Ok(worktrees) => {
            if json {
                print_worktrees_json(&worktrees);
            } else {
                for wt in worktrees {
                    println!(
                        "{} [{}] {:?} port={}",
                        wt.path.display(),
                        wt.branch,
                        wt.state,
                        display_port(wt.port)
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    }
}

/// wt status [--json] [repo]
fn run_status(args: &[String]) {
    let (repo, json) = parse_repo_and_json_args(args, "wt status [--json] [repo]");

    let mgr = match Manager::new(&repo, Config::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    };

    match mgr.list() {
        Ok(worktrees) => {
            if json {
                print_worktrees_json(&worktrees);
            } else {
                print_status_table(&worktrees);
            }
        }
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    }
}

/// `wt create <branch> <path> [--setup] [--port]`
fn run_create(args: &[String]) {
    let mut setup = false;
    let mut allocate_port = false;
    let mut positional = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--setup" => setup = true,
            "--port" => allocate_port = true,
            flag if flag.starts_with("--") => {
                eprintln!("[iso-code] Unknown flag: {flag}");
                eprintln!("[iso-code] Usage: wt create <branch> <path> [--setup] [--port]");
                process::exit(1);
            }
            _ => positional.push(arg.clone()),
        }
    }

    if positional.len() != 2 {
        eprintln!("[iso-code] Usage: wt create <branch> <path> [--setup] [--port]");
        process::exit(1);
    }

    let branch = &positional[0];
    let path = PathBuf::from(&positional[1]);
    let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let (mgr, setup_enabled) = match manager_for_setup(&repo, setup) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    };

    let mut opts = CreateOptions::default();
    opts.setup = setup_enabled;
    opts.allocate_port = allocate_port;

    match mgr.create(branch, &path, opts) {
        Ok((handle, _)) => {
            if let Some(port) = handle.port {
                eprintln!("[iso-code] Port allocated: {port}");
            }
            println!("{}", handle.path.display());
        }
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    }
}

fn parse_repo_and_json_args(args: &[String], usage: &str) -> (PathBuf, bool) {
    let mut json = false;
    let mut repo: Option<PathBuf> = None;

    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            flag if flag.starts_with("--") => {
                eprintln!("[iso-code] Unknown flag: {flag}");
                eprintln!("[iso-code] Usage: {usage}");
                process::exit(1);
            }
            _ if repo.is_none() => repo = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("[iso-code] Usage: {usage}");
                process::exit(1);
            }
        }
    }

    (
        repo.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        json,
    )
}

fn display_port(port: Option<u16>) -> String {
    port.map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn print_status_table(worktrees: &[WorktreeHandle]) {
    println!("{:<8} {:<8} {:<28} PATH", "STATE", "PORT", "BRANCH");
    for wt in worktrees {
        println!(
            "{:<8} {:<8} {:<28} {}",
            format!("{:?}", wt.state),
            display_port(wt.port),
            wt.branch,
            wt.path.display()
        );
    }
}

fn print_worktrees_json(worktrees: &[WorktreeHandle]) {
    let rows: Vec<_> = worktrees
        .iter()
        .map(|wt| {
            serde_json::json!({
                "path": wt.path,
                "branch": wt.branch,
                "state": format!("{:?}", wt.state),
                "port": wt.port,
                "adapter": wt.adapter,
                "setup_complete": wt.setup_complete,
                "session_uuid": wt.session_uuid,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&rows).unwrap());
}

fn manager_for_setup(repo: &Path, setup_requested: bool) -> Result<(Manager, bool), String> {
    if !setup_requested {
        return Manager::new(repo, Config::default())
            .map(|m| (m, false))
            .map_err(|e| e.to_string());
    }

    let cfg = config::load_config(repo);
    match cfg.adapter {
        Some(ref adapter_cfg) => {
            let adapter = config::build_adapter(adapter_cfg);
            Manager::with_adapter(repo, Config::default(), Some(adapter))
                .map(|m| (m, true))
                .map_err(|e| e.to_string())
        }
        None => {
            eprintln!(
                "[iso-code] Warning: --setup requested but no adapter configured; creating worktree without setup"
            );
            Manager::new(repo, Config::default())
                .map(|m| (m, false))
                .map_err(|e| e.to_string())
        }
    }
}

/// `wt delete <path>`
fn run_delete(args: &[String]) {
    if args.len() != 1 {
        eprintln!("[iso-code] Usage: wt delete <path>");
        process::exit(1);
    }

    let path = PathBuf::from(&args[0]);
    let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mgr = match Manager::new(&repo, Config::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    };

    let worktrees = match mgr.list() {
        Ok(wts) => wts,
        Err(e) => {
            eprintln!("[iso-code] Error listing worktrees: {e}");
            process::exit(1);
        }
    };

    let canon_path = dunce::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let handle = match worktrees.iter().find(|wt| {
        dunce::canonicalize(&wt.path)
            .map(|p| p == canon_path)
            .unwrap_or(wt.path == path)
    }) {
        Some(h) => h.clone(),
        None => {
            eprintln!("[iso-code] Worktree not found: {}", path.display());
            process::exit(1);
        }
    };

    if let Err(e) = mgr.delete(&handle, iso_code::DeleteOptions::default()) {
        eprintln!("[iso-code] Error: {e}");
        process::exit(1);
    }

    eprintln!("[iso-code] Deleted worktree: {}", path.display());
}

/// `wt attach <path>` — register an existing external worktree under iso-code management.
fn run_attach(args: &[String]) {
    if args.len() != 1 {
        eprintln!("[iso-code] Usage: wt attach <path>");
        process::exit(1);
    }

    let path = PathBuf::from(&args[0]);
    let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mgr = match Manager::new(&repo, Config::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    };

    match mgr.attach(&path, AttachOptions::default()) {
        Ok(handle) => {
            println!("{}", handle.path.display());
            eprintln!(
                "[iso-code] Attached {} (branch={}, session={})",
                handle.path.display(),
                handle.branch,
                handle.session_uuid
            );
        }
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    }
}

/// `wt gc [--run] [--force] [--max-age-days N]`
///
/// Defaults to a dry run — the same default the library uses — so operators
/// get a preview before deleting anything. Pass `--run` to actually evict.
fn run_gc(args: &[String]) {
    let mut opts = GcOptions::default(); // dry_run = true
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--run" => {
                opts.dry_run = false;
                i += 1;
            }
            "--force" => {
                opts.force = true;
                i += 1;
            }
            "--max-age-days" => {
                if i + 1 >= args.len() {
                    eprintln!("[iso-code] --max-age-days requires a value");
                    process::exit(1);
                }
                opts.max_age_days = Some(match args[i + 1].parse() {
                    Ok(n) => n,
                    Err(_) => {
                        eprintln!("[iso-code] invalid --max-age-days: {}", args[i + 1]);
                        process::exit(1);
                    }
                });
                i += 2;
            }
            unknown => {
                eprintln!("[iso-code] Unknown flag: {unknown}");
                process::exit(1);
            }
        }
    }

    let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mgr = match Manager::new(&repo, Config::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    };

    match mgr.gc(opts) {
        Ok(report) => {
            let tag = if report.dry_run { "dry-run" } else { "gc" };
            for p in &report.orphans {
                println!("[{tag}] orphan: {}", p.display());
            }
            for p in &report.evicted {
                println!("[{tag}] evict:  {}", p.display());
            }
            for p in &report.removed {
                println!("[{tag}] remove: {}", p.display());
            }
            eprintln!(
                "[iso-code] gc summary: orphans={} evicted={} removed={} freed_bytes={}{}",
                report.orphans.len(),
                report.evicted.len(),
                report.removed.len(),
                report.freed_bytes,
                if report.dry_run { " (dry run)" } else { "" }
            );
        }
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    }
}
