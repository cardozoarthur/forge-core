---
name: forge-core
description: Use Forge Core to run autonomous or mixed AI/non-AI workflows with atomic DAGs, cron/wait steps, validation gates, cost reports, notifications, persistence, and controlled self-improvement.
license: MIT
compatibility: codex, opencode
metadata:
  runtime: rust
  cli: forge
---

## What Forge Core Does

Forge Core is an operational runtime, not a chatbot wrapper and not a human-flow builder. Use it when an objective needs to become a persistent execution graph that can mix AI steps, deterministic non-AI steps, scheduled waits/cron and notifications.

## Required Workflow

1. Run `forge plan --goal "<human objective>" --output json`.
2. Inspect the generated atomic tasks and validation rules.
3. Use `forge context --workflow <id> --task <task-id> --budget <bytes> --output json` before giving an agent task-specific context.
4. Run `forge run --workflow <id> --simulate --output json` for a validated dry run of the execution graph.
5. Run `forge validate --workflow <id> --output json` before promotion.
6. Run `forge artifacts --workflow <id> --output json` to inspect generated operational memory.
7. Run `forge improve --workflow <id> --output json` only to generate a controlled experiment. Do not auto-promote without benchmark and validation evidence.

## Safety Rules

- Never mark an execution step complete without validation evidence.
- Do not expose full project history to a task when `forge context` can produce bounded local context.
- Treat model providers as interchangeable execution resources and keep non-AI steps independent from live model calls.
- A notification step can generate an email payload with final workflow costs when that was part of the user's objective.
- Keep self-improvement controlled: experiment, benchmark, compare, then promote only after validation.

## Useful Commands

```bash
forge plan --goal "Create a delivery platform" --output json
forge status --workflow <workflow-id> --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
forge improve --workflow <workflow-id> --output json
```
