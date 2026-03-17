<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="../.github/assets/kagi-cli-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="../.github/assets/kagi-cli-logo-light.svg">
    <img src="../.github/assets/kagi-cli-logo-light.svg" alt="kagi cli" width="720">
  </picture>
</p>

<p align="center">
  bring kagi into the terminal with subscriber auth, paid api commands, and json-first output.
</p>

<p align="center">
  <a href="https://github.com/Microck/kagi-cli/releases"><img src="https://img.shields.io/github/v/release/Microck/kagi-cli?display_name=tag&style=flat-square&label=release&color=000000" alt="release badge"></a>
  <a href="https://github.com/Microck/kagi-cli/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Microck/kagi-cli/ci.yml?branch=main&style=flat-square&label=ci&color=000000" alt="ci badge"></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-mit-000000?style=flat-square" alt="license badge"></a>
</p>

---

`kagi` gives you one command surface for kagi search, assistant, summarization, public feeds, and paid api workflows. it is built for people who want a terminal tool that works well both interactively and inside scripts, agents, and shell pipelines.

the shortest path starts with the session-link url you already have as a kagi subscriber. paste it into `kagi auth set --session-token` and the cli extracts the token for you. if you also use kagi's paid api, add `KAGI_API_TOKEN` and the api-only commands are ready too.

[documentation](https://kagi.micr.dev) | [npm](https://www.npmjs.com/package/kagi-cli) | [github](https://github.com/Microck/kagi-cli)

![search demo](../images/demos/search.gif)

## quickstart

install on macos or linux:

```bash
curl -fsSL https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.sh | sh
kagi --help
```

install on windows powershell:

```powershell
irm https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.ps1 | iex
kagi --help
```

or use a package manager:

```bash
brew tap Microck/kagi
brew install kagi

npm install -g kagi-cli
pnpm add -g kagi-cli
bun add -g kagi-cli
```

the npm package is `kagi-cli`, but the installed command is `kagi`.

run something immediately, no auth required:

```bash
kagi news --category tech --limit 3
kagi smallweb --limit 3
```

add your subscriber session token:

```bash
kagi auth set --session-token 'https://kagi.com/search?token=...'
kagi auth check
```

then use the subscriber path:

```bash
kagi search --pretty "private search tools"
kagi search --lens 2 "developer documentation"
kagi assistant "give me 3 ways to use kagi from the terminal"
kagi summarize --subscriber --url https://kagi.com
```

add an api token when you want the paid public api commands:

```bash
export KAGI_API_TOKEN='...'
kagi summarize --url https://example.com
kagi fastgpt "best practices for private browsing"
kagi enrich web "privacy focused browsers"
```

## usage

`kagi` is meant to be useful before you do any setup at all, then grow with how deeply you use kagi. the public feed commands give you a fast smoke test, subscriber auth unlocks the web-product path, and the paid api token is there when you want the extra api-backed commands.

by default, commands write structured json to stdout so they fit cleanly into `jq`, shell scripts, and agent workflows. when you want a more readable terminal experience, `kagi search --pretty` renders the same search results in a human-friendly format without changing the underlying command behavior.

the normal flow is:

- start with `news` or `smallweb`
- add `KAGI_SESSION_TOKEN` for subscriber search, lenses, assistant, and subscriber summarization
- add `KAGI_API_TOKEN` only if you use the paid public api commands

## auth model

| credential | what it unlocks |
| --- | --- |
| `KAGI_SESSION_TOKEN` | base search by default, `search --lens`, `assistant`, `summarize --subscriber` |
| `KAGI_API_TOKEN` | paid public `summarize`, `fastgpt`, `enrich web`, `enrich news` |
| none | `news`, `smallweb`, `auth status` |

small things that matter:

- `kagi auth set --session-token` accepts either the raw token or the full session-link url
- environment variables override `.kagi.toml`
- base `kagi search` defaults to the session-token path when both credentials are present
- set `[auth] preferred_auth = "api"` if you want base search to prefer the api path instead
- `search --lens` always requires `KAGI_SESSION_TOKEN`
- `auth check` validates the selected primary credential without search fallback behavior

example config:

```toml
[auth]
session_token = "..."
api_token = "..."
preferred_auth = "api"
```

for the full command-to-token matrix, use [`kagi.micr.dev/reference/auth-matrix`](https://kagi.micr.dev/reference/auth-matrix).

## command groups

| group | commands | what they are for |
| --- | --- | --- |
| search and discovery | `search`, `news`, `smallweb` | find results, browse feeds, and pull structured search data into shell workflows |
| ai and summarization | `assistant`, `summarize`, `fastgpt` | ask questions, continue assistant threads, and summarize urls or text |
| enrichment and setup | `enrich`, `auth` | query enrichment indexes and manage or validate credentials |

for automation, stdout stays structured by default. `--pretty` is there for people, not parsers.

## examples

use search as part of a shell pipeline:

```bash
kagi search "rust release notes" | jq -r '.data[0].url'
```

switch the same command to terminal-readable output:

```bash
kagi search --pretty "rust release notes"
```

scope search to one of your lenses:

```bash
kagi search --lens 2 "developer documentation"
```

continue research with assistant:

```bash
kagi assistant "plan a focused research session in the terminal"
```

use the subscriber summarizer:

```bash
kagi summarize --subscriber --url https://kagi.com --summary-type keypoints --length digest
```

use the paid api summarizer:

```bash
kagi summarize --url https://example.com --engine cecil
```

get a faster factual answer through the paid api:

```bash
kagi fastgpt "what changed in rust 1.86?"
```

query enrichment indexes:

```bash
kagi enrich web "local-first software"
kagi enrich news "browser privacy"
```

## what it looks like

if you want a quick feel for the cli before installing it, this is the kind of output you get from the subscriber summarizer, assistant, and public news feed:

![summarize demo](../images/demos/summarize.gif)

![assistant demo](../images/demos/assistant.gif)

![news demo](../images/demos/news.gif)

## documentation

- [installation guide](https://kagi.micr.dev/guides/installation)
- [quickstart guide](https://kagi.micr.dev/guides/quickstart)
- [authentication guide](https://kagi.micr.dev/guides/authentication)
- [workflows](https://kagi.micr.dev/guides/workflows)
- [advanced usage](https://kagi.micr.dev/guides/advanced-usage)
- [auth matrix](https://kagi.micr.dev/reference/auth-matrix)
- [output contract](https://kagi.micr.dev/reference/output-contract)

## project

- [github repository](https://github.com/Microck/kagi-cli)
- [releases](https://github.com/Microck/kagi-cli/releases)
- [contributing](../CONTRIBUTING.md)
- [support](../SUPPORT.md)
- [security](../SECURITY.md)
- [license](../LICENSE)

## license

released under the [mit license](../LICENSE).

last reviewed: March 17, 2026
