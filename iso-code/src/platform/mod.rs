use std::path::Path;

use crate::error::WorktreeError;
use crate::types::{CopyOutcome, ReflinkMode};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

/// Copy a single file from `src` to `dst` respecting the given `ReflinkMode`.
///
/// - `Required`: use CoW only; return `ReflinkNotSupported` if the FS doesn't support it.
/// - `Preferred` (default): try CoW, fall back to standard copy.
/// - `Disabled`: always use standard copy.
///
/// Returns the `CopyOutcome` describing what actually happened.
pub fn copy_file(
    src: &Path,
    dst: &Path,
    mode: ReflinkMode,
) -> Result<CopyOutcome, WorktreeError> {
    match mode {
        ReflinkMode::Required => {
            reflink_copy::reflink(src, dst).map_err(|_| WorktreeError::ReflinkNotSupported)?;
            Ok(CopyOutcome::Reflinked)
        }
        ReflinkMode::Preferred => {
            match reflink_copy::reflink_or_copy(src, dst).map_err(WorktreeError::Io)? {
                None => Ok(CopyOutcome::Reflinked),
                Some(bytes) => Ok(CopyOutcome::StandardCopy {
                    bytes_written: bytes,
                }),
            }
        }
        ReflinkMode::Disabled => {
            let bytes = std::fs::copy(src, dst).map_err(WorktreeError::Io)?;
            Ok(CopyOutcome::StandardCopy {
                bytes_written: bytes,
            })
        }
    }
}

/// Copy all files from `source_worktree` into `target_worktree`, preserving
/// directory structure and respecting `ReflinkMode`. Skips `.git/` directories.
///
/// This is used after `git worktree add` to CoW-copy large build artifacts,
/// node_modules, or other non-tracked files that an EcosystemAdapter might need.
pub fn copy_worktree_files(
    source_worktree: &Path,
    target_worktree: &Path,
    paths: &[&Path],
    mode: ReflinkMode,
) -> Result<CopyOutcome, WorktreeError> {
    if paths.is_empty() {
        return Ok(CopyOutcome::None);
    }

    let mut total_bytes: u64 = 0;
    let mut any_reflinked = false;

    for rel_path in paths {
        let src = source_worktree.join(rel_path);
        let dst = target_worktree.join(rel_path);

        if !src.exists() {
            continue;
        }

        if src.is_dir() {
            copy_dir_recursive(&src, &dst, mode, &mut total_bytes, &mut any_reflinked)?;
        } else {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(WorktreeError::Io)?;
            }
            match copy_file(&src, &dst, mode)? {
                CopyOutcome::Reflinked => any_reflinked = true,
                CopyOutcome::StandardCopy { bytes_written } => total_bytes += bytes_written,
                CopyOutcome::None => {}
            }
        }
    }

    if total_bytes == 0 && !any_reflinked {
        Ok(CopyOutcome::None)
    } else if any_reflinked && total_bytes == 0 {
        Ok(CopyOutcome::Reflinked)
    } else {
        Ok(CopyOutcome::StandardCopy {
            bytes_written: total_bytes,
        })
    }
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    mode: ReflinkMode,
    total_bytes: &mut u64,
    any_reflinked: &mut bool,
) -> Result<(), WorktreeError> {
    std::fs::create_dir_all(dst).map_err(WorktreeError::Io)?;

    for entry in jwalk::WalkDir::new(src)
        .process_read_dir(|_, _, _, children| {
            children.retain(|child| {
                child
                    .as_ref()
                    .map(|e| e.file_name().to_string_lossy() != ".git")
                    .unwrap_or(true)
            });
        })
        .into_iter()
        .flatten()
    {
        let entry_path = entry.path();
        let rel = entry_path.strip_prefix(src).unwrap_or(&entry_path);
        let target = dst.join(rel);

        if entry_path.is_dir() {
            std::fs::create_dir_all(&target).map_err(WorktreeError::Io)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(WorktreeError::Io)?;
            }
            match copy_file(&entry_path, &target, mode)? {
                CopyOutcome::Reflinked => *any_reflinked = true,
                CopyOutcome::StandardCopy { bytes_written } => *total_bytes += bytes_written,
                CopyOutcome::None => {}
            }
        }
    }
    Ok(())
}
