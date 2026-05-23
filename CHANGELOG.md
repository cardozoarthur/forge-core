# Changelog

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
