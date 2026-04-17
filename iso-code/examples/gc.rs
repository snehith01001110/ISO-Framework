//! Demonstrates garbage collection of orphaned worktrees.
//!
//! Run from a git repository:
//!   cargo run --example gc -- /path/to/repo

use iso_code::{Config, GcOptions, Manager};
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().expect("cannot determine cwd"));

    let mgr = Manager::new(&repo, Config::default())?;

    // Dry-run first (default) — reports what would be cleaned
    let report = mgr.gc(GcOptions::default())?;
    println!("GC dry-run:");
    println!("  orphans found: {}", report.orphans.len());
    for path in &report.orphans {
        println!("    {}", path.display());
    }
    println!("  bytes reclaimable: {}", report.freed_bytes);

    if report.orphans.is_empty() {
        println!("\nNo orphans to clean up.");
        return Ok(());
    }

    // Execute for real
    let mut execute_opts = GcOptions::default();
    execute_opts.dry_run = false;
    let report = mgr.gc(execute_opts)?;
    println!("\nGC executed:");
    println!("  removed: {}", report.removed.len());
    println!("  bytes freed: {}", report.freed_bytes);

    Ok(())
}
