# Test ID Index

Flat lookup table of every test ID defined in test-strategy.md.

| Test ID | Name | Layer | Story Ref | Milestone Gate |
|---|---|---|---|---|
| QA-G-001 | `check_branch_not_checked_out` -- branch already checked out in another worktree | Unit | ISO-1.5 | M1 |
| QA-G-002 | `check_disk_space` -- below 500 MB threshold | Unit | ISO-1.5 | M1 |
| QA-G-003 | `check_worktree_count` -- exceeds max_worktrees limit | Unit | ISO-1.5 | M1 |
| QA-G-004 | `check_path_not_exists` -- target path already exists | Unit | ISO-1.5 | M1 |
| QA-G-005 | `check_not_nested_worktree` -- candidate nested inside existing worktree (both directions) | Unit | ISO-1.5 | M1 |
| QA-G-006 | `check_not_network_filesystem` -- target on NFS/CIFS/SMB mount | Unit | ISO-1.5 | M1 |
| QA-G-007 | `check_not_wsl_cross_boundary` -- repo on /mnt, worktree on /home | Unit | ISO-1.5 | M1 |
| QA-G-008 | `check_bare_repo` -- bare repository detection and path adjustment | Unit | ISO-1.5 | M1 |
| QA-G-009 | `check_submodule_context` -- CWD inside a git submodule | Unit | ISO-1.5 | M1 |
| QA-G-010 | `check_total_disk_usage` -- aggregate worktree disk exceeds max_total_disk_bytes | Unit | ISO-1.5 | M1 |
| QA-G-011 | `check_not_network_junction_target` -- Windows junction to UNC path | Unit (platform) | ISO-1.5 | M3 |
| QA-G-012 | `check_git_crypt_pre_create` -- git-crypt encrypted files detected post-checkout | Integration | ISO-1.6 | M1 |
| QA-R-001 | `test_regression_claude_code_38287` -- unmerged commits deleted without warning | Regression | ISO-1.7 | M1 |
| QA-R-002 | `test_regression_claude_code_41010` -- sub-agent cleanup deleted parent CWD | Regression | ISO-1.7 | M1 |
| QA-R-003 | `test_regression_claude_code_29110` -- three agents success, all work lost | Regression | ISO-1.8 | M1 |
| QA-R-004 | `test_regression_claude_code_38538` -- git-crypt worktree committed deletions | Regression | ISO-1.6 | M1 |
| QA-R-005 | `test_regression_claude_code_27881` -- nested worktree inside worktree | Regression | ISO-1.5 | M1 |
| QA-R-006 | `test_regression_vscode_289973` -- background worker cleaned uncommitted changes | Regression | ISO-1.7 | M1 |
| QA-R-007 | `test_regression_vscode_296194` -- runaway worktree add loop (1,526 worktrees) | Regression | ISO-1.5 | M1 |
| QA-R-008 | `test_regression_cursor_forum` -- 9.82 GB consumed in 20 minutes on 2 GB repo | Regression | ISO-1.5 | M1 |
| QA-R-009 | `test_regression_claude_squad_260` -- 5 worktrees x 2 GB node_modules wasted | Regression | ISO-1.6 | M1 |
| QA-R-010 | `test_regression_opencode_14648` -- each retry creates orphan, unbounded disk growth | Regression | ISO-1.6 | M1 |
| QA-C-001 | `test_concurrent_create_same_branch` -- 10 threads, same branch, only one succeeds | Concurrency | ISO-1.6 | M1 |
| QA-C-002 | `test_concurrent_remove_racing_gc` -- delete and gc race on same worktree | Concurrency | ISO-1.7 | M1 |
| QA-C-003 | `test_state_json_read_modify_write_contention` -- 20 threads contending on state.json | Concurrency | ISO-1.10 | M1 |
| QA-C-004 | `test_circuit_breaker_trips_after_three_failures` -- exactly 3 failures trip breaker | Concurrency | ISO-1.3 | M1 |
| QA-C-005 | `test_stale_lock_recovery_after_sigkill` -- SIGKILL holder, recovery within 6s | Concurrency | ISO-1.10 | M1 |
| QA-C-006 | `test_lock_flag_race_window_old_git` -- fallback for Git < 2.17 --lock flag | Concurrency | ISO-1.3 | M1 |
| QA-C-007 | `test_concurrent_merge_check_no_index_lock` -- 20 simultaneous merge checks, no stale lock | Concurrency | ISO-1.6 | M1 |
| QA-C-008 | `test_pid_reuse_false_positive` -- start_time mismatch detects PID reuse | Concurrency | ISO-1.10 | M1 |
| QA-V-001 | `test_git_list_nul_fallback` -- porcelain -z unavailable below 2.36 | Mock git | ISO-1.3 | M1 |
| QA-V-002 | `test_git_repair_fallback` -- worktree repair unavailable below 2.30 | Mock git | ISO-1.3 | M1 |
| QA-V-003 | `test_git_orphan_branch_fallback` -- --orphan unavailable below 2.42 | Mock git | ISO-1.3 | M1 |
| QA-V-004 | `test_git_relative_paths_fallback` -- useRelativePaths unavailable below 2.48 | Mock git | ISO-1.3 | M1 |
| QA-V-005 | `test_git_merge_tree_fallback` -- merge-tree --write-tree unavailable below 2.38 | Mock git | ISO-1.3 | M1 |
| QA-V-006 | `test_git_locked_prunable_fields_absent` -- no locked/prunable fields below 2.31 | Mock git | ISO-1.4 | M1 |
| QA-V-007 | `test_git_lock_flag_fallback` -- --lock unavailable below 2.17 | Mock git | ISO-1.3 | M1 |
| QA-V-008 | `test_git_version_too_old` -- git 2.19 rejected at Manager::new() | Mock git | ISO-1.3 | M1 |
| QA-V-009 | `test_git_not_found` -- git binary absent from PATH | Mock git | ISO-1.3 | M1 |
| QA-I-001 | `test_integration_claude_code_hook` -- wt hook stdin/stdout contract | Integration smoke | ISO-1.12 | M2 |
| QA-I-002 | `test_integration_opencode_retry_orphans` -- no orphans after retry failures | Integration smoke | ISO-1.6 | M2 |
| QA-I-003 | `test_integration_gastown_slash_branch` -- slash-prefixed branch name preserved | Integration smoke | ISO-1.6 | M2 |
| QA-I-004 | `test_integration_claude_squad_gc_cleanup` -- gc cleans 5 unlocked worktrees | Integration smoke | ISO-1.8 | M2 |
| QA-I-005 | `test_integration_cursor_locked_survives_gc` -- locked worktree survives gc(force=true) | Integration smoke | ISO-1.8 | M2 |
| QA-I-006 | `test_integration_vscode_copilot_rate_limit` -- 21st worktree blocked, retry after delete | Integration smoke | ISO-1.5 | M2 |
| QA-I-007 | `test_integration_workmux_port_determinism` -- same port assigned after delete/recreate | Integration smoke | ISO-1.11 | M2 |
| QA-I-008 | `test_integration_worktrunk_attach_state_merge` -- attach from second Manager | Integration smoke | ISO-1.9 | M2 |
| QA-M-001 | `test_mcp_worktree_list` -- JSON-RPC response schema and annotations | MCP contract | ISO-1.13 | M1 |
| QA-M-002 | `test_mcp_worktree_status` -- JSON-RPC response schema and annotations | MCP contract | ISO-1.13 | M1 |
| QA-M-003 | `test_mcp_conflict_check` -- returns not_implemented in v1.0 | MCP contract | ISO-1.13 | M1 |
| QA-M-004 | `test_mcp_worktree_create` -- creates worktree via JSON-RPC | MCP contract | ISO-1.13 | M1 |
| QA-M-005 | `test_mcp_worktree_delete` -- deletes worktree via JSON-RPC | MCP contract | ISO-1.13 | M1 |
| QA-M-006 | `test_mcp_worktree_gc` -- runs gc via JSON-RPC | MCP contract | ISO-1.13 | M1 |
| QA-H-001 | `test_wt_hook_claude_code_stdout_contract` -- exactly one line on stdout, regression for claude-code#27467 | Integration | ISO-1.12 | M1 |
| QA-S-001 | `test_stress_100_cycles_sigkill` -- zero orphans/corruption after 100 create/delete with SIGKILL | Stress | ISO-1.14 | M1 |
| QA-P-001 | `bench_manager_new_cold_start` -- < 500 ms | Performance | ISO-1.1 | M4 |
| QA-P-002 | `bench_manager_create_2gb_repo` -- < 10 s | Performance | ISO-1.6 | M4 |
| QA-P-003 | `bench_manager_gc_20_worktrees` -- < 5 s | Performance | ISO-1.8 | M4 |
| QA-P-004 | `bench_disk_usage_walk_50k_files` -- < 200 ms | Performance | ISO-1.5 | M4 |
| QA-P-005 | `bench_conflict_matrix_20_pairs` -- < 10 s | Performance | TODO | M4 |
| QA-P-006 | `bench_port_hash_assignment_1000` -- < 10 ms | Performance | ISO-1.11 | M4 |
| QA-O-001 | `test_oq1_port_lease_renewal_caller_driven` -- renew_port_lease() extends TTL | Integration | ISO-1.11 | M2 |
| QA-O-002 | `test_oq2_deny_network_filesystem_config` -- Config field escalates warning to error | Unit | ISO-1.5 | M2 |
| QA-O-003 | `test_oq3_attach_bare_repo` -- attach() permitted on bare repos with explicit path | Integration | ISO-1.9 | M2 |
| QA-O-004 | `test_oq4_circuit_breaker_auto_reset` -- auto-reset after circuit_breaker_reset_secs | Integration | ISO-1.3 | M2 |
| QA-O-005 | `test_oq5_bare_repo_worktree_add` -- git -C bare worktree add confirmed safe | Integration | ISO-1.6 | M2 |
| QA-O-006 | `test_oq6_in_use_state_gc_skip` -- gc() skips InUse worktrees even with force=true | Integration | ISO-1.8 | M2 |

---

**Total test IDs: 72**

| Layer | Count |
|---|---|
| Unit (guards) | 11 |
| Integration (git-crypt guard) | 1 |
| Regression | 10 |
| Concurrency | 8 |
| Mock git (version compat) | 9 |
| Integration smoke | 8 |
| MCP contract | 6 |
| Hook contract | 1 |
| Stress | 1 |
| Performance | 6 |
| Open Questions | 6 |
| **Total** | **72** |
