---
name: kagi-assistant
description: Use Kagi Assistant for conversational answers, thread continuity, attachments, streaming, model selection, or custom assistants. Use when the task benefits from an account-backed conversation rather than direct search or page extraction.
allowed-tools: Bash(kagi:*)
---

# Kagi Assistant

Use Assistant for conversational synthesis and continuity. Use
`kagi-research` for source discovery and `kagi-content` for one known page.

Assistant commands require `KAGI_SESSION_TOKEN`.

## Start A Conversation

```bash
kagi auth status
kagi assistant "draft a migration checklist" --format markdown
kagi assistant "compare these options" --format toon
```

State the desired deliverable, constraints, and audience in the prompt. Do not
hide required output structure in follow-up shell processing when the Assistant
can produce it directly.

## Continue A Thread

```bash
kagi assistant --thread-id THREAD_ID "add rollback steps"
kagi assistant thread list
kagi assistant thread get THREAD_ID
kagi assistant thread export THREAD_ID --format markdown
```

Reuse a thread only when its prior context still belongs to the task. Start a
new thread when stale context could bias the answer.

Delete a thread only when the user asked for deletion:

```bash
kagi assistant thread delete THREAD_ID
```

## Attach Local Context

```bash
kagi assistant \
  --attach ./notes.md \
  "turn these notes into a decision record" \
  --format markdown
```

Attach only files needed for the task. Mention the file's role in the prompt so
the Assistant knows whether it is evidence, a template, or background.

## Stream Responses

Use human streaming for interactive terminal work. Use structured streaming
when another program consumes events.

```bash
kagi assistant --stream "explain the tradeoffs"
kagi assistant \
  --stream \
  --stream-output json \
  "produce a release checklist"
```

Do not parse human-oriented streaming output as a machine contract.

## Custom Assistants

```bash
kagi assistant custom list
kagi assistant custom get "Researcher"
kagi assistant custom create \
  "CLI Researcher" \
  --web-access \
  --model gpt-5-mini
```

Inspect an existing custom assistant before changing the workflow around it.
Account configuration belongs in `kagi-account-config`.

## Completion Criteria

Assistant work is complete when:

- the correct thread or a clean new thread was used;
- required files and constraints were supplied;
- output format matches its consumer;
- the final response answers the current prompt, not stale thread context; and
- destructive thread actions occurred only with explicit user intent.
