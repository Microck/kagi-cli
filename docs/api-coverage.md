# Kagi API and Product Coverage

## Current support in this CLI

### Implemented
- **Search / session-token HTML search** - fully implemented and live-verified for base search and lens-aware search
- **Search / official API-token path** - implemented for base search only; if Kagi rejects the API-token search path, base search falls back to session-token search when available
- **Universal Summarizer API** - implemented on the documented paid public API path
- **FastGPT API** - implemented on the documented paid public API path
- **Web and News Enrichment API** - implemented on the documented paid public API path
- **Small Web RSS feed** - implemented and live-verified
- **Subscriber web Summarizer** - implemented on the session-token web-product path via `kagi summarize --subscriber ...`
- **Kagi News public product endpoints** - implemented via `kagi news ...`
- **Subscriber web Assistant prompt flow** - implemented on Kagi Assistant's authenticated tagged stream via `kagi assistant ...`
- **Kagi Translate text mode** - implemented via `kagi translate ...` with runtime bootstrap from `KAGI_SESSION_TOKEN`

## Source of truth

According to Kagi's public API docs, the documented API families are:
- Kagi Search API
- Web and News Enrichment API
- Universal Summarizer API
- FastGPT API
- Kagi Small Web RSS feed

This CLI also implements non-public or product-only seams:
- subscriber web Summarizer via Kagi session-token auth
- subscriber web Assistant prompt flow via Kagi session-token auth
- Kagi News product endpoints

## Notes

- Lens support is not documented on the official Search API. In this CLI it works through Kagi's live HTML/session flow using the `l=<index>` query parameter.
- The official Search API uses `Authorization: Bot <token>` on `https://kagi.com/api/v0/search`.
- Search API access is still account-gated in practice, and API-token search can also fail for billing reasons.
- Base-search fallback to session-token search happens on the user-facing `search` command only. `auth check` validates the selected primary credential without fallback.
- The paid public Summarizer, FastGPT, and Enrichment APIs require `KAGI_API_TOKEN` and sufficient API credit.
- The subscriber web Summarizer requires `KAGI_SESSION_TOKEN` and uses the authenticated `GET /mother/summary_labs?...` stream path instead of the public `/api/v0/summarize` endpoint.
- Live verification on March 18, 2026 showed that direct HTTP bootstrap can mint `translate_session` from the same `KAGI_SESSION_TOKEN` by reading the `Set-Cookie` header from `https://translate.kagi.com/`.
- After bootstrap, the CLI uses normal Rust HTTP requests for `/api/detect`, `/api/translate`, `/api/alternative-translations`, `/api/text-alignments`, `/api/translation-suggestions`, and `/api/word-insights`.
- Assistant requires `KAGI_SESSION_TOKEN` and currently targets `/assistant/prompt` with the same tagged stream protocol used by the web app.
- News uses `https://news.kagi.com/api/...` JSON endpoints and does not require auth.
