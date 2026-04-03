# TODO

Last reviewed against the shipped CLI surface: 2026-04-03 (`v0.4.0`).

This backlog is based on:

- a repo pass over the current CLI surface (`src/main.rs`, `src/cli.rs`, `src/search.rs`, `src/parser.rs`, `src/api.rs`, `src/types.rs`, `docs/api-coverage.md`)
- a full crawl of `https://help.kagi.com/` on 2026-03-20
- 245 help pages reviewed total: 146 Kagi pages, 87 Orion pages, 11 common/company pages, 1 root page

The main conclusion is simple:

- `kagi-cli` already covers most of the documented public API surface and several undocumented subscriber web-product seams.
- The biggest remaining opportunities are not "add one more obvious API command".
- The biggest remaining opportunities are richer search output, search-personalization management, Assistant workflow coverage, and broader product-surface coverage for Translate, News, Maps, and settings.

## Tag legend

- `useful`: value to a real `kagi-cli` user
- `hard`: implementation effort
- `risk`: likelihood the integration is brittle because it relies on undocumented or changing web-product seams
- `scope`: rough size of the change

## Ranked backlog

| Rank | Idea | Tags | Why it belongs |
| --- | --- | --- | --- |
| 1 | Preserve richer search result schema in JSON output | `useful: high` `hard: medium` `risk: low-medium` `scope: m` | The current search output shape is intentionally minimal, but the help docs for the Search API show richer objects and metadata than `kagi-cli` exposes now. This is one of the best value-for-effort upgrades because it improves every automation use case without inventing a new product seam. Help pages: `/kagi/api/search.html`, `/kagi/api/intro/response-format.html` |
| 2 | Parse search widgets and special result cards into structured output | `useful: high` `hard: high` `risk: medium-high` `scope: l` | `src/parser.rs` only extracts regular result blocks. The help docs describe answer boxes, inline maps, discussions, images, videos, quick peeks, summary boxes, podcasts, public records, and more. Right now the CLI drops most of that product value on the floor. Help pages: `/kagi/features/widgets.html`, `/kagi/settings/widgets.html`, `/kagi/features/search-operators.html` |
| 3 | Tighten lens-management docs and coverage | `useful: medium` `hard: low-medium` `risk: low` `scope: s` | Lens CRUD already shipped in v0.4.0. The remaining work is keeping docs, demos, and tests aligned with the existing `kagi lens list|get|enable|disable|create|update|delete` surface. |
| 4 | Add personalized-results management for domains/sites | `useful: high` `hard: high` `risk: high` `scope: l` | Kagi lets users block, demote, boost, normalize, or pin domains. That is highly relevant to power users and agents, and today the CLI has no way to inspect or manage those preferences. Candidate surface: `kagi site-pref list`, `kagi site-pref set example.com --mode pin`. Help pages: `/kagi/features/website-info-personalized-results.html`, `/kagi/settings/personalized-results.html` |
| 5 | Add search-shortcut management beyond shipped snaps/bangs/redirects | `useful: medium-high` `hard: medium-high` `risk: high` `scope: l` | `kagi redirect`, `kagi bang custom`, and `search --snap` already shipped in v0.4.0. The remaining gap is broader shortcut/search-shortcut management if Kagi exposes a stable surface for it. Help pages: `/kagi/features/search-shortcuts.html`, `/kagi/features/snaps.html`, `/kagi/settings/advanced.html`, `/kagi/settings/search.html` |
| 6 | Add Assistant file/document context support | `useful: high` `hard: high` `risk: high` `scope: l` | The docs explicitly describe asking questions about a document or page and uploading files into Assistant context. The current CLI has `ask-page` and prompt/thread flows, but nothing for file upload or document-backed conversations. Candidate surface: `kagi ask-file <path> <question>` or `kagi assistant prompt --file foo.pdf`. Help pages: `/kagi/ai/ask-questions.html`, `/kagi/ai/assistant.html` |
| 7 | Add Translate website/document/dictionary/proofread modes | `useful: high` `hard: high` `risk: high` `scope: xl` | Current CLI support is deep for text translation, but the help docs describe website translation, document translation, proofreading, dictionary mode, presets, and history. This is a large but real product gap. Candidate surface: `kagi translate url`, `kagi translate file`, `kagi translate proofread`, `kagi translate define`. Help pages: `/kagi/translate/`, `/kagi/translate/url-parameters.html` |
| 8 | Extend Assistant beyond the shipped custom-assistant surface | `useful: medium-high` `hard: medium-high` `risk: high` `scope: m` | Custom assistant CRUD and invocation by saved profile already shipped in v0.4.0. The remaining work is higher-level Assistant flows such as file context, save/share helpers, or mode helpers, not basic profile management. |
| 9 | Add settings snapshot/export and selective get/set | `useful: medium-high` `hard: high` `risk: high` `scope: xl` | A CLI is a strong place for "show me my current config" and "apply this known-good profile". Candidate surface: `kagi settings export`, `kagi settings get search`, `kagi settings set ai.auto_quick_answer false`. The docs show enough settings surface to justify it, but the web endpoints will likely be brittle. Help pages: `/kagi/settings/search.html`, `/kagi/settings/general.html`, `/kagi/settings/ai.html`, `/kagi/settings/widgets.html`, `/kagi/settings/assistant.html`, `/kagi/settings/advanced.html` |
| 10 | Add richer Assistant workflow controls: share, save, and research modes | `useful: medium-high` `hard: high` `risk: high` `scope: l` | The CLI already handles prompting, thread list/get/export/delete, custom assistants, and saved-assistant selection. The remaining gap is save/share flows and explicit mode helpers like `--mode quick|research`. Help pages: `/kagi/ai/assistant.html`, `/kagi/ai/kagi-research.html` |
| 11 | Add News preference and filtering management | `useful: medium` `hard: high` `risk: high` `scope: l` | The current `news` command is read-only and endpoint-driven. The product docs describe category customization, content filtering, depth control, sync, and language preferences. Candidate surface: `kagi news prefs`, `kagi news filter add`, `kagi news categories follow`. Help pages: `/kagi/news/` |
| 12 | Add auth/account helpers beyond token storage | `useful: medium` `hard: medium-high` `risk: high` `scope: m` | There is room for pragmatic account tooling around session links, 2FA visibility, and login helpers. Candidate surface: `kagi auth session-link`, `kagi auth inventory --json`, maybe `kagi auth qr-login` if the web flow is scriptable enough. Help pages: `/kagi/settings/account.html`, `/kagi/privacy/log-in-with-qr-code.html`, `/kagi/privacy/two-factor-authentication.html` |
| 13 | Add Maps search support | `useful: medium` `hard: high` `risk: very-high` `scope: l` | Maps is clearly a real Kagi product, but the help docs do not expose a stable public API. This is valuable, but likely requires reverse-engineering changing web calls. Candidate surface: `kagi maps search "coffee near me"` with JSON output. Help pages: `/kagi/maps/`, `/kagi/features/widgets.html` |
| 14 | Add Summarizer workflow helpers around discuss/follow-up and media inputs | `useful: medium` `hard: medium-high` `risk: medium-high` `scope: m` | The CLI already supports the public Summarizer API and subscriber summarizer, so this is not parity work. The gap is around higher-level flows: summarize then discuss, search-result summarize helpers, local transcript helpers, maybe direct "discuss this document" handoff into Assistant. Help pages: `/kagi/summarizer/`, `/kagi/ai/ask-questions.html` |
| 15 | Add Sidekick-related tooling if/when the product opens up | `useful: low-medium` `hard: high` `risk: very-high` `scope: l` | Sidekick looks like a developer-facing Kagi product, but the docs describe it more as an integration offering than an end-user surface. This is interesting, but not a near-term `kagi-cli` priority unless Kagi exposes a stable API or install flow. Help pages: `/kagi/sidekick/` |
| 16 | Add Mail integration only after the product stabilizes | `useful: low` `hard: high` `risk: very-high` `scope: xl` | Kagi Mail is still alpha and the docs explicitly describe it as not fully launched. It is too early to build `kagi-cli` around it unless the goal changes into a broader Kagi ecosystem CLI and a stable Mail API appears. Help pages: `/kagi/mail/` |

## Suggested implementation order

If the goal is to maximize value without overcommitting to brittle reverse-engineering, the first batch should be:

1. richer search schema
2. search widgets / special-card parsing
3. personalized-results management for domains/sites
4. Assistant file/document context
5. settings snapshot/export

That batch fits the project well because it:

- improves the existing CLI's strongest surface area instead of scattering into unrelated products
- increases automation value for both humans and agents
- stays close to Kagi Search and Assistant, which are already the center of this codebase

## Probably not first

These are real product surfaces, but I would not start here:

- Mail - too early, too unstable, too far from the current CLI
- Sidekick - interesting, but not obviously aligned with the current user story
- Maps - appealing, but likely expensive reverse-engineering with weak contract guarantees
- full settings write support - valuable, but it will likely create long-term maintenance burden unless Kagi stabilizes those internal endpoints
