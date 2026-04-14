# Story 1.12: wt hook --stdin-format claude-code

## Status
Draft

## Epic
Epic 1: Foundation

## User Story
As a Claude Code user, I want `wt hook --stdin-format claude-code` to read JSON from stdin and print only the worktree path to stdout so that Claude Code's WorktreeCreate hook integration works without hanging or misinterpreting output.

## Description
Implement the `wt hook` subcommand with the exact stdin/stdout contract defined in PRD Section 12.2. Claude Code sends JSON on stdin with `session_id`, `cwd`, `hook_event_name`, and `name` fields. The command must print ONLY the absolute worktree path to stdout (one line, one newline). All other output (logs, git progress, adapter messages) must go to stderr. Any extra stdout causes Claude Code to hang silently (confirmed bug claude-code#27467).

## Acceptance Criteria
- [ ] stdin JSON is parsed: `{ "session_id": "...", "cwd": "...", "hook_event_name": "WorktreeCreate", "name": "..." }`
- [ ] `name` field is used as the branch name, passed through as-is (no transformation, per Appendix A rule 11)
- [ ] `cwd` field is used as the repository root for `Manager::new()`
- [ ] `Manager::create()` is called with the extracted branch name
- [ ] stdout contains ONLY the absolute worktree path followed by a single newline
- [ ] No other characters appear on stdout -- no log messages, no progress output, no trailing whitespace
- [ ] All log messages, git progress, and adapter output go to stderr with `[worktree-core]` prefix
- [ ] Exit code is 0 on success
- [ ] Exit code is non-zero on failure, with error details on stderr
- [ ] `--setup` flag triggers `EcosystemAdapter::setup()` after creation
- [ ] Invalid or missing stdin JSON returns non-zero exit with descriptive stderr message

## Tasks
- [ ] Add `hook` subcommand to `worktree-core-cli` with `--stdin-format claude-code` and `--setup` flags
- [ ] Implement stdin JSON deserialization: define struct with `session_id`, `cwd`, `hook_event_name`, `name` fields
- [ ] Redirect all library logging to stderr (ensure no `println!` or `print!` calls leak to stdout)
- [ ] Call `Manager::new(cwd, config)` with the cwd from stdin JSON
- [ ] Call `Manager::create(name, path, options)` with branch from `name` field
- [ ] If `--setup` flag is present, set `CreateOptions.setup = true`
- [ ] On success: write `"{absolute_path}\n"` to stdout using `write!` (not `println!` with any extra formatting)
- [ ] On failure: write error message to stderr, exit with code 1
- [ ] Write integration test: pipe JSON to stdin, capture stdout, assert exactly one line with absolute path
- [ ] Write integration test: assert stdout byte count equals path length + 1 (for newline)

## Technical Notes
- PRD Section 12.2: "Claude Code sends JSON on stdin and expects only the absolute path on stdout. Any extra stdout causes Claude Code to hang silently."
- PRD Section 12.2: confirmed bug `claude-code#27467` -- cannot be worked around; the library must comply
- PRD Section 12.2: stderr format is `[worktree-core] <message>`
- Appendix A rule 11: "Branch names are never transformed by the core library"
- The hook config in Claude Code is: `"WorktreeCreate": "wt hook --stdin-format claude-code --setup"`
- Use `std::io::stdin().read_to_string()` for stdin reading
- Use `eprintln!("[worktree-core] ...")` for stderr logging
- Use `std::io::stdout().write_all(path.as_bytes())` and `stdout().write_all(b"\n")` for exact output control
- Do NOT use `println!("{path}")` -- it may add platform-specific line endings

## Test Hints
- QA-H-001: assert stdout has exactly one newline (at the end)
- Integration test: `echo '{"session_id":"test","cwd":"/tmp/repo","hook_event_name":"WorktreeCreate","name":"feature-x"}' | wt hook --stdin-format claude-code`
- Integration test: capture stdout bytes, assert length == path.len() + 1
- Integration test: capture stderr, assert it contains `[worktree-core]` prefix
- Unit test: invalid JSON returns exit code 1 with stderr message
- Unit test: missing `name` field returns exit code 1
- Unit test: `name` field with special characters is passed through unmodified

## Dependencies
ISO-1.5, ISO-1.6

## Estimated Effort
M

## Priority
P0

## Traceability
- PRD: Section 12.2 (wt hook -- Claude Code Integration Contract)
- FR: FR-P0-001 (hook integration)
- Appendix A invariant: Rule 11 (branch names never transformed)
- Bug regression: claude-code#27467 (extra stdout causes hang)
- QA ref: QA-H-001
