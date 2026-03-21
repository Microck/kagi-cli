# Release Runbook

Use this when cutting a new `kagi` release.

## Goal

Ship one version across the Rust CLI, GitHub release assets, npm wrapper, Homebrew tap, Scoop bucket, and public docs.

## Preflight

1. Merge the approved work into `main`.
2. Make sure `main` is green before tagging.
3. Pick the release version `X.Y.Z`.
4. Confirm release automation credentials are present:
   - `NPM_TOKEN` GitHub Actions secret
   - `NPM_PUBLISH_ENABLED=true` repository variable
   - `REPO_SYNC_TOKEN` GitHub Actions secret for `Microck/homebrew-kagi` and `Microck/scoop-kagi`
5. Confirm `CHANGELOG.md` has a complete user-facing entry ready to publish. The release workflow extracts notes from the `## [X.Y.Z]` section, so the heading must exist before the tag is pushed.

## Update release metadata

1. Bump the release version in:
   - `Cargo.toml`
   - `Cargo.lock`
   - `npm/package.json`
2. Move the release notes from `## [Unreleased]` into a new `## [X.Y.Z]` section in `CHANGELOG.md`.
3. Update `docs/index.mdx` if the landing-page footer still shows the old version.
4. Check for any other hardcoded version references that still need the new release number.
5. Commit the release metadata update on `main`.

## Local verification before tagging

Run the same checks the release pipeline depends on:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -q --locked
(cd npm && npm pack --dry-run)
```

If any command fails, fix it before tagging.

## Publish the release

1. Push `main`.
2. Create and push the release tag, for example:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

## What the tag triggers

`.github/workflows/release.yml` runs on `v*` tags and:

- verifies formatting, clippy, and tests
- builds release artifacts for:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- uploads archives plus raw binaries
- generates `kagi-vX.Y.Z-checksums.txt`
- extracts release notes from `CHANGELOG.md`
- creates or refreshes the GitHub release
- syncs `Microck/homebrew-kagi` and `Microck/scoop-kagi`

`.github/workflows/npm-publish.yml` runs after a successful `Release` workflow and publishes `npm/package.json` to npm when `NPM_PUBLISH_ENABLED=true`.

## Post-tag checks

Verify all public release surfaces after the workflows finish:

1. GitHub Release
   - `gh release view vX.Y.Z`
   - confirm the release notes match the new `CHANGELOG.md` section
   - confirm the release includes all platform archives, raw binaries, and `kagi-vX.Y.Z-checksums.txt`
2. Release workflow health
   - `gh run list --workflow Release --limit 5`
   - `gh run list --workflow 'npm Publish' --limit 5`
3. npm
   - `npm view kagi-cli version`
   - confirm it matches `X.Y.Z`
4. Homebrew
   - confirm `Microck/homebrew-kagi` was updated to the new version and checksums
   - if the sync step was skipped or failed, update that repo manually and push `Formula/kagi.rb`
5. Scoop
   - confirm `Microck/scoop-kagi` was updated to the new version and hash
   - if the sync step was skipped or failed, update that repo manually and push `bucket/kagi.json`
6. Installers and scripts
   - `scripts/install.sh` and `scripts/install.ps1` resolve the latest GitHub release dynamically, so they need no per-release version bump
   - the npm wrapper downloads assets using `npm/package.json` version, so npm must stay in lockstep with the GitHub tag

## Package channel notes

### GitHub Releases

This is the canonical release channel. Everything else depends on the tagged GitHub assets.

### npm

The npm package is a wrapper around the native release assets. If `npm/package.json` is out of sync with the tag, installs break because the wrapper downloads `v${package.version}` assets.

### Homebrew

The authoritative formula lives in the companion tap repo `Microck/homebrew-kagi`. The checked-in `packaging/homebrew/Formula/kagi.rb` file in this repo is not the release source of truth.

### Scoop

The authoritative manifest lives in the companion bucket repo `Microck/scoop-kagi`. The checked-in `packaging/scoop/kagi.json` file in this repo is not the release source of truth.

### AUR

There is no AUR automation in this repo and no AUR package metadata tracked here. If an external AUR package exists, update it manually after the GitHub release:

1. bump `pkgver`
2. refresh checksums and any source URLs
3. publish the PKGBUILD update
4. verify a fresh `paru` or `yay` install resolves the new version

If no AUR package is maintained, skip this section.

### Cargo

There is no crates.io publish step. `cargo install` currently pulls from GitHub, so no separate registry release is required.

## Recovery paths

### Rebuild an existing tag

If a release needs to be rebuilt for an existing tag:

1. Run the `Release` workflow manually.
2. Pass `release_tag` with the existing tag, for example `v0.3.1`.

This rebuilds artifacts, refreshes the GitHub release, and re-runs Homebrew and Scoop sync without minting a new version.

### Re-run npm publish

If GitHub release assets are correct but npm did not publish:

1. confirm `NPM_TOKEN` and `NPM_PUBLISH_ENABLED`
2. run the `npm Publish` workflow manually
3. verify `npm view kagi-cli version`

### Homebrew or Scoop sync failed

The release workflow treats package index sync as non-blocking and only emits a warning if it fails. If that happens:

1. inspect the `Release` job logs
2. update the affected companion repo manually
3. push the fix
4. verify install and upgrade on the affected package manager

## Quick checks

- `gh release view vX.Y.Z`
- `gh run list --workflow Release --limit 5`
- `gh run list --workflow 'npm Publish' --limit 5`
- `npm view kagi-cli version`
