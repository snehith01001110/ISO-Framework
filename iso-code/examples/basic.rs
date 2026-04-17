//! Basic worktree lifecycle: create, list, delete.
//!
//! Run from a git repository:
//!   cargo run --example basic -- /path/to/repo

use iso_code::{Config, CreateOptions, DeleteOptions, Manager};
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().expect("cannot determine cwd"));

    let mgr = Manager::new(&repo, Config::default())?;

    // Create a worktree
    let worktree_path = repo.join("../my-feature-worktree");
    let (handle, copy_outcome) =
        mgr.create("feature/example", &worktree_path, CreateOptions::default())?;

    println!("Created worktree at {:?}", handle.path);
    println!("  branch:      {}", handle.branch);
    println!("  base_commit: {}", handle.base_commit);
    println!("  state:       {:?}", handle.state);
    println!("  copy:        {:?}", copy_outcome);

    // List all managed worktrees
    let worktrees = mgr.list()?;
    println!("\n{} managed worktree(s):", worktrees.len());
    for wt in &worktrees {
        println!("  {} — {:?} ({})", wt.branch, wt.state, wt.path.display());
    }

    // Delete the worktree (runs 5-step unmerged commit check)
    mgr.delete(&handle, DeleteOptions::default())?;
    println!("\nDeleted worktree for branch '{}'", handle.branch);

    Ok(())
}
