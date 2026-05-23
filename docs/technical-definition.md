# Forge Core Technical Definition

Forge Core is a workflow runtime that transforms large objectives into validated, context-controlled atomic execution graphs.

Forge Core is less human-dependent than ForgeFlow. ForgeFlow focuses on product creation workflows with explicit human decision paths. Forge Core focuses on executing operational graphs that can run with AI, without AI or with both.

## Runtime Authority

Forge Core should not be treated only as a plugin or skill that adds capability to another agent. A plugin runs inside the agent's operational model. Forge is intended to own the operational model.

Forge owns:

- objective decomposition;
- atomic task graph state;
- context minimization;
- task scheduling and cron/wait continuation;
- validation gates;
- retries and recovery policy;
- artifacts and operational memory;
- workflow cost accounting;
- promotion and self-improvement gates.

Codex, OpenCode, Gemini CLI, Claude Code, Ollama and other engines should be usable as execution targets. They receive bounded task packets and return structured results. Forge decides what context they receive, what they are allowed to do, how their output is validated and whether the workflow can advance.

Close coupling is still valuable when it reduces friction. The target architecture supports both directions:

- CLI calls Forge: interactive agents use Forge commands/skills to create, inspect and validate workflow state.
- Forge calls CLI: Forge launches an executor adapter for long-running or specialized tasks.
- Native CLI integration: open-source CLIs may embed Forge-backed orchestration paths while still leaving Forge as the source of truth for workflow state.

## Core Modules

- Intent parser: extracts goal, constraints, deliverables, risks and unknowns.
- Requirement extractor: normalizes the objective into measurable execution needs.
- Workflow fragmentation engine: produces atomic retryable tasks.
- Atomic task graph: keeps dependency-aware execution state.
- Context controller: injects only task-local context under a byte budget.
- Execution runtime: coordinates task execution and trace collection.
- Scheduled execution: represents future continuation with cron/wait tasks.
- Non-AI execution: runs deterministic command-style steps without requiring a live model call.
- Notification execution: creates final notification payloads such as email cost reports.
- Validation engine: blocks invalid promotion.
- Artifact system: stores reusable outputs with stable paths and hashes.
- Operational memory: persists workflows, events and generated artifacts.
- Self-improvement loop: generates experimental changes without unrestricted promotion.

## v0 Boundary

The first version is a local Rust CLI and skill package. It includes SQLite persistence, simulated execution, AI/non-AI/wait/notification task kinds, cost report generation and controlled improvement artifacts. It does not yet include distributed execution, provider adapters, SaaS UI or WASM plugins.

## Executor Contract Direction

Executor integrations should converge on a bounded packet:

```json
{
  "workflow_id": "wf_...",
  "task_id": "task-...",
  "executor": "codex|opencode|gemini|claude|ollama|command",
  "objective": "Implement JWT middleware",
  "allowed_context": [],
  "artifact_refs": [],
  "validation_rules": [],
  "expected_output": "",
  "cost_budget": {
    "max_usd": 0.0,
    "max_tokens": 0
  }
}
```

The executor response should be structured enough for validation, cost reporting and replay:

```json
{
  "task_id": "task-...",
  "status": "completed|failed|needs_retry",
  "artifacts": [],
  "trace_ref": "",
  "cost": {
    "estimated_usd": 0.0,
    "tokens_in": 0,
    "tokens_out": 0
  },
  "validation_evidence": []
}
```

## Validation Contract

A workflow is only promotable when all tasks are completed and validation rules pass. Until then, `forge validate` returns a blocked status and non-zero exit code.

Self-improvement is intentionally conservative. `forge improve` generates an experiment artifact and does not auto-promote.
