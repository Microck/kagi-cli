# Release Runbook

Use this when cutting a new `kagi` release.

## Goal

Ship one version across the Rust CLI, GitHub release assets, npm wrapper, Homebrew tap, Scoop bucket, the `kagi-cli` AUR package, and Mintlify public docs.

## Preflight

1. Merge the approved work into `main`.
2. Make sure `main` is green before tagging.
3. Pick the release version `X.Y.Z`.
4. Confirm release automation credentials are present:
   - `NPM_TOKEN` GitHub Actions secret
   - `NPM_PUBLISH_ENABLED=true` repository variable
   - `REPO_SYNC_TOKEN` GitHub Actions secret for `Microck/homebrew-kagi` and `Microck/scoop-kagi`
   - `AUR_SSH_PRIVATE_KEY` GitHub Actions secret for `ssh://aur@aur.archlinux.org/kagi-cli.git`
   - `MINTLIFY_DEPLOY_COOKIE` GitHub Actions secret for the private Mintlify deployment trigger
5. Confirm `CHANGELOG.md` has a complete user-facing entry ready to publish. The release workflow extracts notes from the `## [X.Y.Z]` section, so the heading must exist before the tag is pushed.

## Update release metadata

1. Bump the release version in:
   - `Cargo.toml`
   - `Cargo.lock`
   - `npm/package.json`
2. Move the release notes from `## [Unreleased]` into a new `## [X.Y.Z]` section in `CHANGELOG.md`.
3. Update Mintlify docs under `docs/` for any user-facing CLI changes in the release.
4. Update `docs/index.mdx` if the landing-page footer still shows the old version.
5. Check for any other hardcoded version references that still need the new release number.
6. Commit the release metadata update on `main`.

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
- calls Mintlify's private deployment update endpoint when `MINTLIFY_DEPLOY_COOKIE` is configured
- syncs `Microck/homebrew-kagi` and `Microck/scoop-kagi`
- syncs the `kagi-cli` AUR package when `AUR_SSH_PRIVATE_KEY` is configured

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
6. AUR
   - confirm the release workflow updated `ssh://aur@aur.archlinux.org/kagi-cli.git`
   - confirm `PKGBUILD` and `.SRCINFO` use the release commit as a `git+https` source
   - if the sync step was skipped or failed, update that repo manually and push `PKGBUILD` and `.SRCINFO`
   - verify a fresh `makepkg -Csf --noconfirm` or AUR helper build succeeds on Arch
7. Mintlify docs
   - confirm `https://kagi.micr.dev` reflects the committed docs changes from `docs/`
   - if the site still shows old content, inspect the `Trigger Mintlify docs deployment` release step
   - if Mintlify rejected the private deployment trigger, rotate `MINTLIFY_DEPLOY_COOKIE` or trigger the deployment from the Mintlify dashboard
   - verify the changed command, guide, or reference pages render correctly
8. Installers and scripts
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

The maintained package is `kagi-cli` at `ssh://aur@aur.archlinux.org/kagi-cli.git`. The release workflow updates it automatically when `AUR_SSH_PRIVATE_KEY` is configured.

The AUR package intentionally uses a commit-pinned `git+https` source instead of GitHub's generated tag archives. The generated archives are not reliable as checksum-addressed AUR sources because different clients can receive different archive bytes for the same tag URL.

If the workflow skips or fails the AUR sync, update the AUR repo manually after the GitHub release:

1. clone or update `ssh://aur@aur.archlinux.org/kagi-cli.git`
2. resolve the release commit for `vX.Y.Z`
3. bump `pkgver` in `PKGBUILD`
4. regenerate `.SRCINFO`
5. commit and push the AUR repo
6. verify the package page or a fresh `paru` or `yay` install resolves the new version

Example flow:

```bash
git clone ssh://aur@aur.archlinux.org/kagi-cli.git
cd kagi-cli

SOURCE_COMMIT="$(git ls-remote https://github.com/Microck/kagi-cli.git 'refs/tags/vX.Y.Z^{}' | awk '{print $1}')"
if [ -z "$SOURCE_COMMIT" ]; then
  SOURCE_COMMIT="$(git ls-remote https://github.com/Microck/kagi-cli.git 'refs/tags/vX.Y.Z' | awk '{print $1}')"
fi

# update pkgver and the source commit in PKGBUILD, then regenerate metadata
makepkg --printsrcinfo > .SRCINFO

git status
git diff
git add PKGBUILD .SRCINFO
git commit -m "chore: update kagi-cli to vX.Y.Z"
git push origin master

# verify the published AUR metadata
curl -s 'https://aur.archlinux.org/rpc/?v=5&type=info&arg[]=kagi-cli'
```

If `makepkg` is unavailable on the current machine, do the `.SRCINFO` regeneration from an Arch environment before pushing.

### Mintlify

Mintlify docs are source-controlled under `docs/`. Automatic GitHub App deployments are not reliable for this project, so the release workflow calls Mintlify's private deployment update endpoint after the GitHub release is published.

Configure `MINTLIFY_DEPLOY_COOKIE` as a GitHub Actions secret containing the complete `Cookie` header value from an authenticated Mintlify dashboard session that can deploy `kagi-cli`. Do not commit this value, paste it into logs, or store it in repo files.

This uses a private dashboard endpoint, not a stable public API. If the cookie expires, Mintlify changes the endpoint, or Cloudflare rejects the request, the release workflow emits a warning and continues. Rotate the secret or trigger the deployment from the Mintlify dashboard.

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
- open `https://kagi.micr.dev` and confirm changed docs are live
