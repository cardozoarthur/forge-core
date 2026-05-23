# Forge Core Technical Definition

Forge Core is a workflow runtime that transforms large objectives into validated, context-controlled atomic execution graphs.

Forge Core is less human-dependent than ForgeFlow. ForgeFlow focuses on product creation workflows with explicit human decision paths. Forge Core focuses on executing operational graphs that can run with AI, without AI or with both.

## Runtime Authority

Forge Core should not be treated only as a plugin or skill that adds capability to another agent. A plugin runs inside the agent's operational model. Forge is intended to own the operational model.

Forge owns:

- objective decomposition;
- explicit goal hierarchy;
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
- Workflow fragmentation engine: produces atomic retryable tasks with explicit goals.
- Work item controller: tracks backlog state, subtasks, impediments, owner role, acceptance criteria and definition of done.
- Atomic task graph: keeps dependency-aware execution state.
- Context routing engine: compresses, summarizes, selects, versions and shards the minimum correct context for each executor under a budget.
- Execution runtime: coordinates task execution and trace collection.
- Executor policy: detects installed/configured CLIs and persists human authorization before use.
- Runtime substrate policy: detects Docker/Kubernetes/Knative and persists human authorization before use.
- Scheduled execution: represents future continuation with cron/wait tasks.
- Non-AI execution: runs deterministic command-style steps without requiring a live model call.
- Notification execution: creates final notification payloads such as email cost reports.
- Validation engine: blocks invalid promotion.
- Artifact system: stores reusable outputs with stable paths and hashes.
- Operational memory: persists workflows, events and generated artifacts.
- Self-improvement loop: generates experimental changes without unrestricted promotion.

## v0 Boundary

The current version is a local Rust CLI and skill package. It includes SQLite persistence, simulated execution, AI/non-AI/wait/notification task kinds, executor sync/policy, runtime substrate sync/policy, goal-oriented work items, rework validation, runtime goal/artifact mutation, cost report generation and controlled improvement artifacts with changelog generation. It does not yet include distributed execution, real provider adapters, SaaS UI or WASM plugins.

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

## Goal-Oriented Work Contract

Every task and subtask must have a goal. A task is not promotable just because an executor returned output. Forge must evaluate whether the task is definitively ready.

The task work item includes:

- `goal`;
- `backlog_state`;
- `subtasks`;
- `impediments`;
- `acceptance_criteria`;
- `goal_validation.evidence_required`;
- `goal_validation.definitively_ready`;
- `goal_validation.rework_policy`.

If goal evidence is missing, validation reports `goal_readiness` failures and returns rework tasks. The workflow must go back to work instead of advancing as if it were complete.

## Executor Sync Contract

On install and on every sync, Forge should inspect known execution CLIs:

- Codex;
- OpenCode;
- Gemini;
- Claude;
- Ollama.

Forge records whether each CLI is installed and configured. Installed/configured does not mean usable. A CLI becomes usable only after a human explicitly allows it. The local policy is persisted in SQLite.

When Codex and OpenCode are both authorized, Forge records the `opencode_codex_bridge` integration so OpenCode and Codex can be coordinated as bounded execution engines.

## Runtime Substrate Contract

Forge separates cognitive executors from run substrates.

Cognitive executors:

- Codex;
- OpenCode;
- Gemini;
- Claude;
- Ollama.

Run substrates:

- Docker;
- Kubernetes;
- Knative.

Run substrates can execute asynchronous workflow nodes. They still require human authorization before use.

If Docker and Kubernetes are available but Knative is missing, Forge may suggest Knative installation. It must not install Knative or mutate a cluster without explicit user authorization.

## Resource Ownership Contract

Forge must not mutate resources outside its ownership scope.

Allowed without extra approval:

- create Forge-owned resources;
- update Forge-owned resources;
- delete Forge-owned resources.

Blocked without explicit approval:

- update pre-existing Docker/Kubernetes/Knative resources;
- delete pre-existing Docker/Kubernetes/Knative resources;
- patch resources that belong to another app, namespace or context.

Forge-owned resources should be labeled or recorded with ownership metadata. Until real substrate adapters exist, `forge runtime guard` provides the policy decision as a testable contract.

## Runtime Mutation Contract

Workflows are not frozen snapshots. Goals and artifacts can change while execution is active.

Mutation rules:

- every goal change records origin and revision;
- every artifact attachment copies the artifact into Forge workflow storage;
- origins can be `forge_cli`, `codex`, `opencode`, `skill` or another future adapter;
- mutation must not bypass validation;
- downstream tasks must see updated goals/artifacts through Forge context packages.

Codex CLI and OpenCode CLI are therefore human interfaces for Forge as well as possible executor adapters. They can update Forge state through CLI commands while Forge remains the persistent source of truth.

## Context Routing Engine

The context routing engine is a primary Forge differentiator. Forge should not pass broad project history to every executor. It should build minimal, correct context packets.

Responsibilities:

- compress large context into task-relevant summaries;
- select only the files, artifacts, decisions and constraints required by the current task;
- version context packets so executor results can be reproduced;
- shard context by task, subflow, artifact and validation gate;
- avoid redundant reasoning by reusing validated summaries and prior artifacts;
- reduce model cost and hallucination risk by excluding irrelevant history.

The goal is not simply smaller prompts. The goal is maximum relevance with traceable context lineage.

Current `forge context` packets use schema `forge.context.v2` and routing policy
`task_local_revisioned_budget_v2`. Each packet includes the executor-facing content,
the full context checksum, workflow revision, artifact count, lineage hashes, included
and omitted sections, and a deterministic shard manifest with source, priority, byte
count, summary and shard checksum. Runtime goal and artifact mutations are part of
the context lineage, which gives long-running executors a deterministic stale-context
signal while leaving room for persisted summaries, artifact shards and subflow-aware
routing in later versions.

## Personality/Soul Routing

Some workflow outputs are not only machine artifacts. Reports, research summaries,
strategy documents, teaching material and operator updates are read by humans, so
Forge should be able to route a node through an explicit personality, voice or
"soul" profile when that improves clarity.

This capability must remain operationally bounded:

- the persona is a node-level execution setting, not hidden global behavior;
- the context packet records which persona profile was selected and why;
- Codex-style developer/personality instructions and Paperclip-style soul, voice,
  tone or persona models are inputs to the profile contract;
- the persona switch is included in lineage so results are replayable;
- validation gates can reject artifacts that drift away from the requested role,
  audience, constraints or factual content.

The intent is to improve human-facing artifacts without letting personality override
Forge goals, validation rules, safety constraints or source-of-truth state.

## Deterministic + AI Hybrid Graph

Forge workflows should mix AI and non-AI execution in one graph.

Supported graph node classes should include:

- AI executor tasks;
- deterministic local code tasks;
- Python or Node.js code nodes for repeated/frequent logic that does not need model reasoning;
- waits and cron continuation;
- approvals;
- validation gates;
- rollback;
- deployment;
- notifications and cost reports.

Forge should decide whether a node needs AI. If the work is stable, repeated or high-volume, Forge should prefer a deterministic local code node over a model call.

## Long-Running Cognition

Forge must make cognition durable over time.

Long-running workflow support should include:

- pause/resume;
- async continuation;
- durable execution records;
- checkpointing;
- partial retry from the failed node or subflow;
- resumable context packets;
- run state that survives crashes, CLI restarts and executor changes.

## Workflow Registry, Inspect And Subflows

Forge must expose the workflow registry as an operational runtime surface, not only as raw SQLite state.

Required user-facing goals:

- `forge list` lists workflows/runs that are currently running and workflows/runs that are not running.
- Each list row includes a stable id, lifecycle state and the original initial request description, even after later goal mutations.
- Non-infinite workflows should scale to zero when no runnable or scheduled work remains.
- Infinite workflows and infinite subflows remain eligible for scheduling instead of being treated as completed one-shot graphs.
- `forge inspect <id>` renders the workflow graph in the terminal.
- `forge inspect <id> --verbose` includes subflows and a description of each process and subprocess/subflow.
- Workflows may contain subflows recursively. A flow can own many subflows, and each subflow can own many child subflows.
- Subflows can be finite or infinite. Infinite subflows require explicit lifecycle metadata so Forge can distinguish "idle but alive" from "completed".
- Running workflows must remain mutable: list gives stable ids, inspect shows the current graph, and goal/artifact mutations appear as revisions.

Before creating a new workflow from scratch, Forge should inspect available workflows and reusable flow definitions. If an existing flow can satisfy part of the new objective, Forge should propose or attach it as a child subflow instead of duplicating orchestration logic.

## Async Request Contract

When Codex/OpenCode use Forge as a skill, they should not hold the user interaction open for long-running work.

The preferred flow is:

```text
Codex/OpenCode/skill
→ forge request start
→ receives run_id
→ returns run_id to human
→ Forge continues asynchronously
→ human/agent checks forge request status later
```

`run_id` is distinct from `workflow_id`. The workflow is the operational graph; the run is the asynchronous execution instance that can continue, pause, resume and report progress.

`forge request status` must resolve the run id to the current workflow before reporting status. Runtime mutations performed through Forge, including goal updates and attached artifacts, are reflected in request status with the original request preserved as `requested_goal`.

## Self-Evolution Contract

Forge may work on Forge itself only through bounded cycles:

- stop date is mandatory;
- every cycle writes prompt/report artifacts;
- authorized executors are selected from local policy;
- validation must pass before commit;
- push is explicit;
- external Docker/Kubernetes/Knative resources remain out of scope unless explicitly authorized.

## Validation Contract

A workflow is only promotable when all tasks are completed and validation rules pass. Until then, `forge validate` returns a blocked status and non-zero exit code.

Self-improvement is intentionally conservative. `forge improve` generates an experiment artifact plus a version changelog and does not auto-promote.
