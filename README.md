<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/kagi-cli-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset=".github/assets/kagi-cli-logo-light.svg">
    <img src=".github/assets/kagi-cli-logo-light.svg" alt="kagi cli" width="720">
  </picture>
</p>

<p align="center">
  use kagi from your terminal with your session-link url, or drop in an api token when you want the paid api commands too.
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
- get structured JSON for scripts and agents
- use one CLI for search, assistant, summarization, and feeds
- add `KAGI_API_TOKEN` only when you want the paid public API commands

## quickstart

install on macOS or Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.sh | sh
kagi --help
```

install on Windows PowerShell:

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
kagi search --pretty "private search tools"
kagi search --lens 2 "developer documentation"
kagi assistant "give me 3 ways to use kagi from the terminal"
kagi summarize --subscriber --url https://kagi.com
```

add an API token when you want the paid public API commands:

```bash
export KAGI_API_TOKEN='...'
kagi summarize --url https://example.com
kagi fastgpt "best practices for private browsing"
kagi enrich web "privacy focused browsers"
```

## what it covers

- `search` returns structured Kagi results, with `--pretty` when you want terminal output
- `search --lens`, `assistant`, and `summarize --subscriber` use the subscriber session path
- `news` and `smallweb` work without credentials
- `summarize`, `fastgpt`, and `enrich` use `KAGI_API_TOKEN` for paid public API access

## auth model

| credential | what it unlocks |
| --- | --- |
| `KAGI_SESSION_TOKEN` | base search, `search --lens`, `assistant`, `summarize --subscriber` |
| `KAGI_API_TOKEN` | public `summarize`, `fastgpt`, `enrich web`, `enrich news` |
| none | `news`, `smallweb`, `auth status` |

small things that matter:

- `kagi auth set --session-token` accepts either the raw token or the full session-link URL
- environment variables override `.kagi.toml`
- base `kagi search` defaults to the session-token path when both credentials are present
- set `[auth] preferred_auth = "api"` if you want base search to prefer the API path instead
- `search --lens` always requires `KAGI_SESSION_TOKEN`
- `auth check` validates the selected primary credential without using search fallback logic

example config:

```toml
[auth]
session_token = "..."
api_token = "..."
preferred_auth = "api"
```

for the full command-to-token matrix, use the docs page at [`kagi.micr.dev/reference/auth-matrix`](https://kagi.micr.dev/reference/auth-matrix).

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

released under the [mit license](LICENSE).

last reviewed: March 17, 2026
