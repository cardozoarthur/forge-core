---
name: forge-core
description: Use Forge Core to run autonomous or mixed AI/non-AI workflows with goal-oriented atomic DAGs, executor sync, validation gates, cost reports, notifications, persistence, rework loops, and controlled self-improvement.
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
2. For skill-style use, prefer `forge request start --goal "<objective>" --origin codex|opencode|skill --output json` and return the `run_id` to the caller.
3. Run `forge sync all --home "$HOME" --output json` when executor or runtime availability may have changed.
4. Inspect the generated atomic tasks, task goals, subtasks, impediments, async policy and validation rules.
5. Use `forge workflow update-goal ... --origin codex|opencode|forge_cli|skill` when the human changes direction during execution.
6. Use `forge workflow attach-artifact ... --origin codex|opencode|forge_cli|skill` when new artifacts appear during execution.
7. Use `forge context --workflow <id> --task <task-id> --budget <bytes> --strict --output json` before giving an agent task-specific context.
8. Run `forge validate --workflow <id> --output json` before promotion. If `rework_tasks` is not empty, return those tasks to work.
9. Run `forge improve --workflow <id> --target-version <version> --output json` only to generate a controlled experiment and changelog. Do not auto-promote without benchmark and validation evidence.

## MCP Agent Surface

- Use `forge mcp tools --output json` to discover stable agent-facing tools before wiring a Codex/OpenCode workflow.
- For async handoff, call `forge mcp call forge.run.start --input '{"goal":"<objective>","origin":"codex"}' --output json`, return `result.run_id` quickly, and let Forge remain the source of truth.
- Poll later with `forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json`.
- Resume a paused async handoff with `forge mcp call forge.run.resume --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Inspect or route work through `forge.workflow.inspect`, `forge.context.request`, `forge.workflow.attach_artifact`, `forge.workflow.update_goal`, `forge.validation.status` and `forge.artifact.fetch`.
- MCP mutations must still go through Forge so revisions, artifact hashes, origins and validation gates are persisted.

## Safety Rules

- Never mark an execution step complete without validation evidence.
- Never treat task output as enough by itself. The task goal must be definitively ready.
- Do not use detected CLIs until `forge sync executors` has persisted human authorization for them.
- Treat Docker/Kubernetes/Knative as run substrates. Do not install or mutate them without explicit authorization.
- Only mutate Forge-owned runtime resources by default. External resources require a positive `forge runtime guard` decision with explicit authorization.
- Runtime goal/artifact changes must go through Forge so revisions and origins are persisted.
- When Codex/OpenCode use Forge as a skill, they should not wait for long work inline. They should start a request, return `run_id`, and let Forge continue asynchronously.
- Do not expose full project history to a task when `forge context` can produce bounded local context.
- Treat model providers as interchangeable execution resources and keep non-AI steps independent from live model calls.
- A notification step can generate an email payload with final workflow costs when that was part of the user's objective.
- Keep self-improvement controlled: experiment, benchmark, compare, then promote only after validation.

## Useful Commands

```bash
forge plan --goal "Create a delivery platform" --output json
forge request start --goal "Improve Forge Core" --origin codex --output json
forge request status --run <run-id> --output json
forge request resume --run <run-id> --origin codex --output json
forge mcp tools --output json
forge mcp call forge.run.start --input '{"goal":"Improve Forge Core","origin":"codex"}' --output json
forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json
forge sync all --home "$HOME" --allow codex --allow opencode --output json
forge executors --output json
forge runtimes --output json
forge workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
forge workflow attach-artifact --workflow <workflow-id> --path ./artifact.md --kind report --origin opencode --output json
forge mcp call forge.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"./artifact.md","kind":"report","origin":"codex"}' --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"task-001","budget":1200}' --output json
forge runtime guard --substrate knative --resource service/forge-node --namespace forge --action update --owner forge --output json
forge list --output json
forge status --workflow <workflow-id> --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --strict --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
forge improve --workflow <workflow-id> --target-version 0.3.0 --output json
forge self run --repo /home/arthur/projects/forge-core --until 2026-05-25T10:00:00-03:00 --executor codex --executor opencode --max-cycles 1 --output json
```
