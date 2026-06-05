---
name: kagi
description: Agent-native CLI for Kagi search, Assistant, Quick Answer, extraction, summarization, translation, news, and account settings. Use when the user wants web research through Kagi, readable page extraction, subscriber summarization, Assistant threads, Kagi News, or Kagi account configuration from a terminal or automation workflow.
allowed-tools: Bash(kagi:*)
---

# Kagi CLI

`kagi` is a JSON-first command surface for Kagi. It covers search, Quick Answer,
Assistant, ask-page, translation, summarization, readable page extraction, Kagi
News, Small Web, paid API commands, and account settings such as lenses, custom
bangs, custom assistants, and redirects.

## Core Workflow

1. **Load this skill**: `kagi skills get kagi`
2. **Check credentials**: `kagi auth status`
3. **Choose the narrowest command** for the task
4. **Use structured output**: `--format json` for scripts, `--format toon` for LLM context
5. **Verify failures with auth check**: `kagi auth check`

```bash
kagi skills get kagi
kagi auth status
kagi search "rust async cancellation" --format toon --limit 5
```

## Credential Discovery

Use auth commands before choosing a Kagi-backed endpoint.

```bash
kagi auth status
kagi auth check
kagi auth set --session-token "https://kagi.com/search?token=..."
```

| Credential | Unlocks |
| --- | --- |
| `KAGI_SESSION_TOKEN` | subscriber search path, session-only search filters, `quick`, `assistant`, `ask-page`, `translate`, and `summarize --subscriber` |
| `KAGI_API_KEY` | current `/api/v1` Search API and Extract API |
| `KAGI_API_TOKEN` | legacy `/api/v0` `summarize`, `fastgpt`, `enrich web`, and `enrich news` |
| none | `news`, `smallweb`, `auth status`, `skills`, and help |

Environment variables override `.kagi.toml`. `--profile NAME` selects
`[profiles.NAME.auth]` from config. Base `kagi search` prefers the session-token
path when both session and API credentials are present unless config sets
`preferred_auth = "api"`.

## Output Modes

| Format | Use for |
| --- | --- |
| `json` | default structured output for scripts and parsers |
| `toon` | compact structured data for LLM context |
| `markdown` | prompt material, notes, and documentation |
| `compact` | minified JSON for pipelines |
| `csv` | spreadsheets or tabular export |
| `pretty` | human terminal display only |

Prefer `--format toon` when returning search or news results to an LLM. Prefer
`--format json` when another program will parse stdout.

## Search

```bash
kagi search "query" --format toon --limit 5
kagi search "query" --format markdown --limit 10
kagi search "query" --snap reddit --format toon
kagi search "query" --region us --from-date 2026-01-01 --format json
```

Useful search flags:

| Flag | Purpose |
| --- | --- |
| `--limit N` | cap returned results |
| `--snap NAME` | prefix query with a Kagi Snap shortcut |
| `--lens INDEX` | scope to a Kagi lens by index; requires session auth |
| `--region CODE` | region-bias search |
| `--time day|week|month|year` | recent time window; session path |
| `--order default|recency|website|trackers` | result ordering; session path |
| `--verbatim` | request verbatim search mode; session path |
| `--personalized` / `--no-personalized` | force personalization mode; session path |
| `--local-cache` | cache repeatable calls locally |

## Search And Summarize Results

Use `--follow N` when the user wants synthesized coverage of top result pages.
It searches first, then summarizes the top N URLs through the subscriber
summarizer.

```bash
kagi search "what changed in rust 2024 edition" --follow 3 --format markdown
```

## Readable Page Extraction

Use `extract` when the user needs full readable page content as markdown. It
requires `KAGI_API_KEY`.

```bash
kagi extract "https://example.com/article"
```

Use extraction instead of scraping rendered browser text when exact readable
article content is the goal.

## Summarization

Subscriber summarizer, usually best for users with a Kagi account:

```bash
kagi summarize --subscriber --url "https://example.com/article"
kagi summarize --subscriber --text "long text"
kagi summarize --subscriber --url "https://example.com/article" --summary-type keypoints
```

Paid public API summarizer through the legacy API token:

```bash
kagi summarize --url "https://example.com/article"
kagi summarize --text "long text" --engine cecil
```

## Quick Answer

Use `quick` when the user asks a direct question and wants a concise answer with
references from live Kagi results.

```bash
kagi quick "who maintains the rust compiler release train?" --format markdown
kagi quick "current kagi api search endpoint" --format toon
```

## Assistant

Use `assistant` for conversational tasks, thread continuity, attachments, custom
assistants, model selection, or account-backed Assistant behavior.

```bash
kagi assistant "explain this release note" --format markdown
kagi assistant --stream --stream-output json "draft a migration checklist"
kagi assistant --thread-id THREAD_ID "continue from the previous answer"
kagi assistant --attach ./notes.md "summarize the attached notes" --format markdown
```

Thread management:

```bash
kagi assistant thread list
kagi assistant thread get THREAD_ID
kagi assistant thread export THREAD_ID --format markdown
kagi assistant thread delete THREAD_ID
```

Custom assistants:

```bash
kagi assistant custom list
kagi assistant custom get "Researcher"
kagi assistant custom create "CLI Researcher" --web-access --model gpt-5-mini
```

## Ask A Page

Use `ask-page` when the user has one URL and a question about that page.

```bash
kagi ask-page "https://example.com/article" "what are the main claims?" --format markdown
```

## Translation

```bash
kagi translate --text "Bonjour tout le monde" --target-language EN
```

Translation uses session-token auth.

## News And Public Feeds

`news` and `smallweb` do not require credentials.

```bash
kagi news --category tech --limit 10
kagi news --list-categories
kagi news --chaos
kagi smallweb
```

Search the Kagi News vertical with session auth:

```bash
kagi search "open source ai" --news --format toon
```

## Batch And Watch

Use `batch` for parallel search. Use stdin for generated query lists.

```bash
kagi batch "rust" "zig" "go" --format toon --limit 3
printf 'rust\nzig\ngo\n' | kagi batch --format compact
```

Use `watch` when the user wants result changes over time.

```bash
kagi watch "site:example.com release notes" --interval 300
```

## Account Settings

Use these commands when the user asks to inspect or manage Kagi account features.
They require the relevant account credential.

```bash
kagi lens list
kagi lens get "Default"
kagi bang custom list
kagi bang custom create "Docs" --trigger docs --template "https://docs.rs/releases/search?query=%s"
kagi redirect list
kagi assistant custom list
```

## MCP Server

Use `kagi mcp` when another agent or tool needs a stdio MCP server exposing Kagi
tools.

```bash
kagi mcp
```

## Agent-Safe Patterns

- Start with `kagi skills get kagi` for current, version-matched guidance.
- Run `kagi auth status` before declaring credentials unavailable.
- Do not print credential values. Auth commands intentionally summarize
  presence and source without exposing secrets.
- Prefer the narrowest command: `extract` for readable pages, `quick` for direct
  answers, `assistant` for conversational or threaded work, `news` for public
  news feeds.
- Prefer `--format toon` for LLM context and `--format json` for programmatic
  parsing.
- Use `--local-cache` only when stale data is acceptable.
- Treat `--format pretty` as human-facing only.
- Avoid browser automation for Kagi content retrieval unless the workflow
  specifically requires rendered interaction.

## Important Notes

- `search --lens`, `--time`, `--order`, `--verbatim`, and personalization flags
  require session-token auth.
- API-first search can fall back to the session path when configured credentials
  permit fallback.
- `extract` requires the current `KAGI_API_KEY`, not the legacy API token.
- `summarize --subscriber`, `quick`, `assistant`, `ask-page`, and `translate`
  require `KAGI_SESSION_TOKEN`.
- `news`, `smallweb`, `skills`, and help are auth-free.
