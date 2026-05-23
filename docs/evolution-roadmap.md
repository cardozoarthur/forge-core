# Forge Core Evolution Roadmap

Forge Core should become the operational runtime that coordinates AI and non-AI execution. It can integrate tightly with agent CLIs for usability, but it should not depend on a single CLI's internal reasoning loop.

## Architectural Position

The durable split is:

- Forge controls workflow authority: decomposition, graph state, context, scheduling, retries, validation, artifacts, memory, costs and promotion.
- Agent CLIs control local task execution: editing files, running tools, inspecting repositories, using model-specific capabilities and returning structured evidence.

This means both statements are true:

- Tight coupling is good when it makes the product simpler to use.
- Runtime independence is required so Forge can outlive any one model, CLI or provider.

## Integration Modes

### 1. CLI Uses Forge

Codex, OpenCode, Gemini CLI or similar tools call Forge commands during their normal workflow.

Examples:

- `forge plan` to decompose a user objective.
- `forge context` to request a bounded task context.
- `forge run` to advance deterministic or simulated parts of the graph.
- `forge validate` to check whether a workflow can progress.
- `forge artifacts` to list reusable outputs.

This mode is useful for immediate adoption because it looks like a skill/plugin from the user side.

### 2. Forge Uses CLI

Forge launches an executor adapter for long-running or specialized tasks.

Examples:

- Codex adapter for repository edits and test repair.
- OpenCode adapter for local code execution workflows.
- Gemini adapter for model-diverse analysis.
- Claude adapter for alternative review passes.
- Ollama adapter for local/private execution.
- Command adapter for deterministic non-AI work.

The CLI receives a strict task packet and returns a structured result. Forge keeps the workflow state and decides whether the result passes validation.

### 3. Native Integration Inside Open-Source CLIs

When a CLI is open enough to support deeper integration, Forge can become an embedded orchestration backend.

The goal is not to hide Forge behind the CLI. The goal is to make the CLI experience simpler while keeping Forge as the source of truth for:

- task graph;
- context packages;
- executor leases;
- validation state;
- workflow history;
- cost reports;
- artifacts.

## Execution Packet

Every non-deterministic executor should receive a bounded packet:

```json
{
  "workflow_id": "wf_...",
  "task_id": "task-...",
  "executor": "codex",
  "objective": "Implement JWT middleware",
  "allowed_context": [
    "src/auth.rs",
    "tests/auth_contract.rs"
  ],
  "artifact_refs": [
    "artifacts/wf_.../requirements.json"
  ],
  "validation_rules": [
    "cargo test auth_contract",
    "cargo clippy --all-targets -- -D warnings"
  ],
  "expected_output": "JWT middleware implementation plus tests",
  "cost_budget": {
    "max_usd": 2.0,
    "max_tokens": 120000
  }
}
```

The response must be auditable:

```json
{
  "task_id": "task-...",
  "status": "completed",
  "artifacts": [
    "artifacts/wf_.../task-.../summary.md"
  ],
  "trace_ref": "traces/wf_.../task-....jsonl",
  "cost": {
    "estimated_usd": 0.84,
    "tokens_in": 28000,
    "tokens_out": 6100
  },
  "validation_evidence": [
    {
      "command": "cargo test auth_contract",
      "exit_code": 0
    }
  ]
}
```

## Phased Roadmap

### Phase 0: Local Runtime Contract

Status: implemented in the current CLI.

- Rust CLI.
- SQLite persistence.
- Atomic task graph.
- Goal-oriented task/work-item metadata.
- Rework validation when a task is not definitively ready.
- Context budget command.
- Simulated mixed AI/non-AI execution.
- Cron/wait task representation.
- Notification payload with workflow cost report.
- Controlled improvement artifacts.
- Version changelog generation for improvement candidates.
- Executor sync and persisted human authorization policy.
- Runtime substrate sync for Docker/Kubernetes/Knative.
- Runtime scope guard for Forge-owned versus external resources.
- Runtime goal/artifact mutation with origin-tracked revisions.
- Codex/OpenCode-compatible skill installation.

### Phase 1: Adapter Contract

Goal: define the stable executor interface before binding to specific CLIs.

- Add `ExecutorAdapter` trait in Rust.
- Add task lease records so long-running executors can claim, renew and release work.
- Persist executor traces as JSONL.
- Persist per-task cost estimates.
- Add executor response schema validation.
- Add dry-run adapters for authorized `codex`, `opencode`, `gemini`, `claude`, `ollama` and `command`.

### Phase 2: First Real CLI Adapters

Goal: make Forge call external CLIs while keeping execution bounded.

- Implement an OpenCode adapter first because it is open and inspectable.
- Implement a Codex adapter through the stable CLI surface available on this host.
- Add Gemini CLI integration if the installed CLI exposes a usable non-interactive execution mode.
- Add process timeouts, log capture, cancellation and retry classification.
- Pass only `forge context` output plus explicit artifact references to each executor.

### Phase 3: Durable Scheduling

Goal: make future work real, not just represented in the graph.

- Add `forge daemon`.
- Add due-task queue backed by SQLite.
- Add cron evaluation.
- Add task wakeup records.
- Route async tasks to authorized Docker/Kubernetes/Knative substrates.
- Enforce resource ownership labels before update/delete.
- Add notification dispatch adapters.
- Keep email/Telegram/webhook dispatch behind explicit configuration and dry-run defaults.

### Phase 3.5: Workflow Registry, Inspect And Recursive Subflows

Goal: make Forge observable and composable as a long-running runtime.

- Add `forge list` to show running and non-running workflows with stable ids, lifecycle state and the original initial request description.
- Add lifecycle state that distinguishes running, idle, completed, blocked, failed, scaled-to-zero and infinite/daemon-style workflows.
- Implement scale-to-zero semantics for finite workflows when no runnable or scheduled work remains.
- Add `forge inspect <id>` to render the workflow graph in the terminal.
- Add `forge inspect <id> --verbose` to include subflows and descriptions of each process and subprocess/subflow.
- Add recursive subflow records so a workflow can contain many subflows and each subflow can contain child subflows.
- Add infinite subflow metadata so idle long-lived subflows remain schedulable instead of being incorrectly marked complete.
- Before creating a new workflow, search available workflow definitions and prior workflows for compatible reusable flows, then integrate compatible ones as child subflows when appropriate.

### Phase 4: Native CLI Coupling

Goal: make Forge feel simple inside daily coding tools.

- Keep the existing skill/plugin layer for discoverability.
- Add project-local Forge commands that agent CLIs can call predictably.
- For open-source CLIs, explore native patches that let the CLI request Forge context and report task results.
- Keep the native integration optional: Forge must remain usable as a standalone runtime.

### Phase 5: Controlled Self-Improvement

Goal: let Forge improve workflows structurally without unrestricted self-modification.

- Collect metrics per workflow, executor and validation gate.
- Generate experimental workflow variants across task structure, prompt system, process runtime, validation governance and executor policy.
- Benchmark variants against baseline workflows.
- Promote only when validation, cost and quality gates improve.
- Record lineage, reproducibility hashes and rollback metadata.
- Produce a strong changelog for every candidate version.

## Near-Term Implementation Backlog

1. Add executor adapter traits and response schemas.
2. Add task leasing and long-running execution records.
3. Add dry-run CLI adapters for authorized Codex/OpenCode/Gemini.
4. Add real OpenCode adapter with bounded context.
5. Add real Codex adapter and enforce the OpenCode/Codex bridge policy.
6. Add scheduler daemon and due-task queue.
7. Add notification configuration for email, Telegram and webhooks.
8. Add cost accounting across AI, command runtime and notification steps.
9. Add native integration spike for the easiest open-source CLI.
10. Add real Knative node adapter with ownership labels and namespace guard.
11. Add runtime mutation propagation so changed goals invalidate stale downstream context.
12. Add `forge list` with running/non-running lifecycle and original request descriptions.
13. Add `forge inspect` graph rendering with verbose recursive subflow descriptions.
14. Add recursive finite/infinite subflow records and scale-to-zero lifecycle semantics.
15. Add available-flow discovery so new workflows can reuse compatible existing flows as child subflows.

## Non-Goals

Forge should not:

- become only a Codex plugin;
- hard-code one provider as the center of the architecture;
- let executors freely expand context outside the task packet;
- auto-promote self-generated changes without validation;
- hide workflow costs from users;
- require human decisions when the workflow was defined as autonomous.
- mutate user-owned Docker/Kubernetes/Knative resources without explicit authorization.
