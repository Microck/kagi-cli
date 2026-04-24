<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/kagi-cli-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset=".github/assets/kagi-cli-logo-light.svg">
    <img src=".github/assets/kagi-cli-logo-light.svg" alt="kagi cli" width="720">
  </picture>
</p>


<p align="center">
  <a href="https://github.com/Microck/kagi-cli/releases"><img src="https://img.shields.io/github/v/release/Microck/kagi-cli?display_name=tag&style=flat-square&label=release&color=000000" alt="release badge"></a>
  <a href="https://www.npmjs.com/package/kagi-cli"><img src="https://img.shields.io/npm/dt/kagi-cli?style=flat-square&label=downloads&color=000000" alt="npm downloads"></a>
  <a href="https://github.com/Microck/kagi-cli/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Microck/kagi-cli/ci.yml?branch=main&style=flat-square&label=ci&color=000000" alt="ci badge"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-mit-000000?style=flat-square" alt="license badge"></a>
</p>

---

`kagi` is a terminal CLI for Kagi that gives you command-line access to search, quick answers, ask-page, assistant, translate, summarization, public feeds through `news` and `smallweb`, paid API commands like `fastgpt` and `enrich`, and account-level settings like lenses, custom assistants, custom bangs, and redirect rules. it is built for people who want one command surface for interactive use, shell workflows, and structured JSON output.

the main setup path is `kagi auth`. on a real terminal it opens a guided setup flow where you choose `Session Link` or `API Token`, get the official instructions inline, paste the credential, save it to `./.kagi.toml`, and validate it immediately. if you also use Kagi's paid API, the same wizard can add that too.

[documentation](https://kagi.micr.dev) | [npm](https://www.npmjs.com/package/kagi-cli) | [github](https://github.com/Microck/kagi-cli)

![search demo](images/demos/search.gif)

## why

if you already use Kagi and want to access it from scripts, shell workflows, or small tools, this CLI gives you a practical path without making the paid API flow the starting point.

- use your existing session-link URL for subscriber features
- get structured JSON for scripts, agents, and other tooling
- use one CLI for search, quick answers, assistant, translate, summarization, `news`, and `smallweb`
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
npm install -g kagi-cli
pnpm add -g kagi-cli
bun add -g kagi-cli

# homebrew
brew tap Microck/kagi
brew install kagi

# scoop
scoop bucket add kagi https://github.com/Microck/scoop-kagi
scoop install kagi

# AUR (arch linux)
yay -S kagi-cli
```

### auth

run the guided setup:

```bash
kagi auth
```

the wizard is the default setup path. it guides you through:

- `Session Link` from `https://kagi.com/settings/user_details`
- `API Token` from `https://kagi.com/settings/api`
- saving into `./.kagi.toml`
- immediate validation

non-interactive alternative:

add your subscriber session token directly:

how to get it:

1. click the top-right menu icon
2. go into `Settings`
3. click `Account` in the left sidebar
4. in `Session Link`, click `Copy`

![session-link tutorial](images/tutorials/session-link.gif)

```bash
kagi auth set --session-token 'https://kagi.com/search?token=...'
kagi auth check
```

add an api token when you want the paid public api commands:


how to get it:

1. click the top-right menu icon
2. go into `Settings`
3. click `Advanced` in the left sidebar
4. go into `Open API Portal`
5. under `API Token`, click `Generate New Token`

![api token tutorial](images/tutorials/api-token.gif)

```bash
export KAGI_API_TOKEN='...'
```

## auth model

| credential | what it unlocks |
| --- | --- |
| `KAGI_SESSION_TOKEN` | base search fallback, `search --lens`, filtered search, `quick`, `ask-page`, `assistant`, `translate`, `summarize --subscriber` |
| `KAGI_API_TOKEN` | public `summarize`, `fastgpt`, `enrich web`, `enrich news` |
| none | `news`, `smallweb`, `auth status`, `--help` |

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

- `kagi auth` is interactive on TTYs and becomes the default onboarding path
- `kagi auth set --session-token` accepts either the raw token or the full session-link URL
- environment variables override `.kagi.toml`
- base `kagi search` defaults to the session-token path when both credentials are present
- set `[auth] preferred_auth = "api"` if you want base search to prefer the API path instead
- `search --lens` and all runtime search filters require `KAGI_SESSION_TOKEN`
- `auth check` validates the selected primary credential without using search fallback logic

for the full command-to-token matrix, use the [`auth-matrix`](https://kagi.micr.dev/reference/auth-matrix) docs page.

## command surface

| command | purpose |
| --- | --- |
| `kagi search` | search Kagi with `json` by default, or render as `pretty`, `compact`, `markdown`, or `csv` |
| `kagi batch` | run multiple searches in parallel with JSON, compact, pretty, markdown, or csv output and shared filters |
| `kagi auth` | launch the auth wizard, or inspect, validate, and save credentials |
| `kagi summarize` | use the paid public summarizer API or the subscriber summarizer with `--subscriber` |
| `kagi news` | read Kagi News from public JSON endpoints |
| `kagi quick` | get a Quick Answer with references |
| `kagi assistant` | prompt Kagi Assistant, continue threads, manage thread list/export/delete, and manage custom assistants |
| `kagi ask-page` | ask Kagi Assistant about a specific web page |
| `kagi translate` | translate text through Kagi Translate |
| `kagi fastgpt` | query FastGPT through the paid API |
| `kagi enrich` | query Kagi's web and news enrichment indexes |
| `kagi smallweb` | fetch the Kagi Small Web feed |
| `kagi lens` | list, inspect, create, update, enable, disable, and delete search lenses |
| `kagi bang custom` | list, inspect, create, update, and delete custom bangs |
| `kagi redirect` | list, inspect, create, update, enable, disable, and delete redirect rules |

for automation, stdout stays JSON by default. `--format pretty` only changes rendering for humans.

## shell completion

generate a completion script and install it with your shell of choice:

```bash
# bash
kagi --generate-completion bash > ~/.local/share/bash-completion/completions/kagi

# zsh
kagi --generate-completion zsh > ~/.zsh/completion/_kagi

# fish
kagi --generate-completion fish > ~/.config/fish/completions/kagi.fish

# powershell
kagi --generate-completion powershell >> $PROFILE
```

see the [installation guide](https://kagi.micr.dev/guides/installation) for platform-specific setup details.

## examples

use search as part of a shell pipeline:

```bash
kagi search "what is mullvad"
```

switch the same command to terminal-readable output:

```bash
kagi search --format pretty "how do i exit vim"
```

scope search to one of your lenses:

```bash
kagi search --lens 2 "developer documentation"
```

prefix a search with one of your configured snaps:

```bash
kagi search --snap reddit "rust async runtime"
```

run a filtered search against the subscriber web-product path:

```bash
kagi search --region us --time month --order recency "rust release notes"
```

use explicit date bounds instead of a preset time window:

```bash
kagi search --from-date 2026-03-01 --to-date 2026-03-31 "rust release notes"
```

force personalized search on or off for one request:

```bash
kagi search --personalized "best cafes nearby"
kagi search --no-personalized "best cafes nearby"
```

run a few searches in parallel:

```bash
kagi batch "rust programming" "python tutorial" "go language"
```

change batch output format for shell pipelines:

```bash
kagi batch "rust" "python" "go" --format compact
```

continue research with assistant:

```bash
kagi assistant "plan a focused research session in the terminal"
```

run assistant with a saved assistant profile and markdown output:

```bash
kagi assistant --assistant research --format markdown "summarize the latest rust release"
```

attach local files to an assistant prompt:

```bash
kagi assistant --attach ./a.jpg --attach ./b.pdf "tell me everything about this pdf"
```

ask assistant about a page directly:

```bash
kagi ask-page https://rust-lang.org/ "What is this page about?"
```

list or export Assistant threads:

```bash
kagi assistant thread list
kagi assistant thread export <THREAD_ID>
```

`kagi assistant thread list` automatically follows Kagi's pagination cursor so large chat histories are returned in one response.

manage custom assistants:

```bash
kagi assistant custom list
kagi assistant custom get "Release Notes"
kagi assistant custom create "Release Notes" --model gpt-5-mini --web-access --lens 2 --instructions "Focus on release diffs and migration notes."
kagi assistant custom update "Release Notes" --bang-trigger relnotes --no-personalized
```

get a quick answer with references:

```bash
kagi quick --format pretty "what is rust"
```

translate text and keep all text-mode extras:

```bash
kagi translate "Bonjour tout le monde"
```

plain `kagi translate "..."` means `--from auto --to en`.

translate to a specific target language:

```bash
kagi translate "Bonjour tout le monde" --to ja
```

translate only the core text result:

```bash
kagi translate "Bonjour tout le monde" --no-alternatives --no-word-insights --no-suggestions --no-alignments
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

inspect account-level search settings:

```bash
kagi lens list
kagi bang custom list
kagi redirect list
```

inspect or change one lens, bang, or redirect rule:

```bash
kagi lens get "Default"
kagi lens update "Default" --description "primary search profile"
kagi bang custom create "Docs" --trigger docs --template "https://docs.rs/releases/search?query=%s"
kagi bang custom update docs --shortcut-menu
kagi redirect create '^https://old.example.com/(.*)|https://new.example.com/$1'
kagi redirect disable '^https://old.example.com/(.*)|https://new.example.com/$1'
```

## what it looks like

if you want a quick feel for the cli before installing it, this is the kind of output you get from auth setup, quick answer, translate, ask-page, the subscriber summarizer, assistant, and the public news feed:

![quick demo](images/demos/quick.gif)

![summarize demo](images/demos/summarize.gif)

![ask-page demo](images/demos/ask-page.gif)

![assistant demo](images/demos/assistant.gif)

![news demo](images/demos/news.gif)

## building from source

if you are working on the cli itself, build from a local checkout:

```bash
git clone https://github.com/Microck/kagi-cli.git
cd kagi-cli
cargo build --release
./target/release/kagi --help
```

for the fuller install matrix and platform-specific setup, use the [installation guide](https://kagi.micr.dev/guides/installation).

## documentation

- [installation guide](https://kagi.micr.dev/guides/installation)
- [quickstart guide](https://kagi.micr.dev/guides/quickstart)
- [authentication guide](https://kagi.micr.dev/guides/authentication)
- [workflows](https://kagi.micr.dev/guides/workflows)

## contributing

contributions are welcome. please open an issue or pull request on [github](https://github.com/Microck/kagi-cli). see the [issue templates](https://github.com/Microck/kagi-cli/issues/new/choose) for bug reports and feature requests.

## disclaimer

this project is unofficial and not affiliated with, endorsed by, or connected to Kagi Inc. it is an independent, community-built tool.

## license

[mit license](LICENSE)
