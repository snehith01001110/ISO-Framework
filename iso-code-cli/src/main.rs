use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process;

use iso_code::{
    AttachOptions, Config, CreateOptions, DefaultAdapter, EcosystemAdapter, GcOptions, Manager,
    ShellCommandAdapter,
};

#[derive(serde::Deserialize)]
struct ClaudeCodeHookPayload {
    #[serde(default)]
    session_id: String,
    cwd: String,
    #[serde(default)]
    hook_event_name: String,
    name: String,
}

#[derive(serde::Deserialize)]
struct CliConfig {
    adapter: Option<AdapterConfig>,
}

#[derive(serde::Deserialize)]
struct AdapterConfig {
    #[serde(rename = "type")]
    adapter_type: String,
    #[serde(default)]
    files_to_copy: Vec<PathBuf>,
    post_create: Option<String>,
    pre_delete: Option<String>,
    post_delete: Option<String>,
    timeout_ms: Option<u64>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("[iso-code] Usage: wt <subcommand> [args]");
        eprintln!("[iso-code] Subcommands: hook, list, create, delete, attach, gc");
        process::exit(1);
    }

    match args[1].as_str() {
        "hook" => run_hook(&args[2..]),
        "list" => run_list(&args[2..]),
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
    let repo = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let mgr = match Manager::new(&repo, Config::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    };

    match mgr.list() {
        Ok(worktrees) => {
            for wt in worktrees {
                println!("{} [{}] {:?}", wt.path.display(), wt.branch, wt.state);
            }
        }
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    }
}

/// `wt create <branch> <path> [--setup]`
fn run_create(args: &[String]) {
    let mut setup = false;
    let mut positional = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--setup" => setup = true,
            flag if flag.starts_with("--") => {
                eprintln!("[iso-code] Unknown flag: {flag}");
                eprintln!("[iso-code] Usage: wt create <branch> <path> [--setup]");
                process::exit(1);
            }
            _ => positional.push(arg.clone()),
        }
    }

    if positional.len() != 2 {
        eprintln!("[iso-code] Usage: wt create <branch> <path> [--setup]");
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

    match mgr.create(branch, &path, opts) {
        Ok((handle, _)) => {
            println!("{}", handle.path.display());
        }
        Err(e) => {
            eprintln!("[iso-code] Error: {e}");
            process::exit(1);
        }
    }
}

fn manager_for_setup(repo: &Path, setup_requested: bool) -> Result<(Manager, bool), String> {
    if !setup_requested {
        return Manager::new(repo, Config::default())
            .map(|m| (m, false))
            .map_err(|e| e.to_string());
    }

    match load_adapter(repo)? {
        Some(adapter) => Manager::with_adapter(repo, Config::default(), Some(adapter))
            .map(|m| (m, true))
            .map_err(|e| e.to_string()),
        None => {
            eprintln!(
                "[iso-code] WARNING: --setup requested but no adapter is configured; creating worktree without setup"
            );
            Manager::new(repo, Config::default())
                .map(|m| (m, false))
                .map_err(|e| e.to_string())
        }
    }
}

fn load_adapter(repo: &Path) -> Result<Option<Box<dyn EcosystemAdapter>>, String> {
    let Some(config_path) = find_config_path(repo) else {
        return Ok(None);
    };

    let raw = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read {}: {e}", config_path.display()))?;
    let config: CliConfig = toml::from_str(&raw)
        .map_err(|e| format!("failed to parse {}: {e}", config_path.display()))?;

    let Some(adapter) = config.adapter else {
        return Ok(None);
    };

    match adapter.adapter_type.as_str() {
        "default" => Ok(Some(Box::new(DefaultAdapter::new(adapter.files_to_copy)))),
        "shell-command" => {
            let mut shell = ShellCommandAdapter::new();
            if let Some(cmd) = adapter.post_create {
                shell = shell.with_post_create(cmd);
            }
            if let Some(cmd) = adapter.pre_delete {
                shell = shell.with_pre_delete(cmd);
            }
            if let Some(cmd) = adapter.post_delete {
                shell = shell.with_post_delete(cmd);
            }
            if let Some(timeout_ms) = adapter.timeout_ms {
                shell = shell.with_timeout_ms(timeout_ms);
            }
            Ok(Some(Box::new(shell)))
        }
        other => Err(format!(
            "unsupported adapter type {other:?} in {}",
            config_path.display()
        )),
    }
}

fn find_config_path(repo: &Path) -> Option<PathBuf> {
    let project = repo.join(".iso-code.toml");
    if project.exists() {
        return Some(project);
    }

    user_config_path().filter(|p| p.exists())
}

fn user_config_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("iso-code").join("config.toml"))
    }

    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(xdg).join("iso-code").join("config.toml"));
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|p| p.join(".config").join("iso-code").join("config.toml"))
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
