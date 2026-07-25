---
name: kagi-research
description: Research, compare, verify, or discover information with Kagi Search, Quick Answer, News, and Small Web. Use when the user needs source discovery, current facts, multiple perspectives, or a cited answer rather than one known page.
allowed-tools: Bash(kagi:*)
---

# Kagi Research

Use Kagi to discover sources, narrow the question, and return evidence that
matches the user's requested depth.

## Choose The Research Lane

| Need | Command |
| --- | --- |
| Direct answer with references | `kagi quick` |
| Ranked web results | `kagi search` |
| Search and summarize top pages | `kagi search --follow N` |
| Current news stories | `kagi news` or `kagi search --news` |
| Independent personal sites | `kagi smallweb` |
| Several independent queries | `kagi batch` |

`quick`, search filters, and the subscriber search path require session auth.
The current Search API uses `KAGI_API_KEY`. Public `news` and `smallweb` need no
credentials.

## Workflow

1. Define the claim or question that needs evidence.
2. Run `kagi auth status`.
3. Start with one broad search or `quick` for a bounded factual question.
4. Split the topic into distinct queries when one query cannot cover it.
5. Prefer primary sources and direct evidence.
6. Use `--follow` only when summaries of the top result pages are useful.
7. Report source disagreement instead of flattening it.

```bash
kagi auth status
kagi search "rust async cancellation" --format toon --limit 5
kagi quick "what is the current Kagi Search API endpoint?" --format markdown
```

## Search Controls

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

## Query Design

- Quote exact error messages or disputed phrases.
- Add official domains with `site:` when a primary source is known.
- Use native-language query terms for regional or multilingual research.
- Separate discovery, counterevidence, and recency into different queries.
- Use `--region` as a result bias, not as a language guarantee.

```bash
kagi batch \
  "topic official documentation" \
  "topic limitations" \
  "topic independent review" \
  --format toon --limit 5
```

## News And Discovery

```bash
kagi news --category tech --limit 10
kagi news --list-categories
kagi news --chaos
kagi search "open source ai" --news --format toon
kagi smallweb
```

Use `news` for the public Kagi News feed. Use `search --news` when the query must
filter the News vertical.

## Completion Criteria

Research is complete when:

- the answer addresses the exact question;
- important claims trace to source URLs;
- source quality and recency match the claim;
- disagreements or uncertainty are explicit; and
- the output uses the requested format and depth.
