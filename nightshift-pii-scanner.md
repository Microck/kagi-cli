# Nightshift PII Exposure Scanner — kagi-cli

**Repo:** Microck/kagi-cli
**Date:** 2026-04-04
**Scanner:** Nightshift v3 (GLM 5.1)

## Summary

Rust CLI for Kagi search/API. Handles API tokens and session tokens for authentication. The codebase demonstrates strong security practices: credentials are redacted in Debug output, config files get 0600 permissions, and `.gitignore` excludes `.kagi.toml` and `.env` files. No PII leaks found in user-controlled code.

## Findings

### P2 — Session token passed as cookie header (design, not a leak)
- **Files:** `src/api.rs:164`, `src/api.rs:485`, `src/search.rs:202`
- **Detail:** Session tokens are sent as `Cookie: kagi_session={token}` headers. This is the required auth mechanism for Kagi's subscriber APIs (search, assistant, translate, subscriber summarizer).
- **Risk:** Not a leak — this is intentional. Session tokens in cookies are visible in network logs, HTTP debug output, and proxy logs. If `RUST_LOG=debug` is set, reqwest may log these headers.
- **Recommendation:** Ensure the `tracing` setup (see `src/main.rs:80-88`) doesn't log at debug level in production. The current default (`warn`) is safe. Consider adding a note in documentation about avoiding `RUST_LOG=debug` with session tokens.

### P2 — Error messages could leak token context in logs
- **Files:** `src/api.rs:110-113`, `src/search.rs:237-239`
- **Detail:** When tokens are empty, error messages reference "expected KAGI_API_TOKEN" or "expected KAGI_SESSION_TOKEN". These are env var names, not actual tokens, so they're safe.
- **Risk:** Low. The error messages expose env var names, not values. This is standard practice.
- **Recommendation:** No action needed.

### P3 — Config file stores credentials in plaintext
- **File:** `src/auth.rs:402-413`
- **Detail:** `.kagi.toml` stores API and session tokens as plaintext TOML values. The file is secured with `chmod 600` on Unix (line 481), which is appropriate.
- **Risk:** Standard for CLI tools. The 0600 permission mitigates local access. However, tokens are readable by any process running as the same user.
- **Recommendation:** Consider documenting that users on shared systems should use environment variables instead of the config file. The current implementation already supports both (env vars take precedence).

### P3 — Test code references env var names
- **Files:** `src/api.rs:4251`, `src/api.rs:4307`, `src/api.rs:4396`, `src/api.rs:4481`, `src/api.rs:4553`, `src/api.rs:4644`
- **Detail:** Integration tests print "skipping live test because KAGI_SESSION_TOKEN is not set" to stderr.
- **Risk:** Negligible — env var names, not values.
- **Recommendation:** No action needed.

### P3 — `.env.example` documents expected env vars
- **File:** `.env.example`
- **Detail:** Contains `KAGI_API_TOKEN=` and `KAGI_SESSION_TOKEN=` (empty values).
- **Risk:** This is best practice — `.env.example` documents required variables without containing actual secrets.
- **Recommendation:** No action needed.

## Clean Areas (Security Strengths)

- **`Credential::fmt::Debug` redacts values** (`src/auth.rs:77-84`): Debug output shows `<redacted>` instead of actual token values. This prevents accidental token leakage in log output.
- **Config file permissions** (`src/auth.rs:477-488`): `secure_config_permissions()` sets `0o600` on Unix, preventing other users from reading the config.
- **`.gitignore` excludes sensitive files**: `.kagi.toml`, `.env`, and `.env.*` are all excluded (with `!.env.example` allowed).
- **No token logging**: `tracing::debug!` calls in `search.rs` and `api.rs` log status codes and error messages, never token values.
- **No hardcoded credentials**: No API keys, tokens, or secrets in source code.
- **No PII collection**: The CLI doesn't collect, store, or transmit any personal information beyond what's required for Kagi API access.

## Verdict

**Very low risk.** This codebase demonstrates security-conscious credential handling: Debug trait redaction, config file permissions, proper `.gitignore`, and no hardcoded secrets. The only actionable finding is a documentation note about `RUST_LOG=debug` potentially exposing session tokens in HTTP headers via reqwest's internal logging.
