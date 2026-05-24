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

The current version is a local Rust CLI and skill package. It includes SQLite persistence, simulated execution, AI/non-AI/wait/notification task kinds, executor sync/policy, runtime substrate sync/policy, goal-oriented work items, rework validation, runtime goal/artifact mutation, cluster registry/placement metadata, cluster handoff manifests, cost report generation and controlled improvement artifacts with changelog generation. It does not yet include remote distributed execution, real provider adapters, SaaS UI or WASM plugins.

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

Forge validates adapter outputs through `forge task validate-response`. The current
contract uses `forge.executor_response.v1` for executor output and
`forge.executor_response_validation.v1` for Forge's acceptance report. A completed
response must match the target task, include a replayable trace reference, report
finite non-negative cost/token values and carry at least one passing validation
evidence item. Rejected responses are audit events, not task promotion events.

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

## Cluster Registry Contract

Forge must know available cluster nodes before it schedules distributed work.
The first contract is a local registry, populated by explicit operator input, not
by automatic network scans or infrastructure mutation.

Each Forge node record includes:

- CPU cores and memory;
- operating system and architecture;
- GPU inventory and GPU availability;
- installed software;
- Python, Node.js and Docker availability;
- network reachability and endpoint metadata;
- lifecycle status;
- cost, latency and reliability estimates;
- trust level and sandbox permissions.

`forge cluster register` persists a reported node profile in SQLite.
`forge cluster list` returns the current registry with capability summaries.
`forge cluster place --workflow <id> --task <task-id>` derives deterministic
placement requirements from the task's executor and execution policy, then
returns a dry-run placement decision with candidate reasons. Candidates include
active node lease counts and the score penalizes busy eligible nodes, so a
compatible idle node is preferred before distributed handoff. A local Python code
node, for example, requires a registered online and reachable node with the
`python` capability, a trusted LAN/local trust class and the declared sandbox
permission.
`forge cluster handoff --workflow <id> --task <task-id>` turns an eligible
placement into a bounded handoff packet. It acquires the normal Forge task lease
using the selected node id as the executor, returns a node-scoped lease ref and
emits `forge.cluster_sync_manifest.v1` with context, checkpoint, artifact and
context-shard hashes. The manifest also includes a deterministic
`manifest_sha256` over its sync fields, excluding the hash field itself. The
manifest is hash-only and declares remote execution and external mutation
disabled, so it is an auditable staging contract rather than a remote runner.
`forge cluster leases --output json` lists the node-scoped leases created by
cluster handoff. Each row includes the workflow/task identity, lease scope,
active/expired state, selected node metadata, trust level, sandbox permissions and
explicit `remote_execution_enabled=false` / `external_mutation_allowed=false`
markers. Operators can filter by `--node-id` to inspect a single LAN/SSH node
without touching the remote machine.

This stage intentionally does not open SSH sessions, run remote commands, mutate
Docker/Kubernetes/Knative resources or execute remote AI. It gives Forge an
auditable scheduling and handoff precondition so later remote adapters can build
on explicit capabilities, node leases, content hashes and trust policy.

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

Current `forge context` packets use schema `forge.context.v30` and routing policy
`task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30`. Each packet
includes the executor-facing content, the full context checksum, workflow revision,
artifact count, node-scoped persona routing metadata plus a versioned persona profile
and persona contract for human-facing tasks, executor
profile metadata, a versioned routing contract, execution policy metadata, dependency
readiness summaries, proposed child-subflow bindings, requested and effective budgets,
lineage hashes, included and omitted sections, profile-driven omissions, and a deterministic shard manifest with
source, priority, compression state, profile exclusion state, required/missing-required
state, remaining-budget before/after values, selected byte count, minimum-routable
byte count, selected-cost basis points, selection savings, summary and shard checksum.
Packets also include `context_ready`,
`required_sections`, `missing_required_sections`, `handoff_ready`, `handoff_status`,
`handoff_blockers`, aggregate `routing_summary` metrics and a versioned
`routing_contract`, `routing_repair` budget recommendation, `budget_plan` minimum-correct budget contract and
`minimum_correct_set` section receipt plus a `routing_quality` contract. Packets also carry a versioned `next_action` decision so executor adapters
can distinguish fresh handoff, dependency waits, context-budget repair, stale
checkpoint refresh and partial retry with fresh context without first asking for a
separate inspection projection. Packets now include a versioned `prompt_packet`
contract that binds the context schema, routing policy, executor profile, persona
mode/profile, instruction sources, validation gates, context checksum and lineage
checksum into a stable adapter-facing hash. Packets also include a versioned
`replay_manifest` that records the replay command, selector version, budget,
context checksum and content-addressed shard refs; prompt packets bind its checksum
so async executors can pause and resume against the exact route. Packets also
include a versioned `continuation_plan` that converts checkpoint state, context
delta and current route into an adapter-facing action: start fresh, resume from a
reusable checkpoint, refresh context before resume, or run a partial retry with
fresh context. The routing contract names the selector version, executor profile version,
profile id, selection strategy, requested and effective budget, minimum budget,
allowed/required/optional section set and profile hash. The repair contract turns
missing required sections into a deterministic action and recommended budget so
operators can retry with the smallest known budget increase instead of guessing. The
budget plan exposes the required context floor, selected bytes, optional pressure,
and recommended budget. The minimum-correct set records every required section's
included/compressed/missing state, source and content hashes, byte counts, routing
decision and repair action, then binds that section-level floor into the routing
fingerprint. The `routing_economy` contract records baseline bytes, selected bytes,
compression savings, budget omissions, profile-filtered omissions, total avoided bytes,
reduction basis points and whether a deterministic no-AI profile avoided a model call.
Together these contracts let executor adapters choose the smallest correct handoff
budget before spending model or runtime work. The
persona profile gives the selected profile id, routing rationale, source-model
summaries and profile checksum. The persona contract binds the node's mode, scope,
voice, tone, instruction source, source models, validation gate, audit flag and
profile checksum to the context lineage hash and persona-mode hash before executor
handoff. The quality contract scores each packet and emits explicit warnings for missing required
context, budget pressure, compressed summaries and profile-filtered optional context,
so adapters and operators can audit routing risk without reconstructing shard
decisions. Handoff policy can still block incomplete context or pending dependencies
before an executor starts work.

Executor profiles let Forge route different envelopes without changing workflow
authority. Deterministic `command` and `wait` nodes use a no-AI profile that shrinks
the context budget and prioritizes local objective, validation rules, declared context
requirements and dependencies before lower-priority narrative context. Notification
nodes use a smaller deterministic profile that still allows persona routing. AI and
mixed nodes keep the richer reasoning profile. Execution policy metadata records
whether the node is allowed to use AI, whether it is deterministic, whether a local
Python/Node.js code runtime was selected and which validation gate controls the node.
`forge context --strict` emits the same replayable JSON packet but exits non-zero when
`handoff_ready=false`, giving adapters a deterministic readiness gate for missing
required sections and dependency-not-ready holds without hiding routing evidence.
`forge inspect` and `forge request status` project that same handoff decision as
read-only summaries, so operators and async callers can see which task is ready,
blocked by missing context or blocked by dependencies without reconstructing the
context package manually. Those summaries also carry routing quality aggregates and
per-task quality contracts for context-budget and profile-pressure triage.
`forge list --context-actions` exposes the valid registry filter values for
handoff, resume and retry actions, so operators can discover context-action filters
without memorizing summary field names or reading source code.
Every workflow row also exposes versioned `context_action_refs` entries with task id,
title, executor, next action, handoff status, blocker refs, checkpoint refs and the
current routing cache key. This keeps registry filtering actionable without forcing
operators or adapters to open a full `forge inspect` view before deciding which task
needs handoff, dependency wait, context repair, checkpoint resume or partial retry.
`forge inspect <workflow-id> --task <task-id>` provides a focused terminal and JSON
inspection view for one task. It preserves the selected node's context-route,
persona, execution-policy, handoff and child-subflow projections while limiting the
node list, handoff summary and diagram to the focused task. The JSON report includes
both the focused node count and the full workflow task count so adapters can bound
operator context without losing DAG scale metadata.
When the workflow registry has attached a proposed compatible child subflow, the
context package carries the structured binding plus a compact `child_subflows` shard
from `subflow_registry`, which lets executors reuse Forge's planning decision without
rebuilding it from irrelevant history. Runtime goal, artifact and persona routing state
remain part of the context lineage, which gives long-running executors a deterministic
stale-context signal while leaving room for persisted summaries, artifact shards and
active child-subflow execution gates.

Inspection expands those proposed child-subflow bindings as read-only topology
metadata. `forge inspect --output json` records the parent workflow/task, depth,
path, reachability, terminal flag and loaded child workflow/task counts for each
linked subflow, and the terminal diagram prints the same path. This makes recursive
reuse auditable before Forge schedules, executes or promotes a child flow.

Validation turns recursive reuse into an explicit promotion gate. A proposed
child-subflow binding is not promotable by itself: `forge validate` fails the parent
task until the binding is revisioned to a validation-ready state. `forge workflow
validate-subflow` performs that Forge-owned mutation, checks that the child
workflow/task exists, refuses child flows that are not scaled to zero,
stamps the current child lifecycle and validation gate onto the parent binding, and
records a workflow revision plus event. This keeps reuse as an auditable runtime
decision instead of silently treating a registry suggestion as completed work.

## Personality/Soul Routing

Some workflow outputs are not only machine artifacts. Reports, research summaries,
strategy documents, teaching material and operator updates are read by humans, so
Forge should be able to route a node through an explicit personality, voice or
"soul" profile when that improves clarity.

This capability must remain operationally bounded:

- the persona is a node-level execution setting, not hidden global behavior;
- the task graph records mode, scope, source models, voice, tone and validation gate;
- the context packet records which persona profile was selected, includes it as a shard and hashes it into lineage;
- executor handoff packets project the selected persona as a versioned contract so
  adapters can enforce the node mode without parsing unrelated context;
- Codex-style developer/personality instructions and Paperclip-style soul, voice,
  tone or persona models are summarized as explicit inputs to the profile contract;
- the persona switch is included in lineage so results are replayable;
- promotion validation rejects persona switches that are not node-scoped,
  auditable, source-model backed and gated by `persona_routing_required`;
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
- `forge inspect <id>` renders the current workflow graph in the terminal from persisted Forge state.
- `forge inspect <id> --verbose` includes task goals, expected outputs, validation rules, subtasks and proposed child-subflow links.
- `forge inspect <id> --task <task-id>` focuses inspection on one node while retaining full workflow task-count metadata.
- Workflows may contain subflows recursively. A flow can own many subflows, and each subflow can own many child subflows.
- Subflows can be finite or infinite. Infinite subflows require explicit lifecycle metadata so Forge can distinguish "idle but alive" from "completed".
- Running workflows must remain mutable: list gives stable ids, inspect shows the current graph, and goal/artifact mutations appear as revisions.

Before creating a new workflow from scratch, Forge should inspect available workflows and reusable flow definitions. If an existing flow can satisfy part of the new objective, Forge should propose or attach it as a child subflow instead of duplicating orchestration logic.

The first reuse contract is deterministic and registry-derived. `forge list` exposes reusable local code-node subflows with a compatibility key based on execution policy, language, entrypoint and validation gate, plus a context lineage hash derived from the task-local context requirements and validation rules. `forge plan` and `forge request start` report compatible `reuse_candidates` from existing workflows before saving the new workflow and persist the best attachable candidate per requested task as a proposed `child_subflows` link. This gives direct planning and skill-style async requests the same recursive subflow surface without spending a model call, executing local Python/Node.js work or automatically promoting reused subflows. Promotion requires a later `forge workflow validate-subflow` mutation so the parent workflow records when a reused child became validation-ready.

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
- prompt packets are versioned and must load the persisted Forge workflow goal before generic strategic guidance;
- if a human or adapter mutates the self-evolution goal through `forge workflow update-goal`, the next cycle must carry that current goal, initial goal and workflow revision into the executor prompt;
- authorized executors are selected from local policy;
- validation must pass before commit;
- push is explicit;
- external Docker/Kubernetes/Knative resources remain out of scope unless explicitly authorized.

## Validation Contract

A workflow is only promotable when all tasks are completed and validation rules pass. Until then, `forge validate` returns a blocked status and non-zero exit code.

Self-improvement is intentionally conservative. `forge improve` generates an experiment artifact plus a version changelog and does not auto-promote.
