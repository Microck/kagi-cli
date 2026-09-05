## [0.19.0]

### Added

- `summarize --filter` now streams one compact JSON record per input line (`{"input", "ok", ...}`) and reports an aggregate failure count instead of stopping at the first error, matching `extract --filter` (#178).
- Local history, site preferences, and response cache files are now protected by advisory file locks, and site preference updates run read-modify-write under one exclusive lock, so concurrent CLI processes can no longer interleave writes or lose updates (#176, #177).

### Fixed

- `fastgpt --web-search false` is now rejected client-side with an explanatory error because the upstream FastGPT API only supports web search grounding (#179).
- `--generate-completion <SHELL>` combined with a subcommand now exits with standard clap usage error code `2` instead of `1` (#180).
- Updated dependencies: clap 4.6.6, clap_complete 4.6.9, cliclack 0.5.6, futures-util 0.3.34, jsonc-parser 0.33.1, serde_json 1.0.151, thiserror 2.0.20, toml 1.1.5, actions/checkout 7.0.1.

## [Unreleased]

## [0.18.1]

### Fixed

- `kagi usage` no longer rejects valid sessions with an authentication error and parses Kagi's current billing page layout, including the new "AI usage (USD)" cost box (#173).

## [0.18.0]

### Added

- `kagi mcp` now auto-negotiates the wire protocol per request: draft `2026-07-28` requests with `params._meta` metadata keep `server/discover` and cache hints, while requests without it — including `initialize`, `ping`, `tools/list`, and `tools/call` — are answered per the stable MCP specification.
- Added `kagi usage` (`kagi billing`) to report plan, AI cost used and limit, balance, renewal date, and daily calendar-month usage with session-token auth.

## [0.17.2]
