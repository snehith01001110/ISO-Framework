# Story 2.4: wt create --setup CLI Integration

## Status
Draft

## Epic
Epic 2: Environment Lifecycle

## User Story
As a developer using the CLI, I want `wt create --setup` to automatically select and run the appropriate adapter so that I get a fully bootstrapped worktree without manual post-creation steps.

## Description
Wire the `--setup` flag on `wt create` to the adapter system. The CLI reads adapter configuration from the user config file (`config.toml` or project-local `.iso-code.toml`) to determine which adapter to use and how it is configured. When `--setup` is passed, the selected adapter's `setup()` method is invoked after worktree creation. All adapter output (progress, warnings, errors) goes to stderr only -- stdout remains reserved for machine-readable output in hook mode.

## Acceptance Criteria
- [ ] `wt create <branch> --setup` triggers adapter selection and execution
- [ ] Adapter selection reads from config file (project-local `.iso-code.toml` takes precedence over user-level `config.toml`)
- [ ] Config format supports both `DefaultAdapter` and `ShellCommandAdapter` with their respective options
- [ ] If no adapter is configured and `--setup` is passed, log a warning to stderr and proceed without error
- [ ] All adapter output (stdout and stderr from child processes) is forwarded to stderr of the CLI process
- [ ] No adapter output appears on stdout (critical for `wt hook` compatibility per PRD Section 12.2)
- [ ] `wt create` without `--setup` does not invoke any adapter even if one is configured
- [ ] Adapter name is recorded in the `WorktreeHandle.adapter` field in `state.json`
- [ ] `WorktreeHandle.setup_complete` is set to `true` only when `setup()` returns `Ok(())`

## Tasks
- [ ] Add `--setup` flag to `wt create` command in `clap` argument parser
- [ ] Implement adapter config deserialization from `.iso-code.toml` and `config.toml`
- [ ] Implement adapter factory that constructs the right adapter type from config
- [ ] Wire `CreateOptions { setup: true }` when `--setup` flag is present
- [ ] Redirect all subprocess stdout to stderr in CLI context
- [ ] Write test: `--setup` with DefaultAdapter copies `.env`
- [ ] Write test: `--setup` with ShellCommandAdapter runs post_create command
- [ ] Write test: `--setup` without config produces warning, not error
- [ ] Write test: stdout is clean (no adapter output) when `--setup` is used

## Technical Notes
- PRD Section 12.1 defines `wt create` flags including `--setup`.
- PRD Section 12.2 is critical: Claude Code expects only the path on stdout. Any adapter output leaking to stdout breaks the integration.
- Config file format example:
  ```toml
  [adapter]
  type = "shell-command"
  post_create = "npm install"
  ```
  or:
  ```toml
  [adapter]
  type = "default"
  files_to_copy = [".env", ".env.local"]
  ```
- The `directories` crate (PRD Section 14) provides `config_dir()` for user-level config location.

## Test Hints
- Test stdout isolation: capture stdout and stderr separately; assert stdout is empty or contains only the path
- Test config precedence: project-local config overrides user config
- Test missing config: `--setup` without any config file logs a warning to stderr

## Dependencies
- ISO-2.1 (EcosystemAdapter trait)
- ISO-2.2 (DefaultAdapter)
- ISO-2.3 (ShellCommandAdapter)
- ISO-1.12 (wt hook -- stdout/stderr contract)

## Estimated Effort
S

## Priority
P1

## Traceability
- PRD: Section 12.1 (CLI Commands -- wt create)
- PRD: Section 12.2 (wt hook -- stdout contract)
- FR: FR-P1-009
- Appendix A invariant: Rule 11 (branch names never transformed)
- QA ref: QA-2.4-001 through QA-2.4-004
