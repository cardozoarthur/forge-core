---
name: foundry-core
description: Lightweight Foundry Core entrypoint. Load the domain skill that matches the node before loading detailed instructions.
license: MIT
compatibility: codex, opencode, agy, claude
metadata:
  runtime: rust
  cli: foundry
---

## What Foundry Core Does

Foundry Core is the workflow orchestration authority. Use `foundry plan --goal "<objective>" --output json` to turn work into an auditable workflow, then load only the domain skill needed by the current node.

## Required First Steps

1. Run `foundry plan --goal "<human objective>" --output json`.
2. Use `foundry context --workflow <id> --task <task-id> --project-root <project-root> --budget <bytes> --strict --view compact --output json` before giving an agent task-specific context.
3. Use `foundry validate --workflow <id> --output json` before promotion.

## Domain Skill Index

- `foundry-core-runtime`: durable workflows, request lifecycle, handoff, schedules, validation and rework.
- `foundry-core-missions`: persistent squad missions, strict context dispatch, execution receipts, submit/resume, reconciliation, and promotion evidence.
- `foundry-core-context`: bounded context, memory policy/search, deferred discovery and node-scoped context routing.
- `foundry-core-artifacts`: workflow artifacts, tags, documents, reports, fetch/list and lineage.
- `foundry-core-executors`: brains, sessions, CLI integration through the Foundry-owned `foundry harness` namespace, executor quota, `ai-limits`, CLI factory and model fallback.
- `foundry-core-workspaces`: Git worktree registration/binding, `.foundry/worktree.toml`, path guardrails, blocking predecessor tasks and preview/test sandboxes.
- `foundry-core-addons-ui`: Addons, renderer events, operational TUI/web surfaces and interactive panels.
- `foundry-core-documentation`: standards for documenting workflows, tasks, and nodes, including schemas and contract definitions.
- `foundry-core-agent`: agent configuration, brain/soul profiles, executor options, and adapter credentials/quotas.
- `foundry-core-workflow`: creating workflows, updating context, attaching artifacts, managing tasks/subtasks, prioritization, and dependencies/impediments.

Do not load every domain skill by default. Load the smallest skill matching the current workflow node, then ask Foundry for explicit expansion commands when more information is required.

## Skill Modularity Rule

Do not grow Foundry into a giant all-purpose skill. Keep the entrypoint and each domain skill compact. When a skill starts covering multiple domains, unrelated behaviors or a long command encyclopedia, split it into smaller domain skills or single-function skills and keep this entrypoint as the router.
