use std::path::Path;
use std::process::Command;

use crate::error::WorktreeError;
use crate::types::{GitCapabilities, GitVersion, WorktreeHandle, WorktreeState};

/// Parse the output of `git --version` into a `GitVersion`.
///
/// Handles format variations:
/// - "git version 2.43.0"
/// - "git version 2.39.3 (Apple Git-146)"
/// - "git version 2.43.0.windows.1"
pub fn parse_git_version(output: &str) -> Result<GitVersion, WorktreeError> {
    // Extract the version string after "git version "
    let version_str = output
        .trim()
        .strip_prefix("git version ")
        .ok_or_else(|| WorktreeError::GitCommandFailed {
            command: "git --version".to_string(),
            stderr: format!("unexpected output format: {output}"),
            exit_code: 0,
        })?;

    // Take the first space-delimited token (drops "(Apple Git-146)" suffix)
    let version_token = version_str.split_whitespace().next().unwrap_or(version_str);

    // Split by '.' and parse first three components (drops ".windows.1" suffix)
    let parts: Vec<&str> = version_token.split('.').collect();
    if parts.len() < 3 {
        return Err(WorktreeError::GitCommandFailed {
            command: "git --version".to_string(),
            stderr: format!("cannot parse version: {version_token}"),
            exit_code: 0,
        });
    }

    let major = parts[0].parse::<u32>().map_err(|_| WorktreeError::GitCommandFailed {
        command: "git --version".to_string(),
        stderr: format!("cannot parse major version: {}", parts[0]),
        exit_code: 0,
    })?;
    let minor = parts[1].parse::<u32>().map_err(|_| WorktreeError::GitCommandFailed {
        command: "git --version".to_string(),
        stderr: format!("cannot parse minor version: {}", parts[1]),
        exit_code: 0,
    })?;
    let patch = parts[2].parse::<u32>().map_err(|_| WorktreeError::GitCommandFailed {
        command: "git --version".to_string(),
        stderr: format!("cannot parse patch version: {}", parts[2]),
        exit_code: 0,
    })?;

    Ok(GitVersion { major, minor, patch })
}

/// Build a `GitCapabilities` struct from a detected `GitVersion`.
pub fn detect_capabilities(version: &GitVersion) -> GitCapabilities {
    let has_repair = *version >= GitVersion::HAS_REPAIR;               // 2.30+
    let has_list_nul = *version >= GitVersion::HAS_LIST_NUL;           // 2.36+
    let has_merge_tree_write = *version >= GitVersion::HAS_MERGE_TREE_WRITE; // 2.38+
    let has_orphan = *version >= GitVersion { major: 2, minor: 42, patch: 0 }; // 2.42+
    let has_relative_paths = *version >= GitVersion { major: 2, minor: 48, patch: 0 }; // 2.48+

    GitCapabilities::new(
        version.clone(),
        has_list_nul,
        has_repair,
        has_orphan,
        has_relative_paths,
        has_merge_tree_write,
    )
}

/// Run `git --version`, parse the result, and validate against minimum version.
/// Returns `GitCapabilities` on success.
pub fn detect_git_version() -> Result<GitCapabilities, WorktreeError> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Err(WorktreeError::GitNotFound);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_git_version(&stdout)?;

    if version < GitVersion::MINIMUM {
        return Err(WorktreeError::GitVersionTooOld {
            required: format!(
                "{}.{}.{}",
                GitVersion::MINIMUM.major,
                GitVersion::MINIMUM.minor,
                GitVersion::MINIMUM.patch
            ),
            found: format!("{}.{}.{}", version.major, version.minor, version.patch),
        });
    }

    Ok(detect_capabilities(&version))
}

/// Parse the porcelain output of `git worktree list` into [`WorktreeHandle`]s.
///
/// Handles both NUL-delimited (`git -z`, available on Git >= 2.36) and
/// newline-delimited layouts: each worktree is one block of key-value lines,
/// and blocks are separated by double-NUL (or a blank line).
///
/// Path bytes are preserved end-to-end on Unix via `OsStr::from_bytes` so
/// non-UTF8 paths survive through to [`std::path::PathBuf`]. Branch names,
/// HEAD SHAs, and keyword markers are ASCII in practice and are decoded lossily.
pub fn parse_worktree_list_porcelain(
    output: &[u8],
    nul_delimited: bool,
) -> Result<Vec<WorktreeHandle>, WorktreeError> {
    // Block separator: double-NUL in -z mode, blank line (double-LF) otherwise.
    let block_sep: &[u8] = if nul_delimited { b"\0\0" } else { b"\n\n" };
    // Field separator within a block: NUL in -z mode, LF otherwise.
    let field_sep: u8 = if nul_delimited { 0 } else { b'\n' };

    let mut handles = Vec::new();
    for block in split_bytes(output, block_sep) {
        // Trim leading/trailing separator bytes + whitespace from the block.
        let block = trim_block(block);
        if block.is_empty() {
            continue;
        }

        let mut path: Option<std::path::PathBuf> = None;
        let mut head_sha = String::new();
        let mut branch = String::new();
        let mut is_bare = false;
        let mut is_detached = false;
        let mut is_locked = false;
        let mut is_prunable = false;

        for field in block.split(|b| *b == field_sep) {
            let field = trim_field(field);
            if field.is_empty() {
                continue;
            }

            if let Some(p) = strip_prefix_bytes(field, b"worktree ") {
                if !nul_delimited && p.contains(&b'\n') {
                    eprintln!(
                        "WARNING: Worktree path may contain newlines — upgrade to git 2.36 for safe parsing"
                    );
                }
                path = Some(path_from_bytes(p));
            } else if let Some(sha) = strip_prefix_bytes(field, b"HEAD ") {
                head_sha = String::from_utf8_lossy(sha).into_owned();
            } else if let Some(b) = strip_prefix_bytes(field, b"branch ") {
                let s = String::from_utf8_lossy(b);
                branch = s
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&s)
                    .to_string();
            } else if field == b"detached" {
                is_detached = true;
            } else if field == b"bare" {
                is_bare = true;
            } else if field == b"locked" || strip_prefix_bytes(field, b"locked ").is_some() {
                is_locked = true;
            } else if field == b"prunable" || strip_prefix_bytes(field, b"prunable ").is_some() {
                is_prunable = true;
            }
        }

        let Some(wt_path) = path else {
            continue;
        };

        // locked > prunable(orphaned) > active
        let state = if is_locked {
            WorktreeState::Locked
        } else if is_prunable {
            WorktreeState::Orphaned
        } else {
            WorktreeState::Active
        };

        if is_bare || is_detached {
            branch = String::new();
        }

        handles.push(WorktreeHandle::new(
            wt_path,
            branch,
            head_sha,
            state,
            String::new(), // created_at — populated from state.json
            0,             // creator_pid
            String::new(), // creator_name
            None,          // adapter
            false,         // setup_complete
            None,          // port
            String::new(), // session_uuid
        ));
    }

    Ok(handles)
}

/// Build a `PathBuf` from raw bytes. On Unix, preserves non-UTF8 bytes via
/// `OsStr::from_bytes`. On other targets, falls back to lossy UTF-8 decoding
/// (Windows paths are UTF-16 anyway, so non-UTF8 bytes from git there would
/// already be broken).
fn path_from_bytes(b: &[u8]) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(b))
    }
    #[cfg(not(unix))]
    {
        std::path::PathBuf::from(String::from_utf8_lossy(b).into_owned())
    }
}

fn strip_prefix_bytes<'a>(s: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    if s.len() >= prefix.len() && &s[..prefix.len()] == prefix {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

fn trim_field(b: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < b.len() && (b[start] == b' ' || b[start] == b'\t' || b[start] == b'\r') {
        start += 1;
    }
    let mut end = b.len();
    while end > start && (b[end - 1] == b' ' || b[end - 1] == b'\t' || b[end - 1] == b'\r') {
        end -= 1;
    }
    &b[start..end]
}

fn trim_block(b: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < b.len() && matches!(b[start], 0 | b'\n' | b'\r' | b' ' | b'\t') {
        start += 1;
    }
    let mut end = b.len();
    while end > start && matches!(b[end - 1], 0 | b'\n' | b'\r' | b' ' | b'\t') {
        end -= 1;
    }
    &b[start..end]
}

/// Split `haystack` on every occurrence of `needle`, returning the between-slices.
fn split_bytes<'a>(haystack: &'a [u8], needle: &[u8]) -> Vec<&'a [u8]> {
    if needle.is_empty() || haystack.is_empty() {
        return vec![haystack];
    }
    let mut out = Vec::new();
    let mut i = 0;
    let mut start = 0;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            out.push(&haystack[start..i]);
            i += needle.len();
            start = i;
        } else {
            i += 1;
        }
    }
    out.push(&haystack[start..]);
    out
}

/// Run `git worktree list --porcelain [-z]` and parse the output.
pub fn run_worktree_list(
    repo: &Path,
    caps: &GitCapabilities,
) -> Result<Vec<WorktreeHandle>, WorktreeError> {
    let mut cmd = Command::new("git");
    cmd.arg("worktree").arg("list").arg("--porcelain");
    cmd.current_dir(repo);

    if caps.has_list_nul {
        cmd.arg("-z");
    }

    let output = cmd.output().map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Err(WorktreeError::GitCommandFailed {
            command: format!("git worktree list --porcelain{}", if caps.has_list_nul { " -z" } else { "" }),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        });
    }

    parse_worktree_list_porcelain(&output.stdout, caps.has_list_nul)
}

/// Resolve a ref to its 40-char SHA.
pub fn resolve_ref(repo: &Path, refspec: &str) -> Result<String, WorktreeError> {
    let output = Command::new("git")
        .args(["rev-parse", refspec])
        .current_dir(repo)
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Err(WorktreeError::GitCommandFailed {
            command: format!("git rev-parse {refspec}"),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check if a branch already exists.
pub fn branch_exists(repo: &Path, branch: &str) -> Result<bool, WorktreeError> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .current_dir(repo)
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    Ok(output.status.success())
}

/// Run `git worktree add` with the appropriate flags.
/// Returns Ok(()) on success.
pub fn worktree_add(
    repo: &Path,
    path: &Path,
    branch: &str,
    base: Option<&str>,
    new_branch: bool,
    lock: bool,
    lock_reason: Option<&str>,
) -> Result<(), WorktreeError> {
    let mut cmd = Command::new("git");
    cmd.arg("worktree").arg("add");
    cmd.current_dir(repo);

    if lock {
        cmd.arg("--lock");
        if let Some(reason) = lock_reason {
            cmd.arg("--reason").arg(reason);
        }
    }

    cmd.arg(path);

    if new_branch {
        cmd.arg("-b").arg(branch);
        if let Some(base_ref) = base {
            cmd.arg(base_ref);
        }
    } else {
        cmd.arg(branch);
    }

    let output = cmd.output().map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Err(WorktreeError::GitCommandFailed {
            command: format!("git worktree add {}", path.display()),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        });
    }

    Ok(())
}

/// Run `git worktree remove --force <path>`.
pub fn worktree_remove_force(repo: &Path, path: &Path) -> Result<(), WorktreeError> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(path)
        .current_dir(repo)
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Err(WorktreeError::GitCommandFailed {
            command: format!("git worktree remove --force {}", path.display()),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        });
    }

    Ok(())
}

/// Run `git worktree remove <path>` (non-force).
pub fn worktree_remove(repo: &Path, path: &Path) -> Result<(), WorktreeError> {
    let output = Command::new("git")
        .args(["worktree", "remove"])
        .arg(path)
        .current_dir(repo)
        .output()
        .map_err(|_| WorktreeError::GitNotFound)?;

    if !output.status.success() {
        return Err(WorktreeError::GitCommandFailed {
            command: format!("git worktree remove {}", path.display()),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        });
    }

    Ok(())
}

/// Post-create git-crypt check on the new worktree path.
/// Returns Ok(()) if the worktree is safe, Err(GitCryptLocked) if encrypted files detected.
pub fn post_create_git_crypt_check(worktree_path: &Path) -> Result<(), WorktreeError> {
    let gitattributes = worktree_path.join(".gitattributes");
    if !gitattributes.exists() {
        return Ok(());
    }

    let content = match std::fs::read_to_string(&gitattributes) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let has_git_crypt = content.lines().any(|l| l.contains("filter=git-crypt"));
    if !has_git_crypt {
        return Ok(());
    }

    const GIT_CRYPT_MAGIC: &[u8; 10] = b"\x00GITCRYPT\x00";

    for line in content.lines() {
        if !line.contains("filter=git-crypt") {
            continue;
        }
        let pattern = line.split_whitespace().next().unwrap_or("");
        if pattern.is_empty() {
            continue;
        }

        let ls_output = Command::new("git")
            .args(["ls-files", "--", pattern])
            .current_dir(worktree_path)
            .output();

        if let Ok(ls) = ls_output {
            for file_path in String::from_utf8_lossy(&ls.stdout).lines() {
                let full_path = worktree_path.join(file_path);
                if full_path.exists() {
                    if let Ok(true) = is_encrypted(&full_path, GIT_CRYPT_MAGIC) {
                        return Err(WorktreeError::GitCryptLocked);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if a file starts with the git-crypt magic header bytes.
pub(crate) fn is_encrypted(path: &Path, magic: &[u8; 10]) -> std::io::Result<bool> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 10];
    match file.read_exact(&mut header) {
        Ok(_) => Ok(&header == magic),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Version parsing ─────────────────────────────────────────────

    #[test]
    fn parse_standard_version() {
        let v = parse_git_version("git version 2.43.0").unwrap();
        assert_eq!(v, GitVersion { major: 2, minor: 43, patch: 0 });
    }

    #[test]
    fn parse_apple_version() {
        let v = parse_git_version("git version 2.39.3 (Apple Git-146)").unwrap();
        assert_eq!(v, GitVersion { major: 2, minor: 39, patch: 3 });
    }

    #[test]
    fn parse_windows_version() {
        let v = parse_git_version("git version 2.43.0.windows.1").unwrap();
        assert_eq!(v, GitVersion { major: 2, minor: 43, patch: 0 });
    }

    #[test]
    fn parse_with_trailing_newline() {
        let v = parse_git_version("git version 2.20.0\n").unwrap();
        assert_eq!(v, GitVersion { major: 2, minor: 20, patch: 0 });
    }

    #[test]
    fn parse_garbage_input() {
        assert!(parse_git_version("not git output").is_err());
    }

    // ── Minimum version check ───────────────────────────────────────

    #[test]
    fn version_2_19_is_too_old() {
        let v = GitVersion { major: 2, minor: 19, patch: 9 };
        assert!(v < GitVersion::MINIMUM);
    }

    #[test]
    fn version_2_20_is_ok() {
        let v = GitVersion { major: 2, minor: 20, patch: 0 };
        assert!(v >= GitVersion::MINIMUM);
    }

    // ── Capability thresholds ───────────────────────────────────────

    #[test]
    fn capabilities_at_2_20() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 20, patch: 0 });
        assert!(!caps.has_repair);
        assert!(!caps.has_list_nul);
        assert!(!caps.has_merge_tree_write);
        assert!(!caps.has_orphan);
        assert!(!caps.has_relative_paths);
    }

    #[test]
    fn capabilities_at_2_29_no_repair() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 29, patch: 9 });
        assert!(!caps.has_repair);
    }

    #[test]
    fn capabilities_at_2_30_has_repair() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 30, patch: 0 });
        assert!(caps.has_repair);
        assert!(!caps.has_list_nul);
    }

    #[test]
    fn capabilities_at_2_35_no_list_nul() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 35, patch: 9 });
        assert!(caps.has_repair);
        assert!(!caps.has_list_nul);
    }

    #[test]
    fn capabilities_at_2_36_has_list_nul() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 36, patch: 0 });
        assert!(caps.has_repair);
        assert!(caps.has_list_nul);
        assert!(!caps.has_merge_tree_write);
    }

    #[test]
    fn capabilities_at_2_38_has_merge_tree() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 38, patch: 0 });
        assert!(caps.has_merge_tree_write);
        assert!(!caps.has_orphan);
    }

    #[test]
    fn capabilities_at_2_42_has_orphan() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 42, patch: 0 });
        assert!(caps.has_orphan);
        assert!(!caps.has_relative_paths);
    }

    #[test]
    fn capabilities_at_2_48_has_relative_paths() {
        let caps = detect_capabilities(&GitVersion { major: 2, minor: 48, patch: 0 });
        assert!(caps.has_relative_paths);
        // All other caps should be true too
        assert!(caps.has_repair);
        assert!(caps.has_list_nul);
        assert!(caps.has_merge_tree_write);
        assert!(caps.has_orphan);
    }

    // ── Integration: actual git on this machine ─────────────────────

    #[test]
    fn detect_real_git_version() {
        let caps = detect_git_version().expect("git should be installed on CI");
        assert!(caps.version >= GitVersion::MINIMUM);
    }

    // ── Worktree list parser tests ──────────────────────────────────

    #[test]
    fn parse_empty_output() {
        let result = parse_worktree_list_porcelain(b"", false).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_single_worktree_newline_mode() {
        let output = b"worktree /home/user/project\nHEAD abc1234abc1234abc1234abc1234abc1234abc1234\nbranch refs/heads/main\n\n";
        let result = parse_worktree_list_porcelain(output, false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, std::path::PathBuf::from("/home/user/project"));
        assert_eq!(result[0].branch, "main");
        assert_eq!(result[0].base_commit, "abc1234abc1234abc1234abc1234abc1234abc1234");
        assert_eq!(result[0].state, WorktreeState::Active);
    }

    #[test]
    fn parse_multi_block_newline_mode() {
        let output = b"worktree /home/user/project\nHEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nbranch refs/heads/main\n\nworktree /home/user/project-feature\nHEAD bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nbranch refs/heads/feature/test\n\nworktree /home/user/project-detached\nHEAD cccccccccccccccccccccccccccccccccccccccc\ndetached\n\n";
        let result = parse_worktree_list_porcelain(output, false).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].branch, "main");
        assert_eq!(result[1].branch, "feature/test");
        assert_eq!(result[2].branch, ""); // detached
        assert_eq!(result[2].state, WorktreeState::Active);
    }

    #[test]
    fn parse_locked_worktree_no_reason() {
        let output = b"worktree /tmp/wt\nHEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nbranch refs/heads/test\nlocked\n\n";
        let result = parse_worktree_list_porcelain(output, false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, WorktreeState::Locked);
    }

    #[test]
    fn parse_locked_worktree_with_reason() {
        let output = b"worktree /tmp/wt\nHEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nbranch refs/heads/test\nlocked important work in progress\n\n";
        let result = parse_worktree_list_porcelain(output, false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, WorktreeState::Locked);
    }

    #[test]
    fn parse_prunable_worktree() {
        let output = b"worktree /tmp/wt\nHEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nbranch refs/heads/test\nprunable gitdir file points to non-existent location\n\n";
        let result = parse_worktree_list_porcelain(output, false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, WorktreeState::Orphaned);
    }

    #[test]
    fn parse_bare_worktree() {
        let output = b"worktree /tmp/bare.git\nbare\n\n";
        let result = parse_worktree_list_porcelain(output, false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].branch, "");
    }

    #[test]
    fn parse_nul_delimited_mode() {
        // Real git -z format: fields separated by NUL within a block,
        // blocks separated by double NUL.
        let output = b"worktree /home/user/project\0HEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0branch refs/heads/main\0\0worktree /home/user/project-feature\0HEAD bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\0branch refs/heads/feature\0\0";
        let result = parse_worktree_list_porcelain(output, true).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].branch, "main");
        assert_eq!(result[1].branch, "feature");
    }

    #[test]
    fn parse_nul_delimited_path_with_spaces() {
        let output = b"worktree /home/user/my project\0HEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0branch refs/heads/main\0\0";
        let result = parse_worktree_list_porcelain(output, true).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, std::path::PathBuf::from("/home/user/my project"));
    }

    #[cfg(unix)]
    #[test]
    fn parse_nul_delimited_preserves_non_utf8_path_bytes() {
        // A path byte that's valid on Unix but not valid UTF-8 (0xff).
        let mut output: Vec<u8> = Vec::new();
        output.extend_from_slice(b"worktree /tmp/wt-");
        output.push(0xff);
        output.extend_from_slice(b"-end\0HEAD aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0branch refs/heads/x\0\0");

        let result = parse_worktree_list_porcelain(&output, true).unwrap();
        assert_eq!(result.len(), 1);

        use std::os::unix::ffi::OsStrExt;
        let bytes = result[0].path.as_os_str().as_bytes();
        assert!(bytes.contains(&0xff), "non-UTF8 byte should survive");
    }

    #[test]
    fn parse_integration_real_repo() {
        // Run against the actual ISO repo
        let caps = detect_git_version().expect("git should be installed");
        let result = run_worktree_list(std::path::Path::new("."), &caps);
        // Should succeed — we're in a git repo
        assert!(result.is_ok());
        let handles = result.unwrap();
        // At minimum, the main worktree should be present
        assert!(!handles.is_empty());
    }
}
