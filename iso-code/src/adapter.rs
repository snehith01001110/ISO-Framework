//! Ecosystem adapter trait and environment-variable injection.
//!
//! An [`EcosystemAdapter`] bootstraps language- or framework-specific state
//! inside a new worktree (e.g. `npm install`, copying `.env`, creating a
//! `uv` venv) and tears that state down when the worktree is deleted.
//!
//! The core library is responsible for populating the environment with
//! [`EnvVars`] before invoking the adapter. Adapters read those variables
//! either directly via [`std::env::var`] or by forwarding them to a child
//! process via [`std::process::Command::envs`].
//!
//! PRD reference: Section 6 (EcosystemAdapter Trait). The trait signature is
//! authoritative — do not add or rename methods in place; new capabilities
//! get new methods with sensible defaults.

use std::path::Path;

use crate::error::WorktreeError;
use crate::types::ReflinkMode;

/// Trait for language/framework-specific setup in new worktrees.
///
/// ## Environment variables injected before `setup()`
///
/// The core library sets 11 environment variables before every call to
/// [`EcosystemAdapter::setup`]. Six are canonical `ISO_CODE_*` variables; the
/// other five are compatibility aliases for tools that predate iso-code.
///
/// Canonical variables:
///
/// - `ISO_CODE_PATH` — absolute path to the new worktree on disk.
/// - `ISO_CODE_BRANCH` — branch name exactly as passed to `create()` (never transformed).
/// - `ISO_CODE_REPO` — absolute path to the repository root (the source worktree).
/// - `ISO_CODE_NAME` — `creator_name` from [`Config`](crate::types::Config) (e.g. `"claude-squad"`).
/// - `ISO_CODE_PORT` — allocated port, or empty string if no port was leased.
/// - `ISO_CODE_UUID` — stable session UUID for the worktree's lifetime.
///
/// Compatibility aliases (set to the same values as their canonical counterparts):
///
/// - `CCMANAGER_WORKTREE_PATH` ↔ `ISO_CODE_PATH`
/// - `CCMANAGER_BRANCH_NAME` ↔ `ISO_CODE_BRANCH`
/// - `CCMANAGER_GIT_ROOT` ↔ `ISO_CODE_REPO`
/// - `WM_WORKTREE_PATH` ↔ `ISO_CODE_PATH`
/// - `WM_PROJECT_ROOT` ↔ `ISO_CODE_REPO`
///
/// In-process adapters see these via [`std::env::var`]. Shell-out adapters
/// (see upcoming `ShellCommandAdapter`) receive them via
/// [`std::process::Command::envs`].
pub trait EcosystemAdapter: Send + Sync {
    /// Name used in `state.json` and log messages (e.g. `"default"`, `"shell-command"`, `"pnpm"`).
    fn name(&self) -> &str;

    /// Return `true` if this adapter should run for the given worktree path.
    ///
    /// Called during auto-detection. Inspect `package.json`, `Cargo.toml`,
    /// `pyproject.toml`, or whatever marker files identify the ecosystem.
    fn detect(&self, worktree_path: &Path) -> bool;

    /// Set up the environment in the new worktree.
    ///
    /// `worktree_path` is the newly-created worktree; `source_worktree` is
    /// the repository root the worktree was spawned from (used for copying
    /// `.env` files and other un-tracked state that needs to cross over).
    /// `ctx` carries per-call options from [`CreateOptions`](crate::types::CreateOptions)
    /// that the adapter must respect — currently
    /// [`SetupContext::reflink_mode`] for file-copying adapters.
    ///
    /// All 11 environment variables listed in the trait-level doc comment
    /// are set by the caller before this method runs.
    fn setup(
        &self,
        worktree_path: &Path,
        source_worktree: &Path,
        ctx: &SetupContext,
    ) -> Result<(), WorktreeError>;

    /// Clean up adapter-managed resources when the worktree is deleted.
    ///
    /// Called before `git worktree remove` so the adapter can still inspect
    /// files it owns. Failures are logged but do not propagate — a broken
    /// teardown must not strand a worktree on disk.
    fn teardown(&self, worktree_path: &Path) -> Result<(), WorktreeError>;

    /// Optionally transform the branch name before use.
    ///
    /// Default: identity (no transformation). The core library NEVER calls
    /// this internally. Only adapters that opt in use it.
    fn branch_name(&self, input: &str) -> String {
        input.to_string()
    }
}

/// Per-call context passed to [`EcosystemAdapter::setup`] alongside the
/// worktree paths.
///
/// Carries options from [`CreateOptions`](crate::types::CreateOptions) and
/// [`AttachOptions`](crate::types::AttachOptions) that the adapter must honor
/// (e.g. [`ReflinkMode`] for file-copying adapters). New per-call options
/// are added as fields here — not as new method parameters — so the trait
/// signature stays stable as the caller's needs evolve.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct SetupContext {
    /// Copy-on-Write mode for any file-copying the adapter performs.
    /// Mirrors [`CreateOptions::reflink_mode`](crate::types::CreateOptions::reflink_mode).
    pub reflink_mode: ReflinkMode,
}

impl SetupContext {
    /// Construct a `SetupContext` with an explicit reflink mode.
    pub fn new(reflink_mode: ReflinkMode) -> Self {
        Self { reflink_mode }
    }
}

/// Values injected into the environment before calling [`EcosystemAdapter::setup`].
///
/// Built by the `Manager` from the in-flight [`WorktreeHandle`](crate::types::WorktreeHandle)
/// and applied via [`EnvVars::apply_to_process`] (for in-process adapters) or
/// [`EnvVars::as_pairs`] (for shell-out adapters). Keeping the mapping in one
/// place guarantees the canonical and compatibility names stay in sync.
#[derive(Debug, Clone)]
pub(crate) struct EnvVars {
    pub path: String,
    pub branch: String,
    pub repo: String,
    pub name: String,
    pub port: String,
    pub uuid: String,
}

impl EnvVars {
    /// Flatten to the 11 `(key, value)` pairs documented on [`EcosystemAdapter`].
    pub(crate) fn as_pairs(&self) -> [(&'static str, &str); 11] {
        [
            ("ISO_CODE_PATH", &self.path),
            ("ISO_CODE_BRANCH", &self.branch),
            ("ISO_CODE_REPO", &self.repo),
            ("ISO_CODE_NAME", &self.name),
            ("ISO_CODE_PORT", &self.port),
            ("ISO_CODE_UUID", &self.uuid),
            ("CCMANAGER_WORKTREE_PATH", &self.path),
            ("CCMANAGER_BRANCH_NAME", &self.branch),
            ("CCMANAGER_GIT_ROOT", &self.repo),
            ("WM_WORKTREE_PATH", &self.path),
            ("WM_PROJECT_ROOT", &self.repo),
        ]
    }

    /// Apply the pairs to the current process environment.
    ///
    /// Returns a [`ScopedEnv`] guard that restores the prior values on drop.
    /// The restore step is what lets concurrent tests (or serial create calls
    /// in a long-running process) observe a clean environment after each
    /// `setup()`. It is NOT a safety barrier against concurrent writers —
    /// `std::env::set_var` is process-global, so `Manager` instances called
    /// from multiple threads still need external synchronization.
    pub(crate) fn apply_to_process(&self) -> ScopedEnv {
        let pairs = self.as_pairs();
        let mut prior: Vec<(&'static str, Option<std::ffi::OsString>)> =
            Vec::with_capacity(pairs.len());
        for (k, v) in pairs {
            prior.push((k, std::env::var_os(k)));
            std::env::set_var(k, v);
        }
        ScopedEnv { prior }
    }
}

/// RAII guard restoring the environment variables captured by
/// [`EnvVars::apply_to_process`] when dropped.
pub(crate) struct ScopedEnv {
    prior: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        for (k, v) in self.prior.drain(..) {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_vars_as_pairs_contains_all_11_keys() {
        let env = EnvVars {
            path: "/tmp/wt".to_string(),
            branch: "feature".to_string(),
            repo: "/tmp/repo".to_string(),
            name: "test".to_string(),
            port: "3100".to_string(),
            uuid: "uuid-1".to_string(),
        };
        let keys: Vec<&str> = env.as_pairs().iter().map(|(k, _)| *k).collect();
        assert_eq!(keys.len(), 11);
        for expected in [
            "ISO_CODE_PATH",
            "ISO_CODE_BRANCH",
            "ISO_CODE_REPO",
            "ISO_CODE_NAME",
            "ISO_CODE_PORT",
            "ISO_CODE_UUID",
            "CCMANAGER_WORKTREE_PATH",
            "CCMANAGER_BRANCH_NAME",
            "CCMANAGER_GIT_ROOT",
            "WM_WORKTREE_PATH",
            "WM_PROJECT_ROOT",
        ] {
            assert!(keys.contains(&expected), "missing env key: {expected}");
        }
    }

    #[test]
    fn compatibility_aliases_mirror_canonical_values() {
        let env = EnvVars {
            path: "/abs/wt".to_string(),
            branch: "b".to_string(),
            repo: "/abs/repo".to_string(),
            name: "n".to_string(),
            port: String::new(),
            uuid: "u".to_string(),
        };
        let m: std::collections::HashMap<&str, &str> =
            env.as_pairs().into_iter().collect();
        assert_eq!(m["CCMANAGER_WORKTREE_PATH"], m["ISO_CODE_PATH"]);
        assert_eq!(m["CCMANAGER_BRANCH_NAME"], m["ISO_CODE_BRANCH"]);
        assert_eq!(m["CCMANAGER_GIT_ROOT"], m["ISO_CODE_REPO"]);
        assert_eq!(m["WM_WORKTREE_PATH"], m["ISO_CODE_PATH"]);
        assert_eq!(m["WM_PROJECT_ROOT"], m["ISO_CODE_REPO"]);
    }

    #[test]
    fn scoped_env_restores_prior_values_on_drop() {
        let key = "ISO_CODE_PATH";
        let sentinel = "previous-value";
        std::env::set_var(key, sentinel);

        let env = EnvVars {
            path: "/tmp/other".to_string(),
            branch: "b".to_string(),
            repo: "/tmp/repo".to_string(),
            name: "n".to_string(),
            port: String::new(),
            uuid: "u".to_string(),
        };
        {
            let _guard = env.apply_to_process();
            assert_eq!(std::env::var(key).unwrap(), "/tmp/other");
        }
        assert_eq!(
            std::env::var(key).unwrap(),
            sentinel,
            "ScopedEnv must restore the prior ISO_CODE_PATH value"
        );
        std::env::remove_var(key);
    }
}
