# Releasing

This project uses [`cargo-release`](https://github.com/crate-ci/cargo-release) to
publish all three workspace crates (`iso-code`, `iso-code-cli`, `iso-code-mcp`)
to crates.io in lockstep.

## One-time setup

```sh
cargo install cargo-release
```

You'll also need:

- A crates.io API token (`cargo login <token>`).
- Push access to `main` on `snehith01001110/ISO-Framework`.
- A clean working tree on the `main` branch.

## Cutting a release

From the workspace root:

```sh
# Dry run (default — prints what would happen, changes nothing).
cargo release minor

# Actually do it.
cargo release minor --execute
```

Level can be `patch`, `minor`, `major`, `rc`, `beta`, `alpha`, or an explicit
version like `1.0.0`.

## What happens in one command

1. **Version bump.** All three crates' `version = "x.y.z"` fields are bumped to
   the same new version. The `iso-code` path-dep pins inside `iso-code-cli` and
   `iso-code-mcp` are bumped to match.
2. **CHANGELOG rewrite.** `## [Unreleased]` becomes `## [x.y.z] - <date>`, a
   fresh empty `## [Unreleased]` is inserted above it, and the link references
   at the bottom of `CHANGELOG.md` are updated.
3. **One commit.** A single `chore: release vX.Y.Z` commit captures the version
   bump and changelog rewrite across all crates.
4. **One tag.** A single `vX.Y.Z` tag is created (only `iso-code` is configured
   to tag — the other two have `tag = false` to avoid duplicates).
5. **Publish in dependency order.** `iso-code` is published first (since the
   other two depend on it), followed by `iso-code-cli` and `iso-code-mcp`.
6. **Push.** The commit and tag are pushed to `origin/main`.

## Day-to-day workflow

As you land changes, add entries under `## [Unreleased]` in `CHANGELOG.md`
following the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format
(`### Added`, `### Changed`, `### Fixed`, `### Removed`, etc.). When you're
ready to ship, run the release command — the `[Unreleased]` block becomes the
new release's notes automatically.

## Releasing a single crate

The default config assumes lockstep releases, because `iso-code-cli` and
`iso-code-mcp` pin `iso-code` by both path and version — releasing them
independently usually causes those pins to drift.

If you genuinely need to release one crate alone:

```sh
cargo release patch -p iso-code-cli --execute
```

Make sure the `iso-code` version pin in that crate's `Cargo.toml` still points
at a published version on crates.io.

## Configuration files

- `release.toml` (workspace root) — shared defaults: `shared-version`,
  `consolidate-commits`, tag name, commit message, branch allowlist, push and
  publish behavior.
- `iso-code/release.toml` — owns the `CHANGELOG.md` rewrite (runs once, from
  the primary crate).
- `iso-code-cli/release.toml`, `iso-code-mcp/release.toml` — set `tag = false`
  so only one workspace-wide tag is created.

## Recovering from a failed release

If `cargo release` fails partway through:

- **Before publish.** The version bump commit may already exist locally. Reset
  with `git reset --hard origin/main` (only if you have not pushed) and retry.
- **After one or two crates published.** crates.io publishes are immutable —
  you cannot republish the same version. Bump to the next patch version and
  re-run, or use `cargo release -p <remaining-crate>` to publish only the
  crates that didn't make it.
- **After tag pushed but publish failed.** Delete the remote tag
  (`git push --delete origin vX.Y.Z`) and the local tag (`git tag -d vX.Y.Z`)
  before retrying with the same version. Otherwise, bump and re-run.
