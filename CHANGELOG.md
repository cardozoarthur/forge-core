# Changelog

## 0.4.34 - 2026-05-24

### Added

- `forge task checkpoint` now accepts an optional `--context-routing-cache-key` and persists it with the checkpoint record.
- `forge task handoff` now emits `forge.executor_handoff.v3` with a `resume_plan` derived from the latest checkpoint and the current Context Routing Engine cache key.
- The handoff `resume_plan` reports checkpoint identity, checkpoint context SHA-256, checkpoint routing cache key, current routing cache key, explicit resume status, adapter action and whether a partial retry with fresh context is recommended.

### Changed

- Executor adapters no longer need to infer resumability from `resume_context_status` alone. They can distinguish fresh starts, stale checkpoints, unknown checkpoint routes, unchanged routes and changed routes directly from the handoff envelope.
- Current checkpoints whose recorded route differs from the current handoff route are surfaced as `checkpoint_route_changed` with action `partial_retry_with_fresh_context`.

### Safety

- The resume plan is read-only handoff metadata derived from Forge-owned checkpoint and context-routing state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.

## 0.4.33 - 2026-05-24

### Added

- `forge inspect --output json` now projects the Context Routing Engine fingerprint into each node's compact `context_route`.
- Terminal inspection diagrams include a short routing cache key beside the context profile, handoff status and selected/effective bytes.
- `forge task handoff` now emits `forge.executor_handoff.v2` with the context routing fingerprint schema, cache key and lineage hash.

### Changed

- Bounded executor adapters can now read the context cache identity from the handoff packet without opening the full nested `context.routing_fingerprint` body.
- Operators can compare inspect output and handoff packets against the same Forge-owned routing cache key.

### Safety

- The new fields are read-only projections derived from the existing `forge.context.routing_fingerprint.v1` contract.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.

## 0.4.32 - 2026-05-24

### Added

- `forge context --output json` now includes a versioned `routing_fingerprint` contract.
- The fingerprint uses schema `forge.context.routing_fingerprint.v1` and carries a stable `cache_key`, workflow revision, executor profile id, context SHA-256, lineage SHA-256 and named component hashes for routing policy, executor profile, lineage, budget, selected/omitted sections, missing required sections, dependency state, child subflows, resume state and context payload.
- Added CLI contract coverage proving the fingerprint is stable for the same workflow/task/budget and changes after a traced workflow goal mutation.

### Changed

- Executor adapters can now make deterministic context cache/reuse decisions from Forge-owned routing metadata instead of comparing full context packet bodies.
- The existing `forge.context.v14` packet remains backward compatible; this release adds a nested fingerprint schema rather than changing the top-level context schema.

### Safety

- The fingerprint is read-only metadata derived from Forge-owned workflow graph, lineage, dependency, checkpoint and context-routing state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.

## 0.4.31 - 2026-05-24

### Added

- `forge inspect --output json` now includes a compact `context_route` projection on every DAG node.
- Each route reuses the Context Routing Engine packet and reports schema/routing policy, executor profile, effective budget, context SHA-256, context readiness, handoff status, resume status, missing required sections, included/omitted sections and the shard `routing_summary`.
- Human terminal diagrams now annotate every node with context profile, handoff state and selected/effective context bytes.

### Changed

- Workflow inspection no longer exposes only high-level handoff status; it now carries enough context-routing evidence for operators to distinguish dependency blockers, missing required context and budget pressure directly from `forge inspect`.
- The `forge.context.v14` packet stays unchanged; this release projects the existing versioned packet into inspection rather than introducing a new context schema.

### Safety

- The inspection route projection is read-only and derived from Forge-owned workflow graph, checkpoint and deterministic context-routing state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by `forge task handoff`, strict context readiness and task leases.

## 0.4.30 - 2026-05-24

### Added

- `forge list --output json` now includes a compact `context_handoff` projection on each workflow row and on the global registry summary.
- The registry projection uses schema `forge.registry_context_handoff.v1` and reports total tasks, ready tasks, blocked tasks, missing-context blockers, dependency blockers and combined blockers.
- Added CLI contract coverage proving registry-level handoff counts are derived from the Context Routing Engine instead of loose task-status heuristics.

### Changed

- `forge context` now emits schema `forge.context.v14` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_v14`.
- Context shard selection now ranks required sections before optional sections inside each executor profile. This prevents optional workflow context from consuming a deterministic executor's bounded budget while required task-local sections are omitted.
- `forge task handoff`, `forge inspect` and `forge request status` inherit the v14 context readiness contract.

### Safety

- The registry handoff projection is read-only and reuses Forge-owned workflow graph, checkpoint and deterministic context routing state.
- The routing-order change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Promotion remains controlled by `forge validate`, and executor ownership remains controlled by `forge task handoff` and task leases.

## 0.4.29 - 2026-05-24

### Added

- Added `forge task handoff` as an executor adapter contract around strict context readiness and task leases.
- The command returns `forge.executor_handoff.v1` with the selected executor, task executor kind, lease status/id, context schema, context SHA-256, handoff status, expected output, execution policy mode, validation gate and validation rules.
- Handoff responses include the full bounded context package so adapters can use one Forge-owned command for lease acquisition plus replayable executor context.

### Changed

- Executor handoff now has a stable CLI envelope instead of requiring adapters to manually combine `forge context --strict` and `forge task acquire`.
- Forge acquires a task lease only when the context handoff is ready; missing required context or dependency blockers return `handoff_blocked` without claiming the task.

### Safety

- The new handoff command mutates only Forge-owned task lease state after context readiness passes.
- It does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative, or mutate Docker/Kubernetes/Knative resources.
- Context readiness and promotion remain controlled by the existing context and validation gates.

### Validation

- Added CLI contract coverage proving `forge task handoff` acquires a lease for a ready task, emits `forge.executor_handoff.v1`, links the packet checksum to the context package, carries the validation gate, reports lease conflicts without overwriting the existing executor lease and does not lease a task when context readiness is blocked.

## 0.4.28 - 2026-05-24

### Added

- `forge inspect --output json` now includes a workflow-level `handoff_summary` and per-node `handoff_ready`, `handoff_status` and `handoff_blockers` fields.
- Terminal inspection now annotates each DAG node with the context handoff status derived from the Context Routing Engine.
- `forge request status --output json` now includes `handoff_summary` so async callers can see ready, dependency-blocked and missing-context tasks without separately calling `forge context`.
- Added a reusable `build_context_handoff_summary` projection that reuses the same context package readiness contract used by `forge context --strict`.

### Changed

- Operator surfaces now distinguish dependency-not-ready holds from missing-context holds during workflow inspection and async status polling.
- The existing `forge.context.v13` packet remains unchanged; this release projects its handoff decision into higher-level inspection/status reports.

### Safety

- Handoff summaries are read-only projections over Forge-owned workflow graph, checkpoints and deterministic context routing metadata.
- The change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative, or mutate Docker/Kubernetes/Knative resources.
- Promotion remains controlled by `forge validate`, and executor handoff remains controlled by `forge context --strict`.

### Validation

- Added CLI contract coverage proving both `forge inspect` and `forge request status` surface `blocked_dependencies` for a downstream task whose prerequisite is still pending.

## 0.4.27 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v13` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_v13`.
- Context packets include `handoff_ready`, `handoff_status` and structured `handoff_blockers` so executor adapters can distinguish missing required context from dependency-not-ready holds.
- `handoff_blockers` carries typed blocker records with `kind`, `message` and `refs` for replayable executor-handoff decisions.

### Changed

- `forge context --strict` now exits non-zero when `handoff_ready=false`, including the case where all required context sections fit but upstream dependency tasks are still pending, running, blocked, failed or missing.
- Context contract tests now target schema `forge.context.v13`.

### Safety

- Handoff readiness is read-only metadata derived from Forge-owned workflow graph state and the deterministic shard manifest. It does not complete dependencies, promote workflows, authorize CLIs, execute local Python/Node.js code, mutate Docker/Kubernetes/Knative resources or bypass validation gates.
- Non-strict `forge context` remains inspectable and backwards-compatible for consumers that only need the emitted JSON package.

### Validation

- Added CLI contract coverage proving `forge context --strict` blocks a downstream executor handoff when dependency readiness is false even though required context sections are present.

## 0.4.26 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v12` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_budget_summary_required_v12`.
- Context packets now include top-level `dependency_summary` and `dependency_refs` so executor adapters can inspect prerequisite readiness without reparsing the DAG or relying on loose dependency IDs.
- The `dependencies` shard now renders dependency title, status and blocking/missing markers in the executor-facing content.

### Changed

- Context routing now treats dependency readiness as auditable handoff context instead of a compact ID list.
- CLI contract tests now target schema `forge.context.v12`.

### Safety

- Dependency readiness is a read-only projection of Forge-owned workflow graph state. It does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, mutate Docker/Kubernetes/Knative resources or bypass validation gates.
- Blocking dependency metadata is exposed for executor policy decisions, while promotion remains controlled by `forge validate`.

### Validation

- Added CLI contract coverage proving `forge context` emits structured dependency readiness and an executor-facing dependency shard for a blocked downstream task.

## 0.4.25 - 2026-05-24

### Added

- `forge validate` now enforces the node-scoped Personality/Soul Routing contract on tasks that declare persona metadata.
- Persona-routed tasks are blocked from promotion when the persona mode is empty, scope is not `node`, auditability is false, voice/tone are missing, the validation gate is not `persona_routing_required`, or required Codex/Paperclip source model references are absent.
- Validation reports now emit `failed_rules.kind="persona_routing"` plus a rework task when persona routing is incomplete or non-auditable.

### Changed

- Personality/Soul Routing is now validation-gated runtime behavior rather than context-only metadata.
- Human-facing persona switches remain optional per node, but any declared switch must be explicit, auditable and replayable before promotion.

### Safety

- The new gate is read-only validation over Forge-owned workflow metadata. It does not select a provider, run a model, authorize CLIs, execute local code, mutate Docker/Kubernetes/Knative resources or promote any workflow.
- Persona-free legacy tasks remain valid under the existing task-status and goal-readiness gates.

### Validation

- Added CLI contract coverage proving a completed workflow is still blocked when its stored persona routing metadata is corrupted after execution.

## 0.4.24 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v11` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_budget_summary_required_v11`.
- Context packets expose `context_ready`, `required_sections` and `missing_required_sections` so executor adapters can tell whether the package contains the minimum correct context for the selected executor profile.
- Context shard manifests now mark each shard with `required` and `missing_required`.
- `routing_summary` now includes `required_shards` and `required_omitted_shards` for readiness and cost audits.
- `forge context --strict` prints the same auditable JSON package but exits non-zero when required sections are missing.

### Changed

- Executor context profiles now carry explicit required section contracts in addition to section allow-lists and byte caps.
- Context contract tests now target schema `forge.context.v11`.

### Safety

- Strict context readiness is read-only validation metadata. It does not mutate workflow state, complete tasks, select executors, authorize CLIs, execute local code, mutate Docker/Kubernetes/Knative resources or promote subflows.
- Non-strict `forge context` remains backward-compatible for inspection and debugging. The strict path only changes the process exit code after emitting replayable JSON evidence.

### Validation

- Added CLI contract coverage proving `forge context --strict` blocks an executor handoff when a tight budget omits required context shards.

## 0.4.23 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v10` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_budget_summary_v10`.
- Context packets include a top-level `routing_summary` derived from the shard manifest, including total, included, omitted, compressed, profile-omitted and budget-omitted shard counts.
- The routing summary reports selected bytes, original bytes, omitted bytes, compression savings, effective budget, remaining budget and budget utilization in basis points.

### Changed

- Executor adapters and operators can audit context cost and routing pressure from one bounded summary instead of recomputing aggregate metrics from every shard.
- Context contract tests now target schema `forge.context.v10`.

### Safety

- Routing summaries are read-only metadata derived from the selected shard manifest. They do not change workflow state, select executors, authorize CLIs, execute local code, mutate Docker/Kubernetes/Knative resources or promote subflows.
- The summary is computed after deterministic shard routing, so it cannot bypass profile omissions, budget omissions, checkpoint freshness or validation gates.

### Validation

- Added CLI contract coverage proving `routing_summary` matches the emitted shard manifest and reports compression savings plus omitted-byte pressure for constrained context packages.

## 0.4.22 - 2026-05-24

### Added

- Added persisted task checkpoint records through `forge task checkpoint`.
- `forge request status` now projects `checkpoint_count` and `latest_checkpoint` so async callers can resume from Forge's workflow source of truth instead of keeping executor-local progress state.
- `forge context` now emits schema `forge.context.v9` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_budget_decisions_v9`.
- Context packets include `latest_checkpoint`, `resume_context_status`, `resume_context_reason` and a checkpoint shard when the task has a checkpoint.

### Changed

- Resumable context routing now marks checkpoints as `checkpoint_current` when their recorded workflow revision matches the current workflow revision and `checkpoint_stale` after runtime goal/artifact mutations advance the workflow revision.
- Context contract tests now target schema `forge.context.v9`.

### Safety

- Checkpoints are Forge-owned metadata. Recording a checkpoint does not complete a task, promote a workflow, execute local code, authorize external CLIs, or mutate Docker/Kubernetes/Knative resources.
- Stale checkpoints remain visible for audit and partial retry decisions, but executor adapters must refresh context before resuming from an older workflow revision.

### Validation

- Added CLI contract coverage for `forge task checkpoint`, request-status checkpoint projection, checkpoint context shards and stale checkpoint detection after a goal mutation.

## 0.4.21 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v8` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_budget_decisions_v8`.
- Context shard manifests now expose `routing_decision` and `decision_reason` for every emitted shard.
- Routing decisions distinguish `included_full`, `included_compressed`, `omitted_profile` and `omitted_budget`, making context selection auditable without replaying the routing algorithm manually.

### Changed

- Budget-omitted shards now report `bytes = 0` and hash the empty selected payload, reflecting that no shard content was sent to the executor.
- Context contract tests now target schema `forge.context.v8`.

### Safety

- Routing decisions are read-only metadata in the context packet. They do not authorize CLIs, run local code, mutate Docker/Kubernetes/Knative resources, or promote subflows.
- Profile omissions remain deterministic and executor-policy scoped.

### Validation

- Added CLI contract coverage proving deterministic no-AI context shards explain full inclusion, profile exclusion and budget omission decisions.

## 0.4.20 - 2026-05-23

### Added

- `forge context` now emits schema `forge.context.v7` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_budget_v7`.
- Context packets now expose `child_subflow_count` and `child_subflows` for tasks that carry proposed reusable child-subflow bindings.
- Added a `child_subflows` context shard sourced from the subflow registry so executor adapters receive Forge's reuse decision inside the bounded task-local context package.

### Changed

- Deterministic no-AI context profiles now prioritize proposed child-subflow bindings after execution policy and before validation/context narrative sections. This reduces duplicate reasoning and duplicate local code-node work when Forge has already found a compatible reusable subflow.
- Context contract tests now target schema `forge.context.v7`.

### Safety

- Child-subflow routing is read-only context metadata. Forge does not execute, promote, mutate or auto-complete a reused child subflow from `forge context`.
- The full proposed binding remains auditable through top-level structured `child_subflows`; the executor-facing text stays compact so deterministic nodes keep their bounded no-AI envelope.
- This change does not authorize CLIs, run Python/Node.js code, or mutate Docker/Kubernetes/Knative resources.

### Validation

- Added CLI contract coverage proving a context package for a reused deterministic code node includes the proposed child-subflow binding, shard source, binding status and schema v7 routing policy.

## 0.4.19 - 2026-05-23

### Added

- Added persisted `child_subflows` metadata on atomic tasks so compatible reusable deterministic code-node candidates can be attached to the new workflow graph as proposed child subflows.
- `forge plan` now reports `attached_subflows` and saves one best attachable child-subflow reference per requested task when the registry finds a compatible reusable local code-node flow.
- `forge inspect --verbose` now renders persisted child subflow links in both structured JSON and the terminal DAG diagram.

### Changed

- Reuse candidates are no longer only transient plan-output hints. The planned workflow now carries the proposed recursive subflow relationship forward for later validation, execution policy and inspection cycles.

### Safety

- Child subflow bindings are `proposed` metadata only. Forge does not execute, promote, mutate or auto-complete reused child subflows during planning.
- Attachment is limited to candidates already marked attachable by the registry lifecycle policy: `idle`, `completed` or `scaled_to_zero`.
- This change does not authorize CLIs, run Python/Node.js code, or mutate Docker/Kubernetes/Knative resources.

### Validation

- Added CLI contract coverage proving `forge plan` persists a compatible reusable code-node candidate as a proposed child subflow and `forge inspect --verbose` renders it.

## 0.4.18 - 2026-05-23

### Added

- Added registry-derived reusable deterministic subflow entries for repeated/frequent local code-node tasks.
- `forge list` now exposes `summary.reusable_subflows` and per-workflow `reusable_subflows` with task id, executor, policy mode, reuse hint, human-readable compatibility key, context lineage hash, language, entrypoint, validation gate and lifecycle state.
- `forge plan` now reports `reuse_candidates` before saving the new workflow when an existing workflow contains a compatible reusable local code-node subflow.

### Changed

- Planning now consults Forge's persisted workflow registry before creating duplicate deterministic Python/Node.js code-node work, while still keeping Forge as the source of truth.
- Reuse candidate matching requires both the execution-policy compatibility key and task-local context lineage hash to match.

### Safety

- The reuse registry is read-only projection metadata. It does not execute local Python/Node.js code, authorize CLIs, mutate Docker/Kubernetes/Knative, or attach child subflows automatically.
- Candidates are only marked `attachable_as_child_subflow` when the existing workflow lifecycle is idle, completed or scaled to zero.

### Validation

- Added CLI contract coverage for `forge list` surfacing reusable code-node subflows with compatibility keys.
- Added CLI contract coverage for `forge plan` reporting compatible reuse candidates from a previously validated workflow before duplicating a deterministic code node.

## 0.4.17 - 2026-05-23

### Added

- Added Forge-owned `execution_policy` metadata to every atomic task with deterministic/AI allowance, reuse hint, validation gate and optional local code runtime.
- `forge context` now emits schema `forge.context.v6` with routing policy `task_local_revisioned_persona_compressed_executor_policy_budget_v6`.
- Context packets include top-level `execution_policy` metadata and an `execution_policy` shard so executor adapters can audit why a node should run as a model, mixed adapter, deterministic executor or local code node.
- Planner now selects a `local_code_node` policy for deterministic non-AI steps when the goal explicitly requests local Python or Node.js work, including reusable hints for repeated or frequent work.

### Changed

- Deterministic context profiles now preserve execution policy before lower-priority narrative context, keeping no-AI code-node decisions visible inside bounded context packets.
- Context contract tests now target schema `forge.context.v6`.

### Safety

- Execution policy selection is metadata only. It does not execute local Python/Node.js code during planning, authorize external CLIs, mutate Docker/Kubernetes/Knative, bypass validation gates or make an installed CLI the source of truth.
- Code-node policy remains Forge-owned and validation-gated through `deterministic_code_node_validation_required`.

### Validation

- Added a CLI contract test proving repeated local Python work without AI receives a deterministic `local_code_node` policy in both the planned task and the routed context packet.

## 0.4.16 - 2026-05-23

### Added

- `forge context` now emits schema `forge.context.v5` with routing policy `task_local_revisioned_persona_compressed_executor_profile_budget_v5`.
- Added executor-aware context profiles to every context packet, including executor kind, deterministic/no-AI flag, reasoning allowance, profile section allow-list and profile-specific byte cap.
- Added `requested_budget`, `effective_budget` and `profile_omitted_sections` so operators can see when Forge deliberately shrinks deterministic executor context below the caller's maximum budget.
- Context shard manifests now expose `profile_excluded` to distinguish profile-based omissions from budget pressure.

### Changed

- Deterministic `command` and `wait` nodes now use a no-AI context profile that preserves local objective, validation rules, task context requirements and dependencies before lower-priority narrative context.
- Notification nodes use a smaller deterministic profile while still allowing persona routing for human-facing payloads.
- AI and mixed nodes keep the richer reasoning-oriented context profile.

### Safety

- Executor profiles only affect context selection. They do not authorize external CLIs, change workflow state, mutate runtime substrates or bypass validation gates.
- Profile omissions are auditable in the context packet and shard manifest.

### Validation

- Added a CLI contract test proving that a deterministic no-AI task receives the `no_ai_deterministic` profile, a reduced effective budget and profile-audited omissions for nonessential sections.
- Updated context contract tests for schema `forge.context.v5` and profile-aware compression coverage.

## 0.4.15 - 2026-05-23

### Added

- `forge context` now emits schema `forge.context.v4` with routing policy `task_local_revisioned_persona_compressed_budget_v4`.
- Added deterministic compressed shard fallback for tight context budgets: when a full high-priority shard does not fit, Forge now attempts to include a compact summary payload before omitting the shard.
- Context shard manifests now expose `compressed` and `original_bytes` so operators can audit when executor-facing context was reduced.

### Changed

- Context routing preserves more high-priority workflow state under constrained budgets without exposing whole history or exceeding the requested byte budget.

### Safety

- Compression is deterministic and local to the context packet. It does not change workflow goals, artifacts, executor policy, validation rules or external runtime substrates.

### Validation

- Added a CLI contract test proving that an oversized `workflow_goal` shard is included as a compressed summary when it fits inside the remaining context budget.

## 0.4.14 - 2026-05-23

### Added

- Added `forge inspect <workflow-id>` as a read-only workflow inspection surface.
- Added `src/inspection.rs` to render persisted Forge workflows as terminal DAG text with lifecycle state, dependency edges, executor kinds and node-scoped persona annotations.
- Added structured JSON inspection output with task nodes, validation rules, subtasks and reserved subflow fields for the upcoming recursive subflow registry.

### Safety

- `forge inspect` derives its view from Forge's SQLite workflow source of truth and registry projection. It does not mutate workflow state, executor policy or external runtime substrates.

### Validation

- Added a CLI contract test proving that `forge inspect --verbose --output json` exposes lifecycle state, dependency edges, persona annotations, validation rules and subtasks for the persisted DAG.

## 0.4.13 - 2026-05-23

### Added

- Added `PersonaRoutingSpec` to atomic tasks so human-facing nodes can declare an explicit node-scoped persona mode.
- Added default `operator_report` persona routing for documentation tasks and `stakeholder_notice` for workflow cost email notifications.
- `forge context` now emits schema `forge.context.v3` with routing policy `task_local_revisioned_persona_budget_v3`.
- Context packages include top-level persona metadata, a `persona_routing` shard and persona mode/scope data in lineage.

### Safety

- Persona routing remains node-scoped, explicit and auditable; it does not change workflow goals, validation rules, executor policy or runtime substrate authorization.
- Source-model metadata records the local contract inputs for Codex developer/personality instructions and Paperclip-style soul, voice, tone or persona modeling.

### Validation

- Added CLI contract tests proving that planned human-facing tasks carry persona routing metadata and that `forge context` exposes persona lineage for those nodes.

## 0.4.12 - 2026-05-23

### Added

- Added a persistent Personality/Soul Routing goal to Forge self-evolution prompts.
- Documented the future persona profile contract for human-facing artifacts: node-scoped, explicit, auditable in context lineage and validation-gated.
- Added roadmap coverage for inspecting Codex developer/personality instructions and Paperclip soul, voice, tone or persona models before implementation.

### Validation

- Added a CLI contract assertion so `forge self run --dry-run` must include the Personality/Soul Routing goal in the executor prompt.

## 0.4.11 - 2026-05-23

### Added

- `forge context` now emits schema `forge.context.v2` with routing policy `task_local_revisioned_budget_v2`.
- Added top-level `workflow_revision`, `artifact_count` and `lineage` fields to context packages.
- Added lineage hashes for the current workflow goal, task goal and artifact manifest so executor context can be replayed and checked for staleness.
- Added a `workflow_goal` shard so runtime goal mutations are visible in the executor-facing context body.

### Changed

- Context routing now includes the current workflow goal, initial goal, revision and artifact count alongside task-local objective data.
- `forge context` reflects `workflow update-goal` and `workflow attach-artifact` mutations without requiring callers to inspect status separately.

### Safety

- The legacy executor-facing `content` field remains present.
- No external runtime substrate is touched; lineage is derived from Forge's SQLite workflow state and artifact records.

## 0.4.10 - 2026-05-23

### Added

- `forge context` now returns a versioned context packet with `schema_version = "forge.context.v1"`.
- Added deterministic `task_local_priority_budget_v1` routing metadata to each context response.
- Added a context shard manifest with section, source, priority, inclusion decision, byte count, summary and SHA-256 checksum for every candidate shard.
- Added whole-packet `context_sha256` plus explicit `omitted_sections` so executor runs can be replayed and audited against the exact bounded context selected for the task.

### Changed

- Context selection now uses task-local priority ordering across local objective, context requirements, validation rules, dependencies, work item metadata and workflow constraints.
- The legacy `content` and `included_sections` fields remain available for executor compatibility.

### Validation

- Added a CLI contract test that verifies `forge context` emits the versioned shard manifest and stays within the requested budget.

## 0.4.9 - 2026-05-23

### Fixed

- `forge list` now loads older workflow records that were created before `async_policy` existed on tasks, defaulting them to synchronous inline execution policy.

### Added

- Added persistent goals for the Context Routing Engine: compression, summarization, selection, versioning and sharding of minimal correct context.
- Added persistent goals for deterministic + AI hybrid graphs, including local Python/Node.js code nodes for repeated work that does not need model calls.
- Added persistent goals for long-running cognition: pause/resume, async continuation, durable execution, checkpointing, partial retry and resumable context.
- Added the same goals to the self-evolution prompt so future Forge cycles can work on them directly.

## 0.4.8 - 2026-05-23

### Added

- Added the first workflow registry surface through `forge list`.
- Registry rows include stable workflow ids, associated run ids, run statuses, current goal, initial request, workflow status, derived lifecycle state, revision, artifact count and task status summary.
- New workflows persist `initial_goal` so the original request remains visible after runtime goal mutations.

### Changed

- Completed finite workflows are projected as `scaled_to_zero` in the registry when all tasks are completed, giving operators a first lifecycle signal without mutating Docker/Kubernetes/Knative resources.

### Safety

- `forge list` is read-only and derives its view from Forge's SQLite source of truth.
- Existing workflow records without `initial_goal` still load; list falls back to the async run's original request when available, then to the current goal.

## 0.4.7 - 2026-05-23

### Added

- Added persistent runtime goals for workflow registry visibility, terminal graph inspection, recursive subflows, infinite subflows, scale-to-zero lifecycle state and flow composition/reuse.
- Added the same goals to the self-evolution prompt so future cycles prioritize `forge list`, `forge inspect`, subflow lifecycle and compatible-flow reuse.

### Direction

- `forge list` should show running and non-running workflows with stable ids and the original initial request description.
- `forge inspect <id>` should render the graph in the terminal, with `--verbose` showing subflows and process/subprocess descriptions.
- Forge should inspect available flows before creating new ones and integrate compatible existing flows as child subflows when possible.

## 0.4.6 - 2026-05-23

### Added

- Added `latest_validation_evidence` to `forge request status` so async callers can see the latest self-evolution validation artifact without manually listing files.
- The compact evidence summary includes artifact path, SHA-256, schema version, prompt packet version, cycle, executor, validation status and command counts.

### Changed

- Request status now derives validation evidence from persisted workflow artifacts at read time, preserving Forge as the source of truth instead of copying validation state into run records.

### Safety

- The original validation artifact remains the canonical evidence. `request status` only projects a compact summary and keeps the full report auditable through the persisted artifact path and checksum.

## 0.4.5 - 2026-05-23

### Added

- Added versioned self-evolution validation evidence artifacts:
  - schema version: `forge.self_evolution.validation.v1`;
  - per-cycle `self-evolution-cycle-NNN-validation.json` artifacts;
  - cycle report fields for validation report path and SHA-256 checksum.

### Changed

- Self-evolution validation now runs the required commands as a structured sequence and records command status, exit code, duration and captured stdout/stderr.
- Failed validation still keeps `forge self run --output json` machine-readable by sending diagnostic command logs to stderr while persisting the full evidence in the validation artifact.

### Safety

- Validation remains fail-closed: post-validation local install and GitHub publication only run after every required validation command passes.
- Commands after the first failed validation gate are recorded as skipped so operators can see exactly where promotion stopped.

## 0.4.4 - 2026-05-23

### Fixed

- Captured self-evolution validation output so `forge self run --output json` remains machine-readable after Codex/OpenCode cycles.
- Validation details are now emitted to stderr only when the validation gate fails.

## 0.4.3 - 2026-05-23

### Added

- Added source-of-truth async request status projection:
  - `forge request status` now loads the current workflow behind the run id;
  - status output includes the current workflow goal, original requested goal, workflow status, latest revision, artifact count and task status summary.

### Changed

- `forge request status` no longer behaves as a stale run-record lookup for Codex/OpenCode skill callers. The run id now resolves to the current workflow state after runtime mutations such as `workflow update-goal` and `workflow attach-artifact`.

### Safety

- The original request goal is preserved as `requested_goal`, while `goal` reflects the current Forge workflow goal. This keeps Forge as the source of truth without losing the initial request intent.

## 0.4.2 - 2026-05-23

### Added

- Added persisted task leases:
  - `forge task acquire`;
  - `forge task release`;
  - SQLite-backed `task_leases` records keyed by workflow task;
  - JSON lease conflict reports when a second executor attempts to acquire an unexpired task lease.
- Added explicit self-evolution cycle report fields and non-dry execution for local Forge install updates and GitHub publication contract commands after validation, using `gh auth token` as the local credential gate.

### Safety

- Lease acquisition is guarded by Forge-owned workflow state and records acquisition, conflict and release events.
- Expired task leases may be replaced, but active leases block concurrent executor ownership until released or expired.
- Self-evolution prompts now declare post-validation local install and GitHub publication obligations instead of leaving them implicit.
- Public project publishing uses `gh auth status`, `gh repo view --json url,visibility` and a timed `git push`; non-public repositories are not pushed by that path.

## 0.4.1 - 2026-05-23

### Added

- Added versioned self-evolution prompt packets:
  - prompt packet version: `forge.self_evolution.prompt.v1`;
  - required validation commands embedded in each executor prompt;
  - SHA-256 prompt checksum persisted in each cycle report.

### Changed

- `forge self run --dry-run` now emits replayable executor prompt metadata so Codex/OpenCode runs can be audited against the exact prompt packet they received.

## 0.4.0 - 2026-05-23

### Added

- Added async request handoff:
  - `forge request start`;
  - `forge request status`.
- Added run records with stable `run_id` identifiers.
- Added `forge self run` for bounded Forge self-evolution cycles.
- Added self-evolution prompt/report artifacts per cycle.
- Added stop-date validation for autonomous work windows.

### Changed

- Codex/OpenCode skill flow now prefers returning a `run_id` instead of waiting for long work inline.
- Forge self-evolution can alternate authorized Codex/OpenCode executors while preserving validation gates.
- Fixed Codex self-evolution invocation to pass approval policy as a top-level Codex CLI option.

## 0.3.0 - 2026-05-23

### Added

- Added runtime substrate sync for Docker, Kubernetes and Knative.
- Added `forge sync runtimes`, `forge sync all` and `forge runtimes`.
- Added Knative install suggestion when Docker and Kubernetes are available but Knative is missing.
- Added runtime ownership guard through `forge runtime guard`.
- Added async policy metadata on tasks that target Docker/Kubernetes/Knative-style execution.
- Added runtime workflow mutation commands:
  - `forge workflow update-goal`;
  - `forge workflow attach-artifact`.
- Added workflow revision history with mutation origin tracing for Codex, OpenCode, Forge CLI and skills.

### Safety

- Forge may mutate resources it created.
- Pre-existing Docker/Kubernetes/Knative resources require explicit human authorization before update/delete/patch/apply.
- Attached runtime artifacts are copied into Forge workflow storage instead of depending on external loose files.

## 0.2.0 - 2026-05-23

### Added

- Added executor sync with persisted local policy for Codex, OpenCode, Gemini, Claude and Ollama.
- Added explicit human authorization before Forge may use an installed/configured CLI as an execution engine.
- Added `forge sync executors` and `forge executors`.
- Added `opencode_codex_bridge` policy metadata when both OpenCode and Codex are authorized.
- Added goal-oriented task metadata: task goal, subtasks, definition of done, backlog state, impediments, acceptance criteria and owner role.
- Added goal readiness validation and `rework_tasks` output so unfinished goals return to work instead of being promoted.
- Added structural self-improvement domains: task structure, prompt system, process runtime, validation governance and executor policy.
- Added `--target-version` to `forge improve`.
- Added Markdown changelog generation for every improvement candidate.

### Changed

- `forge skill install` now runs executor sync as part of installation and includes the sync report in JSON output.
- Simulated execution now marks subtasks complete and task goals definitively ready.
- `forge improve` now creates both a JSON experiment artifact and a changelog artifact.

### Validation

- Test suite expanded from 9 to 15 CLI contract tests.
- New tests cover executor detection, saved human authorization, OpenCode/Codex bridge policy, goal-oriented task metadata, rework validation and changelog generation.
