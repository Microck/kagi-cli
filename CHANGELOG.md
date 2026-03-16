# Changelog

All notable user-facing changes to this project should be documented in this file.

This project follows Keep a Changelog style and uses semantic versioning after `1.0.0`.
Before `1.0.0`, breaking changes may still ship in minor releases.

## [Unreleased]

### Added

- Ongoing documentation and release process improvements

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
