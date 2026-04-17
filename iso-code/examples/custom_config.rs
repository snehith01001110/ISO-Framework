//! Demonstrates customizing Config and using CreateOptions.
//!
//! Run from a git repository:
//!   cargo run --example custom_config -- /path/to/repo

use iso_code::{Config, CreateOptions, DeleteOptions, Manager, ReflinkMode};
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().expect("cannot determine cwd"));

    // Custom configuration — start from defaults then override fields
    let mut config = Config::default();
    config.max_worktrees = 5;
    config.min_free_disk_mb = 1000;
    config.disk_threshold_percent = 80;
    config.creator_name = "my-orchestrator".to_string();
    config.offline = true; // skip network checks

    let mgr = Manager::new(&repo, config)?;

    // Create with options
    let mut opts = CreateOptions::default();
    opts.base = Some("main".to_string()); // branch from main instead of HEAD
    opts.lock = true;                     // lock immediately after creation
    opts.lock_reason = Some("automated test run".to_string());
    opts.reflink_mode = ReflinkMode::Preferred;
    opts.allocate_port = true;

    let worktree_path = repo.join("../custom-worktree");
    let (handle, _) = mgr.create("feature/custom", &worktree_path, opts)?;

    println!("Created locked worktree:");
    println!("  path:   {}", handle.path.display());
    println!("  state:  {:?}", handle.state);
    println!("  port:   {:?}", handle.port);

    // Must force-delete locked worktrees
    let mut delete_opts = DeleteOptions::default();
    delete_opts.force_locked = true;
    mgr.delete(&handle, delete_opts)?;
    println!("Force-deleted locked worktree");

    Ok(())
}
