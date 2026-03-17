# Handoff

## Current status

Project: `kagi` Rust CLI

The CLI is no longer search-only. It currently implements:

### Working / implemented
- **Search** via session-token HTML flow
- **Lens-aware search** via session-token HTML flow (`--lens <INDEX>`)
- **Pretty output** for search (`--format pretty`)
- **Subscriber web Summarizer** via session-token web-product flow (`kagi summarize --subscriber ...`)
- **Kagi News** via live public product JSON (`kagi news ...`)
- **Subscriber web Assistant prompt flow** via session-token tagged stream (`kagi assistant ...`)
- **Dual auth model**
  - `KAGI_API_TOKEN`
  - `KAGI_SESSION_TOKEN`
- **Auth fallback for base search**
  - base search prefers API token
  - if Kagi rejects the API-token search path, base search falls back to session-token search when available
- **Auth config commands**
  - `kagi auth status`
  - `kagi auth check`
  - `kagi auth set --api-token ...`
  - `kagi auth set --session-token <TOKEN_OR_URL>`
- **Session Link normalization**
  - `auth set --session-token` accepts either raw token or full `https://kagi.com/search?token=...` URL and extracts `token=` automatically
- **Public API command surfaces**
  - `kagi summarize`
  - `kagi fastgpt`
  - `kagi enrich web`
  - `kagi enrich news`
  - `kagi smallweb`
- **Small Web live verified**
  - `kagi smallweb --limit 3` returned real feed XML wrapped in JSON

### Tests
- `cargo test -q` currently passes
- Last known passing count: **40 tests**

## Live verification already completed

### Session-token flow: live verified
These worked with session token only:
- `kagi auth check`
- `kagi search "rust lang"`
- `kagi search --format pretty "rust lang"`
- `kagi search --lens 2 "rust lang"`

### API-token flow: implementation exists, but account constraints observed
- Search API path uses documented:
  - `GET https://kagi.com/api/v0/search`
  - `Authorization: Bot <token>`
- Actual runtime on provided account/token:
  - Search API returned HTTP `400`
  - the upstream error body included:
    - `api_balance: 0.0`
    - `Insufficient credit to perform this request`
- Therefore:
  - base-search fallback to session-token remains useful on the `search` command
  - `auth check` now validates the selected primary credential without fallback, so rejected API tokens fail truthfully

### Paid public APIs: implemented, but blocked by Kagi API billing state
Observed live:
- `kagi summarize --url https://example.com`
- Response: HTTP 400 with explicit Kagi error about **insufficient credit**
- Response payload included `api_balance: 0.0`

Implication:
- `summarize`, `fastgpt`, `enrich web`, and `enrich news` are implemented against documented API endpoints
- but full live proof is blocked unless API credits are added

### Free public API: live verified
- `kagi smallweb --limit 3` works live

### Newly completed product flows: live verified
- `kagi news --category world --limit 1`
  - returned the latest batch metadata plus world stories
- `kagi news --list-categories`
  - returned resolved batch categories with metadata
- `kagi assistant 'Reply with the word pear.'`
  - returned a completed Assistant reply plus thread id
- `kagi assistant --thread-id <ID> 'Now reply with melon.'`
  - successfully continued an existing Assistant thread

### Demo assets: completed
- Terminal demo GIFs now exist under:
  - `docs/demo-assets/search.gif`
  - `docs/demo-assets/summarize.gif`
  - `docs/demo-assets/news.gif`
  - `docs/demo-assets/assistant.gif`
- They were recorded with `asciinema` and rendered with the official asciinema `agg` binary.
- Important local tooling note:
  - an unrelated npm package also installs an `agg` command
  - the official renderer on this machine is `~/.cargo/bin/agg`
  - a symlink was added in `~/.local/bin/agg` so plain `agg` now resolves to the official binary first

## Translate status from 2026-03-16

Earlier notes in this file over-claimed Translate support. Live re-verification on **March 16, 2026** showed:

- `GET https://translate.kagi.com/api/auth` returned `null` even when the same Session Link token worked on `kagi.com`
- the Translate SSR bootstrap also embedded `session:null`
- no signing keys were issued for that Session Link state

Implication:

- `KAGI_SESSION_TOKEN` is still the right primary story for search, subscriber summarizer, and assistant
- but it is **not** enough for `translate.kagi.com` as currently deployed
- `translate` was removed from the public CLI surface because the repo is marketed around Session Link auth
- add Translate back only after there is a live-verified Session Link compatible implementation

## Files changed in recent work

Core implementation:
- `src/main.rs`
- `src/cli.rs`
- `src/auth.rs`
- `src/search.rs`
- `src/types.rs`
- `src/api.rs`
- `src/parser.rs`

Docs/artifacts:
- `README.md`
- `docs/api-coverage.md`
- `.gsd/STATE.md`
- `.gsd/milestones/M001-0a8b4i/M001-0a8b4i-DOD-AUDIT.md`
- `.gsd/milestones/M001-0a8b4i/slices/S02/S02-RESEARCH.md`
- `.gsd/milestones/M001-0a8b4i/slices/S06-PLAN.md`

## Current scope truth

### Implemented in code today
- Search CLI
- Subscriber session-token search path
- Subscriber session-token Summarizer path
- Kagi News product path
- Subscriber session-token Assistant prompt path
- API-token search path
- Session-based lens search
- Pretty rendering
- Auth configuration workflow
- Public documented Kagi API commands (summarizer / FastGPT / enrich / smallweb)

Maps was explicitly deprioritized / can be skipped.

## Research conclusions already established

Using `help.kagi.com`, these look like real subscriber-facing products and good candidates for reverse engineering through authenticated web flows:
- Kagi Search
- Kagi Assistant
- Kagi Translate
- Kagi News
- Kagi Summarize

Best reverse-engineering order:
1. **Translate**
2. **News**
3. **Assistant**

Reasoning:
- Summarize is already implemented on the subscriber web-product path
- Translate is the next most promising target
- News may have structured backing data
- Assistant is likely most complex (threads, models, files, search grounding, possible streaming)

## Important external constraints

### Search API
The official docs still describe Kagi Search API as invite-only / closed beta.
So API token generation does **not** guarantee Search API access.

### Public APIs requiring credits
These use API token + prepaid balance:
- Summarizer API
- FastGPT API
- Enrichment APIs

Current provided account/token had `api_balance: 0.0`.

### Small Web
- free endpoint
- already working live

## Where the last reverse-engineering attempt stopped

A browser session was opened and Playwright trace started, but the browser was **not authenticated** against the Kagi account.

What happened:
- browser opened on `https://kagi.com/`
- network log showed repeated polling to:
  - `POST https://kagi.com/login/qr_remote`
- this indicates the page/browser session was effectively sitting in an anonymous login/QR polling state, not authenticated subscriber state

So no useful reverse engineering of Summarize/Translate/News/Assistant happened yet.

## Continuation findings from 2026-03-16

This follow-up session confirmed that the current environment is still missing usable Kagi credentials:

- `cargo run -- auth status` reported:
  - `selected: none`
  - `api token: not configured`
  - `session token: not configured`
- No `KAGI_API_TOKEN` or `KAGI_SESSION_TOKEN` env vars were present in this shell.
- `agent-browser` opened Kagi in an anonymous state and showed a visible **Sign in** link, so there was no persisted authenticated browser profile to reuse.

Implication:
- full authenticated reverse engineering for **Summarize** and **Assistant** is still blocked
- **Translate** endpoint discovery can continue without auth, but successful requests still require auth
- **News** has public anonymous data endpoints that can already be inspected directly

### Unauthenticated browser/product findings

#### Summarizer
- `https://kagi.com/summarizer` redirected to the generic signup flow when fetched anonymously.
- No product-specific authenticated traffic was available without a session.

#### Assistant
- `https://kagi.com/assistant` also redirected to the generic signup flow when fetched anonymously.
- No product-specific authenticated traffic was available without a session.

#### Translate
- `https://translate.kagi.com/` renders an anonymous UI, but real translation requests require auth.
- Live browser capture showed:
  - `POST https://translate.kagi.com/api/detect`
  - request body: `{"text":"Bonjour tout le monde","include_alternatives":true}`
  - anonymous response: HTTP `401` with `{"error":"Not authenticated"}`
- `GET https://translate.kagi.com/api/auth/check-header` is public and returned:
  - `{"hasPrivacyPass":false}`

Translate frontend endpoint map recovered from the shipped JS:
- `POST /api/detect`
- `POST /api/translate`
- `POST /api/alternative-translations`
- `POST /api/text-alignments`
- `POST /api/translation-suggestions`
- `POST /api/word-insights`
- `GET /api/auth/check-header`

Translate request-shape findings from the frontend bundle:
- Main translate flow uses `POST /api/translate` with JSON and SSE-style streaming.
- Observed payload fields for `/api/translate`:
  - `text`
  - `from`
  - `to`
  - `stream`
  - optional `prediction`
  - optional `predicted_language`
  - `formality`
  - `speaker_gender`
  - `addressee_gender`
  - `language_complexity`
  - `translation_style`
  - `context`
  - optional `model`
  - optional `session_token`
  - optional `dictionary_language`
  - optional `time_format`
  - optional `use_definition_context`
  - optional `enable_language_features`
  - optional `context_memory`
  - optional `preserve_formatting`
- Observed request headers for `/api/translate` include:
  - `Content-Type: application/json`
  - `X-Signal: abortable|non-abortable`
  - plus extra headers injected by internal helpers `Ja(...)` / `Ga()` that still need authenticated capture to decode confidently
- `POST /api/alternative-translations` uses `FormData` and includes:
  - `quality`
  - optional `translation_options` JSON
- `POST /api/text-alignments` uses `FormData` with:
  - `source_text`
  - `target_text`
  - `include_semantic_roles`
  - `ui_language`
  - `stream=true`
- `POST /api/translation-suggestions` uses JSON with:
  - `originalText`
  - `translatedText`
  - `sourceLanguage`
  - `targetLanguage`
  - `translationOptions`
  - `language`
  - and `Accept: text/event-stream`
- `POST /api/word-insights` uses `FormData` with at least:
  - `target_explanation_language`
  - `stream=true`
  - additional text/translation option fields around the current source/target content

Most important translate conclusion:
- direct HTTP implementation still looks viable
- auth is likely cookie/session based rather than API-token based
- whether an extra CSRF/privacy-pass header is required for authenticated calls is still unproven

#### News
- `https://news.kagi.com/world/latest` rendered fully usable content anonymously.
- The app exposed stable public JSON endpoints in browser resource timing.

Live anonymous News endpoint map:
- `GET /api/batches/latest?lang=default`
- `GET /api/categories/metadata`
- `GET /api/batches/<batch-id>/categories?lang=default`
- `GET /api/batches/<batch-id>/chaos?lang=default`
- `GET /api/batches/<batch-id>/categories/<category-uuid>/stories?limit=12&lang=default`
- `GET /api/favicon-proxy?...`
- `GET /api/image-proxy?...`

Sample live responses observed:
- `/api/batches/latest?lang=default` returned a batch id like `aa9ba317-96b7-408d-a87d-aa7a52711c7c`
- `/api/categories/metadata` returned category metadata keyed by stable public ids like `world`, `usa`, `business`, `tech`, `science`
- `/api/batches/<batch-id>/categories?lang=default` returned category UUIDs plus display names, timestamps, read counts, and cluster counts
- `/api/batches/<batch-id>/categories/<category-uuid>/stories?...` returned rich story JSON including:
  - `title`
  - `short_summary`
  - `talking_points`
  - `quote`
  - `perspectives`
  - source/domain metadata
- `/api/batches/<batch-id>/chaos?lang=default` returned:
  - `chaosIndex`
  - `chaosDescription`
  - `chaosLastUpdated`

Most important news conclusion:
- Kagi News appears implementable right now as a public HTTP integration without waiting for subscriber auth
- but no CLI contract for a `kagi news ...` command has been chosen in this repo yet, so code should not guess the user-facing surface without an explicit decision

## Authenticated reverse-engineering findings from 2026-03-16

A valid Session Link was provided during this session and used successfully for live verification.

Important handling note:
- the session link was used transiently for browser verification
- `.kagi.toml` was intentionally restored afterward so the live token is **not** left in a tracked file
- if future CLI verification needs the token again, prefer an env var or another transient session-link flow over writing it into tracked config

### Auth proof

Confirmed live:
- `agent-browser` opened `https://kagi.com/` in an authenticated state
  - the homepage showed subscriber UI like `Kagi apps`, `Control Center`, and the search box instead of `Sign in`
- `cargo run -- auth check` passed when the session token was briefly present locally

### Summarizer: authenticated transport identified

Subscriber summarizer is **not** the public paid API path.

Observed live browser request:
- `GET /mother/summary_labs?url=<ENCODED_URL>&stream=1&target_language=&summary_type=article&summary_length=medium`
- request header:
  - `Accept: application/vnd.kagi.stream`

Observed live response properties:
- HTTP `200`
- `Content-Type: text/html`
- body is a NUL-delimited stream, not plain JSON

Observed stream frames included:
- `hi:{...}`
- `new_message.json:{...}`

Observed `new_message.json` shape included:
- `id`
- `thread_id`
- `created_at`
- `state`
- `prompt`
- `reply`
- `md`
- `metadata`
- `documents`

Observed failure example for `https://example.com`:
- `state: "error"`
- `reply: "We are sorry, we are not able to extract the source."`

Most important summarizer conclusion:
- direct HTTP implementation looks viable
- it needs session-cookie auth, not API-token auth
- response parsing should follow the same tagged stream frame pattern already seen in the browser, not the public API envelope

### Translate: authenticated request/response sequence identified

Observed live request sequence after typing `Bonjour tout le monde`:
1. `POST /api/detect`
2. `POST /api/translate`
3. `POST /api/word-insights`
4. `POST /api/alternative-translations`
5. `POST /api/translation-suggestions`
6. `POST /api/text-alignments`

Observed successful detection response:
- HTTP `200`
- body:
  - `{"iso":"fr","label":"French","isUncertain":false}`

Observed authenticated `/api/translate` request:
- method: `POST`
- transport: XHR / streaming response
- JSON body fields observed:
  - `text`
  - `from`
  - `to`
  - `stream`
  - `formality`
  - `speaker_gender`
  - `addressee_gender`
  - `language_complexity`
  - `translation_style`
  - `context`
  - `model`
  - `session_token`
  - `dictionary_language`
  - `use_definition_context`
  - `enable_language_features`

Important auth detail:
- the translate frontend injects its own `session_token` field into the `/api/translate` body
- do **not** assume this is the same raw Session Link token from the CLI
- the captured value looked like an app-issued JWT-like token and should be treated as an internal web-app auth artifact

Observed `/api/translate` response:
- HTTP `200`
- SSE-style text frames:
  - `data: {"detected_language": ...}`
  - `data: {"feedback": ...}`
  - `data: {"delta":"Hello everyone"}`
  - `data: {"text_done":true}`
  - `data: {"done":true}`

Observed `/api/text-alignments` response:
- HTTP `200`
- `Content-Type: text/event-stream`
- frames like:
  - `segmentation`
  - `alignment`
  - `done`

Observed `/api/alternative-translations` response:
- HTTP `200`
- `Content-Type: text/event-stream`
- frames like:
  - `original_description`
  - repeated `translation`
  - `translations_done`
  - repeated `explanation_start`
  - repeated `explanation_delta`
  - repeated `explanation_done`
  - `done`

Observed `/api/translation-suggestions` request:
- JSON body with:
  - `originalText`
  - `translatedText`
  - `sourceLanguage`
  - `targetLanguage`
  - `translationOptions`
  - `language`
- header:
  - `Accept: text/event-stream`

Most important translate conclusion:
- a CLI implementation is likely possible, but it may require more than the raw session cookie
- the page appears to mint or expose an internal per-app auth token used in translation calls
- the implemented CLI now extracts and reuses that app-issued token from the authenticated page at runtime

### Assistant: architecture identified from live page + shipped bundle

The live assistant page loaded a dedicated browser bundle:
- `/asset/47355c3/js/build/assistant/assistant.bundle.js?...`

The bundle showed that Assistant uses a shared streaming action system:
- base URL:
  - `/assistant/`
- action names:
  - `prompt`
  - `stop`
  - `thread_open`
  - `thread_list`
  - `thread_modify`
  - `thread_delete`
  - `message_regenerate`
  - `message_edit`

Bundle-derived transport details:
- `execute(actionName, options)` sends requests to `/assistant/<action>`
- requests use `POST`
- non-file requests send JSON with:
  - `Content-Type: application/json`
- all requests set:
  - `Accept: application/vnd.kagi.stream`
- file requests switch to `FormData` with a JSON `state` part plus uploaded files and generated image thumbnails
- stream messages are tagged and decoded into events like:
  - `thread.json`
  - `messages.json`
  - `new_message.json`
  - `tokens.json`
  - `location.json`
  - `limit_notice.html`
  - `unauthorized`

Additional assistant endpoints found in the bundle:
- `POST /assistant/search`
- `GET /assistant/stop/<trace_id>`
- `GET /api/quick_settings/<pageId>`
- `POST /accounts/ast_limit_ack`

Most important assistant conclusion:
- assistant is the most complex target, exactly as expected
- it is implementable over direct HTTP, and the current CLI now ships a dedicated tagged-stream parser for the prompt flow
- the current surface focuses on prompting and thread continuation rather than the full thread-management action set

## Completion update from 2026-03-16

The remaining product work in this milestone is now implemented, except for Translate:

- Translate reintroduction TODO
  - removed from the public CLI surface
  - keep the reverse-engineering findings in this handoff
  - only add it back after a live-verified Session Link compatible implementation exists
- `kagi news`
  - uses the public News product endpoints:
    - `/api/batches/latest`
    - `/api/categories/metadata`
    - `/api/batches/<batch>/categories`
    - `/api/batches/<batch>/categories/<id>/stories`
    - `/api/batches/<batch>/chaos`
  - supports stories, category listing, and chaos output
- `kagi assistant`
  - calls `POST /assistant/prompt`
  - uses the same NUL-delimited tagged stream protocol as the web app
  - returns resolved thread/message metadata
  - supports follow-up prompts with `--thread-id <ID>`

## What the next session should do

### Goal
Only remaining functional verification gap is the paid public API billing state.

### First step
If API credits become available, live-verify:
- `kagi summarize --url ...`
- `kagi fastgpt ...`
- `kagi enrich web ...`
- `kagi enrich news ...`

### Recommended workflow
1. Preserve the current direct HTTP implementations for subscriber surfaces.
2. Do not write live session links into tracked config.
3. If extending Translate or Assistant beyond the current CLI surface, reuse the same authenticated product seams already proven here rather than falling back to browser automation.

### Strong guidance
- Prefer direct HTTP implementation if the web app exposes stable fetch/XHR endpoints.
- Avoid browser automation fallback unless no stable HTTP seam exists.
- Keep JSON-first CLI output.
- Preserve current explicit failure behavior.
- For subscriber product routes, reuse session-token auth rather than API token where possible.

## Suggested next commands

Sanity check current state:
```bash
cargo test -q
cargo run -- --help
cargo run -- auth --help
```

Check docs already written:
- `README.md`
- `docs/api-coverage.md`
- `docs/handoff.md`

Then begin browser reverse engineering with authenticated state.

If credentials are unavailable in a future shell, the subscriber commands that require them are:
- `search --lens`
- `summarize --subscriber`
- `translate`
- `assistant`

## Known good facts to preserve

- Search via session token is real and works.
- Lens transport is `l=<index>` and indices are user-specific numeric values.
- `auth set --session-token` already accepts the full Session Link URL.
- Public API command surfaces already exist; do not remove them.
- Base search fallback from rejected API-token path to session-token path is intentional and useful.

## Open work summary

### Still left
- Live verify paid public APIs once Kagi API credits are available
- Re-add Translate only after solving the `translate.kagi.com` Session Link auth mismatch documented above
- Optional later cleanup:
  - replace placeholder repo/homepage metadata in `Cargo.toml`
  - normalize remaining GSD artifacts if desired

### Not left
- Base search implementation
- Lens search implementation
- Pretty output
- Auth configuration workflow
- Small Web support
- Subscriber Summarizer implementation
- Kagi News implementation
- Subscriber Assistant prompt implementation
