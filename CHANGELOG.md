# Changelog

All notable user-facing changes to this project should be documented in this file.

This project follows Keep a Changelog style and uses semantic versioning after `1.0.0`.
Before `1.0.0`, breaking changes may still ship in minor releases.

## [Unreleased]

## [0.17.0]

### Changed

- Updated `kagi mcp` to MCP `2026-07-28`: requests use per-request metadata, `server/discover` replaces the legacy initialization handshake, list results expose cache hints, tool metadata includes behavior annotations, and JSON tool output is also available as `structuredContent`.
- MCP tool execution failures now use `isError: true` tool results so agents can inspect and correct failed calls. Unknown tools and malformed protocol requests remain JSON-RPC errors.

### Breaking

- `kagi mcp` no longer accepts the legacy initialization handshake. MCP clients must send `params._meta.io.modelcontextprotocol/protocolVersion` and `params._meta.io.modelcontextprotocol/clientCapabilities` on every request.

## [0.16.1]

### Added

- Install and run `kagi` through the repository's Nix flake, including NixOS and Home Manager integration.
- `kagi assistant models` now lists every base model available to the account without requiring a saved custom assistant.

### Fixed

- Assistant final stream events and thread responses now include prompt, completion, and total token counts plus the upstream USD cost when Kagi supplies usage data.

## [0.16.0]

### Added

- Install the repository as a Claude Code plugin through its bundled marketplace manifest.
- Use `kagi-ai` to select cost-aware Kagi Assistant, Research, Summarizer, Translate, and file-analysis workflows.

### Changed

- Consolidate the former `kagi`, `kagi-research`, and `kagi-content` guides into the unified `kagi-usage` skill.

## [0.15.0]

### Added

- `kagi skills` now includes focused guides for research, page content, Assistant conversations, monitoring and automation, and account configuration.

### Fixed

- `kagi assistant` and `kagi ask-page` now use Kagi's current Assistant conversation API instead of the removed legacy prompt endpoint.

## [0.14.3]

### Fixed

- `llms.txt` now covers all sitemap pages (was 76%, now 100%). Added 9 missing command pages (batch, extract, history, mcp, notify, site-pref, skills, translate, watch) and reorganized command section into navigation-matching subgroups.

## [0.14.2]

### Fixed

- Assistant `thread list`, `thread get`, `thread export`, and `thread delete` now use Kagi's current Assistant conversation API, so new `assistant.kagi.com/chat/<uuid>` chats work instead of failing on missing legacy stream frames.

## [0.14.1]

### Fixed

- Assistant `thread_open` no longer fails on new threads after Kagi's backend URL change. When the `thread.json` stream frame is absent, the parser now falls back to the `thread.html` frame to extract thread metadata. The error message for missing thread frames now lists which stream tags were received to aid future debugging.

## [0.14.0]

### Added

- `kagi auth` now offers to install the Kagi MCP server after credentials are saved.
- Added `kagi mcp install`, `kagi mcp setup`, and `kagi mcp auth` to configure Kagi MCP for supported AI agents without starting the stdio MCP server.
- Added guided MCP setup for Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, Gemini CLI, OpenCode, Cline, Roo Code, Droid, and Antigravity CLI.

### Changed

- MCP setup merges Kagi into existing agent configuration files and preserves unrelated MCP servers instead of replacing the full config.

## [0.13.0]

### Added

- `kagi mcp` now exposes CLI-parity read/query tools for Search, batch search, Summarize, Extract, Quick Answer, News, Assistant, Translate, FastGPT, enrichment, Small Web, lenses, custom bangs, redirects, auth status, history, and local site preferences.
- Added `kagi mcp --default-output <FORMAT>` so MCP clients can set a server-wide default such as `toon` while individual tool calls can still override output where supported.
- Added `kagi mcp --enable-mutating-tools` to opt into account or local-state mutation tools, including lens, custom bang, redirect, site preference, Assistant settings, and `kagi_cli` passthrough workflows.

### Changed

- MCP tool calls now return JSON-RPC errors for unsupported tools and recover cleanly from malformed JSON input without stopping the server.
- MCP Extract, Assistant thread export, and other explicit JSON output modes now honor the caller's requested JSON format even when the server default output is TOON or compact.

### Fixed

- `kagi_cli` MCP passthrough no longer inherits the open MCP stdin connection when no explicit stdin payload is supplied, preventing stdin-reading commands from hanging the server.
- `kagi_batch_search` now rejects `concurrency: 0` before spawning workers instead of creating a zero-permit semaphore that never completes.

## [0.12.0]

### Added

- `kagi assistant` now supports built-in and file-backed JSON contracts for prompt responses, validates the final Assistant output before printing it, and makes one repair attempt when the model returns invalid JSON.
- CLI failures can opt into structured JSON error envelopes with `--error-format json` or `KAGI_ERROR_FORMAT=json`.
- `kagi search --lens` now accepts exact enabled lens names as well as numeric lens positions.
- `kagi extract --filter` now reads HTTPS URLs from stdin and emits ordered JSONL records for pipeline-friendly batch extraction.

### Changed

- Release automation now emits explicit warnings when optional Homebrew, Scoop, AUR, or Mintlify sync credentials are missing or stale.

### Fixed

- Updated `quinn-proto` to clear the current RustSec `cargo audit` gate.

## [0.11.0]

### Changed

- The configuration file now lives at a fixed XDG location instead of a working-directory-relative `./.kagi.toml`. It is resolved in this order: `$KAGI_CONFIG` (an explicit full file path), then `$XDG_CONFIG_HOME/kagi-cli/config.toml`, then `~/.config/kagi-cli/config.toml`. The file is still created with `0600` permissions, and the `KAGI_API_KEY`/`KAGI_API_TOKEN`/`KAGI_SESSION_TOKEN` environment variables continue to take precedence over it.

  **Breaking:** credentials previously saved in a working-directory-relative `./.kagi.toml` are no longer read. Run `kagi auth set` (or `kagi auth`) once to re-save them at the new location, or point `KAGI_CONFIG` at your existing file.

## [0.10.0]

### Added

- `kagi search` and `kagi batch` now preserve related search metadata from the Search API in structured output.
- `kagi extract` now supports `--format json` and `--format compact` to expose the full Extract API envelope, including retained link metadata when returned by Kagi.

### Changed

- Assistant thread output now uses folders instead of tags, matching Kagi's current thread organization model.

### Fixed

- `kagi translate` now reports inactive subscription failures with a clearer message when Kagi Translate rejects access.

## [0.9.6]

### Fixed

- MCP tool discovery now declares each tool's input schema, including required parameters, so clients can populate `kagi_search`, `kagi_extract`, `kagi_news_search`, and other tool calls correctly.

## [0.9.5]

### Fixed

- Improved CLI error messages so authentication, validation, and network failures explain what was not completed and how to recover.

## [0.9.4]

### Fixed

- `kagi assistant` prompt mode now reads a missing `QUERY` from non-empty stdin, including streamed prompts such as `echo "hello" | kagi assistant --stream`.

## [0.9.3]

### Fixed

- Hardened demo scripts so they no longer use a predictable shared `/tmp` PATH shim before running authenticated `kagi` commands.
- Made Unix and PowerShell installers replace existing binaries through staged same-directory writes instead of overwriting the installed binary directly.
- Tightened release, publish, security, coverage, CI, and Makefile checks for more reproducible and complete release validation.
- Isolated integration test home and XDG directories so local user config cannot leak into test runs.
- Reduced default CLI failure output to a single user-facing diagnostic.

## [0.9.2]

### Added

- Added `kagi skills` and `kagi agent` so agents can load embedded, version-matched CLI usage guidance directly from the installed binary.
- Added Mintlify command documentation for the new embedded skill workflow.

## [0.9.1]

### Changed

- Search auth routing now preserves session-preferred fallback behavior when API credentials are unavailable or rejected.
- Lens create, update, enable, and disable flows now validate lens names more consistently and behave more reliably against live Kagi accounts.
- Assistant examples and live-test coverage now use the current model catalog.
- Release and package metadata were updated for the current GitHub Actions workflow shapes.

## [0.9.0]

### Added

- Added `kagi completion generate` and `kagi completion install` to generate or install shell completions for Bash, Zsh, Fish, and PowerShell.
- Added configurable Assistant streaming output. `kagi assistant --stream` now writes incremental text deltas by default, and `--stream-output json` keeps structured newline-delimited JSON events.

### Changed

- Base search in API-first mode now falls back to the session-token path when the Search API rejects the API key, including rate-limit and quota-style failures.
- Updated README and Mintlify docs for auth routing, completion installation, and Assistant streaming behavior.

## [0.8.1]

### Added

- Added V1 Search API support for `--region`, `--from-date`, `--to-date`, and `--limit` when search is routed through `KAGI_API_KEY`.
- Added a release workflow step that triggers the Mintlify docs deployment when `MINTLIFY_DEPLOY_COOKIE` is configured.

### Changed

- Clarified current `/api/v1` API key behavior and legacy `/api/v0` API token behavior across README and Mintlify docs.

## [0.8.0]

### Added

- Added `kagi assistant models` for stable JSON output of available Assistant base-model slugs.
- Added `kagi assistant --stream` to emit NDJSON updates with `md_delta` while Assistant responses are generated.
- Added `kagi assistant --once --model <MODEL>` to create a temporary custom assistant for one prompt and delete it afterward.

### Changed

- `kagi extract` and MCP `kagi_extract` now require `KAGI_API_KEY` directly instead of trying to derive an API key from session auth.

## [0.7.0]

### Added

- Added `KAGI_API_KEY`, `[auth].api_key`, and `kagi auth set --api-key` for current `/api/v1` Search and Extract API credentials.

### Changed

- Breaking: split current API keys from legacy API tokens. `KAGI_API_TOKEN` and `[auth].api_token` now represent legacy `/api/v0` credentials only, while base Search API mode requires `KAGI_API_KEY` or `[auth].api_key`.

## [0.6.2]

### Added

- Added `kagi_extract` to the built-in MCP server tool list, matching the existing paid Extract API command behavior.

## [0.6.1]

### Changed

- Updated `serde_json` to 1.0.150.

## [0.6.0]

### Added

- Added `kagi extract <URL>` to extract a page's full content as markdown through Kagi's v1 Extract API.
- Added the matching MCP `kagi_extract` tool for full-page markdown extraction.
- Documented Extract API auth, command usage, and coverage across the README and docs site.

## [0.5.4]

### Fixed
- MCP server no longer crashes when a tool call fails; instead propagate errors as JSON-RPC responses.

## [0.5.3]

### Added

- Added `--format toon` for compact structured output across CLI commands that support formatted responses
- Documented TOON output in the README, command reference, output contract, coverage reference, and bundled skill docs

## [0.5.2]

### Added

- `kagi mcp` now exposes a `kagi_news` tool that fetches Kagi News stories without authentication (accepts `category`, `limit`, and `lang`)
- `kagi search --news` searches the News tab of kagi.com and returns results grouped into story clusters (session auth required); supports `--region`, `--time` (day/week/month), `--order` (default/recency/website), `--limit`, and local caching
- `kagi mcp` exposes a `kagi_news_search` tool wrapping the News-tab vertical (accepts `query`, `region`, `freshness`, `order`, `limit`)

### Fixed

- `kagi news` no longer fails to parse live responses; `total_stories` is now an integer in the output (previously typed as a string, which never matched the API's actual integer payload)
- `kagi mcp` no longer replies to JSON-RPC notifications and now returns proper JSON-RPC errors for unsupported methods

## [0.5.1]

### Added

- `kagi search` and `kagi batch` accept `--limit <N>` to cap the number of results returned (truncated locally; Kagi's search endpoints have no native count parameter)

### Changed

- Updated `clap` to 4.6.1 and `tokio` to 1.52.1

### Fixed

- `kagi assistant` no longer cuts off long streamed prompt responses at the generic 30 second API timeout

## [0.5.0]

### Added

- Added product workflow commands for local profiles, search follow/watch, result templates, stdin batch input, local history, local site preferences, assistant REPL sessions, MCP stdio integration, and webhook notifications
- Added local cache and history storage for automation-friendly workflows without requiring account-level state changes
- Added documentation for the new command surfaces across the README, command reference, auth guide, and coverage reference

### Changed

- Expanded batch, search, summarize, translate, assistant, quick-answer, and FastGPT workflows with more shell-friendly input and output paths
- Updated TODO/backlog coverage to distinguish shipped local site preferences from remaining account-synced personalized-results work

### Fixed

- Error diagnostics now include request URLs, HTTP status codes, bounded response-body details, and operation-specific context where available
- Batch partial failures now report the failed query names and the number of successful queries
- Auth wizard validation warnings now point users to the relevant API or session-token recovery steps

## [0.4.7]

### Added

- `kagi assistant` now accepts repeated `--attach <PATH>` flags and uploads local files through Kagi Assistant's multipart prompt flow so prompts can include PDFs, images, and other supported documents

## [0.4.6]

### Fixed

- `kagi assistant thread list` now follows Kagi's pagination cursor so large Assistant histories return beyond the first 100 threads, and thread-list parsing now tolerates object-shaped cursors plus nullable `total_counts`

## [0.4.5]

### Changed

- Updated the backlog dependency and workflow set: `actions/setup-node v6`, `actions/upload-artifact v7.0.1`, `rand 0.10.0`, `cliclack 0.5.4`, `tokio 1.51.1`, `clap_complete 4.6.2`, `toml 1.1.2`, and `rustls-webpki 0.103.13`

### Fixed

- Credentials now save `.kagi.toml` via an atomic same-directory write so interrupted writes do not leave behind truncated config files
- Shared HTTP client initialization now retries after transient setup failures instead of permanently caching the first error for the rest of the process
- Error-body reads now preserve a diagnostic placeholder when the response body itself cannot be read
- Redirect parsing no longer uses a production `unwrap()` on rule splitting

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
