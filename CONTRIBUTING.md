# Contributing

## Scope

This repository is a Rust CLI for Kagi workflows. Contributions should stay focused on the current CLI surface, its docs, and its verification tooling.

## Local Setup

```bash
cargo build --release
cargo test -q
```

Optional checks before opening a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

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

## Code of Conduct

By participating in this project, you agree to follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
