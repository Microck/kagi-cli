<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/kagi-cli-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset=".github/assets/kagi-cli-logo-light.svg">
    <img src=".github/assets/kagi-cli-logo-light.svg" alt="kagi cli" width="720">
  </picture>
</p>


<p align="center">
  <a href="https://github.com/Microck/kagi-cli/releases"><img src="https://img.shields.io/github/v/release/Microck/kagi-cli?display_name=tag&style=flat-square&label=release&color=000000" alt="release badge"></a>
  <a href="https://github.com/Microck/kagi-cli/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Microck/kagi-cli/ci.yml?branch=main&style=flat-square&label=ci&color=000000" alt="ci badge"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-mit-000000?style=flat-square" alt="license badge"></a>
</p>

---

`kagi` is a terminal CLI for Kagi that gives you command-line access to search, lenses, assistant, summarization, feeds, and paid API commands. it is built for people who want one command surface for interactive use, shell workflows, and structured JSON output.

the main setup path is your existing Kagi session-link URL. paste it into `kagi auth set --session-token` and the CLI extracts the token for you. if you also use Kagi's paid API, add `KAGI_API_TOKEN` and the public API commands are available too.

[documentation](https://kagi.micr.dev) | [npm](https://www.npmjs.com/package/kagi-cli) | [github](https://github.com/Microck/kagi-cli)

![search demo](images/demos/search.gif)

## why

if you already use Kagi and want to access it from scripts, shell workflows, or small tools, this CLI gives you a practical path without making the paid API flow the starting point.

- use your existing session-link URL for subscriber features
- get structured JSON for scripts, agents, and other tooling
- use one CLI for search, assistant, summarization, and feeds
- add `KAGI_API_TOKEN` only when you want the paid public API commands

## quickstart

### Linux or macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.sh | sh
kagi --help
```

### Windows

```powershell
irm https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.ps1 | iex
kagi --help
```

### using a package manager

```bash
brew tap Microck/kagi
brew install kagi

npm install -g kagi-cli
pnpm add -g kagi-cli
bun add -g kagi-cli
```

### auth

add your subscriber session token:

```bash
kagi auth set --session-token 'https://kagi.com/search?token=...'
kagi auth check
```

add an api token when you want the paid public api commands:


```bash
export KAGI_API_TOKEN='...'
```

## auth model

| credential | what it unlocks |
| --- | --- |
| `KAGI_SESSION_TOKEN` | base search, `search --lens`, `assistant`, `summarize --subscriber` |
| `KAGI_API_TOKEN` | public `summarize`, `fastgpt`, `enrich web`, `enrich news` |
| none | `news`, `smallweb`, `auth status` |

example config:

```toml
[auth]
# Full Kagi session-link URL or just the raw token value.
session_token = "https://kagi.com/search?token=kagi_session_demo_1234567890abcdef"

# Paid API token for summarize, fastgpt, and enrich commands.
api_token = "kagi_api_demo_abcdef1234567890"

# Base `kagi search` auth preference: "session" or "api".
preferred_auth = "api"
```
notes:

- `kagi auth set --session-token` accepts either the raw token or the full session-link URL
- environment variables override `.kagi.toml`
- base `kagi search` defaults to the session-token path when both credentials are present
- set `[auth] preferred_auth = "api"` if you want base search to prefer the API path instead
- `search --lens` always requires `KAGI_SESSION_TOKEN`
- `auth check` validates the selected primary credential without using search fallback logic

for the full command-to-token matrix, use the [`auth-matrix`](https://kagi.micr.dev/reference/auth-matrix) docs page.

## command surface

| command | purpose |
| --- | --- |
| `kagi search` | search Kagi with JSON by default or `--pretty` for terminal output |
| `kagi auth` | inspect, validate, and save credentials |
| `kagi summarize` | use the paid public summarizer API or the subscriber summarizer with `--subscriber` |
| `kagi news` | read Kagi News from public JSON endpoints |
| `kagi assistant` | prompt Kagi Assistant with a subscriber session token |
| `kagi fastgpt` | query FastGPT through the paid API |
| `kagi enrich` | query Kagi's web and news enrichment indexes |
| `kagi smallweb` | fetch the Kagi Small Web feed |

for automation, stdout stays JSON by default. `--pretty` only changes rendering for humans.

## examples

use search as part of a shell pipeline:

```bash
kagi search "what is mullvad"'
```

switch the same command to terminal-readable output:

```bash
kagi search --pretty "how do i exit vim"
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

![summarize demo](images/demos/summarize.gif)

![assistant demo](images/demos/assistant.gif)

![news demo](images/demos/news.gif)

## documentation

- [installation guide](https://kagi.micr.dev/guides/installation)
- [quickstart guide](https://kagi.micr.dev/guides/quickstart)
- [authentication guide](https://kagi.micr.dev/guides/authentication)
- [workflows](https://kagi.micr.dev/guides/workflows)

## license

[mit license](LICENSE).
