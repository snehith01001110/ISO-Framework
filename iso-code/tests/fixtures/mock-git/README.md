# Mock git binaries

Shell scripts that emulate specific git versions. Tests set `PATH` to this
directory followed by the real `PATH` and (crucially) rename the script to
`git` on copy — the scripts here act as a per-test fixture. Each script
reports its own version via `git --version` and delegates the rest of its
subcommands to the real `git` found further down `PATH`.

Used by `tests/version_compat.rs` (QA-V-001 through QA-V-009).

## Versions simulated

| Script            | `git --version` reported | Notes                                                       |
|-------------------|--------------------------|-------------------------------------------------------------|
| `git-2.19`        | git version 2.19.0       | Below minimum — `Manager::new()` must refuse.               |
| `git-2.20`        | git version 2.20.0       | Minimum supported. No `repair`, no `-z`, no `merge-tree`.   |
| `git-2.30`        | git version 2.30.0       | Adds `worktree repair`.                                     |
| `git-2.35`        | git version 2.35.0       | Below `-z` cutoff (2.36). Parser must fall back.            |
| `git-2.37`        | git version 2.37.0       | Below `merge-tree --write-tree` cutoff (2.38).              |
| `git-2.41`        | git version 2.41.0       | Below `--orphan` cutoff (2.42).                             |
| `git-2.47`        | git version 2.47.0       | Below `worktree.useRelativePaths` cutoff (2.48).            |

The scripts do NOT implement feature gating beyond the `--version` report.
The library's capability detection is keyed off that string, so every
feature decision downstream is a pure function of the reported version.
Tests that need to see the actual fallback path also read the capability
map returned by `detect_capabilities()`.
