---
name: kagi-ai
description: "Use Kagi's paid AI allowance for reasoning, coding, research, file analysis, structured generation, or model comparison. Use when a task should spend Assistant value instead of only searching the web or transforming one known page."
allowed-tools: Bash(kagi:*) Skill(kagi:*)
---

# Kagi AI

Spend the Assistant allowance on reasoning and synthesis. When the active plan
includes unlimited Summarizer or Translate usage, use those tools for direct
transformations and reserve the allowance for harder work.

## Choose the lane

| Need | Lane |
| --- | --- |
| Find or verify sources | Load `kagi-usage` and search |
| Summarize one URL or text | `kagi summarize --subscriber` |
| Translate supplied text | `kagi translate` |
| Get a fast cited answer | `kagi quick` |
| Reason, code, synthesize, or analyze files | `kagi assistant` |
| Run multi-step research or tool use | An account-visible Research assistant |
| Manage threads or custom assistants | Load `kagi-assistant` |

The Assistant allowance is separate from API credit. A plan's monthly value
sets its token allowance across Assistant models. Premium models, web access,
large files, and long thread histories consume more of that allowance.

## Start with live account state

```bash
kagi auth status
kagi assistant models
kagi assistant custom list
```

Use only model slugs and assistant names returned by the account. When the
model catalog is empty, use the account default instead of guessing a slug.
Kagi Quick and Kagi Research are usable through `--assistant` only when they
appear in the assistant listing.

## Use Assistant deliberately

Use a closed model call for reasoning that needs no current information:

```bash
kagi assistant \
  "find the weakest assumption in this design and propose a simpler one" \
  --no-web-access \
  --format markdown
```

Enable web access when the answer needs current sources:

```bash
kagi assistant \
  "compare these database options for this workload and cite current limits" \
  --web-access \
  --format markdown
```

Choose a live model explicitly only when its cost, speed, or capability matters:

```bash
kagi assistant \
  "review this migration plan for data-loss risks" \
  --model MODEL_SLUG \
  --no-web-access \
  --format markdown
```

For a model comparison, send the same bounded prompt to each model in a fresh
thread. Compare correctness, latency, and usefulness, not prose style alone.

## Analyze files

Attach only the files needed for the answer. Assistant accepts documents,
spreadsheets, images, and audio; one upload can be at most 30 MB.

```bash
kagi assistant \
  --attach ./report.pdf \
  "extract the decision, evidence, risks, and unresolved questions" \
  --no-web-access \
  --format markdown
```

Uploaded content remains in the thread context. Start a new thread when the
next task should not inherit it.

## Require structured output

Use a built-in contract for reusable decisions, plans, and checklists:

```bash
kagi assistant \
  "decide whether this service is ready to launch" \
  --contract decision \
  --format json
```

Use `--contract-file PATH` when the consumer needs a different small JSON
shape. Contract mode validates the response and makes one repair attempt.

## Budget discipline

1. Use subscriber Summarizer or Translate for direct transformations when the
   active plan includes them.
2. Use the account default or a lower-cost model for simple work.
3. Disable web access for closed-context work.
4. Keep prompts specific and attach only relevant files.
5. Start a new thread for an unrelated task.
6. Reserve Research assistants and premium models for work that needs their
   extra tools or reasoning depth.

## Completion criteria

AI work is complete when:

- the selected lane matches the task;
- every explicit model or assistant came from live account discovery;
- web access and attachments are limited to what the answer needs;
- structured output passes its contract when a machine will consume it;
- the response answers the task instead of spending allowance on avoidable
  transformations; and
- you changed or deleted account state only when the user asked.
