use std::path::{Path, PathBuf};

use crate::error::WorktreeError;
use crate::git;
use crate::guards;
use crate::ports;
use crate::state::{self, ActiveWorktreeEntry};
use crate::types::{
    AttachOptions, Config, CopyOutcome, CreateOptions, DeleteOptions, EcosystemAdapter, GcOptions,
    GcReport, GitCapabilities, PortLease, WorktreeHandle, WorktreeState,
};
use crate::util;

/// Check if a PID is alive via kill(pid, 0) on Unix.
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) just checks existence, sends no signal.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: u32) -> bool {
    true // Conservative: assume alive on non-Unix
}

/// Calculate total disk size of a worktree tree. Thin wrapper over the shared
/// helper so existing call sites in manager.rs don't need to change shape.
fn calculate_dir_size(path: &Path) -> u64 {
    util::dir_size_skipping_git([path].iter().copied())
}

/// Core worktree lifecycle manager. Entry point for all iso-code operations.
///
/// `Manager` is `Send` but not `Sync` — the circuit-breaker counter uses
/// `Cell` for zero-overhead single-threaded access. To share a Manager
/// across threads, wrap it in `Arc<Mutex<Manager>>`.
pub struct Manager {
    repo_root: PathBuf,
    config: Config,
    capabilities: GitCapabilities,
    /// Tracks consecutive git command failures for circuit breaker.
    consecutive_git_failures: std::cell::Cell<u32>,
    /// Optional ecosystem adapter for post-create/attach setup (e.g., npm install).
    adapter: Option<Box<dyn EcosystemAdapter>>,
}

impl Manager {
    /// Construct a Manager for the given repository root.
    ///
    /// Construction performs, in order:
    ///   1. Validate `git --version` is at least 2.20.
    ///   2. Canonicalize `repo_root` and detect [`GitCapabilities`].
    ///   3. Ensure `.git/iso-code/` exists and initialize `state.json` on first use.
    ///   4. Scan for orphaned worktrees (non-fatal; emits warnings only).
    ///   5. Sweep expired port leases.
    pub fn new(
        repo_root: impl AsRef<Path>,
        config: Config,
    ) -> Result<Self, WorktreeError> {
        Self::with_adapter(repo_root, config, None)
    }

    /// Construct a Manager with an explicit EcosystemAdapter.
    ///
    /// The adapter's `setup()` method will be called after `create()` and `attach()`
    /// when the corresponding options have `setup = true`.
    pub fn with_adapter(
        repo_root: impl AsRef<Path>,
        config: Config,
        adapter: Option<Box<dyn EcosystemAdapter>>,
    ) -> Result<Self, WorktreeError> {
        // Validate git, canonicalize the repo root, and probe capabilities.
        let capabilities = git::detect_git_version()?;
        let repo_root = dunce::canonicalize(repo_root.as_ref()).map_err(WorktreeError::Io)?;

        state::ensure_state_dir(&repo_root, config.home_override.as_deref())?;

        let mgr = Self {
            repo_root,
            config,
            capabilities,
            consecutive_git_failures: std::cell::Cell::new(0),
            adapter,
        };

        // Startup orphan scan — non-fatal; surface as a warning only.
        if let Ok(worktrees) = mgr.list_raw() {
            let orphan_paths: Vec<PathBuf> = worktrees
                .iter()
                .filter(|wt| wt.state == WorktreeState::Orphaned)
                .map(|wt| wt.path.clone())
                .collect();
            if !orphan_paths.is_empty() {
                eprintln!(
                    "[iso-code] WARNING: {} orphaned worktree(s) detected at startup",
                    orphan_paths.len()
                );
            }
        }

        // Drop leases whose TTL elapsed while we were absent.
        if let Err(e) = mgr.with_state(|s| {
            let now = chrono::Utc::now();
            ports::sweep_expired_leases(&mut s.port_leases, now);
            Ok(())
        }) {
            eprintln!("[iso-code] WARNING: startup port lease sweep failed: {e}");
        }

        Ok(mgr)
    }

    /// Read-modify-write state.json under the configured lock timeout.
    fn with_state<F>(&self, f: F) -> Result<state::StateV2, WorktreeError>
    where
        F: FnOnce(&mut state::StateV2) -> Result<(), WorktreeError>,
    {
        state::with_state_timeout(
            &self.repo_root,
            self.config.home_override.as_deref(),
            self.config.lock_timeout_ms,
            f,
        )
    }

    /// Check if the circuit breaker is open (too many consecutive git failures).
    fn check_circuit_breaker(&self) -> Result<(), WorktreeError> {
        let failures = self.consecutive_git_failures.get();
        if failures >= self.config.circuit_breaker_threshold {
            return Err(WorktreeError::CircuitBreakerOpen {
                consecutive_failures: failures,
            });
        }
        Ok(())
    }

    /// Record a git command success — resets the failure counter.
    fn record_git_success(&self) {
        self.consecutive_git_failures.set(0);
    }

    /// Record a git command failure — increments the failure counter.
    fn record_git_failure(&self) {
        self.consecutive_git_failures.set(self.consecutive_git_failures.get() + 1);
    }

    /// Return the detected git capabilities.
    pub fn git_capabilities(&self) -> &GitCapabilities {
        &self.capabilities
    }

    /// Return the repository root path.
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Return the current configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Raw git worktree list — no state reconciliation.
    fn list_raw(&self) -> Result<Vec<WorktreeHandle>, WorktreeError> {
        self.check_circuit_breaker()?;
        match git::run_worktree_list(&self.repo_root, &self.capabilities) {
            Ok(result) => {
                self.record_git_success();
                Ok(result)
            }
            Err(e) => {
                self.record_git_failure();
                Err(e)
            }
        }
    }

    /// List all worktrees, reconciling git porcelain output with state.json.
    ///
    /// Reconciliation runs on every call:
    ///   1. Run `git worktree list --porcelain`.
    ///   2. Enrich each handle with state.json metadata (created_at, session_uuid, ...).
    ///   3. Move state entries missing from git output to `stale_worktrees`.
    ///   4. Purge stale entries whose `expires_at` has passed.
    ///   5. Sweep port leases: drop dead holders and expired leases.
    pub fn list(&self) -> Result<Vec<WorktreeHandle>, WorktreeError> {
        let mut git_worktrees = self.list_raw()?;

        // Try to reconcile with state — if state read fails, just return git list
        let state = match state::read_state(
            &self.repo_root,
            self.config.home_override.as_deref(),
        ) {
            Ok(s) => s,
            Err(_) => return Ok(git_worktrees),
        };

        // Enrich git handles with state.json metadata. base_commit in the
        // porcelain output is the current HEAD, but WorktreeHandle.base_commit
        // is documented as the creation-time base — take the latter from state.
        for wt in &mut git_worktrees {
            if let Some(entry) = state.active_worktrees.get(&wt.branch) {
                wt.base_commit.clone_from(&entry.base_commit);
                wt.created_at = entry.created_at.to_rfc3339();
                wt.creator_pid = entry.creator_pid;
                wt.creator_name.clone_from(&entry.creator_name);
                wt.session_uuid.clone_from(&entry.session_uuid);
                wt.adapter.clone_from(&entry.adapter);
                wt.setup_complete = entry.setup_complete;
                wt.port = entry.port;
            }
        }

        // Reconcile: move state entries not in git to stale_worktrees, sweep leases
        let git_branches: std::collections::HashSet<String> =
            git_worktrees.iter().map(|wt| wt.branch.clone()).collect();
        let now = chrono::Utc::now();

        if let Err(e) = self.with_state(|s| {
                // Move orphaned state entries to stale.
                // Skip Pending/Creating entries: a concurrent Manager::create()
                // may have written the entry but not yet completed
                // `git worktree add`, so the branch legitimately isn't in git's
                // registry yet. Evicting it here races the create() and leaves
                // state.json pointing at the wrong bucket.
                let orphaned_keys: Vec<String> = s
                    .active_worktrees
                    .iter()
                    .filter(|&(k, v)| {
                        !git_branches.contains(k)
                            && !matches!(
                                v.state,
                                WorktreeState::Creating | WorktreeState::Pending
                            )
                    })
                    .map(|(k, _)| k.clone())
                    .collect();

                for key in orphaned_keys {
                    if let Some(entry) = s.active_worktrees.remove(&key) {
                        s.stale_worktrees.insert(
                            key,
                            state::StaleWorktreeEntry {
                                original_path: entry.path,
                                branch: entry.branch,
                                base_commit: entry.base_commit,
                                creator_name: entry.creator_name,
                                session_uuid: entry.session_uuid,
                                port: entry.port,
                                last_activity: entry.last_activity,
                                evicted_at: now,
                                eviction_reason: "reconciliation: not in git worktree list"
                                    .to_string(),
                                expires_at: now
                                    + chrono::Duration::days(
                                        i64::from(self.config.stale_metadata_ttl_days),
                                    ),
                                extra: std::collections::HashMap::new(),
                            },
                        );
                    }
                }

                // Purge expired stale entries
                s.stale_worktrees.retain(|_, v| v.expires_at > now);

                // Sweep port leases
                ports::sweep_expired_leases(&mut s.port_leases, now);

                Ok(())
            },
        ) {
            eprintln!("[iso-code] WARNING: list reconciliation failed: {e}");
        }

        Ok(git_worktrees)
    }

    /// Create a new managed worktree.
    ///
    /// The sequence is ordered and must not be reshuffled:
    ///   1. Run every pre-create guard.
    ///   2. Write a `Creating` entry to state.json.
    ///   3. Run `git worktree add`.
    ///   4. Post-create git-crypt verification.
    ///   5. Run [`EcosystemAdapter::setup`] if the caller requested it.
    ///   6. Transition the entry to `Active` (or `Locked` if `options.lock`).
    ///
    /// If any step after `git worktree add` fails, the worktree is force-removed
    /// with `git worktree remove --force` and the `Creating` entry is cleared
    /// before the error propagates.
    pub fn create(
        &self,
        branch: impl Into<String>,
        path: impl AsRef<Path>,
        options: CreateOptions,
    ) -> Result<(WorktreeHandle, CopyOutcome), WorktreeError> {
        let branch = branch.into();
        let target_path = path.as_ref().to_path_buf();

        // Step 1: Run pre-create guards
        let existing = self.list().unwrap_or_default();
        let crypt_status = guards::run_pre_create_guards(guards::PreCreateArgs {
            repo: &self.repo_root,
            branch: &branch,
            target_path: &target_path,
            caps: &self.capabilities,
            existing_worktrees: &existing,
            max_worktrees: self.config.max_worktrees,
            min_free_disk_mb: self.config.min_free_disk_mb,
            max_total_disk_bytes: self.config.max_total_disk_bytes,
            ignore_disk_limit: options.ignore_disk_limit,
            disk_threshold_percent: Some(self.config.disk_threshold_percent),
        })?;

        // Guard 12 returns a status enum; Locked / LockedNoKey means any new
        // worktree will inherit the encrypted blobs and git-crypt smudge
        // filters won't run. Fail fast rather than relying on the post-create
        // magic-byte check to catch it.
        match crypt_status {
            crate::types::GitCryptStatus::Locked
            | crate::types::GitCryptStatus::LockedNoKey => {
                return Err(WorktreeError::GitCryptLocked);
            }
            _ => {}
        }

        // Determine whether we'll create a new branch. This decides how base is used:
        // - new branch: base is the starting point for `git worktree add -b branch path base`.
        // - existing branch: `git worktree add` checks it out at its current tip. An
        //   explicit base that doesn't match the tip is rejected so WorktreeHandle
        //   .base_commit reflects what's actually on disk.
        let is_new_branch = !git::branch_exists(&self.repo_root, &branch)?;

        let base_commit = if is_new_branch {
            let base_ref = options.base.as_deref().unwrap_or("HEAD");
            git::resolve_ref(&self.repo_root, base_ref)?
        } else {
            let branch_commit =
                git::resolve_ref(&self.repo_root, &format!("refs/heads/{branch}"))?;
            if let Some(requested_base) = options.base.as_deref() {
                let requested_commit = git::resolve_ref(&self.repo_root, requested_base)?;
                if requested_commit != branch_commit {
                    return Err(WorktreeError::BranchExistsWithDifferentBase {
                        branch: branch.clone(),
                        branch_commit,
                        requested_base: requested_base.to_string(),
                        requested_commit,
                    });
                }
            }
            branch_commit
        };

        let session_uuid = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now();
        let creator_pid = std::process::id();

        // Step 2: Persist a `Creating` entry so a crash before the worktree
        // exists is still recoverable.
        if let Err(e) = self.with_state(|s| {
                s.active_worktrees.insert(
                    branch.clone(),
                    ActiveWorktreeEntry {
                        path: target_path.to_string_lossy().to_string(),
                        branch: branch.clone(),
                        base_commit: base_commit.clone(),
                        state: WorktreeState::Creating,
                        created_at,
                        last_activity: Some(created_at),
                        creator_pid,
                        creator_name: self.config.creator_name.clone(),
                        session_uuid: session_uuid.clone(),
                        adapter: None,
                        setup_complete: false,
                        port: None,
                        extra: std::collections::HashMap::new(),
                    },
                );
                Ok(())
            },
        ) {
            eprintln!("[iso-code] WARNING: failed to persist Creating state: {e}");
        }

        // Step 3: Materialize the worktree on disk.
        let add_result = git::worktree_add(
            &self.repo_root,
            &target_path,
            &branch,
            options.base.as_deref(),
            is_new_branch,
            options.lock,
            options.lock_reason.as_deref(),
        );

        if let Err(e) = add_result {
            // git worktree add may leave a half-created directory behind.
            // Scrub it directly — calling `git worktree remove` on a path that
            // was never registered would itself fail.
            let _ = std::fs::remove_dir_all(&target_path);
            // Clean up the Creating entry from state.json
            if let Err(se) = self.with_state(|s| { s.active_worktrees.remove(&branch); Ok(()) }) {
                eprintln!("[iso-code] WARNING: failed to clean up state after add failure: {se}");
            }
            return Err(e);
        }

        // Step 4: Post-create git-crypt verification.
        if let Err(e) = git::post_create_git_crypt_check(&target_path) {
            // Roll back the successful `git worktree add` before surfacing the
            // git-crypt failure, so we never leave a half-initialized worktree.
            let _ = git::worktree_remove_force(&self.repo_root, &target_path);
            if let Err(se) = self.with_state(|s| { s.active_worktrees.remove(&branch); Ok(()) }) {
                eprintln!("[iso-code] WARNING: failed to clean up state after git-crypt failure: {se}");
            }
            return Err(e);
        }

        // Step 5: EcosystemAdapter::setup() if requested
        let (adapter_name, setup_complete) = if options.setup {
            let Some(ref adapter) = self.adapter else {
                // setup=true but no adapter registered: fail loudly rather than silently.
                let _ = git::worktree_remove_force(&self.repo_root, &target_path);
                if let Err(se) = self.with_state(|s| { s.active_worktrees.remove(&branch); Ok(()) }) {
                    eprintln!("[iso-code] WARNING: failed to clean up state after missing-adapter error: {se}");
                }
                return Err(WorktreeError::SetupRequestedWithoutAdapter);
            };

            let repo_root = self.repo_root.clone();
            match adapter.setup(&target_path, &repo_root) {
                Ok(()) => (Some(adapter.name().to_string()), true),
                Err(e) => {
                    // Roll back the worktree so adapter failures don't leave a
                    // half-configured checkout on disk.
                    let _ = git::worktree_remove_force(&self.repo_root, &target_path);
                    if let Err(se) = self.with_state(|s| { s.active_worktrees.remove(&branch); Ok(()) }) {
                        eprintln!("[iso-code] WARNING: failed to clean up state after adapter failure: {se}");
                    }
                    return Err(e);
                }
            }
        } else {
            (None, false)
        };

        // Step 6: Build the handle and transition the entry to its final state.
        let final_state = if options.lock {
            WorktreeState::Locked
        } else {
            WorktreeState::Active
        };

        // Allocate port if requested
        let port = if options.allocate_port {
            let repo_id = state::compute_repo_id(&self.repo_root);
            self.with_state(|s| {
                let p = ports::allocate_port(
                    &repo_id,
                    &branch,
                    &session_uuid,
                    self.config.port_range_start,
                    self.config.port_range_end,
                    &s.port_leases,
                )?;
                let lease = ports::make_lease(p, &branch, &session_uuid, creator_pid);
                s.port_leases.insert(branch.clone(), lease);
                Ok(())
            })
            .ok()
            .and_then(|s| s.port_leases.get(&branch).map(|l| l.port))
        } else {
            None
        };

        let canon_path = dunce::canonicalize(&target_path).unwrap_or(target_path);

        // Persist Active state to state.json
        if let Err(e) = self.with_state(|s| {
                if let Some(entry) = s.active_worktrees.get_mut(&branch) {
                    entry.state = final_state.clone();
                    entry.path = canon_path.to_string_lossy().to_string();
                    entry.port = port;
                    entry.adapter.clone_from(&adapter_name);
                    entry.setup_complete = setup_complete;
                }
                Ok(())
            },
        ) {
            eprintln!("[iso-code] WARNING: failed to persist Active state: {e}");
        }

        let handle = WorktreeHandle::new(
            canon_path,
            branch,
            base_commit,
            final_state,
            created_at.to_rfc3339(),
            creator_pid,
            self.config.creator_name.clone(),
            adapter_name,
            setup_complete,
            port,
            session_uuid,
        );

        Ok((handle, CopyOutcome::None))
    }

    /// Attach an existing worktree (already in git's registry) under iso-code management.
    ///
    /// Never calls `git worktree add` — the worktree must already appear in
    /// `git worktree list --porcelain`. If the path is already tracked in
    /// `active_worktrees` the existing handle is returned (idempotent). If a
    /// matching entry exists in `stale_worktrees`, its port and session_uuid
    /// are recovered.
    pub fn attach(
        &self,
        path: impl AsRef<Path>,
        options: AttachOptions,
    ) -> Result<WorktreeHandle, WorktreeError> {
        let target_path = dunce::canonicalize(path.as_ref()).map_err(WorktreeError::Io)?;

        // Verify worktree exists in git's registry
        let worktrees = self.list()?;
        let git_entry = worktrees
            .iter()
            .find(|wt| {
                // Compare canonicalized paths to handle symlinks/relative paths
                dunce::canonicalize(&wt.path)
                    .map(|p| p == target_path)
                    .unwrap_or(false)
            })
            .ok_or_else(|| WorktreeError::WorktreeNotInGitRegistry(target_path.clone()))?;

        // Run git worktree repair if available (Git >= 2.30) to fix broken gitdir links
        if self.capabilities.has_repair {
            let _ = std::process::Command::new("git")
                .args(["worktree", "repair"])
                .arg(&target_path)
                .current_dir(&self.repo_root)
                .output();
        }

        // Try to recover session_uuid and port from stale_worktrees
        let existing_state = state::read_state(
            &self.repo_root,
            self.config.home_override.as_deref(),
        ).ok();

        let path_str = target_path.to_string_lossy().to_string();
        let branch = git_entry.branch.clone();

        // Check if already in active_worktrees (idempotent)
        if let Some(ref st) = existing_state {
            if let Some(entry) = st.active_worktrees.get(&branch) {
                return Ok(WorktreeHandle::new(
                    target_path,
                    branch,
                    entry.base_commit.clone(),
                    git_entry.state.clone(),
                    entry.created_at.to_rfc3339(),
                    entry.creator_pid,
                    entry.creator_name.clone(),
                    entry.adapter.clone(),
                    entry.setup_complete,
                    entry.port,
                    entry.session_uuid.clone(),
                ));
            }
        }

        // Try stale recovery: look up session_uuid and port from stale_worktrees.
        // Prefer an exact path match; only fall back to branch-name match if no
        // path match exists. A bare `||` would return whichever iterator order
        // surfaces first, which is non-deterministic and can donate port/UUID
        // from an unrelated stale entry.
        let recovered_stale_key: Option<String> = existing_state.as_ref().and_then(|st| {
            st.stale_worktrees
                .iter()
                .find(|(_, v)| v.original_path == path_str)
                .or_else(|| st.stale_worktrees.iter().find(|(_, v)| v.branch == branch))
                .map(|(k, _)| k.clone())
        });
        let (session_uuid, port) = recovered_stale_key
            .as_ref()
            .and_then(|k| existing_state.as_ref()?.stale_worktrees.get(k))
            .map(|stale| (stale.session_uuid.clone(), stale.port))
            .unwrap_or_else(|| (uuid::Uuid::new_v4().to_string(), None));

        let created_at = chrono::Utc::now();
        let creator_pid = std::process::id();

        // EcosystemAdapter::setup() if requested
        let (adapter_name, setup_complete) = if options.setup {
            let Some(ref adapter) = self.adapter else {
                return Err(WorktreeError::SetupRequestedWithoutAdapter);
            };
            let repo_root = self.repo_root.clone();
            match adapter.setup(&target_path, &repo_root) {
                Ok(()) => (Some(adapter.name().to_string()), true),
                Err(e) => {
                    eprintln!("[iso-code] WARNING: adapter setup failed during attach: {e}");
                    (Some(adapter.name().to_string()), false)
                }
            }
        } else {
            (None, false)
        };

        let handle = WorktreeHandle::new(
            target_path.clone(),
            branch.clone(),
            git_entry.base_commit.clone(),
            git_entry.state.clone(),
            created_at.to_rfc3339(),
            creator_pid,
            self.config.creator_name.clone(),
            adapter_name.clone(),
            setup_complete,
            port,
            session_uuid.clone(),
        );

        // Persist to state.json — remove only the specific recovered stale entry, add to active
        if let Err(e) = self.with_state(|s| {
                if let Some(ref k) = recovered_stale_key {
                    s.stale_worktrees.remove(k);
                }

                // Add to active_worktrees
                s.active_worktrees.insert(
                    branch.clone(),
                    ActiveWorktreeEntry {
                        path: path_str.clone(),
                        branch: branch.clone(),
                        base_commit: git_entry.base_commit.clone(),
                        state: git_entry.state.clone(),
                        created_at,
                        last_activity: Some(created_at),
                        creator_pid,
                        creator_name: self.config.creator_name.clone(),
                        session_uuid: session_uuid.clone(),
                        adapter: adapter_name,
                        setup_complete,
                        port,
                        extra: std::collections::HashMap::new(),
                    },
                );

                Ok(())
            },
        ) {
            eprintln!("[iso-code] WARNING: failed to persist attach state: {e}");
        }

        Ok(handle)
    }

    /// Delete a managed worktree.
    ///
    /// Ordered pre-flight checks (each may abort the delete):
    ///   1. Refuse to delete the caller's current working directory.
    ///   2. Reject a dirty working tree unless `options.force_dirty`.
    ///   3. Reject unmerged commits unless `options.force`.
    ///   4. Reject worktrees held by `git worktree lock`.
    ///
    /// Once the checks pass, the entry transitions to `Deleting`, `git worktree
    /// remove` runs, and the entry is finally cleared from state.json.
    pub fn delete(
        &self,
        handle: &WorktreeHandle,
        options: DeleteOptions,
    ) -> Result<(), WorktreeError> {
        // Step 1: Not deleting CWD
        guards::check_not_cwd(&handle.path)?;

        // Step 2: Uncommitted changes check
        if !options.force_dirty {
            guards::check_no_uncommitted_changes(&handle.path)?;
        }

        // Step 3: Five-step unmerged commit check (skipped if force)
        if !options.force {
            guards::five_step_unmerged_check(&handle.branch, &self.repo_root, self.config.offline)?;
        }

        // Step 4: Not locked (skipped if force_locked).
        if !options.force_locked {
            guards::check_not_locked(handle)?;
        }

        // Step 5: Transition to Deleting in state.json
        let branch = handle.branch.clone();
        if let Err(e) = self.with_state(|s| {
                if let Some(entry) = s.active_worktrees.get_mut(&branch) {
                    entry.state = WorktreeState::Deleting;
                }
                Ok(())
            },
        ) {
            eprintln!("[iso-code] WARNING: failed to persist Deleting state: {e}");
        }

        // Step 6: EcosystemAdapter::teardown() if setup ran during create/attach.
        // Called before the worktree is physically removed so the adapter can
        // still inspect files it owns. Errors are logged, not propagated —
        // teardown failure shouldn't block the delete and leak a worktree.
        if handle.setup_complete {
            if let Some(ref adapter) = self.adapter {
                if let Err(e) = adapter.teardown(&handle.path) {
                    eprintln!(
                        "[iso-code] WARNING: adapter teardown failed for {}: {e}",
                        handle.path.display()
                    );
                }
            }
        }

        // Step 7: Remove worktree
        // Try removing .DS_Store first on macOS (it blocks git worktree remove)
        #[cfg(target_os = "macos")]
        {
            let ds_store = handle.path.join(".DS_Store");
            if ds_store.exists() {
                let _ = std::fs::remove_file(&ds_store);
            }
        }

        if options.force_locked {
            git::worktree_remove_force(&self.repo_root, &handle.path)?;
        } else {
            git::worktree_remove(&self.repo_root, &handle.path)?;
        }

        // Steps 7-8: Transition to Deleted, release port lease, remove from active
        if let Err(e) = self.with_state(|s| {
                s.active_worktrees.remove(&branch);
                s.port_leases.remove(&branch);
                Ok(())
            },
        ) {
            eprintln!("[iso-code] WARNING: failed to persist Deleted state: {e}");
        }

        Ok(())
    }

    /// Garbage collect orphaned and stale worktrees.
    ///
    /// The default [`GcOptions`] is dry-run. Locked worktrees are always
    /// preserved, regardless of `options.force`. Evicted entries are moved to
    /// `stale_worktrees` so their metadata can be recovered — `gc()` never
    /// silently drops state.
    pub fn gc(&self, options: GcOptions) -> Result<GcReport, WorktreeError> {
        let max_age_days = options
            .max_age_days
            .unwrap_or(self.config.gc_max_age_days);

        // Get current git worktree list (source of truth).
        let mut git_worktrees = git::run_worktree_list(&self.repo_root, &self.capabilities)?;

        // Enrich handles from state.json so the age gate and PID-liveness gate
        // below have values to read. The porcelain parser leaves created_at
        // empty and creator_pid zero; without enrichment both gates silently
        // skip every worktree. We do not reconcile here (that is list()'s job)
        // so gc() stays side-effect-free until the mutation block further down.
        if let Ok(state) = state::read_state(
            &self.repo_root,
            self.config.home_override.as_deref(),
        ) {
            for wt in &mut git_worktrees {
                if let Some(entry) = state.active_worktrees.get(&wt.branch) {
                    wt.base_commit.clone_from(&entry.base_commit);
                    wt.created_at = entry.created_at.to_rfc3339();
                    wt.creator_pid = entry.creator_pid;
                    wt.creator_name.clone_from(&entry.creator_name);
                    wt.session_uuid.clone_from(&entry.session_uuid);
                    wt.port = entry.port;
                }
            }
        }

        let mut orphans: Vec<PathBuf> = Vec::new();
        // Parallel lists: path for reporting, branch (optional) for state lookup.
        let mut removed_entries: Vec<(PathBuf, Option<String>)> = Vec::new();
        let mut evicted_entries: Vec<(PathBuf, Option<String>)> = Vec::new();
        let mut freed_bytes: u64 = 0;

        let now = chrono::Utc::now();
        let age_cutoff = now - chrono::Duration::days(i64::from(max_age_days));

        // Find orphans: worktrees that appear in git list but have broken state,
        // or are prunable (git marked them as prunable)
        for wt in &git_worktrees {
            if wt.state == WorktreeState::Orphaned {
                orphans.push(wt.path.clone());
            }
        }

        // Also find old worktrees eligible for gc (non-main, non-locked, old enough)
        // We skip the main worktree (no branch means bare/main) and locked ones
        for wt in &git_worktrees {
            // Locked worktrees are off-limits to gc, even under `force`.
            if wt.state == WorktreeState::Locked {
                continue;
            }

            // Skip the main worktree (empty path = main or bare)
            if wt.branch.is_empty() {
                continue;
            }

            // Check if worktree is old enough to be a gc candidate
            // Parse created_at — if empty or unparseable, skip age check
            let is_old_enough = if wt.created_at.is_empty() {
                false
            } else {
                chrono::DateTime::parse_from_rfc3339(&wt.created_at)
                    .map(|t| t.with_timezone(&chrono::Utc) < age_cutoff)
                    .unwrap_or(false)
            };

            if !is_old_enough && wt.state != WorktreeState::Orphaned {
                continue;
            }

            // PID-liveness check: if creator_pid is still alive, skip eviction
            if wt.state == WorktreeState::Active && wt.creator_pid != 0
                && is_pid_alive(wt.creator_pid) {
                continue;
            }

            // Five-step unmerged check (skip if force or orphaned)
            if !options.force && wt.state != WorktreeState::Orphaned
                && guards::five_step_unmerged_check(
                    &wt.branch,
                    &self.repo_root,
                    self.config.offline,
                ).is_err() {
                continue; // Has unmerged commits — skip
            }

            // Calculate disk usage before removal
            let disk_usage = calculate_dir_size(&wt.path);

            if !orphans.contains(&wt.path) {
                evicted_entries.push((wt.path.clone(), Some(wt.branch.clone())));
            }

            if !options.dry_run {
                // Remove .DS_Store first on macOS
                #[cfg(target_os = "macos")]
                {
                    let ds = wt.path.join(".DS_Store");
                    if ds.exists() {
                        let _ = std::fs::remove_file(&ds);
                    }
                }

                if git::worktree_remove(&self.repo_root, &wt.path).is_ok() {
                    removed_entries.push((wt.path.clone(), Some(wt.branch.clone())));
                    freed_bytes += disk_usage;
                }
            }
        }

        // Call git worktree prune to clean stale git metadata (not dry_run gated)
        if !options.dry_run {
            let _ = std::process::Command::new("git")
                .args(["worktree", "prune"])
                .current_dir(&self.repo_root)
                .output();
        }

        // Projection used in the final report — path-only lists.
        let evicted: Vec<PathBuf> = evicted_entries.iter().map(|(p, _)| p.clone()).collect();
        let removed: Vec<PathBuf> = removed_entries.iter().map(|(p, _)| p.clone()).collect();

        // Persist evictions to stale_worktrees and record GC history in state.json.
        // Eviction never silently drops state — the entry migrates to
        // stale_worktrees so callers can still recover ports and metadata.
        // The same pass sweeps abandoned Creating/Pending entries whose creator
        // is dead; these are the leftovers when create()'s cleanup path failed
        // to reacquire the lock.
        let orphan_paths = orphans.clone();
        let evicted_inputs = evicted_entries.clone();
        let removed_inputs = removed_entries.clone();
        if !options.dry_run || !evicted_inputs.is_empty() || !removed_inputs.is_empty() {
            if let Err(e) = self.with_state(|s| {
                let now = chrono::Utc::now();
                let ttl_days = i64::from(self.config.stale_metadata_ttl_days);

                // Helper: move an active_worktrees entry to stale_worktrees by
                // branch name (preferred) or canonicalized path fallback.
                let move_to_stale = |s: &mut state::StateV2,
                                     branch_hint: Option<&str>,
                                     path: &std::path::Path,
                                     reason: &str| {
                    let key: Option<String> = match branch_hint {
                        Some(b) if s.active_worktrees.contains_key(b) => Some(b.to_string()),
                        _ => {
                            let canon_target = dunce::canonicalize(path).ok();
                            s.active_worktrees
                                .iter()
                                .find(|(_, v)| {
                                    let v_path = std::path::Path::new(&v.path);
                                    let canon_entry = dunce::canonicalize(v_path).ok();
                                    match (&canon_target, &canon_entry) {
                                        (Some(a), Some(b)) => a == b,
                                        _ => v.path == path.to_string_lossy(),
                                    }
                                })
                                .map(|(k, _)| k.clone())
                        }
                    };

                    if let Some(key) = key {
                        if let Some(entry) = s.active_worktrees.remove(&key) {
                            s.stale_worktrees.insert(
                                key.clone(),
                                state::StaleWorktreeEntry {
                                    original_path: entry.path,
                                    branch: entry.branch,
                                    base_commit: entry.base_commit,
                                    creator_name: entry.creator_name,
                                    session_uuid: entry.session_uuid,
                                    port: entry.port,
                                    last_activity: entry.last_activity,
                                    evicted_at: now,
                                    eviction_reason: reason.to_string(),
                                    expires_at: now + chrono::Duration::days(ttl_days),
                                    extra: std::collections::HashMap::new(),
                                },
                            );
                            // Transition any remaining port lease to "stale".
                            if let Some(lease) = s.port_leases.get_mut(&key) {
                                lease.status = "stale".to_string();
                            }
                        }
                    }
                };

                for (path, branch) in &evicted_inputs {
                    move_to_stale(s, branch.as_deref(), path, "gc: age exceeded");
                }
                for path in &orphan_paths {
                    // Only persist the orphan→stale move once we've actually
                    // removed it (orphans are just reported in dry-run).
                    if removed_inputs.iter().any(|(p, _)| p == path) {
                        move_to_stale(s, None, path, "gc: orphaned worktree");
                    }
                }

                // Sweep abandoned Creating/Pending entries older than 10 minutes
                // whose creator process is dead. These are leftovers from a
                // create() that died between writing the Creating entry and
                // wiring state to Active (and whose cleanup branch could not
                // reacquire the lock).
                let sweep_cutoff = now - chrono::Duration::minutes(10);
                let sweep_keys: Vec<String> = s
                    .active_worktrees
                    .iter()
                    .filter(|(_, v)| {
                        matches!(
                            v.state,
                            WorktreeState::Creating | WorktreeState::Pending
                        ) && v.created_at < sweep_cutoff
                            && v.creator_pid != 0
                            && !is_pid_alive(v.creator_pid)
                    })
                    .map(|(k, _)| k.clone())
                    .collect();
                for key in sweep_keys {
                    if let Some(entry) = s.active_worktrees.remove(&key) {
                        s.stale_worktrees.insert(
                            key.clone(),
                            state::StaleWorktreeEntry {
                                original_path: entry.path,
                                branch: entry.branch,
                                base_commit: entry.base_commit,
                                creator_name: entry.creator_name,
                                session_uuid: entry.session_uuid,
                                port: entry.port,
                                last_activity: entry.last_activity,
                                evicted_at: now,
                                eviction_reason: "gc: abandoned Creating entry"
                                    .to_string(),
                                expires_at: now + chrono::Duration::days(ttl_days),
                                extra: std::collections::HashMap::new(),
                            },
                        );
                        if let Some(lease) = s.port_leases.get_mut(&key) {
                            lease.status = "stale".to_string();
                        }
                    }
                }

                if !options.dry_run {
                    s.gc_history.push(state::GcHistoryEntry {
                        timestamp: now,
                        removed: removed_inputs.len() as u32,
                        evicted: evicted_inputs.len() as u32,
                        freed_mb: freed_bytes / (1024 * 1024),
                        extra: std::collections::HashMap::new(),
                    });
                }

                Ok(())
            }) {
                eprintln!("[iso-code] WARNING: failed to persist GC state: {e}");
            }
        }

        Ok(GcReport::new(orphans, removed, evicted, freed_bytes, options.dry_run))
    }

    /// Mark `branch` as recently active by bumping its `last_activity`
    /// timestamp in `state.json`. Callers wrap user-visible actions (shell
    /// into the worktree, run a build, etc.) with this so `gc()` can tell
    /// "idle since creation" apart from "recently used." Returns
    /// `InvalidStateTransition`-style `StateCorrupted` if the branch isn't
    /// tracked in `active_worktrees`.
    pub fn touch(&self, branch: &str) -> Result<(), WorktreeError> {
        let branch_owned = branch.to_string();
        self.with_state(|s| {
            match s.active_worktrees.get_mut(&branch_owned) {
                Some(entry) => {
                    entry.last_activity = Some(chrono::Utc::now());
                    Ok(())
                }
                None => Err(WorktreeError::StateCorrupted {
                    reason: format!("touch: branch '{branch_owned}' not in active_worktrees"),
                }),
            }
        })?;
        Ok(())
    }

    /// Return the active port lease for a branch, if any.
    pub fn port_lease(&self, branch: &str) -> Option<PortLease> {
        let s = state::read_state(
            &self.repo_root,
            self.config.home_override.as_deref(),
        ).ok()?;
        let now = chrono::Utc::now();
        s.port_leases
            .get(branch)
            .filter(|l| !ports::is_lease_expired(l, now))
            .cloned()
    }

    /// Allocate a port lease for a branch without creating a worktree.
    pub fn allocate_port(&self, branch: &str, session_uuid: &str) -> Result<u16, WorktreeError> {
        let repo_id = state::compute_repo_id(&self.repo_root);
        let mut allocated_port: u16 = 0;
        self.with_state(|s| {
            let port = ports::allocate_port(
                &repo_id,
                branch,
                session_uuid,
                self.config.port_range_start,
                self.config.port_range_end,
                &s.port_leases,
            )?;
            let lease = ports::make_lease(port, branch, session_uuid, std::process::id());
            s.port_leases.insert(branch.to_string(), lease);
            allocated_port = port;
            Ok(())
        })?;
        Ok(allocated_port)
    }

    /// Return the on-disk byte size of a worktree tree, skipping the `.git/`
    /// subtree and deduplicating hardlinks on Unix.
    pub fn disk_usage(&self, path: &Path) -> u64 {
        calculate_dir_size(path)
    }

    /// Release a port lease explicitly.
    pub fn release_port(&self, branch: &str) -> Result<(), WorktreeError> {
        self.with_state(|s| {
            s.port_leases.remove(branch);
            Ok(())
        })?;
        Ok(())
    }

    /// Extend an active port lease's TTL by another 8 hours from now.
    ///
    /// Callers running a long-lived dev server invoke this roughly every
    /// TTL/3 to keep the lease from expiring mid-session. Returns
    /// `StateCorrupted` if the branch has no active lease — expired leases
    /// must be re-allocated via [`Manager::allocate_port`] rather than
    /// renewed, since their port may have been reassigned.
    pub fn renew_port_lease(&self, branch: &str) -> Result<(), WorktreeError> {
        let branch_owned = branch.to_string();
        self.with_state(|s| {
            let now = chrono::Utc::now();
            match s.port_leases.get_mut(&branch_owned) {
                Some(lease) if !ports::is_lease_expired(lease, now) => {
                    ports::renew_lease(lease);
                    Ok(())
                }
                Some(_) => Err(WorktreeError::StateCorrupted {
                    reason: format!(
                        "renew_port_lease: lease for '{branch_owned}' is expired — reallocate instead"
                    ),
                }),
                None => Err(WorktreeError::StateCorrupted {
                    reason: format!("renew_port_lease: no active lease for '{branch_owned}'"),
                }),
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Create a temporary git repo for testing.
    fn create_test_repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        run_git(dir.path(), &["init", "-b", "main"]);
        // CI runners typically have no global user.name/user.email; configure
        // locally so `git commit` below succeeds.
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "Test"]);
        run_git(dir.path(), &["commit", "--allow-empty", "-m", "initial"]);
        dir
    }

    /// Run a git command in `dir` and panic with stderr if it fails.
    fn run_git(dir: &std::path::Path, args: &[&str]) {
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

    #[test]
    fn test_manager_new() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default());
        assert!(mgr.is_ok());
    }

    #[test]
    fn test_manager_list() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();
        let list = mgr.list().unwrap();
        assert!(!list.is_empty()); // At least the main worktree
    }

    #[test]
    fn test_create_and_delete_worktree() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("test-wt");
        let (handle, outcome) = mgr
            .create("test-branch", &wt_path, CreateOptions::default())
            .unwrap();

        assert!(wt_path.exists());
        assert_eq!(handle.branch, "test-branch");
        assert_eq!(handle.state, WorktreeState::Active);
        assert!(!handle.base_commit.is_empty());
        assert!(!handle.session_uuid.is_empty());
        assert!(handle.creator_pid > 0);
        assert!(!handle.created_at.is_empty());
        assert_eq!(outcome, CopyOutcome::None);

        // Verify it shows up in git worktree list
        let list = mgr.list().unwrap();
        assert!(list.len() >= 2); // main + test-branch

        // Delete it
        mgr.delete(&handle, DeleteOptions::default()).unwrap();
        assert!(!wt_path.exists());
    }

    #[test]
    fn test_create_worktree_cleanup_on_add_failure() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        // Try to create a worktree for a branch that's already checked out (main)
        // This should fail with BranchAlreadyCheckedOut
        let wt_path = repo.path().join("test-wt-fail");
        let result = mgr.create("main", &wt_path, CreateOptions::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_create_worktree_with_lock() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("locked-wt");
        let opts = CreateOptions {
            lock: true,
            lock_reason: Some("testing".to_string()),
            ..Default::default()
        };
        let (handle, _) = mgr.create("locked-branch", &wt_path, opts).unwrap();

        assert_eq!(handle.state, WorktreeState::Locked);

        // Clean up — need force since it's locked
        let _ = git::worktree_remove_force(&mgr.repo_root, &wt_path);
    }

    #[test]
    fn test_delete_with_unmerged_commits_returns_error() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        // Create a worktree with a new branch
        let wt_path = repo.path().join("unmerged-wt");
        let (handle, _) = mgr
            .create("unmerged-branch", &wt_path, CreateOptions::default())
            .unwrap();

        // Make an unmerged commit on the worktree branch
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "unmerged work"])
            .current_dir(&wt_path)
            .output()
            .unwrap();

        // Attempt delete without force — should fail with UnmergedCommits
        let result = mgr.delete(&handle, DeleteOptions::default());
        assert!(result.is_err());
        match result.unwrap_err() {
            WorktreeError::UnmergedCommits { branch, commit_count } => {
                assert_eq!(branch, "unmerged-branch");
                assert!(commit_count > 0);
            }
            other => panic!("expected UnmergedCommits, got: {other}"),
        }

        // Cleanup with force
        let _ = mgr.delete(
            &handle,
            DeleteOptions { force: true, force_dirty: true, ..Default::default() },
        );
    }

    #[test]
    fn test_delete_with_force_skips_unmerged_check() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("force-wt");
        let (handle, _) = mgr
            .create("force-branch", &wt_path, CreateOptions::default())
            .unwrap();

        // Make an unmerged commit
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "unmerged work"])
            .current_dir(&wt_path)
            .output()
            .unwrap();

        // Delete with force — should succeed despite unmerged commits
        let result = mgr.delete(
            &handle,
            DeleteOptions {
                force: true,
                ..Default::default()
            },
        );
        assert!(result.is_ok());
        assert!(!wt_path.exists());
    }

    #[test]
    fn test_delete_merged_branch_succeeds() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("merged-wt");
        let (handle, _) = mgr
            .create("merged-branch", &wt_path, CreateOptions::default())
            .unwrap();

        // Don't make any new commits — branch is at same point as main
        // So merge-base --is-ancestor should return exit 0 (SAFE)
        let result = mgr.delete(&handle, DeleteOptions::default());
        assert!(result.is_ok());
        assert!(!wt_path.exists());
    }

    #[test]
    fn test_delete_locked_worktree_returns_error() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("locked-del-wt");
        let opts = CreateOptions {
            lock: true,
            lock_reason: Some("important work".to_string()),
            ..Default::default()
        };
        let (handle, _) = mgr.create("locked-del-branch", &wt_path, opts).unwrap();

        // Try to delete locked worktree — should fail
        let result = mgr.delete(&handle, DeleteOptions::default());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorktreeError::WorktreeLocked { .. }
        ));

        // Cleanup
        let _ = git::worktree_remove_force(&mgr.repo_root, &wt_path);
    }

    // ── attach() tests ──────────────────────────────────────────────────

    #[test]
    fn test_attach_manually_created_worktree() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        // Create a worktree manually via git (outside iso-code)
        let wt_path = repo.path().join("manual-wt");
        let output = Command::new("git")
            .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", "manual-branch"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "git worktree add failed: {}", String::from_utf8_lossy(&output.stderr));

        // Attach it via Manager
        let handle = mgr.attach(&wt_path, AttachOptions::default()).unwrap();

        assert_eq!(handle.branch, "manual-branch");
        assert!(!handle.base_commit.is_empty());
        assert!(!handle.session_uuid.is_empty());
        assert!(handle.creator_pid > 0);
        assert_eq!(handle.state, WorktreeState::Active);

        // Verify it appears in list
        let list = mgr.list().unwrap();
        assert!(list.iter().any(|wt| {
            dunce::canonicalize(&wt.path).ok() == dunce::canonicalize(&wt_path).ok()
        }));

        // Clean up
        let _ = git::worktree_remove_force(&mgr.repo_root, &wt_path);
    }

    #[test]
    fn test_attach_nonexistent_path_errors() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let bad_path = repo.path().join("does-not-exist");
        let result = mgr.attach(&bad_path, AttachOptions::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_attach_path_not_in_git_registry_errors() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        // Create a regular directory (not a git worktree)
        let dir_path = repo.path().join("just-a-dir");
        std::fs::create_dir_all(&dir_path).unwrap();

        let result = mgr.attach(&dir_path, AttachOptions::default());
        assert!(result.is_err());
        // Ensure it's specifically the WorktreeNotInGitRegistry error
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not found in git registry"),
            "Expected WorktreeNotInGitRegistry, got: {err}"
        );
    }

    #[test]
    fn test_attach_idempotent() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        // Create a worktree manually
        let wt_path = repo.path().join("idempotent-wt");
        let output = Command::new("git")
            .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", "idem-branch"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success());

        // Attach twice — both should succeed
        let handle1 = mgr.attach(&wt_path, AttachOptions::default()).unwrap();
        let handle2 = mgr.attach(&wt_path, AttachOptions::default()).unwrap();

        assert_eq!(handle1.branch, handle2.branch);
        assert_eq!(handle1.base_commit, handle2.base_commit);
        assert_eq!(
            dunce::canonicalize(&handle1.path).unwrap(),
            dunce::canonicalize(&handle2.path).unwrap()
        );

        // Clean up
        let _ = git::worktree_remove_force(&mgr.repo_root, &wt_path);
    }

    #[test]
    fn test_attach_after_create_and_delete() {
        // Simulate: create via Manager, delete, re-create manually, then attach
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("reattach-wt");

        // Create and delete via Manager
        let (handle, _) = mgr
            .create("reattach-branch", &wt_path, CreateOptions::default())
            .unwrap();
        mgr.delete(&handle, DeleteOptions::default()).unwrap();
        assert!(!wt_path.exists());

        // Re-create manually via git
        let output = Command::new("git")
            .args(["worktree", "add", wt_path.to_str().unwrap(), "reattach-branch"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "git worktree add failed: {}", String::from_utf8_lossy(&output.stderr));

        // Attach — should succeed with fresh session_uuid
        let attached = mgr.attach(&wt_path, AttachOptions::default()).unwrap();
        assert_eq!(attached.branch, "reattach-branch");
        assert!(!attached.session_uuid.is_empty());

        // Clean up
        let _ = git::worktree_remove_force(&mgr.repo_root, &wt_path);
    }

    // ── gc() tests ────────────────────────────────────────────────────────

    #[test]
    fn test_gc_dry_run_returns_report_without_deleting() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("gc-test-wt");
        let (handle, _) = mgr
            .create("gc-branch", &wt_path, CreateOptions::default())
            .unwrap();

        // dry_run = true (default) — should not delete anything
        let report = mgr.gc(GcOptions::default()).unwrap();
        assert!(report.dry_run);
        assert!(report.removed.is_empty());

        // Worktree should still exist
        assert!(wt_path.exists());

        // Cleanup
        mgr.delete(&handle, DeleteOptions { force: true, ..Default::default() }).unwrap();
    }

    #[test]
    fn test_gc_locked_worktree_never_touched() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("gc-locked-wt");
        let opts = CreateOptions {
            lock: true,
            ..Default::default()
        };
        let (_handle, _) = mgr.create("gc-locked-branch", &wt_path, opts).unwrap();

        // GC with force=true — a locked worktree must still survive.
        let report = mgr
            .gc(GcOptions { dry_run: false, force: true, ..Default::default() })
            .unwrap();

        // The locked worktree must NOT appear in removed or evicted
        assert!(!report.removed.iter().any(|p| p == &wt_path));
        assert!(!report.evicted.iter().any(|p| p == &wt_path));

        // Worktree still exists
        assert!(wt_path.exists());

        // Cleanup
        let _ = git::worktree_remove_force(&mgr.repo_root, &wt_path);
    }

    #[test]
    fn test_gc_default_is_dry_run() {
        assert!(GcOptions::default().dry_run);
    }

    // ── Reconciliation and gc regression tests ────────────────────────────

    /// `list()` reconciliation must not evict in-flight `Creating` entries.
    /// Simulates the window during `Manager::create()` between writing the
    /// `Creating` entry to state.json and `git worktree add` completing — a
    /// concurrent `list()` must leave that entry alone.
    #[test]
    fn test_list_preserves_creating_entry_during_reconcile() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        // Inject a Creating entry for a branch not in git's registry yet.
        state::with_state(mgr.repo_root(), None, |s| {
            s.active_worktrees.insert(
                "in-flight-branch".to_string(),
                ActiveWorktreeEntry {
                    path: repo.path().join("in-flight").to_string_lossy().to_string(),
                    branch: "in-flight-branch".to_string(),
                    base_commit: "a".repeat(40),
                    state: WorktreeState::Creating,
                    created_at: chrono::Utc::now(),
                    last_activity: Some(chrono::Utc::now()),
                    creator_pid: std::process::id(),
                    creator_name: "test".to_string(),
                    session_uuid: "uuid-in-flight".to_string(),
                    adapter: None,
                    setup_complete: false,
                    port: None,
                    extra: std::collections::HashMap::new(),
                },
            );
            Ok(())
        })
        .unwrap();

        // list() triggers reconciliation against git output. The Creating
        // entry is absent from git but must survive.
        let _ = mgr.list().unwrap();

        let state_after = state::read_state(mgr.repo_root(), None).unwrap();
        assert!(
            state_after.active_worktrees.contains_key("in-flight-branch"),
            "Creating entry must remain in active_worktrees after list() reconciliation"
        );
        assert!(
            !state_after.stale_worktrees.contains_key("in-flight-branch"),
            "Creating entry must NOT be moved to stale_worktrees: {:?}",
            state_after.stale_worktrees.keys().collect::<Vec<_>>()
        );
    }

    /// `gc()` must evict worktrees older than `gc_max_age_days` once their
    /// `creator_pid` is no longer alive. The age gate relies on `created_at`
    /// being enriched from state.json — without that, git porcelain output
    /// alone lacks the timestamp and no worktree ever ages out.
    #[test]
    fn test_gc_evicts_old_worktree_with_dead_creator_pid() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("aged-wt");
        let (handle, _) = mgr
            .create("aged-branch", &wt_path, CreateOptions::default())
            .unwrap();

        // Backdate the worktree and mark the creator as dead.
        state::with_state(mgr.repo_root(), None, |s| {
            let entry = s.active_worktrees.get_mut(&handle.branch).unwrap();
            entry.created_at = chrono::Utc::now() - chrono::Duration::days(30);
            entry.creator_pid = 99_999_999; // definitely-dead PID
            Ok(())
        })
        .unwrap();

        let report = mgr
            .gc(GcOptions { dry_run: true, force: true, ..Default::default() })
            .unwrap();

        let canon_wt = dunce::canonicalize(&wt_path).unwrap();
        let is_evicted = report.evicted.iter().any(|p| {
            dunce::canonicalize(p).ok().as_deref() == Some(&canon_wt)
        });
        assert!(
            is_evicted,
            "Old worktree with dead creator_pid must be evicted by gc(), got evicted={:?}",
            report.evicted
        );

        // Cleanup
        let _ = mgr.delete(
            &handle,
            DeleteOptions { force: true, force_dirty: true, ..Default::default() },
        );
    }

    #[test]
    fn test_touch_updates_last_activity() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("touch-wt");
        let (handle, _) = mgr
            .create("touch-branch", &wt_path, CreateOptions::default())
            .unwrap();

        // Backdate last_activity so touch() has room to move it forward.
        state::with_state(mgr.repo_root(), None, |s| {
            let e = s.active_worktrees.get_mut(&handle.branch).unwrap();
            e.last_activity = Some(chrono::Utc::now() - chrono::Duration::days(3));
            Ok(())
        })
        .unwrap();

        mgr.touch(&handle.branch).unwrap();
        let after = state::read_state(mgr.repo_root(), None).unwrap();
        let entry = after.active_worktrees.get(&handle.branch).unwrap();
        let la = entry.last_activity.unwrap();
        assert!(chrono::Utc::now() - la < chrono::Duration::seconds(5));

        mgr.delete(&handle, DeleteOptions { force: true, ..Default::default() })
            .unwrap();
    }

    #[test]
    fn test_touch_unknown_branch_errors() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();
        let result = mgr.touch("never-created");
        assert!(matches!(result, Err(WorktreeError::StateCorrupted { .. })));
    }

    /// gc() must sweep abandoned Creating/Pending entries whose creator is
    /// dead and whose created_at is older than the 10-minute grace window.
    /// These are leftovers from a crashed create() whose cleanup path failed
    /// to re-acquire the lock.
    #[test]
    fn test_gc_sweeps_abandoned_creating_entries() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        // Inject an abandoned Creating entry with a dead PID and old timestamp.
        state::with_state(mgr.repo_root(), None, |s| {
            s.active_worktrees.insert(
                "abandoned-branch".to_string(),
                ActiveWorktreeEntry {
                    path: "/tmp/abandoned-wt".to_string(),
                    branch: "abandoned-branch".to_string(),
                    base_commit: "a".repeat(40),
                    state: WorktreeState::Creating,
                    created_at: chrono::Utc::now() - chrono::Duration::hours(1),
                    last_activity: None,
                    creator_pid: 99_999_999, // dead
                    creator_name: "test".to_string(),
                    session_uuid: "uuid-abandoned".to_string(),
                    adapter: None,
                    setup_complete: false,
                    port: None,
                    extra: std::collections::HashMap::new(),
                },
            );
            Ok(())
        })
        .unwrap();

        // Even a dry_run=false gc triggers the sweep. The entry isn't in git's
        // list so it isn't evicted the normal way — only the sweep reaches it.
        let _ = mgr
            .gc(GcOptions { dry_run: false, force: true, ..Default::default() })
            .unwrap();

        let after = state::read_state(mgr.repo_root(), None).unwrap();
        assert!(
            !after.active_worktrees.contains_key("abandoned-branch"),
            "abandoned Creating entry must be removed from active_worktrees"
        );
        assert!(
            after.stale_worktrees.contains_key("abandoned-branch"),
            "abandoned Creating entry must land in stale_worktrees"
        );
        let stale = &after.stale_worktrees["abandoned-branch"];
        assert_eq!(stale.eviction_reason, "gc: abandoned Creating entry");
    }

    /// gc() evicting a worktree must transition its port lease to "stale".
    #[test]
    fn test_gc_transitions_port_lease_to_stale() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("lease-wt");
        let (handle, _) = mgr
            .create(
                "lease-branch",
                &wt_path,
                CreateOptions { allocate_port: true, ..Default::default() },
            )
            .unwrap();
        assert!(handle.port.is_some());

        // Backdate + kill the creator so gc evicts.
        state::with_state(mgr.repo_root(), None, |s| {
            let entry = s.active_worktrees.get_mut(&handle.branch).unwrap();
            entry.created_at = chrono::Utc::now() - chrono::Duration::days(30);
            entry.creator_pid = 99_999_999;
            Ok(())
        })
        .unwrap();

        let _ = mgr
            .gc(GcOptions { dry_run: false, force: true, ..Default::default() })
            .unwrap();

        let after = state::read_state(mgr.repo_root(), None).unwrap();
        let lease = after
            .port_leases
            .get("lease-branch")
            .expect("lease should survive eviction with stale status");
        assert_eq!(lease.status, "stale");
    }

    // ── Port lease renewal ────────────────────────────────────────────────

    #[test]
    fn test_renew_port_lease_extends_expiry() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let port = mgr.allocate_port("renew-branch", "uuid-renew").unwrap();
        let before = mgr.port_lease("renew-branch").unwrap().expires_at;

        // Backdate the lease so renewal produces an observable change.
        state::with_state(mgr.repo_root(), None, |s| {
            let lease = s.port_leases.get_mut("renew-branch").unwrap();
            lease.expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
            Ok(())
        })
        .unwrap();

        mgr.renew_port_lease("renew-branch").unwrap();
        let after = mgr.port_lease("renew-branch").unwrap().expires_at;

        assert!(after > before, "renew should push expires_at forward");
        assert_eq!(mgr.port_lease("renew-branch").unwrap().port, port);
    }

    #[test]
    fn test_renew_port_lease_unknown_branch_errors() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();
        let err = mgr.renew_port_lease("no-such-branch").unwrap_err();
        assert!(matches!(err, WorktreeError::StateCorrupted { .. }));
    }

    #[test]
    fn test_renew_port_lease_expired_errors() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        mgr.allocate_port("exp-branch", "uuid-exp").unwrap();
        // Backdate the lease past its expiry
        state::with_state(mgr.repo_root(), None, |s| {
            let lease = s.port_leases.get_mut("exp-branch").unwrap();
            lease.expires_at = chrono::Utc::now() - chrono::Duration::hours(1);
            Ok(())
        })
        .unwrap();

        let err = mgr.renew_port_lease("exp-branch").unwrap_err();
        assert!(matches!(err, WorktreeError::StateCorrupted { .. }));
    }

    // ── EcosystemAdapter teardown wiring ──────────────────────────────────

    /// A probe adapter that records whether teardown was invoked.
    struct TeardownProbe {
        teardown_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl crate::types::EcosystemAdapter for TeardownProbe {
        fn name(&self) -> &str { "teardown-probe" }
        fn detect(&self, _worktree_path: &Path) -> bool { true }
        fn setup(&self, _worktree_path: &Path, _source: &Path) -> Result<(), WorktreeError> {
            Ok(())
        }
        fn teardown(&self, _worktree_path: &Path) -> Result<(), WorktreeError> {
            self.teardown_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn test_delete_invokes_teardown_when_setup_completed() {
        let repo = create_test_repo();
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let probe = Box::new(TeardownProbe { teardown_called: called.clone() });
        let mgr = Manager::with_adapter(repo.path(), Config::default(), Some(probe)).unwrap();

        let wt_path = repo.path().join("teardown-wt");
        let (handle, _) = mgr
            .create(
                "teardown-branch",
                &wt_path,
                CreateOptions { setup: true, ..Default::default() },
            )
            .unwrap();
        assert!(handle.setup_complete, "setup should have completed");

        mgr.delete(&handle, DeleteOptions::default()).unwrap();
        assert!(
            called.load(std::sync::atomic::Ordering::SeqCst),
            "teardown must be called when setup_complete is true"
        );
    }

    #[test]
    fn test_delete_skips_teardown_when_setup_not_completed() {
        let repo = create_test_repo();
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let probe = Box::new(TeardownProbe { teardown_called: called.clone() });
        let mgr = Manager::with_adapter(repo.path(), Config::default(), Some(probe)).unwrap();

        let wt_path = repo.path().join("no-setup-wt");
        let (handle, _) = mgr
            .create("no-setup-branch", &wt_path, CreateOptions::default())
            .unwrap();
        assert!(!handle.setup_complete);

        mgr.delete(&handle, DeleteOptions::default()).unwrap();
        assert!(
            !called.load(std::sync::atomic::Ordering::SeqCst),
            "teardown must not fire when setup never ran"
        );
    }

    /// `gc()` must preserve worktrees whose `creator_pid` is still alive, even
    /// once the entry is older than `gc_max_age_days`. This guards against
    /// evicting a worktree another process is actively using.
    #[test]
    fn test_gc_preserves_old_worktree_with_live_creator_pid() {
        let repo = create_test_repo();
        let mgr = Manager::new(repo.path(), Config::default()).unwrap();

        let wt_path = repo.path().join("live-wt");
        let (handle, _) = mgr
            .create("live-branch", &wt_path, CreateOptions::default())
            .unwrap();

        // Backdate the worktree but keep creator_pid = this process (alive).
        state::with_state(mgr.repo_root(), None, |s| {
            let entry = s.active_worktrees.get_mut(&handle.branch).unwrap();
            entry.created_at = chrono::Utc::now() - chrono::Duration::days(30);
            assert_eq!(
                entry.creator_pid,
                std::process::id(),
                "fixture: expected creator_pid to be this process"
            );
            Ok(())
        })
        .unwrap();

        let report = mgr
            .gc(GcOptions { dry_run: true, force: true, ..Default::default() })
            .unwrap();

        let canon_wt = dunce::canonicalize(&wt_path).unwrap();
        let is_evicted = report.evicted.iter().any(|p| {
            dunce::canonicalize(p).ok().as_deref() == Some(&canon_wt)
        });
        assert!(
            !is_evicted,
            "Worktree with live creator_pid must NOT be evicted, got evicted={:?}",
            report.evicted
        );

        // Cleanup
        let _ = mgr.delete(
            &handle,
            DeleteOptions { force: true, force_dirty: true, ..Default::default() },
        );
    }
}
