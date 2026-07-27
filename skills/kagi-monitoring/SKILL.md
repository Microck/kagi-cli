---
name: kagi-monitoring
description: Automate repeated Kagi queries with batch search, watches, notifications, history, caching, and MCP. Use when the user needs bulk processing, scheduled discovery, change detection, or a stable integration surface.
allowed-tools: Bash(kagi:*)
---

# Kagi Monitoring and Automation

Build repeatable workflows from structured Kagi output. Keep stdout
machine-readable and send progress or diagnostics to stderr.

## Choose the workflow

| Need | Command |
| --- | --- |
| Run independent queries together | `kagi batch` |
| Detect changes in results | `kagi watch` |
| Deliver a result or alert | `kagi notify` |
| Inspect prior local activity | `kagi history` |
| Expose Kagi tools to an agent | `kagi mcp` |

Run `kagi auth status` before automating authenticated commands.

## Batch search

```bash
kagi batch "rust" "zig" "go" --format toon --limit 3
printf 'rust\nzig\ngo\n' | kagi batch --format compact
```

Use argument queries for short fixed sets and stdin for generated lists. Keep
the query list as the recoverable input so a failed run can be repeated.

## Watch search results

```bash
kagi watch "site:example.com release notes" --interval 300
```

Define what counts as a meaningful change before starting a long-running
watch. Use a descriptive query and an interval that matches how often the
source updates.

## Notifications

Use `kagi notify --help` to select the configured delivery target, then connect
it to a watch or batch result. Never put secrets in notification text or
command history.

## History and caching

Use `kagi history` to inspect local command records before duplicating work.
Use `--local-cache` only for calls where stale data is acceptable. Never cache
research that must reflect a current price, release, outage, or policy.

## MCP

Use the stdio server when another agent host needs Kagi tools:

```bash
kagi mcp
```

Keep MCP stdout reserved for protocol messages. Treat a log written to stdout
as a protocol-breaking bug.

## Automation rules

- Prefer `json`, `compact`, or `toon` over `pretty`.
- Preserve nonzero exit codes instead of turning failure into empty output.
- Keep credentials in Kagi auth storage or environment variables, never scripts.
- Limit concurrency to what the endpoint and account can sustain.
- Record enough input to reproduce a failed item.
- Use a process supervisor or scheduler for long-running watches.

## Completion criteria

Automation is complete when:

- inputs and outputs have stable machine-readable shapes;
- failures remain observable and retryable;
- credential values never appear in scripts or logs;
- cache and polling choices match the freshness requirement; and
- long-running processes have a clear owner and stop condition.
