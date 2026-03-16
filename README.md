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

# kagi

## overview

`kagi` is a cli for people who want kagi in the terminal without juggling a bunch of different setup paths.

the main nice part is the session-link flow: you can paste your full kagi session-link url directly into `kagi auth set --session-token`, and the cli pulls out the `token=` value for you. that gives you the subscriber features people usually care about most, like lens search, assistant, and subscriber summarizer.

if you also have kagi api access, `kagi` can use that too for the paid public api commands like public summarizer, fastgpt, and enrich.

## quickstart

### install

macos and linux:

```bash
curl -fsSL https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.sh | sh
kagi --help
```

windows powershell:

```powershell
irm https://raw.githubusercontent.com/Microck/kagi-cli/main/scripts/install.ps1 | iex
kagi --help
```

or install through a package manager:

```bash
brew tap Microck/kagi
brew install kagi
npm install -g kagi-cli
pnpm add -g kagi-cli
bun add -g kagi-cli
kagi --help
```

the npm package is `kagi-cli`, but the command you run is `kagi`.

windows scoop:

```powershell
scoop bucket add kagi https://github.com/Microck/scoop-kagi
scoop install kagi
kagi --help
```

### session-link url setup

this is the path most people will want.

```bash
kagi auth set --session-token 'https://kagi.com/search?token=...'
kagi auth check
kagi search --pretty "rust lang"
kagi search --lens 2 "rust lang"
kagi assistant "reply with the word pear."
kagi summarize --subscriber --url https://www.rust-lang.org/
```

`kagi auth set` saves the token in `.kagi.toml`, and it accepts either the raw token or the full session-link url.

if you prefer env vars, this works too:

```bash
export KAGI_SESSION_TOKEN='...'
```

### api token setup

if you use kagi's paid public api, add an api token as well:

```bash
export KAGI_API_TOKEN='...'
kagi auth check
kagi summarize --url https://example.com
kagi fastgpt "python 3.11"
kagi enrich web "rust lang"
```

## what you can do

- search kagi from the terminal with json by default or `--pretty` when you want nicer human output
- use your subscriber session for lens-aware search with `kagi search --lens <INDEX> "query"`
- run the subscriber summarizer with `kagi summarize --subscriber --url <URL>`
- talk to kagi assistant with `kagi assistant "prompt"`
- read public feeds like `kagi news` and `kagi smallweb` without any auth
- use paid api commands like `fastgpt`, public `summarize`, and `enrich` when you have `KAGI_API_TOKEN`

some quick examples:

```bash
kagi news --category world --limit 3
kagi smallweb --limit 3
kagi search "rust lang"
kagi search --pretty "rust lang"
kagi news --list-categories
kagi news --chaos
```

## auth

`kagi` supports two credential types:

- `KAGI_SESSION_TOKEN` is the best default if you want subscriber features. it powers lens search, subscriber summarizer, and assistant, and `kagi auth set --session-token` accepts the full session-link url directly.
- `KAGI_API_TOKEN` is for the paid public api path, including public summarizer, fastgpt, and enrich.

small notes that matter:

- environment variables win over `.kagi.toml`
- base search can use either token path
- `news` and `smallweb` do not need auth

## for automation

the README is user-first, but the cli still works well in scripts and agents.

stdout is json by default, and `--pretty` only changes how results are rendered in the terminal. the command surface stays the same either way.

## more docs

- [installation guide](guides/installation.mdx)
- [authentication guide](guides/authentication.mdx)
- [workflows](guides/workflows.mdx)
- [auth command](commands/auth.mdx)
- [search command](commands/search.mdx)
- [summarize command](commands/summarize.mdx)
- [assistant command](commands/assistant.mdx)
- [news command](commands/news.mdx)
- [smallweb command](commands/smallweb.mdx)

## project links

- [contributing](CONTRIBUTING.md)
- [support](SUPPORT.md)
- [security](SECURITY.md)
- [license](LICENSE)
