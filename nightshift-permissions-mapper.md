# Nightshift: Permissions & Auth Surface Map

**Repo:** Microck/kagi-cli
**Task:** permissions-mapper (Permissions/Auth Surface Mapper)
**Date:** 2026-04-03
**Analyst:** Nightshift v3 (GLM 5.1)

---

## Executive Summary

kagi-cli is a Rust CLI tool for interacting with Kagi's search and AI APIs. The auth surface is concentrated in two credential types (API token, session token) with a dual-resolution system (environment variables + TOML config file). The codebase has a well-structured auth module (`auth.rs` + `auth_wizard.rs`) but several noteworthy permission boundaries and potential concerns.

**Key findings:**
- 2 distinct auth mechanisms with different capabilities and security properties
- Session tokens provide broader access (search, quick answers, assistant, lenses) but are sent as cookies
- API tokens provide narrower access (summarize, fastgpt, enrich, news) via Authorization header
- Config file stores credentials in plaintext TOML at `~/.kagi.toml`
- No encryption at rest, no keychain integration, no token rotation mechanism

---

## Auth Architecture Overview

```
┌─────────────────────────────────────────────────┐
│                   Credential Sources             │
│  ┌──────────────┐    ┌─────────────────────┐    │
│  │ Env Vars     │    │ ~/.kagi.toml config  │    │
│  │ KAGI_API_    │    │ [auth]               │    │
│  │ TOKEN        │    │ api_token = "..."     │    │
│  │ KAGI_SESSION │    │ session_token = "..." │    │
│  │ _TOKEN       │    │ preferred_auth = ...  │    │
│  └──────┬───────┘    └──────────┬───────────┘    │
│         │                       │                 │
│         └───────────┬───────────┘                 │
│                     ▼                             │
│  ┌──────────────────────────────────────────┐    │
│  │     auth.rs: CredentialInventory         │    │
│  │  - load_credential_inventory()           │    │
│  │  - Resolves: env > config                │    │
│  │  - Credential { kind, source, value }    │    │
│  └──────────────────┬───────────────────────┘    │
│                     │                             │
│         ┌───────────┴───────────┐                │
│         ▼                       ▼                │
│  ┌─────────────┐    ┌─────────────────┐         │
│  │  API Token   │    │  Session Token   │         │
│  │  (Bearer)    │    │  (Cookie)        │         │
│  └──────┬──────┘    └───────┬─────────┘         │
│         │                   │                     │
│         ▼                   ▼                     │
│  ┌─────────────┐    ┌─────────────────┐         │
│  │ api.rs:     │    │ search.rs:       │         │
│  │ - summarize │    │ - HTML search    │         │
│  │ - fastgpt   │    │ quick.rs:        │         │
│  │ - enrich    │    │ - quick answers  │         │
│  │ - news      │    │ api.rs:          │         │
│  │ - translate │    │ - assistant      │         │
│  │ - smallweb  │    │ - lenses         │         │
│  └─────────────┘    │ - redirects      │         │
│                     │ - custom bangs   │         │
│                     └─────────────────┘         │
└─────────────────────────────────────────────────┘
```

---

## Auth Surface Inventory

### 1. Credential Types

| Credential | Source Constant | Resolution | Storage | Format |
|-----------|----------------|------------|---------|--------|
| API Token | `KAGI_API_TOKEN` env / `[auth].api_token` config | `auth.rs:resolve_api_token()` | Plaintext in env or TOML | String, validated by length/prefix |
| Session Token | `KAGI_SESSION_TOKEN` env / `[auth].session_token` config | `auth.rs:resolve_session_token()` | Plaintext in env or TOML | String, validated by format |

### 2. Transmission Methods

| Endpoint Category | Auth Method | Header | File |
|-------------------|------------|--------|------|
| API endpoints (summarize, fastgpt, enrich, news, translate, smallweb) | API Token | `Authorization: Bearer {token}` | `api.rs` |
| Session endpoints (search, quick, assistant, lenses, redirects, bangs) | Session Token | `Cookie: kagi_session={token}` | `search.rs`, `quick.rs`, `api.rs` |

### 3. Auth Requirement Levels

| Level | Constant | Used By | Description |
|-------|----------|---------|-------------|
| `Base` | `SearchAuthRequirement::Base` | Standard search, quick answers | Any valid session token |
| `Lens` | `SearchAuthRequirement::Lens` | Lens management (CRUD) | Session token + lens permissions |
| `Filtered` | `SearchAuthRequirement::Filtered` | News content filters | Session token + filter access |

---

## Permission Boundaries

### P1 — Session Token Has Broad Access

**File:** `search.rs`, `quick.rs`, `api.rs`
**Severity:** P1 (High)

The session token is sent as a plain cookie to every session-authenticated endpoint. If leaked, an attacker gains access to:
- Full search history and results
- Quick answers (streaming)
- Assistant threads (read/write/delete)
- Lens management (create/update/delete)
- Redirect rules (create/update/delete)
- Custom bangs (create/update/delete)
- News filters and preferences

**Suggestion:** Consider scope-limited tokens or session-specific permission flags if Kagi's API supports them.

### P2 — Plaintext Credential Storage

**File:** `auth.rs:save_credentials()` (line ~400+)
**Severity:** P2 (Medium)

Credentials are written to `~/.kagi.toml` as plaintext TOML values:
```toml
[auth]
api_token = "sk_..."
session_token = "..."
```

No OS keychain integration, no file permission enforcement beyond default umask. The `.kagi.toml` file is in the exclusion list in `Cargo.toml` (won't be committed), but any process running as the same user can read it.

**Suggestion:** Add file permission check (`chmod 600`) on write, or integrate with `keyring` crate for OS-level secret storage.

### P3 — Credential Debug Safety

**File:** `auth.rs` lines 77-85
**Severity:** P3 (Low) — Already Handled

The `Credential` struct implements `Debug` with `<redacted>` for the value field. This is good practice and prevents accidental token leakage in log output.

```rust
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("kind", &self.kind)
            .field("source", &self.source)
            .field("value", &"<redacted>")
            .finish()
    }
}
```

### P2 — No Token Validation Before Use

**File:** `quick.rs` lines 19-23, `search.rs`, `api.rs`
**Severity:** P2 (Medium)

Tokens are checked for `trim().is_empty()` but not structurally validated before being sent to the API. Invalid tokens result in a round-trip to Kagi's servers before being rejected. The `auth_wizard.rs` does validate by running a test search (`VALIDATION_QUERY = "rust lang"`), but this only happens during interactive setup.

**Suggestion:** Add basic format validation (prefix check for API tokens, length check for session tokens) before making API calls.

### P3 — HTTP Client Configuration

**File:** `http.rs`
**Severity:** P3 (Low)

Two HTTP clients are initialized with `OnceLock`:
- `CLIENT_20S` — 20 second timeout (used for most endpoints)
- `CLIENT_30S` — 30 second timeout (used for longer operations)

Both use `rustls-tls` (not native TLS), which is a good security practice. User-Agent includes version from `CARGO_PKG_VERSION`.

### P2 — Search Auth Preference System

**File:** `auth.rs:SearchAuthPreference`, `auth.rs:SearchCredentials`
**Severity:** P2 (Medium)

The codebase supports a `preferred_auth` config setting (`session` or `api`) that controls which credential is used for search operations. The resolution logic is:
1. Load both credentials from env/config
2. Respect `preferred_auth` setting
3. Fall back to whatever is available
4. `SearchCredentials` holds `primary` + optional `fallback_session`

This is well-designed but the fallback behavior is implicit — if a user sets `preferred_auth = "api"` but only has a session token, the session token is silently used for search.

---

## Endpoint Auth Matrix

| Command | Endpoint | Auth Type | Token Type | File |
|---------|----------|-----------|------------|------|
| `search` | `/html/search` | Cookie | Session | `search.rs` |
| `search` | `/api/v0/search` | Bearer | API | `api.rs` |
| `quick` | `/mother/context` | Cookie | Session | `quick.rs` |
| `summarize` | `/api/v0/summarize` | Bearer | API | `api.rs` |
| `summarize` (subscriber) | `/mother/summary_labs` | Cookie | Session | `api.rs` |
| `fastgpt` | `/api/v0/fastgpt` | Bearer | API | `api.rs` |
| `enrich/web` | `/api/v0/enrich/web` | Bearer | API | `api.rs` |
| `enrich/news` | `/api/v0/enrich/news` | Bearer | API | `api.rs` |
| `news` | `/api/batches/latest` | Bearer | API | `api.rs` |
| `news categories` | `/api/batches/categories` | Bearer | API | `api.rs` |
| `news chaos` | `/api/chaos` | Bearer | API | `api.rs` |
| `translate` | `/api/v0/translate` | Bearer | API | `api.rs` |
| `smallweb` | `/api/v0/smallweb` | Bearer | API | `api.rs` |
| `assistant` | Various `/mother/assistant/*` | Cookie | Session | `api.rs` |
| `lens *` | Various `/mother/lens/*` | Cookie | Session | `api.rs` |
| `redirect *` | Various `/mother/redirect/*` | Cookie | Session | `api.rs` |
| `bang *` | Various `/mother/bang/*` | Cookie | Session | `api.rs` |
| `ask-page` | `/mother/context` (POST) | Cookie | Session | `api.rs` |

---

## Error Handling & Auth Failure Paths

| Scenario | Detection | Error Type | Recovery |
|----------|-----------|------------|----------|
| Missing token | `trim().is_empty()` check | `KagiError::Auth` | Prompts user to set env var or run wizard |
| Expired session | HTML unauthenticated markers | `KagiError::Auth` | Detects by checking response HTML for login page markers |
| Invalid API token | HTTP 401/403 status | `KagiError::Auth` | Direct error from status code |
| Rate limiting | HTTP 429 | Retry with backoff | Some endpoints retry, others return error |
| Network timeout | `reqwest::Error::is_timeout()` | `KagiError::Network` | No retry, returns immediately |

### Auth Detection in search.rs (lines 12-16)

The session token validation relies on detecting unauthenticated HTML responses by checking for specific markers:
```rust
const UNAUTHENTICATED_MARKERS: [&str; 3] = [
    "<title>Kagi Search - A Premium Search Engine</title>",
    "Welcome to Kagi",
    "paid search engine that gives power back to the user",
];
```

This is fragile — any Kagi frontend change to the login page title or copy breaks detection. Consider using HTTP status codes or response headers instead.

### Auth Detection in quick.rs (line 70)

```rust
if looks_like_html_document(&body) {
    return Err(KagiError::Auth(
        "invalid or expired Kagi session token".to_string(),
    ));
}
```

Quick endpoint detection uses `looks_like_html_document()` which checks if the response is HTML instead of the expected JSON/stream. This is more robust than string matching.

---

## Auth Wizard Surface

**File:** `auth_wizard.rs` (655 lines)

The interactive auth wizard:
1. Detects terminal capability (`supports_interactive_auth()`)
2. Shows ASCII art branding
3. Offers credential type selection (API token vs session token)
4. Validates credentials by running a test search against Kagi
5. Saves to `~/.kagi.toml` with user preference

**Validation flow:** `validate_credential()` → test search → save if valid
**Config write:** `save_credentials_with_preference()` in `auth.rs`

The wizard enforces that credentials are valid before saving, which is good. But it doesn't verify file permissions on the config file after writing.

---

## Recommendations

1. **P1:** Add `chmod 600` enforcement on `~/.kagi.toml` after write operations
2. **P2:** Replace HTML-string-based auth detection with HTTP status code checks where possible
3. **P2:** Add structural token validation (prefix/length checks) before API calls to avoid unnecessary network round-trips
4. **P3:** Consider `keyring` crate integration for optional OS-level secret storage
5. **P3:** Document the auth preference fallback behavior in the config file template

---

## Statistics

| Metric | Count |
|--------|-------|
| Total source files analyzed | 11 |
| Auth-related files | 3 (`auth.rs`, `auth_wizard.rs`, `http.rs`) |
| Credential types | 2 (API token, session token) |
| Auth transmission methods | 2 (Bearer header, Cookie header) |
| API endpoints | ~18 |
| Session-only endpoints | 8 |
| API-only endpoints | 8 |
| Dual-auth endpoints | 2 (search, summarize) |
| P0 findings | 0 |
| P1 findings | 1 |
| P2 findings | 3 |
| P3 findings | 2 |
