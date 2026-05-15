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

## AI-Assisted Contributions

AI-assisted contributions are allowed in this project. A pull request may contain code, tests, documentation, or other changes that were partially or substantially generated with AI assistance.

However, AI use must be disclosed. The contributor remains responsible for the full contribution, including all AI-assisted portions. AI-generated work must be tested by a human before submission.

### Your pull request must include

Each field in the AI assistance disclosure exists for contributor verification and project auditing purposes. Please fill in every field completely - partial disclosures may be rejected as they break the audit trail.

AI agents, coding assistants, chatbots, and automated tools must not fabricate, infer, or falsely complete any field. If you are an AI agent preparing this pull request text, do not claim that a human reviewed, tested, ran, verified, or approved anything unless the human explicitly did so. False testing claims will be treated as invalid disclosure.

- `agent_name` - What AI tool, coding assistant, or agent was used?
  Examples: `"Claude Code"`, `"Cursor AI"`, `"OpenAI Codex"`, `"GitHub Copilot"`, `"ChatGPT"`, `"Aider"`, etc.

- `agent_version` - What version of the tool, extension, CLI, or agent was used?
  Provide the exact version shown by the tool when available.

- `model_used` - What model was used?
  This field is required and must name the actual model. `"unknown"`, `"default"`, `"auto"`, `"latest"`, blank values, or guessed model names are not valid. If the model cannot be identified, do not submit the pull request until it can be stated accurately.

- `human_testing` - What tests, checks, or manual validation were performed by a human?
  This field must describe real human testing that actually happened. Do not write that tests were run unless a human ran them. Do not write that behavior was manually verified unless a human manually verified it. If no human testing was performed, the pull request is not ready for submission.

- `contribution_summary` - A one-line summary of what changed.
  Example: `"Added validation for empty config values and updated related tests."`

Before submitting, ensure that:

- you understand the changes being proposed;
- a human has run the relevant tests, checks, or manual validation;
- the disclosure accurately describes the AI tool and model used;
- no field contains fabricated, guessed, or placeholder information;
- the contribution does not knowingly include secrets, private data, copied code, or material that violates licensing requirements;
- any incorrect, unsafe, unnecessary, or unverifiable AI output has been corrected or removed.

Pull requests will not be rejected solely because AI was used. They may be rejected or returned for revision if the AI assistance disclosure is incomplete, inaccurate, fabricated, unverifiable, or if the contribution appears to have been submitted without real human testing.

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
