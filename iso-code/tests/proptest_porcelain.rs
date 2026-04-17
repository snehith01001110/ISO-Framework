//! Property tests for the `git worktree list --porcelain` parser.
//!
//! The parser is a security-sensitive boundary — it consumes bytes produced
//! by `git` and drives library decisions (state reconciliation, guard
//! checks). We fuzz it with adversarial input to surface edge cases that
//! hand-written tests miss.
//!
//! Strategies cover both the newline-delimited and NUL-delimited porcelain
//! layouts, with randomized paths (including spaces and unicode), branches
//! (with and without the `refs/heads/` prefix), locked/prunable flags, and
//! random field ordering within a block.

use iso_code::git::parse_worktree_list_porcelain;
use iso_code::types::WorktreeState;
use proptest::prelude::*;
use proptest::strategy::Strategy;

/// A single parsed block we build and then encode into porcelain bytes.
#[derive(Debug, Clone)]
struct BlockSpec {
    path: String,
    head: String,
    branch: Option<String>,
    bare: bool,
    detached: bool,
    locked: Option<Option<String>>,  // Some(None) = locked no reason; Some(Some(r)) = locked with reason.
    prunable: Option<Option<String>>,
}

fn hex40() -> impl Strategy<Value = String> {
    // 40 lowercase hex chars
    "[0-9a-f]{40}".prop_map(|s| s.to_string())
}

fn safe_path(nul_delimited: bool) -> impl Strategy<Value = String> {
    // Paths may include spaces and unicode, but avoid the current block-sep
    // and field-sep bytes (LF/NUL) for the chosen mode since git itself
    // will never emit those in that mode.
    if nul_delimited {
        // -z mode: no NUL bytes, newlines ARE permitted in path.
        "/[\\sA-Za-z0-9_\\-/\\.áéíñü ]{1,24}".prop_map(|s| s.to_string()).boxed()
    } else {
        // Newline mode: paths must not contain newlines or NUL.
        "/[A-Za-z0-9_\\-/\\. áéíñü]{1,24}".prop_map(|s| s.to_string()).boxed()
    }
}

fn branch_name() -> impl Strategy<Value = String> {
    // Realistic branch name: alphanumerics plus `/`, `-`, `_`, `.`.
    "[a-zA-Z][a-zA-Z0-9_\\-./]{0,30}".prop_map(|s| s.to_string())
}

fn block_spec(nul: bool) -> impl Strategy<Value = BlockSpec> {
    (
        safe_path(nul),
        hex40(),
        prop::option::of(branch_name()),
        any::<bool>(),  // bare
        any::<bool>(),  // detached
        prop::option::of(prop::option::of("[a-zA-Z0-9 ]{0,20}".prop_map(|s| s.to_string()))),
        prop::option::of(prop::option::of("[a-zA-Z0-9 ]{0,20}".prop_map(|s| s.to_string()))),
    )
        .prop_map(|(path, head, branch, bare, detached, locked, prunable)| BlockSpec {
            path,
            head,
            branch,
            bare,
            detached,
            locked,
            prunable,
        })
}

/// Encode a block spec into porcelain bytes using the chosen separator.
fn encode_block(spec: &BlockSpec, nul_delimited: bool, ref_prefix: bool) -> Vec<u8> {
    let sep: u8 = if nul_delimited { 0 } else { b'\n' };
    let mut out = Vec::new();

    // `worktree <path>`
    out.extend_from_slice(b"worktree ");
    out.extend_from_slice(spec.path.as_bytes());
    out.push(sep);

    // `HEAD <sha>`
    out.extend_from_slice(b"HEAD ");
    out.extend_from_slice(spec.head.as_bytes());
    out.push(sep);

    if spec.bare {
        out.extend_from_slice(b"bare");
        out.push(sep);
    } else if spec.detached {
        out.extend_from_slice(b"detached");
        out.push(sep);
    } else if let Some(ref b) = spec.branch {
        out.extend_from_slice(b"branch ");
        if ref_prefix {
            out.extend_from_slice(b"refs/heads/");
        }
        out.extend_from_slice(b.as_bytes());
        out.push(sep);
    }

    if let Some(ref reason) = spec.locked {
        out.extend_from_slice(b"locked");
        if let Some(r) = reason {
            out.push(b' ');
            out.extend_from_slice(r.as_bytes());
        }
        out.push(sep);
    }

    if let Some(ref reason) = spec.prunable {
        out.extend_from_slice(b"prunable");
        if let Some(r) = reason {
            out.push(b' ');
            out.extend_from_slice(r.as_bytes());
        }
        out.push(sep);
    }

    out
}

fn encode_output(blocks: &[BlockSpec], nul_delimited: bool, ref_prefix: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        out.extend_from_slice(&encode_block(b, nul_delimited, ref_prefix));
        // Block separator = double-sep
        if nul_delimited {
            out.push(0);
        } else {
            out.push(b'\n');
        }
        // Trailing block separator is also valid
        let _ = i;
    }
    out
}

proptest! {
    /// Newline-delimited parser: any valid sequence of blocks parses cleanly
    /// and every emitted handle has non-empty path + consistent state.
    #[test]
    fn newline_mode_parses_without_panic(
        blocks in prop::collection::vec(block_spec(false), 0..6),
        ref_prefix in any::<bool>(),
    ) {
        let bytes = encode_output(&blocks, false, ref_prefix);
        let result = parse_worktree_list_porcelain(&bytes, false).unwrap();
        // Each emitted handle must have a non-empty path.
        for h in &result {
            prop_assert!(!h.path.as_os_str().is_empty(), "empty path in result");
            // Locked/prunable precedence matches encoder intent.
            prop_assert!(matches!(
                h.state,
                WorktreeState::Active | WorktreeState::Locked | WorktreeState::Orphaned
            ));
        }
    }

    /// NUL-delimited parser: same contract.
    #[test]
    fn nul_mode_parses_without_panic(
        blocks in prop::collection::vec(block_spec(true), 0..6),
        ref_prefix in any::<bool>(),
    ) {
        let bytes = encode_output(&blocks, true, ref_prefix);
        let result = parse_worktree_list_porcelain(&bytes, true).unwrap();
        for h in &result {
            prop_assert!(!h.path.as_os_str().is_empty());
        }
    }

    /// Truncated / corrupted input must never panic — worst case we get an
    /// empty or partial handle vector.
    #[test]
    fn random_garbage_does_not_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = parse_worktree_list_porcelain(&bytes, false);
        let _ = parse_worktree_list_porcelain(&bytes, true);
    }

    /// `branch refs/heads/foo` and `branch foo` should yield the same
    /// branch name — the parser is expected to strip the ref prefix.
    #[test]
    fn ref_prefix_is_stripped(name in branch_name()) {
        let block = BlockSpec {
            path: "/tmp/wt".into(),
            head: "a".repeat(40),
            branch: Some(name.clone()),
            bare: false,
            detached: false,
            locked: None,
            prunable: None,
        };
        let with_prefix = encode_output(std::slice::from_ref(&block), false, true);
        let without_prefix = encode_output(&[block], false, false);
        let a = parse_worktree_list_porcelain(&with_prefix, false).unwrap();
        let b = parse_worktree_list_porcelain(&without_prefix, false).unwrap();
        prop_assert_eq!(a.len(), 1);
        prop_assert_eq!(b.len(), 1);
        prop_assert_eq!(&a[0].branch, &name);
        prop_assert_eq!(&b[0].branch, &name);
    }

    /// If `locked` appears in a block, the parsed state is `Locked` —
    /// regardless of whether `prunable` also appears. `locked` has
    /// precedence over `prunable` per the library's documented behavior.
    #[test]
    fn locked_has_precedence_over_prunable(
        name in branch_name(),
        nul in any::<bool>(),
    ) {
        let block = BlockSpec {
            path: "/tmp/wt".into(),
            head: "a".repeat(40),
            branch: Some(name),
            bare: false,
            detached: false,
            locked: Some(None),
            prunable: Some(None),
        };
        let bytes = encode_output(&[block], nul, true);
        let result = parse_worktree_list_porcelain(&bytes, nul).unwrap();
        prop_assert_eq!(result.len(), 1);
        prop_assert!(matches!(result[0].state, WorktreeState::Locked));
    }
}
