# Contributing

## Scope

This repository is a Rust CLI for Kagi workflows. Contributions should stay focused on the current CLI surface, its docs, and its verification tooling.

## Local Setup

```bash
cargo build --release
cargo test -q
make check
```

Optional checks before opening a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Local Tooling

Canonical local commands live in [Makefile](Makefile):

```bash
make check
make coverage
make setup-hooks
```

If you want local pre-commit enforcement, run `make setup-hooks` once to point Git at [`.githooks/pre-commit`](.githooks/pre-commit).

## Pull Requests

- Keep changes scoped to one goal
- Update docs when user-facing behavior changes
- Add or update tests when behavior changes
- Avoid committing secrets, local tokens, or personal config files

## Auth and Test Safety

- Do not commit `.env`, `.kagi.toml`, session tokens, or API tokens
- Prefer unit tests and parser fixtures over live authenticated tests
- If a change requires live verification, document the exact manual steps in the pull request

## Release Notes

- Add a short entry to [CHANGELOG.md](CHANGELOG.md) for notable user-facing changes
- Call out breaking CLI changes explicitly

## CI Permissions

- CI and coverage workflows use read-only `contents` permissions
- The security workflow adds `security-events: write` for audit reporting
- The release workflow uses `contents: write` only to publish tagged GitHub Releases

## Review Policy

This repository is currently solo-maintained.

- Required status checks stay enabled on `main`
- Required approving reviews are intentionally not enforced, because they would block the sole maintainer
- Contributors should still open pull requests with verification details when possible

## Code of Conduct

By participating in this project, you agree to follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
