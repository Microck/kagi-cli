---
name: kagi-account-config
description: Configure Kagi CLI credentials, profiles, lenses, custom bangs, redirects, site preferences, and account-backed assistants. Use when the user wants to inspect or change how Kagi authenticates, searches, or applies account settings.
allowed-tools: Bash(kagi:*)
---

# Kagi Account Configuration

Inspect current state before changing it. Never print, echo, or paste stored
credential values into output.

## Credential model

| Credential | Main capabilities |
| --- | --- |
| `KAGI_SESSION_TOKEN` | subscriber search, Quick Answer, Assistant, ask-page, translation, subscriber summarization |
| `KAGI_API_KEY` | current Search API and Extract API |
| `KAGI_API_TOKEN` | legacy summarize, FastGPT, and Enrichment APIs |

Environment variables override `.kagi.toml`. `--profile NAME` selects a named
profile. Base search prefers session auth when both search credentials are
available, unless `preferred_auth = "api"` is configured.

## Inspect and verify auth

```bash
kagi auth status
kagi auth check
```

Use `status` to see credential presence and source. Use `check` when a
configured credential fails at request time.

Use the auth wizard or explicit auth subcommands to set credentials:

```bash
kagi auth
kagi auth set --session-token "https://kagi.com/search?token=..."
```

Never pass secrets on a shared shell command line. Prefer the interactive
wizard or protected environment injection in shared environments.

## Profiles

Use profiles when separate accounts or credential policies must coexist.

```bash
kagi --profile work auth status
kagi --profile personal search "query" --format toon
```

Verify the selected profile before changing any setting.

## Search settings

Inspect before creating, updating, or deleting:

```bash
kagi lens list
kagi lens get "Default"
kagi bang custom list
kagi redirect list
kagi site-pref list
```

Example custom bang:

```bash
kagi bang custom create \
  "Docs" \
  --trigger docs \
  --template "https://docs.rs/releases/search?query=%s"
```

Use each command's `--help` for the flags a mutation needs. Never delete or
overwrite an account setting unless the user asked for it.

## Custom assistants

```bash
kagi assistant custom list
kagi assistant custom get "Researcher"
```

Conversation and thread use belongs in `kagi-assistant`. This skill owns the
account-level configuration of custom assistants.

## Completion criteria

Configuration work is complete when:

- you know the active profile and credential source;
- the requested setting is visible through its read command;
- no secret value appears in output or persisted scripts;
- unrelated account settings remain unchanged; and
- you made a destructive change only when the user asked for it.
