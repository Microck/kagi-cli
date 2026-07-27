---
name: kagi-usage
description: "Search the web, extract or summarize a page, translate it, or check the news with the kagi CLI instead of WebSearch or WebFetch. Load this first for any Kagi task: it covers search and page content, and routes Assistant, automation, and account work to the right skill."
allowed-tools: Bash(kagi:*) Skill(kagi:*)
---

# Kagi CLI

Search the web and read pages with `kagi`. Route every other Kagi task to the
skill named below.

## Route the task

| User intent | Where |
| --- | --- |
| Find, compare, or verify information from the web | Search the web (below) |
| Read, summarize, question, or translate a known page | Work with one page (below) |
| Use paid AI for reasoning, coding, research, or files | `kagi-ai` |
| Continue a conversation or manage Assistant state | `kagi-assistant` |
| Run searches in bulk or monitor changes over time | `kagi-monitoring` |
| Configure credentials or account search behavior | `kagi-account-config` |

Load the skill when it appears in the available skills listing. Otherwise run
`kagi skills get <name>` for the same guidance.

```bash
kagi skills list
kagi skills get kagi-assistant
```

## Search the web

Discover sources, narrow the question, and return evidence that matches the
depth the user asked for.

| Need | Command |
| --- | --- |
| Direct answer with references | `kagi quick` |
| Ranked web results | `kagi search` |
| Search and summarize top pages | `kagi search --follow N` |
| Current news stories | `kagi news` or `kagi search --news` |
| Independent personal sites | `kagi smallweb` |
| Several independent queries | `kagi batch` |

`quick`, session-only search controls, and the subscriber search path require
session auth.
The current Search API uses `KAGI_API_KEY`. Public `news` and `smallweb` need no
credentials.

1. Define the claim or question that needs evidence.
2. Start with one broad search, or `quick` for a bounded factual question.
3. Split the topic into distinct queries when one query cannot cover it.
4. Prefer primary sources and direct evidence.
5. Use `--follow` only when the user needs the top result pages summarized.
6. Report source disagreement instead of flattening it.

```bash
kagi search "rust async cancellation" --format toon --limit 5
kagi quick "what is the current Kagi Search API endpoint?" --format markdown
```

### Search controls

```bash
kagi search "query" --region us --format toon --limit 10
kagi search "query" --time month --order recency --format json
kagi search "query" --snap reddit --format toon
kagi search "query" --lens 2 --format toon
kagi search "query" --follow 3 --format markdown
```

Session-only controls include `--lens`, `--time`, `--order`, `--verbatim`,
personalization flags, and News vertical search. Use `--local-cache` only when
stale results are acceptable.

### Query design

- Quote exact error messages or disputed phrases.
- Add official domains with `site:` when a primary source is known.
- Use native-language query terms for regional or multilingual research.
- Separate discovery, counterevidence, and recency into different queries.
- Use `--region` as a result bias, never as a language guarantee.

```bash
kagi batch \
  "topic official documentation" \
  "topic limitations" \
  "topic independent review" \
  --format toon --limit 5
```

### News and discovery

```bash
kagi news --category tech --limit 10
kagi news --list-categories
kagi news --chaos
kagi search "open source ai" --news --format toon
kagi smallweb
```

Use `news` for the public Kagi News feed. Use `search --news` when the query must
filter the News vertical.

## Work with one page

Work from the source the user supplied. Search the web only when the task needs
evidence that page cannot give.

| Outcome | Command | Credential |
| --- | --- | --- |
| Full readable page markdown | `kagi extract` | `KAGI_API_KEY` |
| Summary of a URL or text | `kagi summarize` | session or legacy API token |
| Answer about one page | `kagi ask-page` | session token |
| Translation | `kagi translate` | session token |

### Extract a page

Use `extract` when downstream work needs the article body rather than a
summary. Prefer `extract` over browser scraping for readable main content. It
requires the current `KAGI_API_KEY`, never the legacy `KAGI_API_TOKEN`.

```bash
kagi extract "https://example.com/article"
```

### Summarize

Prefer subscriber mode when a session token is available.

```bash
kagi summarize --subscriber --url "https://example.com/article"
kagi summarize --subscriber --text "long text"
kagi summarize --subscriber \
  --url "https://example.com/article" \
  --summary-type keypoints
```

Use the public API mode only when the legacy API token is configured or the
task needs its engine controls.

```bash
kagi summarize --url "https://example.com/article"
kagi summarize --text "long text" --engine cecil
```

Never summarize content that must be quoted or checked precisely. Extract it
and inspect the relevant passage instead.

### Ask one page

Use `ask-page` for a focused question whose evidence should come from one URL.

```bash
kagi ask-page \
  "https://example.com/article" \
  "what evidence supports the main claim?" \
  --format markdown
```

When the question needs outside evidence or comparison, search the web instead.

### Translate

```bash
kagi translate \
  --text "Bonjour tout le monde" \
  --target-language EN
```

Preserve code, URLs, product names, and other text that must stay untranslated.
State the target language explicitly.

## Shared rules

1. Run `kagi auth status` before treating a command as unavailable.
2. Choose the narrowest command that produces the requested outcome.
3. Use `--format json` for programmatic parsing.
4. Use `--format toon` for compact LLM context.
5. Use `--format markdown` for prose that a person will read.
6. Run `kagi auth check` when credentials exist but a request fails.
7. Never print credential values.

```bash
kagi auth status
kagi auth check
```

## Completion criteria

Web research is complete when:

- the answer addresses the exact question;
- important claims trace to source URLs;
- source quality and recency match the claim;
- disagreement and uncertainty appear in the output; and
- the output uses the requested format and depth.

Page work is complete when:

- the command matches the requested transformation;
- the output stays grounded in the supplied URL or text;
- quotes remain distinguishable from summaries;
- the output keeps the requested language and format; and
- you report a source-access failure instead of guessing past it.
