# Changelog

All notable user-facing changes to this project should be documented in this file.

This project follows Keep a Changelog style and uses semantic versioning after `1.0.0`.
Before `1.0.0`, breaking changes may still ship in minor releases.

## [Unreleased]

## [0.4.4]

### Added

- Rust doc comments on all previously undocumented public functions across the crate

### Fixed

- Patched `rustls-webpki` to `0.103.12` to pick up the current TLS validation fixes
- `kagi summarize` now fails fast when neither `--url` nor `--text` is provided
- Parse-failure debug logging now emits bounded body previews and body lengths instead of full raw response bodies
- Batch worker task failures now log at error level with query context
- Auth/config tests now isolate environment mutation safely and use tempfile-backed cleanup
- Rate limiter tests now use less timing-sensitive assertions during release verification
- Replaced `map_or` with `is_none_or` to resolve `clippy::unnecessary_map_or` lint
- Corrected stale README badges, broken links, and missing documentation sections
- Applied Clippy pedantic and nursery lint auto-fixes across the codebase
- `timeout-minutes` guards on CI, release, coverage, and security workflows to prevent hung runs
- `persist-credentials: false` on all checkout steps to avoid stale token leakage
- Dependabot configuration for the npm wrapper package

## [0.4.3]

### Fixed

- Restored Assistant references in `--format pretty` and `--format markdown` output so footnotes and source links match the JSON response

## [0.4.2]

### Fixed

- Made Assistant thread parsing tolerate missing `expires_at` values from `thread.json` stream frames so thread commands stop failing when Kagi omits that field

## [0.4.1]

### Added

- Demo coverage for lens management, custom bangs, redirects, and saved-assistant selection with new recorded GIF assets

### Changed

- Synced the README, docs site, and bundled skill docs with the current CLI surface for settings management and Assistant/search flows
- Improved transport and batch error visibility with lightweight tracing hooks and clearer parse diagnostics for debugging session-backed commands

### Fixed

- Redacted credential values from debug output so tokens do not leak through `Debug` formatting

## [0.4.0]

### Added

- Account-level settings commands for custom assistants, lenses, custom bangs, and redirect rules
- `kagi search --snap` for snap-prefixed search flows
- `kagi assistant --assistant` for selecting a saved assistant by name, id, or invoke-profile slug
- Assistant prompt output formats for `json`, `pretty`, `compact`, and `markdown`

### Changed

- Expanded the docs, auth matrix, output contract, and command reference set to cover the new settings and assistant/search parity features
- Added live CRUD and round-trip coverage for custom assistants, lenses, custom bangs, redirects, and Assistant thread flows

## [0.3.3]

### Added

- Local `kagi news` content filters with built-in presets, custom keywords, hide mode, blur-mode tagging, and preset listing

### Changed

- Moved `kagi news` filtering examples out of the top-level README and kept them in the command docs instead
- Updated cargo dependencies in line with the current Dependabot PR set: `cliclack 0.5.2`, `scraper 0.26.0`, `toml 1.0.7+spec-1.1.0`, and `rustls-webpki 0.103.10`

## [0.3.2]

### Added

- Shared cached HTTP clients for the search, quick-answer, and API-backed command paths

### Changed

- Reduced CLI startup overhead by switching the runtime entrypoint to Tokio `current_thread`
- Removed extra batch JSON serialization churn by keeping batch search responses structured until final output rendering

## [0.3.1]

### Added

- Interactive `kagi auth` wizard for TTY setup with guided Session Link and API Token flows
- Recorded auth demo assets and auth-wizard onboarding coverage across the docs

### Changed

- Made `kagi auth` the primary local setup path while keeping `auth status`, `auth check`, and `auth set` for explicit non-interactive use
- Tightened auth copy, terminal presentation, and config-save flow with overwrite prompts, preferred-auth prompts, and environment override notices

## [0.3.0]

### Added

- `kagi quick` with JSON, compact, pretty, and markdown output plus structured references and follow-up questions
- `kagi translate` text-mode support with detection, alternatives, alignments, suggestions, and word insights

### Changed

- Expanded docs, demos, and output contracts to cover Quick Answer and Translate alongside the existing search and Assistant flows
- Optimized bundled demo and tutorial image assets across the repo

### Fixed

- Made translate bootstrap retry the flaky missing-cookie path instead of failing on the first transient response
- Fixed the release workflow package-index sync step to export the GitHub token for the Homebrew tap and Scoop bucket push path

## [0.2.0]

### Added

- Search V2 session-backed filters for runtime search refinement and batch parity
- Assistant thread management with list, get, export, and delete flows
- `ask-page` for page-focused Assistant questions with structured JSON output

### Changed

- Updated auth handling to accept full Session Link URLs consistently for session-backed commands
- Expanded docs, contracts, and demo coverage for filtered search, Assistant threads, and ask-page

## [0.1.7]

### Added

- Multiple output formats: JSON, Pretty, Compact, Markdown, and CSV
- Batch search capability with parallel execution and rate limiting
- Shell completion generation for Bash, Zsh, Fish, and PowerShell
- Colorized terminal output with `--no-color` option
- Comprehensive lens support for scoped searches

### Changed

- Improved help text and documentation
- Restructured CLI argument parsing
- Enhanced error handling and user feedback

## [0.1.6]

### Added

- Automated release sync for the Homebrew tap and Scoop bucket companion repositories

### Changed

- Switched npm publishing automation to use an explicit registry token path for release publishes

## [0.1.5]

### Fixed

- Added ARM64 Linux release artifacts so install flows work on `aarch64-unknown-linux-gnu`
- Made unsupported Windows ARM64 installs fail fast with a clear error instead of a 404
- Switched npm publishing automation to run after the `Release` workflow completes

## [0.1.4]

### Fixed

- Tagged the release from the corrected commit so GitHub Releases and npm publication use the synchronized package metadata

## [0.1.3]

### Fixed

- Synchronized the Rust package version in `Cargo.lock` with `Cargo.toml` so locked release builds succeed

## [0.1.2]

### Fixed

- Corrected cross-platform release workflow runner selection for macOS Intel builds
- Aligned the npm wrapper version with the release tag used for native binary downloads

## [0.1.1]

### Added

- Cross-platform GitHub Release packaging and install scripts for the native `kagi` binary
- npm wrapper package so global installs still expose the `kagi` command

### Changed

- Added publish-ready package metadata and Rust package exclusions for cleaner release artifacts

## [0.1.0]

### Added

- Initial public CLI release with GitHub repository setup, docs, policies, and CI automation
