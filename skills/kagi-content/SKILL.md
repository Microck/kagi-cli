---
name: kagi-content
description: Read, extract, summarize, question, or translate a URL or supplied text with Kagi. Use when the source is already known and the user needs its readable content, key points, an answer about the page, or a translation.
allowed-tools: Bash(kagi:*)
---

# Kagi Content

Use the narrowest content command. Do not run a broad web search when the user
already supplied the source.

## Choose The Command

| Outcome | Command | Credential |
| --- | --- | --- |
| Full readable page markdown | `kagi extract` | `KAGI_API_KEY` |
| Summary of a URL or text | `kagi summarize` | session or legacy API token |
| Answer about one page | `kagi ask-page` | session token |
| Translation | `kagi translate` | session token |

Run `kagi auth status` before selecting between subscriber and API
summarization.

## Extract A Page

Use `extract` when downstream work needs the article body rather than a
summary.

```bash
kagi extract "https://example.com/article"
```

Prefer extraction over browser scraping for readable main content. Extraction
requires the current `KAGI_API_KEY`, not the legacy `KAGI_API_TOKEN`.

## Summarize

Prefer subscriber mode when a session token is available.

```bash
kagi summarize --subscriber --url "https://example.com/article"
kagi summarize --subscriber --text "long text"
kagi summarize --subscriber \
  --url "https://example.com/article" \
  --summary-type keypoints
```

Use the public API mode only when the legacy API token is configured or its
engine controls are required.

```bash
kagi summarize --url "https://example.com/article"
kagi summarize --text "long text" --engine cecil
```

Do not summarize content that must be quoted or checked precisely. Extract it
and inspect the relevant passage instead.

## Ask One Page

Use `ask-page` for a focused question whose evidence should come from one URL.

```bash
kagi ask-page \
  "https://example.com/article" \
  "what evidence supports the main claim?" \
  --format markdown
```

If the question requires outside evidence or comparison, switch to
`kagi-research`.

## Translate

```bash
kagi translate \
  --text "Bonjour tout le monde" \
  --target-language EN
```

Preserve code, URLs, product names, and other text that should not be
translated. State the desired target language explicitly.

## Completion Criteria

Content work is complete when:

- the command matches the requested transformation;
- the output stays grounded in the supplied URL or text;
- quotes remain distinguishable from summaries;
- the requested language and format are preserved; and
- any source-access failure is reported rather than filled with guesses.
