# Changelog

All notable user-facing changes to this project should be documented in this file.

This project follows Keep a Changelog style and uses semantic versioning after `1.0.0`.
Before `1.0.0`, breaking changes may still ship in minor releases.

## [Unreleased]

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
