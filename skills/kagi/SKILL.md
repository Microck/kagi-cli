---
name: kagi
description: Route Kagi CLI work to version-matched skills for research, page content, Assistant, monitoring, or account configuration. Use when a task involves Kagi but the correct command or specialized Kagi skill is not yet clear.
allowed-tools: Bash(kagi:*)
---

# Kagi CLI

Use this skill as the entry point for `kagi`. Route the task to the narrowest
workflow skill before running substantive commands.

## Route The Task

| User intent | Load |
| --- | --- |
| Find, compare, or verify information from the web | `kagi skills get kagi-research` |
| Read, summarize, question, or translate a page | `kagi skills get kagi-content` |
| Start or continue an Assistant conversation | `kagi skills get kagi-assistant` |
| Run searches in bulk or monitor changes over time | `kagi skills get kagi-monitoring` |
| Configure credentials or account search behavior | `kagi skills get kagi-account-config` |

```bash
kagi skills list
kagi skills get kagi-research
```

If one task crosses boundaries, load only the skills needed for the next action.
For example, research first with `kagi-research`, then load `kagi-content` only
when a result page needs closer reading.

## Shared Rules

1. Run `kagi auth status` before deciding that a command is available.
2. Choose the narrowest command for the requested outcome.
3. Use `--format json` for programmatic parsing.
4. Use `--format toon` for compact LLM context.
5. Use `--format markdown` for prose that a person will read.
6. Run `kagi auth check` when credentials exist but a request fails.
7. Never print credential values.

```bash
kagi auth status
kagi auth check
```

## Completion Criteria

Routing is complete when the narrowest workflow skill has been loaded and its
completion criteria have been satisfied. Do not keep this router in context
when a specialized skill contains all remaining guidance.
