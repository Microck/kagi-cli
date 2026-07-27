---
name: kagi-assistant
description: Use Kagi Assistant for conversational answers, thread continuity, attachments, streaming, model selection, or custom assistants. Use when the task benefits from an account-backed conversation rather than direct search or page extraction.
allowed-tools: Bash(kagi:*)
---

# Kagi Assistant

Use Assistant for conversation mechanics and continuity. Use `kagi-ai` to
choose when to spend AI allowance, and `kagi-usage` for direct search or page
work.

Assistant commands require `KAGI_SESSION_TOKEN`.

## Start a conversation

```bash
kagi auth status
kagi assistant "draft a migration checklist" --format markdown
kagi assistant "compare these options" --format toon
```

State the deliverable, constraints, and audience in the prompt. Never rebuild
the output structure in follow-up shell processing when the Assistant can
produce it directly.

## Continue a thread

```bash
kagi assistant --thread-id THREAD_ID "add rollback steps"
kagi assistant thread list
kagi assistant thread get THREAD_ID
kagi assistant thread export THREAD_ID --format markdown
```

Reuse a thread only when its prior context still belongs to the task. Start a
new thread when stale context could bias the answer.

Delete a thread only when the user asks for it:

```bash
kagi assistant thread delete THREAD_ID
```

## Attach local context

```bash
kagi assistant \
  --attach ./notes.md \
  "turn these notes into a decision record" \
  --format markdown
```

Attach only files the task needs. Name the file's role in the prompt so the
Assistant knows whether it is evidence, a template, or background.

## Stream responses

Use human streaming for interactive terminal work. Use structured streaming
when another program consumes events.

```bash
kagi assistant --stream "explain the tradeoffs"
kagi assistant \
  --stream \
  --stream-output json \
  "produce a release checklist"
```

Never parse human streaming output as a machine contract.

## Custom assistants

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

## Completion criteria

Assistant work is complete when:

- you used the correct thread or a clean new one;
- you supplied the required files and constraints;
- the output format matches its consumer;
- the final response answers the current prompt, not stale thread context; and
- you deleted a thread only when the user asked for it.
