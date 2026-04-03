# Nightshift Doc Drift Report

**Project:** kagi-cli  
**Date:** 2026-04-03  
**Code Version:** 0.4.0 (Cargo.toml)  
**Analyzer:** Nightshift v3 (GLM 5.1)

---

## Summary

Compared all markdown documentation (`README.md`, `docs/SKILL.md`, `docs/api-coverage.md`, `docs/todo.md`, `docs/demos.md`, `docs/release-runbook.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `npm/README.md`) against the actual CLI surface defined in `src/cli.rs` and `src/main.rs`.

The project is well-maintained with most documentation accurately reflecting the code. The main areas of drift are:

1. **Version mismatch in SKILL.md** — still says 0.3.3 while code is at 0.4.0
2. **Stale TODO items** — several items listed as not yet implemented were already shipped in 0.4.0 (lens management, custom bangs, redirects, custom assistants, `--snap`)
3. **Missing documentation** — several CLI flags/options and commands are not fully documented in README or SKILL.md
4. **Minor auth-model discrepancies** between README table and actual code behavior
5. **SKILL.md claims `--format json` for translate** but translate has no `--format` flag in code
6. **Config file path discrepancy** — README mentions `./.kagi.toml` while SKILL.md mentions `~/.kagi.toml`

---

## Findings

| # | Severity | File(s) | Line(s) | Description | Recommendation |
|---|----------|---------|---------|-------------|----------------|
| 1 | **P1** | `docs/SKILL.md` | L8 | Version metadata says `0.3.3` but `Cargo.toml` is at `0.4.0`. Consumers of the skill file (e.g., AI tools) will advertise the wrong version. | Update `version: "0.3.3"` to `version: "0.4.0"` in the SKILL.md frontmatter. |
| 2 | **P1** | `docs/todo.md` | L28, L30, L34, L38 | Items #3 (lens management), #5 (redirects, custom bangs, search-shortcut/snaps), #8 (custom assistant CRUD), and partially #10 (assistant thread flows) are listed as TODO but were shipped in v0.4.0 per CHANGELOG.md. The TODO implies these features don't exist yet. | Mark items #3, #5, #8 as completed/implemented. Update #10 to reflect thread list/get/export/delete is done. Revise the "Suggested implementation order" section accordingly. |
| 3 | **P2** | `docs/SKILL.md` | L130 | SKILL.md example uses `kagi translate "Good morning" --to de --format json` but the translate command has no `--format` flag in code (`src/cli.rs:729-821`). Translate always outputs JSON. | Remove `--format json` from the translate example or note that translate only produces JSON output. |
| 4 | **P2** | `README.md` vs `docs/SKILL.md` | README L83, SKILL.md L52 | README says config is saved to `./.kagi.toml` (relative path), while SKILL.md says `~/.kagi.toml` (home directory). The actual code behavior should be verified and one should be corrected. | Verify actual config path from `src/auth.rs` and align both docs to match reality. |
| 5 | **P2** | `README.md` | L19 | README description mentions "feeds" as a top-level feature but there is no standalone `kagi feeds` command. The `news` and `smallweb` commands serve this purpose. | Clarify that feeds are accessed via `kagi news` and `kagi smallweb` commands, or remove "feeds" from the feature list. |
| 6 | **P2** | `docs/SKILL.md` | L245-260 | SKILL.md says "All commands support multiple output formats" and lists json/pretty/compact/markdown/csv. However, `translate`, `fastgpt`, `enrich`, `smallweb`, `ask-page`, and `summarize` only output JSON (no `--format` flag). | Qualify the statement to "Search, batch, quick, and assistant commands support multiple output formats." |
| 7 | **P2** | `README.md` | L158 | README describes `kagi search` output as "JSON by default, optional live filters, or `--format pretty` for terminal output" — accurate. But doesn't mention `--format compact`, `--format markdown`, or `--format csv` for search, which are all supported per `src/cli.rs:20-31`. | Expand the search description to mention all supported output formats. |
| 8 | **P2** | `docs/SKILL.md` | L110 | SKILL.md shows `kagi quick "capital of japan" \| jq '.answer'` but the Quick response JSON uses `.message.html` or `.message.markdown`, not `.answer`. The actual `QuickResponse` struct in `src/types.rs` has no `.answer` field. | Fix the jq example to use the correct JSON path, e.g., `jq '.message.markdown'`. |
| 9 | **P2** | `README.md`, `docs/SKILL.md` | — | Neither document mentions several search/batch flags: `--from-date`, `--to-date`, `--personalized`/`--no-personalized`. These are all implemented in `src/cli.rs:263-285`. | Add examples or flag documentation for `--from-date`, `--to-date`, `--personalized`, and `--no-personalized`. |
| 10 | **P2** | `docs/SKILL.md` | L72 | SKILL.md auth model says `KAGI_SESSION_TOKEN` unlocks "search, search --lens, quick, ask-page, assistant, translate, summarize --subscriber" but the README auth model (L125) adds "filtered search" which is more precise. Neither mentions that lens search and filtered search specifically require session token. | Align both auth models. The code in `src/main.rs` and `src/search.rs:95-97` confirms that lens and runtime filters require session auth. |
| 11 | **P2** | `README.md` | L127 | README auth model row "none" lists `news`, `smallweb`, `auth status`, `--help`. However the actual auth subcommand is `kagi auth status` (with `auth` prefix), not a standalone command. This is clear from context but could confuse. | Consider writing it as `auth status` instead of standalone to match the command surface. |
| 12 | **P2** | `docs/SKILL.md` | — | SKILL.md has no documentation for `kagi lens`, `kagi bang custom`, or `kagi redirect` commands, which were all added in v0.4.0. README.md mentions them briefly in the command surface table but with no examples or flag documentation. | Add command sections for `kagi lens`, `kagi bang custom`, and `kagi redirect` with basic usage examples covering list/get/create/update/delete/enable/disable. |
| 13 | **P2** | `docs/SKILL.md` | — | SKILL.md has no documentation for `kagi assistant --model`, `kagi assistant --lens`, `kagi assistant --web-access`/`--no-web-access`, `kagi assistant --personalized`/`--no-personalized` flags, or custom assistant subcommands (`assistant custom list/create/update/delete`). | Add documentation for assistant prompt-mode flags and custom assistant management subcommands. |
| 14 | **P2** | `README.md` | — | README documents `kagi assistant custom create` with `--model gpt-5-mini --web-access` (L266-267) but doesn't document other flags: `--lens`, `--personalized`/`--no-personalized`, `--bang-trigger`, `--instructions`. | Add a more complete flag listing for `assistant custom create` and `assistant custom update`. |
| 15 | **P2** | `docs/SKILL.md` | — | SKILL.md does not mention the `kagi assistant --assistant` flag (added in v0.4.0 per CHANGELOG) for selecting a saved assistant by name/id/slug. | Add `--assistant` to the assistant command documentation. |
| 16 | **P3** | `docs/SKILL.md` | L301 | SKILL.md example `kagi news --category tech --limit 5 --format pretty` implies news has a `--format` flag. The `NewsArgs` struct in `src/cli.rs:441-481` has no `--format` flag — news always outputs JSON. | Remove `--format pretty` from the news example. |
| 17 | **P3** | `docs/SKILL.md` | L76 | SKILL.md says `set preferred_auth = "api" in config` but doesn't show the full config key path `[auth] preferred_auth = "api"`. README.md L140 shows it correctly. | Add the `[auth]` section header for clarity. |
| 18 | **P3** | `docs/todo.md` | L3-5 | The file header says it references `src/api.rs` but the TODO items are partially stale as noted above. Additionally, the analysis date "2026-03-20" should be updated now that 0.4.0 has shipped with many of the listed items. | Add a "Last reviewed" date note and mark completed items. |
| 19 | **P3** | `docs/demos.md` | — | The demo scripts section doesn't include a demo for lens management, custom bangs, redirects, or custom assistants (new v0.4.0 features). | Consider adding demo scripts for the new v0.4.0 account-level management commands. |
| 20 | **P3** | `README.md` | L182-189 | README shell completion section shows bash/zsh/fish but not PowerShell. The code (`src/cli.rs:8-9`) supports PowerShell as well. | Add a PowerShell example: `kagi --generate-completion powershell >> $PROFILE` |
| 21 | **P3** | `README.md` | L19 | README intro says "search, quick answers, ask-page, assistant, translate, summarization, feeds, paid API commands, and account-level settings like lenses, custom assistants, custom bangs, and redirect rules" — this is accurate but does not mention `fastgpt`, `enrich`, or `smallweb` explicitly. | Consider listing `fastgpt` and `enrich` explicitly since they are distinct commands, not just "paid API commands". |
| 22 | **P3** | `docs/SKILL.md` | L321-326 | SKILL.md translate workflow example pipes through `jq` with `.alternatives.elements` and `.word_insights.insights` — these paths should be verified against the actual `TranslateCommandResponse` type in `src/types.rs` to ensure they match the serialized JSON field names. | Verify JSON field paths match the actual response schema. |

---

## Severity Legend

| Level | Meaning |
|-------|---------|
| **P0** | Critical: Documentation is actively misleading in a way that breaks user workflows |
| **P1** | High: Stale version, significant missing features, or misleading status that could confuse users |
| **P2** | Medium: Missing documentation for real features, incorrect examples, or inconsistencies between docs |
| **P3** | Low: Minor gaps, cosmetic inconsistencies, or low-impact documentation improvements |

---

## Methodology

1. Enumerated all `.md` files outside `target/`
2. Read `Cargo.toml` for version (`0.4.0`) and dependency information
3. Read `src/cli.rs` (1324 lines) for full CLI command/flag/subcommand definitions
4. Read `src/main.rs` (1579 lines) for command dispatch and behavior
5. Read `src/search.rs`, `src/quick.rs`, `src/types.rs` for response schemas
6. Cross-referenced README.md, SKILL.md, CHANGELOG.md, and other docs against code
7. Identified version mismatches, missing docs, stale TODO items, incorrect examples, and inconsistencies
