---
name: kagi-cli
description: Use the kagi CLI to perform web searches, summarize content, interact with AI assistants, and access news feeds from the command line. Supports both API tokens for paid features and session tokens for subscriber features.
license: MIT
compatibility: Requires Kagi subscription for full features. Works on macOS, Linux, and Windows with Rust toolchain or npm installation.
metadata:
  author: Microck
  version: "1.0"
  repository: https://github.com/Microck/kagi-cli
  npm: https://www.npmjs.com/package/kagi-cli
---

## Capabilities

The kagi CLI provides command-line access to Kagi search and AI services:

- **Web Search**: Search the web with optional lens filtering for specialized results
- **Content Summarization**: Summarize web pages, articles, and documents using multiple AI engines
- **AI Assistant**: Interactive conversations with context threading
- **FastGPT**: Quick AI queries for rapid answers
- **News Feeds**: Access latest news across categories without authentication
- **Content Enrichment**: Enhance web content and news with additional metadata
- **Small Web Discovery**: Browse the independent web

## Authentication

The CLI supports two authentication methods:

1. **API Token (KAGI_API_TOKEN)**: Required for paid features
   - Universal Summarizer
   - FastGPT
   - Enrichment APIs
   - Search API

2. **Session Token (KAGI_SESSION_TOKEN)**: For subscriber web product features
   - Search with lenses
   - AI Assistant
   - Web summarizer
   - News and Small Web (public, no auth needed)

## Common Workflows

### Search Workflow
```bash
# Basic search (uses API token or falls back to session)
kagi search "your query"

# Search with lens (requires session token)
kagi search --lens 2 "programming query"

# Get URLs only
kagi search "query" | jq -r '.data[].url'
```

### Summarization Workflow
```bash
# Summarize with API token
kagi summarize --url https://example.com/article

# Summarize with session token
kagi summarize --subscriber --url https://example.com/article

# Get key points only
kagi summarize --subscriber --url "$URL" --summary-type keypoints | jq -r '.data.output'
```

### Assistant Workflow
```bash
# Start a conversation
THREAD_ID=$(kagi assistant "Explain quantum computing" | jq -r '.thread.id')

# Continue the conversation
kagi assistant --thread-id "$THREAD_ID" "Give me an example"
```

### Automation Workflow
```bash
# Daily news briefing
kagi news --category tech --limit 5 | jq -r '.stories[] | "\(.title)\n  \(.articles[0].link)\n"'

# Batch URL summarization
for url in $(cat urls.txt); do
  kagi summarize --subscriber --url "$url" | jq -r '.data.output'
done
```

## Input Requirements

- **Search queries**: Text strings, optionally with lens index
- **URLs**: Valid HTTP/HTTPS URLs for summarization
- **Thread IDs**: Alphanumeric strings for assistant conversations
- **Categories**: news categories (tech, world, business, science, etc.)

## Constraints

- API token required for Universal Summarizer, FastGPT, and Enrichment
- Session token required for lens search and AI Assistant
- Rate limits apply based on Kagi subscription tier
- Search API has usage costs; session-based search included with subscription

## Integration

- **Shell integration**: Works with bash, zsh, fish
- **JSON output**: Compatible with jq and other CLI tools
- **CI/CD**: Suitable for automated workflows and scripts
- **Docker**: Can run in containerized environments

## Error Handling

Common errors and resolutions:
- `missing credentials`: Set KAGI_SESSION_TOKEN or KAGI_API_TOKEN
- `auth check failed`: Verify token is valid and not expired
- `403/401`: Check token permissions and subscription status

## Resources

- Documentation: https://kagi.micr.dev
- GitHub: https://github.com/Microck/kagi-cli
- npm: https://www.npmjs.com/package/kagi-cli
- Kagi: https://kagi.com
