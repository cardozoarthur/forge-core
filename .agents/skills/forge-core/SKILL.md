---
name: forge-core
description: Lightweight Forge Core entrypoint. Load the domain skill that matches the node before loading detailed instructions.
license: MIT
compatibility: codex, opencode, agy, claude
metadata:
  runtime: rust
  cli: forge
---

## What Forge Core Does

Forge Core is the workflow orchestration authority. Use `forge plan --goal "<objective>" --output json` to turn work into an auditable workflow, then load only the domain skill needed by the current node.

## Required First Steps

1. Run `forge plan --goal "<human objective>" --output json`.
2. Use `forge context --workflow <id> --task <task-id> --project-root <project-root> --budget <bytes> --strict --output json` before giving an agent task-specific context.
3. Use `forge validate --workflow <id> --output json` before promotion.

## Domain Skill Index

- `forge-core-runtime`: durable workflows, request lifecycle, handoff, schedules, validation and rework.
- `forge-core-context`: bounded context, memory policy/search, deferred discovery and node-scoped context routing.
- `forge-core-artifacts`: workflow artifacts, tags, documents, reports, fetch/list and lineage.
- `forge-core-executors`: brains, sessions, harness, executor quota, `ai-limits`, CLI factory and model fallback.
- `forge-core-addons-ui`: Addons, renderer events, operational TUI/web surfaces and interactive panels.
- `forge-core-documentation`: standards for documenting workflows, tasks, and nodes, including schemas and contract definitions.
- `forge-core-agent`: agent configuration, brain/soul profiles, executor options, and adapter credentials/quotas.
- `forge-core-workflow`: creating workflows, updating context, attaching artifacts, managing tasks/subtasks, prioritization, and dependencies/impediments.

Do not load every domain skill by default. Load the smallest skill matching the current workflow node, then ask Forge for explicit expansion commands when more information is required.

## Skill Modularity Rule

Do not grow Forge into a giant all-purpose skill. Keep the entrypoint and each domain skill compact. When a skill starts covering multiple domains, unrelated behaviors or a long command encyclopedia, split it into smaller domain skills or single-function skills and keep this entrypoint as the router.
