---
name: kagi-cli
description: Terminal CLI for Kagi subscribers with search, quick answers, assistant, translate, summarization, batch processing, and JSON-first output for automation. Use this skill when the user mentions kagi, wants to search via kagi, needs CLI-based AI assistance, or wants terminal access to Kagi features.
license: MIT
compatibility: Requires Kagi subscription for most features. Works on macOS, Linux, and Windows. Install via Homebrew, Scoop, npm, or direct script.
metadata:
  author: Microck
  version: "0.3.3"
  repository: https://github.com/Microck/kagi-cli
  npm: https://www.npmjs.com/package/kagi-cli
  docs: https://kagi.micr.dev
---

## Overview

kagi-cli is a terminal CLI that provides command-line access to Kagi search, Quick Answer, AI Assistant, translation, summarization, and news feeds. It outputs JSON by default for scripting and automation, with `--format pretty` for human-readable terminal output.

The CLI prioritizes the subscriber session-token path, so existing Kagi subscribers can use most features without paying for API access. Paid API features (summarize, fastgpt, enrich) are available by setting `KAGI_API_TOKEN`.

## Installation

```bash
# Homebrew (macOS/Linux)
brew tap Microck/kagi
brew install kagi

# Scoop (Windows)
scoop bucket add kagi https://github.com/Microck/scoop-kagi
scoop install kagi

# npm (cross-platform)
npm install -g kagi-cli

# Direct install (macOS/Linux)
curl -fsSL https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.sh | sh

# Direct install (Windows PowerShell)
irm https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.ps1 | iex
```

## Authentication

### Interactive Setup (Recommended)

```bash
kagi auth
```

Opens a guided TTY wizard that walks through:
- Choosing Session Link (subscriber, free) or API Token (paid)
- Pasting credentials
- Saving to `~/.kagi.toml`
- Immediate validation

### Non-Interactive Setup

```bash
# Session token (from https://kagi.com/settings/user_details)
kagi auth set --session-token 'https://kagi.com/search?token=...'

# Or via environment variable
export KAGI_SESSION_TOKEN='...'

# API token (from https://kagi.com/settings/api)
export KAGI_API_TOKEN='...'
```

### Auth Model

| Credential | What It Unlocks |
|------------|-----------------|
| `KAGI_SESSION_TOKEN` | search, search --lens, quick, ask-page, assistant, translate, summarize --subscriber |
| `KAGI_API_TOKEN` | summarize, fastgpt, enrich web, enrich news |
| none | news, smallweb, auth status, --help |

Environment variables override `~/.kagi.toml`. When both tokens are present, base `kagi search` defaults to session token; set `preferred_auth = "api"` in config to prefer API.

## Commands

### kagi search

Search Kagi with JSON output by default.

```bash
# Basic search
kagi search "query"

# Pretty terminal output
kagi search --format pretty "query"

# Search with lens
kagi search --lens 2 "query"

# Filtered search
kagi search --time month --region us --order recency "rust release notes"

# Output formats: json (default), pretty, compact, markdown, csv
kagi search --format markdown "query" > results.md
```

### kagi quick

Get a direct answer with references instead of a list of results.

```bash
# Quick answer with pretty output
kagi quick --format pretty "what is rust"

# JSON for scripting
kagi quick "capital of japan" | jq '.answer'

# Markdown for documentation
kagi quick --format markdown "explain async/await" > notes.md
```

Output includes the answer, structured references, and follow-up questions.

### kagi translate

Translate text through Kagi Translate with language detection and extras.

```bash
# Auto-detect source, translate to English (default)
kagi translate "Bonjour tout le monde"

# Translate to specific target
kagi translate "Hello world" --to es

# JSON output
kagi translate "Good morning" --to de --format json | jq '.translation.translation'

# Skip extras for faster response
kagi translate "text" --to ja --no-alternatives --no-word-insights
```

Includes alternatives, word insights, alignments, and suggestions by default.

### kagi batch

Run multiple searches in parallel with rate limiting.

```bash
# Parallel searches
kagi batch "rust async" "python tutorial" "go concurrency"

# With output format
kagi batch "query1" "query2" --format compact

# CSV for spreadsheet analysis
kagi batch "product A review" "product B review" --format csv > comparison.csv
```

### kagi assistant

Prompt Kagi Assistant and manage conversation threads.

```bash
# Start conversation
kagi assistant "Explain quantum computing"

# Continue existing thread
kagi assistant --thread-id "<thread-id>" "Give me an example"

# List threads
kagi assistant thread list

# Export thread
kagi assistant thread export "<thread-id>" --format markdown > thread.md

# Delete thread
kagi assistant thread delete "<thread-id>"
```

### kagi ask-page

Ask the Assistant about a specific web page.

```bash
kagi ask-page https://example.com/article "What are the main points?"
```

### kagi summarize

Summarize URLs or text using Kagi's summarizer.

```bash
# Subscriber summarizer (free with subscription)
kagi summarize --subscriber --url https://example.com

# With options
kagi summarize --subscriber --url "$URL" --summary-type keypoints --length digest

# Paid API summarizer
kagi summarize --url https://example.com --engine cecil
```

### kagi news

Fetch Kagi News (public, no auth required), optionally with local content filters.

```bash
# Tech news
kagi news --category tech --limit 5

# JSON output
kagi news --category world | jq '.stories[0].title'

# List built-in content-filter presets
kagi news --list-filter-presets

# Hide stories that match the politics preset
kagi news --filter-preset politics

# Keep matching stories in output, but tag them for downstream tools
kagi news --filter-preset politics --filter-mode blur
```

### kagi smallweb

Fetch the Kagi Small Web feed (public, no auth required).

```bash
kagi smallweb --limit 10
```

### kagi fastgpt

Quick factual answers through the paid API.

```bash
kagi fastgpt "what changed in rust 1.86?"
```

### kagi enrich

Query Kagi's enrichment indexes (paid API).

```bash
kagi enrich web "local-first software"
kagi enrich news "browser privacy"
```

## Output Formats

All commands support multiple output formats:

| Format | Use Case |
|--------|----------|
| `json` | Default, for scripting and jq pipelines |
| `pretty` | Human-readable terminal output with colors |
| `compact` | Condensed output for quick scanning |
| `markdown` | Documentation-ready output |
| `csv` | Spreadsheet-compatible |

```bash
kagi search "query" --format json | jq '.'
kagi search "query" --format pretty
kagi search "query" --format markdown > results.md
kagi search "query" --format csv > results.csv
```

## Shell Completions

Generate completion scripts for Bash, Zsh, Fish, and PowerShell:

```bash
# Bash
kagi --generate-completion bash > ~/.local/share/bash-completion/completions/kagi

# Zsh
kagi --generate-completion zsh > ~/.zsh/completion/_kagi

# Fish
kagi --generate-completion fish > ~/.config/fish/completions/kagi.fish

# PowerShell
kagi --generate-completion powershell >> $PROFILE
```

## Common Workflows

### Research Pipeline

```bash
# Quick overview
kagi quick --format pretty "topic overview"

# Deep search with filters
kagi search --time month --format pretty "topic research"

# Batch related searches
kagi batch "topic history" "topic applications" "topic future" --format compact

# Ask assistant about findings
kagi assistant "Summarize what I found about topic"
```

### Daily News Briefing

```bash
kagi news --category tech --limit 5 --format pretty
```

### Content Analysis

```bash
# Summarize an article
kagi summarize --subscriber --url "$URL" --summary-type keypoints

# Ask about a page
kagi ask-page "$URL" "What is the author's main argument?"
```

### Translation Workflow

```bash
# Quick translation
kagi translate "text" --to es

# Full analysis
kagi translate "text" --to de --format json | jq '{
  translation: .translation.translation,
  alternatives: .alternatives.elements[0:3],
  insights: .word_insights.insights[0:5]
}'
```

### Assistant Thread Management

```bash
# Start research thread
kagi assistant "Help me understand X" > thread.json
THREAD_ID=$(cat thread.json | jq -r '.thread.id')

# Continue later
kagi assistant --thread-id "$THREAD_ID" "Now explain Y"

# Export for documentation
kagi assistant thread export "$THREAD_ID" --format markdown > research.md
```

### Batch Research

```bash
# Compare multiple topics
kagi batch "rust vs go" "python vs ruby" "react vs vue" --format pretty

# Save as CSV
kagi batch "topic1" "topic2" "topic3" --format csv > comparison.csv
```

## Input Requirements

- **Search queries**: Text strings; optionally with `--lens`, `--time`, `--region`, `--order` filters
- **Quick queries**: Natural language questions
- **Translate text**: Text string; optionally `--from` and `--to` language codes
- **URLs**: Valid HTTP/HTTPS URLs for summarize and ask-page
- **Thread IDs**: Alphanumeric strings from assistant responses
- **Categories**: News categories: world, usa, tech, science, business, etc.

## Constraints

- Session token required for: search --lens, quick, ask-page, assistant, translate, summarize --subscriber
- API token required for: summarize (public API), fastgpt, enrich
- Rate limits apply based on Kagi subscription tier
- API usage has per-query costs; session-based features included with subscription
- Translation requires session token

## Error Handling

| Error | Resolution |
|-------|------------|
| `missing credentials` | Run `kagi auth` or set KAGI_SESSION_TOKEN |
| `auth check failed` | Verify token is valid and not expired |
| `403/401` | Check token permissions and subscription status |
| `invalid lens` | Use valid lens index from your Kagi account |
| `rate limited` | Wait and retry; reduce batch concurrency |

## Resources

- Documentation: https://kagi.micr.dev
- GitHub: https://github.com/Microck/kagi-cli
- npm: https://www.npmjs.com/package/kagi-cli
- Kagi: https://kagi.com
- Auth Matrix: https://kagi.micr.dev/reference/auth-matrix
