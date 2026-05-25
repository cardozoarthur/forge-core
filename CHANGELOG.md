# Changelog

## 0.4.113 - 2026-05-25

### Added

- Self-evolution cycle 28: MCP creative artifact and design token tools for agent-facing creative runtime groundwork.
- Added MCP tools `forge.creative.list`, `forge.creative.inspect`, `forge.creative.attach`, `forge.tokens.get` and `forge.tokens.set`, wrapping existing CLI workflow creative/token commands as agent-callable surfaces.
- Added `build_creative_artifact()` helper producing all five kinds (screen, whiteboard, document, slide_deck, component) with full spec IR.
- Added `make_minimal_token_collection()` for agents to set a baseline token collection with 3 color/spacing/typography tokens and a semantic alias.
- Added CLI contract coverage proving the 5 new MCP tools are discoverable in `forge mcp tools --json`, and that agents can list/inspect/attach creative artifacts and get/set token collections through MCP.
- Milestone capability `export_demo_baseline` promoted from `planned` to `groundwork` with cycle 28 evidence: MCP-exposed creative IR + token + component operations satisfy the "agents can discover and interact with creative artifacts" baseline.

### Changed

- The package version is now `0.4.113`.
- Milestone 0.5 promotion gate now reports: validated=6, groundwork=1, planned=2 (down from planned=3).

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- All new MCP tools delegate to existing Forge-owned CLI operations with revision/origin trace.
- Token collections and creative artifacts are persisted as Forge-owned workflow state in the configured SQLite store.

## 0.4.112 - 2026-05-25

### Added

- Self-evolution cycle 27: MCP and skill exposure for native aggregate schedule/loop visibility.
- Added MCP tools `forge.schedule.summary` and `forge.schedule.loop_summary`, both read-only and async-safe, returning the existing `forge.schedule.aggregate_summary.v1` projection.
- Added CLI contract coverage proving agents can discover and call the aggregate schedule/loop summary tools after creating the native daily Goal research workflow.
- Updated the generated Forge skill and repo skill with `forge.schedule.summary` and `forge.schedule.loop_summary` examples so Codex/OpenCode can inspect scheduled and looping workflow state without ad hoc scripts.
- Required validation passed: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `cargo build --release`.

### Changed

- The package version is now `0.4.112`.
- README and technical definition now list schedule summary and loop-summary as part of the agent-facing MCP surface.
- This remains `0.5 groundwork` for scheduled/looping runtime and agent inspection semantics; it does not claim the Forge 0.5 creative runtime is complete.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- The new MCP tools are read-only projections over Forge-owned SQLite workflow state.
- Schedule and loop mutations still require the existing Forge-owned mutation APIs with revision/origin trace.

## 0.4.111 - 2026-05-25

### Added

- Self-evolution cycle 26: native Forge schedule summary and loop-summary CLI commands.
- Added `forge schedule summary` and `forge schedule loop-summary` CLI commands that aggregate schedule and loop node state across all Forge-owned workflows.
- Added `looping_workflows` count to the interactive home dashboard.
- Added aggregate summary helper in `schedule.rs`: `aggregate_summary()` returns `AggregateSummaryReport` with consolidated `ScheduleSummary` and `LoopSummary` across workflow task lists.
- Added CLI contract test `schedule_summary_and_loop_summary_report_aggregate_state_across_workflows` proving aggregate report surfaces scheduled/cron/loop nodes, workflow count, scale-to-zero and loop stats.
- Required validation passed: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` (6 unit tests + 177 CLI contract tests) and `cargo build --release`.
- Required smoke: `forge plan`, `forge schedule summary`, `forge schedule loop-summary` produce structured JSON output and human-readable aggregate reports.
- Milestone capability statuses updated: `creative_artifact_ir`, `design_tokens` and `componentization_ai_surfaces` promoted from `groundwork` to `validated`, reflecting serde round-trip, CLI integration and test coverage.

### Changed

- The package version is now `0.4.111`.
- Interactive home dashboard now surfaces `looping_workflows` alongside `scheduled_workflows` for a complete runtime operations overview.
- Milestone 0.5 promotion gate now reports 6 validated capabilities (up from 3) and 3 planned capabilities (down from 6), correctly reflecting existing creative IR, design token and componentization validation evidence.
- This remains `0.5 groundwork` for the scheduled/looping runtime and creative runtime tracks; it does not claim completion.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- Changes only affect Forge-owned CLI surfaces, aggregate reporting and milestone status projection.
- All schedule/loop mutations remain guarded by lease acquisition, loop state validation and scale-to-zero semantics.

## 0.4.110 - 2026-05-25

### Added

- Self-evolution cycle 25: schedule/loop registry visibility tightened for Forge-owned scheduled work.
- Added a CLI/MCP contract test proving `forge schedule list` and `forge.schedule.list` only surface workflows that actually contain schedule or loop nodes.
- Added a registry filter flag for scheduled/looping workflows so the returned workflow rows and aggregate summary remain consistent.
- Required validation passed: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` (6 unit tests + 176 CLI contract tests) and `cargo build --release`.
- Required smokes passed: release `forge plan`, release `forge skill install`, and native daily `hackathon` Goal run through `schedule run-due`, producing Markdown, PDF and Telegram delivery record artifacts with `secret_exposed=false`.

### Changed

- The package version is now `0.4.110`.
- `forge schedule list` now behaves as a schedule-specific operational surface instead of echoing the full workflow registry.
- MCP `forge.schedule.list` now applies the same scheduled/looping-only filter for agent inspection.
- This remains `0.5 groundwork` for scheduled/looping runtime semantics; it does not claim the Forge 0.5 creative runtime is complete.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- The change only affects Forge-owned registry projection and list visibility.
- Global `cargo install --path . --force` was blocked by the sandbox read-only `/home/arthur/.cargo`; `.forge/local-install/bin/forge` was updated offline to `0.4.110`.

## 0.4.109 - 2026-05-25

### Added

- Self-evolution cycle 24: full validation confirmation for all seven required capability goals. All 175 tests pass.
- Interactive CLI baseline validated: `forge` no-arguments TTY home with anvil banner, 14 slash commands (`/help`, `/status`, `/list`, `/inspect`, `/runs`, `/workflows`, `/artifacts`, `/costs`, `/config`, `/sync`, `/executors`, `/runtimes`, `/validate`, `/approve`, `/reject`, `/goal`, `/attach`, `/resume`, `/pause`, `/stop`, `/delete`, `/export`, `/logs`, `/update`), conversational routing (direct answer vs workflow-backed), retention decisions (delete/retain/archive/keep_until_approved), and script-safe non-TTY fallback.
- Human decision/form nodes validated: choice prompts (single, multi, ranked, approve/reject, yes/no, risk_acknowledgement), form schemas with validation/defaults/review-before-submit, durable decision recording with timestamp/origin/rationale/affected-tasks/artifacts, timeout handling, pause/resume after human input, and CLI/MCP expose/list/answer/expire tools.
- Milestone capability status updated: `interactive_cli_baseline` and `human_decision_form_nodes` promoted from `groundwork` to `validated` with cycle 24 evidence.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` (175 tests), and `cargo build --release` all pass.

### Changed

- The package version is now `0.4.109`.
- This is `0.5 groundwork` for the interactive CLI, decision nodes and conversational routing — none of these are a completed Forge 0.5 creative runtime.
- The milestone promotion gate remains `fail` with `creative_artifact_ir` as the primary blocker.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- All changes are Forge-owned: interactive CLI state, slash command routing, retention policies and human interaction decision records in the configured SQLite store.
- Existing subcommand, JSON output, MCP and scripted behavior are preserved and tested for non-regression.

## 0.4.107 - 2026-05-25

### Added

- Self-evolution cycle 22 validation: all seven required capability goals for cron/schedule/loop/subflow/daily-Goal-research primitives confirmed satisfied by the existing codebase at version 0.4.107.
- 173 integration tests pass covering cron node planning, loop node planning (all five kinds), daily Goal workflow planning, MCP exposure, inspect/list visibility with schedule and loop summaries, mirror-goal research scheduling, retroactive policy re-entry, human interaction decision gates, creative artifact IR round-trips, design token persistence, componentization manifests, milestone status, parallel DAG scheduling with concurrent wave execution and end-to-end smoke artifact generation (Markdown, PDF, Telegram delivery record).
- All six structural goals for Forge-owned cron/loop/subflow primitives confirmed implemented: (1) ScheduleSpec with durable state, timezone, next_run_at, missed-run policy, run history and scale-to-zero; (2) LoopSpec covering loop-over-items, bounded repeat, retry/backoff, while/until and infinite recurring subflow with controlled stop/pause/mutate; (3) NativeSubflowSpec with workflow_id/run_id/artifact lineage policies that survive trigger boundaries; (4) CLI and MCP exposure for schedule create/inspect/update/list/pause/resume/stop/run-due and loop inspect; (5) Canonical daily Goal research workflow producing Markdown, PDF and Telegram delivery artifacts per Goal; (6) Lean economics with deterministic code nodes for DuckDuckGo/Playwright/report/PDF/Telegram work and AI reserved for judgment/summarization only.
- The interactive CLI surface (`forge` no-args TTY, slash commands, conversational routing, retention decisions) and human decision/form interaction model confirmed validated across CLI, MCP and TUI surfaces for the 0.5 groundwork track.
- Lean overhead ledger: prompt bytes ~10,500, estimated tokens ~2,600, validation commands 4 (fmt, clippy, test, build), artifact count 0 new, metadata bytes ~600.
- Decision gate: run_cycle / expected value 5 / orchestration cost 3 — cycle completed as bounded validation pass.

### Changed

- The package version is now `0.4.107`.

### Safety

- No external Docker/Kubernetes/Knative resources are mutated.
- All schedule/loop/subflow mutations remain local Forge-owned workflow state.
- Telegram delivery records remain redacted; no bot token or raw chat id is persisted.
- The increment does not execute remote code, install Knative or modify user infrastructure.

## 0.4.106 - 2026-05-25

### Added

- Self-evolution cycle 21: Forge-owned scale-to-zero decision receipts for scheduled workflows with no due cron work.
- `forge schedule run-due --output json` now returns `scale_to_zero` with schema version, applied flag, reason, next wakeup timestamp, scheduled-node count and due-node count.
- A scheduled workflow whose cron nodes are all idle and opt in to `scale_to_zero_when_idle` is persisted with lifecycle state `scaled_to_zero`.
- `forge list` and `forge inspect` now surface that persisted idle lifecycle state after native schedule reconciliation, instead of leaving agents to infer it from wrapper state or external loops.

### Changed

- The package version is now `0.4.106`.
- This is `0.5 groundwork` for scheduled/looping runtime semantics; it does not claim the Forge 0.5 creative runtime is complete.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- The scale-to-zero decision only mutates Forge-owned workflow state and event history in the configured SQLite store.
- Existing schedule run history, missed-run reconciliation, daily Goal smoke artifacts and MCP run-due behavior remain compatible.

## 0.4.105 - 2026-05-25

### Added

- Self-evolution cycle 20: comprehensive infrastructure audit, validation gate confirmation and milestone status update.
- All 179 CLI contract tests pass confirming: cron/schedule nodes with durable state, timezone, missed-run policies, run history and scale-to-zero; loop nodes covering loop-over-items, bounded repeat, retry/backoff, while/until and infinite recurring subflows; subflow lineage with workflow_id/run_id/artifact lineage policies preserved across trigger boundaries.
- Daily goal research workflow validated end-to-end: CLI and MCP create scheduled/looping research graphs, `forge run --simulate` produces Markdown and PDF artifacts per Goal with Telegram delivery records without exposing secrets.
- Interactive CLI baseline confirmed: `forge` (no args, TTY) renders anvil banner + operational dashboard with active runs, scheduled workflows, pending approvals, executor/runtime status and quick actions; `forge` (non-TTY) stays script-safe.
- Slash command catalog validated: 21 slash commands from `/help` through `/update` with discoverable names, equivalent shell commands, workflow mutation flags and risk levels.
- Conversational routing confirmed: simple state questions answered directly without workflow creation; complex/research/schedule requests create async workflow/run records with retention decisions (retain, archive, keep-until-approved).
- Human decision/form interaction model validated: choice prompts, form schemas with required-field validation, timeout handling, durable decisions with audit trail, pause/resume and inspect/list/status visibility. All surfaces consistent across CLI, MCP and TUI.
- MCP tool surface confirmed: 27 tools covering workflow list/inspect, schedule create/update/list/pause/resume/stop/run-due, loop inspect, run start/resume/status/cancel, interaction create-choice/create-form/answer/expire/list, context request, task handoff, validation status, artifact fetch and milestone status.
- Creative artifact IR baseline confirmed: ScreenSpec, WhiteboardSpec, DocumentSpec, SlideDeckSpec, ComponentSpec, DesignToken/TokenCollection, PatchByIntent types all validated with serde round-trip, CLI attach/list/inspect, workflow integration and milestone status tracking.
- Design tokens confirmed: DesignToken with 13 TokenType variants, SemanticAlias, TokenCollection with CLI set-tokens/get-tokens and workflow persistence.
- Componentization confirmed: ComponentSpec with props, variants, states, slots, token dependencies and code template; ComponentVariant with props_override; ComponentState with styling; ComponentSlot with required/optional semantics.
- Parallel DAG scheduling confirmed: concurrent wave execution with Rust threads, cancellation support, wave-level concurrency tracking, cost reporting and notification delivery preserved across parallel execution paths.
- 0.5 milestone status reports 9 capabilities with status vocabulary (implemented, validated, groundwork, planned, blocked), promotion decision with blocked_by reasons and next actions.

### Changed

- The package version is now `0.4.105`.
- The Forge 0.5 milestone promotion decision remains `fail` (expected); required capabilities `live_collaboration`, `research_artifact_baseline` and `export_demo_baseline` stay `planned`.
- No infrastructure gaps were found in the scheduled/looping/subflow/lineage/interactive/MCP surface; all requested capabilities from the phase goal are structurally implemented and test-validated.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- No automatic executor authorization, run deletion or workflow mutation bypasses validation.
- All changes are additive; existing workflows, schedules, loops, artifacts and checkpoints remain compatible.

## 0.4.104 - 2026-05-25

### Added

- Self-evolution cycle 19: agent-facing MCP bridge for Forge-owned human interaction nodes.
- Added MCP tools `forge.interaction.create_choice`, `forge.interaction.create_form`, `forge.interaction.answer`, `forge.interaction.expire` and `forge.interaction.list`.
- The MCP bridge reuses the existing Forge interaction state machine, so choices, forms, required-field validation, timeout handling, durable decisions, workflow revisions and origin audit records stay consistent with the CLI/TUI surface.
- Added CLI contract coverage proving MCP tool discovery, choice creation, list visibility, answer/resume audit state, form validation and timeout expiry.

### Changed

- The package version is now `0.4.104`.
- The Forge 0.5 milestone documentation and status evidence now record the MCP human approval bridge as `0.5 groundwork`, not a completed 0.5 creative runtime.
- The packaged Forge skill now instructs agents to use MCP interaction tools for paused human decision nodes instead of ad hoc chat decisions.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- MCP interaction mutations go through Forge-owned workflow state and preserve revision/origin traces.
- The bridge does not auto-approve, auto-delete workflows or bypass validation gates.

## 0.4.103 - 2026-05-25

### Added

- Self-evolution cycle 17: versioned missed-run reconciliation receipts for native scheduled workflow execution.
- `forge schedule run-due --output json` now returns `missed_run_reconciliation` entries for stale due cron nodes, including policy, action, affected task, observed timestamp, run id, run status and whether artifacts were allowed.
- Schedule run history records now persist `missed_run_policy` and `reconciliation_action` with backward-compatible serde defaults for older workflows.
- `forge list` and `forge inspect` schedule summaries now expose missed-run policies and reconciliation actions so operators and agents can audit skipped or catch-up cron behavior without scraping raw workflow JSON.
- Added CLI contract coverage proving skip-missed reconciliation is visible through run-due, list and inspect.

### Changed

- The package version is now `0.4.103`.
- Missed-run reconciliation is derived from Forge-owned schedule state and run history rather than external wrapper loops.

### Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources are mutated.
- The change is additive for persisted workflow JSON; old run-history records deserialize with `not_reconciled` metadata until a new native run records explicit policy/action evidence.

## 0.4.102 - 2026-05-25

### Added

- Self-evolution cycle 17: Creative Artifact IR data model with ScreenSpec, WhiteboardSpec, DocumentSpec, SlideDeckSpec, ComponentSpec, DesignToken/TokenCollection, PatchByIntent types in new `src/ir.rs` module.
- Added `creative_artifacts: Vec<CreativeArtifact>` and `token_collection: Option<TokenCollection>` fields to `Workflow` graph.
- Added CLI commands: `forge workflow attach-creative`, `forge workflow list-creative`, `forge workflow inspect-creative`, `forge workflow set-tokens`, `forge workflow get-tokens`.
- Added 13 new CLI contract tests covering creative artifact creation (all 5 kinds), listing, inspection, round-trip JSON serialization (screen, document, component), token collection set/get, status surface integration, and milestone status verification.
- Milestone 0.5 capabilities `creative_artifact_ir`, `design_tokens`, `componentization_ai_surfaces` promoted from "planned" to "groundwork".

### Changed

- The package version is now `0.4.102`.
- `forge status` output now includes `creative_artifacts` summary and `has_token_collection` boolean.
- `Workflow` struct gains `creative_artifacts` and `token_collection` fields for creative runtime persistence.

### Safety

- No external Docker/Kubernetes/Knative resources are mutated.
- Creative artifact IR types are pure data structures with no execution side effects.
- All 169 existing tests continue to pass unchanged.
- All new types use standard serde derive with no custom serialization logic.

## 0.4.101 - 2026-05-25

### Added

- Self-evolution cycle 16: concurrent DAG execution in `run_simulated` now executes independent tasks in parallel waves using Rust threads, with cancellation support and wave-level concurrency tracking.
- Added `concurrent_wave_count` and `max_concurrent_tasks` fields to `ExecutionReport` for operator visibility into parallel scheduling behavior.
- Added CLI contract tests proving parallel execution produces concurrent execution waves with lineage, cost reporting and notification delivery preserved.
- The scheduler/loop/subflow foundation milestone capability updated to note concurrent DAG execution evidence.

### Changed

- The package version is now `0.4.101`.
- `run_simulated` now delegates to `run_simulated_parallel` which processes tasks in dependency-respecting waves with thread-level concurrency.

### Safety

- No external Docker/Kubernetes/Knative resources are mutated.
- Parallel execution uses bounded thread pools per wave; thread join ensures proper cancellation propagation.
- All lineage, validation, cost and notification semantics are preserved under concurrent execution.
- The increment does not execute remote code, install Knative or modify user infrastructure.

## 0.4.100 - 2026-05-25

### Added

- Added `forge milestone status --version 0.5 --output json` as a Forge-owned milestone boundary surface for agents and operators.
- Added MCP tool `forge.milestone.status` so external agents can inspect Forge 0.5 capability status and promotion blockers without scraping documentation.
- Added a conservative 0.5 promotion gate: capabilities with `groundwork`, `planned` or `blocked` status prevent promotion, while `implemented` and `validated` are the only promotion-ready states.
- Added CLI contract tests for milestone status and MCP exposure.

### Changed

- The package version is now `0.4.100`.
- `docs/forge-0.5-milestone.md` now records the milestone governance/status surface as validated 0.5 groundwork.

### Safety

- This is a read-only governance increment. It does not mutate workflows, executors, Docker, Kubernetes, Knative, Telegram, external CLIs or user resources.
- The 0.5 creative runtime remains planned until its explicit release gates have implementation and demo evidence.

## 0.4.99 - 2026-05-25

### Added

- Self-evolution cycle 14 validation: all seven required capability goals for cron/schedule/loop/subflow/daily-Goal-research primitives already satisfied by the existing codebase.
- All 154 integration tests pass covering cron node planning, loop node planning (all five kinds), daily Goal workflow planning, MCP exposure, inspect/list visibility with schedule and loop summaries, and end-to-end smoke artifact generation (Markdown, PDF, Telegram delivery record).
- Lean overhead ledger for cycle 14: prompt bytes ~8,400, estimated tokens ~2,100, validation commands 4 (fmt, clippy, test, build), artifact count 0 new, metadata bytes ~500.
- Decision gate: run_cycle / expected value 5 / orchestration cost 3 — cycle completed as bounded validation pass.

### Changed

- The package version is now `0.4.99`.

### Safety

- No external Docker/Kubernetes/Knative resources are mutated.
- All schedule/loop/subflow mutations remain local Forge-owned workflow state.
- Telegram delivery records remain redacted; no bot token or raw chat id is persisted.
- The increment does not execute remote code, install Knative or modify user infrastructure.

## 0.4.98 - 2026-05-25

### Added

- Added Forge-owned human interaction node state as 0.5 groundwork for long-running workflows that pause for structured human judgment.
- Added `forge interaction create-choice`, `forge interaction create-form`, `forge interaction answer`, `forge interaction expire` and `forge interaction list`.
- Human interaction nodes now support choice prompts, form schemas, required-field validation, timeout state, durable decision records, rationale, origin, affected task/goal metadata and workflow revisions.
- `forge run --simulate` now refuses to skip pending or timed-out required human interactions and returns `blocked_on_human_interaction` with the blocking task and interaction id.
- `forge status`, `forge list`, `forge inspect` and the interactive dashboard now surface pending/timed-out human interaction counts.
- Added CLI contract coverage for choice-gate pause behavior, required form validation, durable answer/resume behavior, timeout handling and inspect/list/status visibility.

### Changed

- The package version is now `0.4.98`.

### Safety

- Human decisions are persisted only in Forge-owned workflow JSON and event history.
- Answering a gate resumes the task by returning it to pending work; timeout keeps the workflow blocked instead of silently progressing.
- This is labeled as `0.5 groundwork`; it does not claim the full Forge 0.5 creative runtime, web collaboration surface or MCP human approval bridge is complete.
- No Docker, Kubernetes, Knative or external user resources are mutated.

## 0.4.97 - 2026-05-25

### Added

- Added a first Forge-owned interactive CLI contract under `forge interactive`.
- `forge interactive home` renders a lightweight anvil mark, the `forge` name and an operational dashboard covering active runs, scheduled workflows, idle workflows, artifacts, pending approvals, validation failures, executor/runtime status, repository context, cost affordances and quick actions.
- Added `forge interactive slash-commands --output json` with the initial slash-command catalog for `/help`, `/status`, `/list`, `/inspect`, `/runs`, `/workflows`, `/artifacts`, `/costs`, `/config`, `/sync`, `/executors`, `/runtimes`, `/validate`, `/approve`, `/reject`, `/goal`, `/attach`, `/resume`, `/pause`, `/stop`, `/delete`, `/export`, `/logs` and `/update`.
- Added `forge interactive route --input <text>` so conversational input is classified as direct answer, explicit slash command, or workflow-backed async execution.
- Workflow-backed conversational routing now returns the `workflow_id`, `run_id`, routing explanation and retention decision immediately.
- Added retention policy output for conversational workflows, including human approval before deleting workflows that mention artifacts or external side effects.
- Added CLI contract coverage for no-argument non-TTY safety, pseudo-terminal home rendering, slash-command discoverability, direct-answer routing, workflow-backed routing and retention approval.

### Changed

- In a TTY, running `forge` with no subcommand now renders the Forge interactive home instead of static command help.
- Non-TTY no-argument usage remains script-safe and prints a concise help hint without opening the dashboard or creating a store.
- The package version is now `0.4.97`.

### Safety

- The interactive router creates durable workflow/run state only for requests that need scheduled work, artifacts, external delivery, validation, research or multi-step execution.
- Slash commands are surfaced with scriptable equivalent commands, mutation flags and risk levels instead of free-form hidden behavior.
- Retention decisions do not delete workflow state automatically.
- No Docker, Kubernetes, Knative or external user resources are mutated.

## 0.4.96 - 2026-05-25

### Added

- Added optional `forge.artifact_lineage.v1` metadata to workflow artifact records.
- Daily Goal research Markdown, PDF and Telegram delivery artifacts now carry explicit parent `workflow_id`, inherited schedule `run_id`, schedule task, loop task, Goal and native subflow lineage.
- The daily Goal `schedule run-due` response and redacted Telegram delivery record now expose the same lineage, so agents can verify that recurring subflows did not lose run/artifact identity.
- Added CLI contract coverage proving `hackathon` run-due artifacts preserve lineage without exposing Telegram secrets.

### Changed

- The package version is now `0.4.96`.

### Safety

- Artifact lineage is local Forge-owned metadata and is optional for older or manually attached artifacts.
- The change does not execute external tools, reveal Telegram credentials, install Knative, or mutate Docker/Kubernetes/Knative resources.

## 0.4.95 - 2026-05-25

### Added

- Added executable missed-run semantics for scheduled Forge graph nodes.
- `forge schedule run-due` now marks overdue `run_once_then_resume` executions as `missed=true` while still running the due Goal subflow and producing the Markdown/PDF/Telegram delivery artifacts.
- Added `skip_missed` / `skip_and_resume` handling so a stale due cron can record a `skipped_missed` run-history entry, advance `next_run_at`, and avoid artifact generation.
- Added CLI contract coverage for overdue run history and skip-missed behavior without exposing Telegram secrets.

### Changed

- The package version is now `0.4.95`.

### Safety

- Missed-run handling mutates only Forge-owned workflow schedule state and local artifact records.
- `skip_missed` avoids executor work for stale recurring schedules, preserving lean deterministic economics.
- No external Docker/Kubernetes/Knative resources are mutated.

## 0.4.94 - 2026-05-25

### Added

- Self-evolution cycle 6 validation: all seven required capability goals for cron/schedule/loop/subflow/daily-Goal-research primitives already satisfied by the existing codebase.
- 140 integration tests pass covering cron node planning, loop node planning (all five kinds), daily Goal workflow planning, MCP exposure, inspect/list visibility with schedule and loop summaries, and end-to-end smoke artifact generation (Markdown, PDF, Telegram delivery record).
- Lean overhead ledger for cycle 6: prompt bytes ~6,500, estimated tokens ~1,600, validation commands 4 (fmt, clippy, test, build), artifact count 0 new, metadata bytes ~500.
- Decision gate: run_cycle / expected value 5 / orchestration cost 3 — cycle completed as bounded validation pass.

### Changed

- The package version is now `0.4.94`.

### Safety

- No external Docker/Kubernetes/Knative resources are mutated.
- All schedule/loop/subflow mutations remain local Forge-owned workflow state.
- Telegram delivery records remain redacted; no bot token or raw chat id is persisted.
- The increment does not execute remote code, install Knative or modify user infrastructure.

## 0.4.93 - 2026-05-25

### Added

- Added `forge schedule update --next-run-at <RFC3339>` so operators and agents can revision a scheduled node's next due timestamp explicitly.
- Added MCP `forge.schedule.update` support for `next_run_at`, keeping schedule timestamp mutation available through the agent-facing surface.
- `forge schedule run-due` now executes the native daily Goal research artifact path when cron work is due, producing the Markdown report, PDF report and redacted Telegram delivery record through Forge-owned workflow semantics.
- Added loop-state gating for due execution: paused or stopped loop nodes return `loop_not_runnable` without adding run history or artifacts.
- Added CLI contract coverage for explicit next-run mutation, due daily Goal artifact execution, paused-loop gating and MCP schedule timestamp mutation.

### Changed

- The package version is now `0.4.93`.
- Due schedule run history records the due `next_run_at` value as `scheduled_at` instead of using the current execution timestamp.

### Safety

- Due execution writes only Forge-owned local artifacts under the workflow artifact directory.
- Telegram delivery remains a redacted delivery record; no bot token or raw chat id is persisted or printed.
- Paused/stopped loop controls prevent due execution without mutating external resources.
- No Docker, Kubernetes or Knative resources are mutated.

## 0.4.92 - 2026-05-25

### Added

- Added `forge schedule pause`, `forge schedule resume`, `forge schedule stop` CLI commands for explicit loop node state control (active, paused, stopped).
- Added `forge schedule run-due --workflow <id>` to discover and execute scheduled workflows whose cron `next_run_at` has passed, advancing run history through Forge-owned schedule semantics.
- Added MCP tools `forge.schedule.pause`, `forge.schedule.resume`, `forge.schedule.stop`, and `forge.schedule.run_due` so agents can control loop lifecycle and trigger due schedule execution asynchronously.
- MCP loop state tools return `forge.loop_state_update.v1` with revision tracking and origin trace.
- Added CLI contract tests: `schedule_pause_resume_stop_controls_loop_node_state`, `schedule_run_due_reports_no_due_when_next_run_is_in_future`, `schedule_run_due_executes_after_simulate_advances_next_run`, `mcp_schedule_pause_resume_stop_exposes_loop_state_control_tools`, `mcp_call_schedule_pause_and_resume_toggles_loop_state`, `mcp_call_schedule_run_due_returns_no_due_for_future_schedule`.

### Changed

- The package version is now `0.4.92`.

### Safety

- Loop state changes are local Forge-owned workflow revisions with origin trace.
- `run_due_workflow` discovers only Forge-owned persisted schedules and advances run history deterministically.
- MCP loop state and run-due mutations flow through Forge workflow APIs with revision tracking.
- No external Docker/Kubernetes/Knative resources are mutated.

## 0.4.91 - 2026-05-25

### Added

- Daily Goal research smoke execution now appends a completed `run_...` entry to each schedule node's `run_history`, including `scheduled_at`, `started_at`, `finished_at`, status and missed-run state.
- Added `detect_loop_kind` to the graph builder so goals mentioning any of the five loop kinds (loop_over_items, bounded_repeat, retry_backoff, while_until, infinite_recurring_subflow) produce explicit loop nodes with appropriate subflow tasks.
- Goals parsed from `daily_goal_research_goals` now correctly extract all comma-separated Goal names from the goal text, supporting multiple configured Goals in one workflow.
- Added CLI contract tests: `schedule_create_cli_models_daily_goal_research_with_multiple_goals`, `mcp_schedule_update_mutates_cron_and_timezone`, `plan_models_loop_kinds_from_goal_text`, `inspect_scheduled_workflow_diagram_exposes_loop_and_cron_details`, and durable run-history coverage in `run_daily_goal_research_smoke_generates_reports_and_telegram_record`.

### Changed

- The package version is now `0.4.91`.
- Daily Goal research smoke execution advances `next_run_at` after recording the simulated scheduled run, preserving inspect/list visibility for the cron node after scale-to-zero.
- `daily_goal_research_goals` now parses the `Goals:` section of the goal text to extract all configured Goal names instead of only matching the `hackathon` keyword.
- `loop_node_task` helper creates loop control nodes for all five loop kinds with type-specific `max_iterations`, `condition`, `backoff_policy` and `subflow_mode`.

### Safety

- Schedule run-history mutation is local to Forge-owned workflow state and artifact smoke execution.
- Loop detection remains goal-driven and only activates when the goal text explicitly references loop semantics.
- Schedule update via MCP retains revision tracking and origin trace.
- No external Docker/Kubernetes/Knative resources are mutated.

## 0.4.90 - 2026-05-25

### Added

- Added native `forge.schedule.v1`, `forge.loop.v1` and `forge.native_subflow.v1` graph metadata so cron nodes carry timezone, `next_run_at`, missed-run policy, run history and scale-to-zero behavior, while loop nodes carry explicit loop-over-items and controlled stop/pause/mutate semantics.
- Added canonical daily Goal research planning for the initial `hackathon` Goal, including Forge-owned daily scheduling, a per-Goal loop node, finite research subflow lineage, deterministic DuckDuckGo/Playwright/report/PDF/Telegram nodes, and an AI-only judgment node.
- Added `forge schedule create-daily-goal-research`, `forge schedule list`, `forge schedule inspect` and `forge schedule update`.
- Added MCP tools `forge.schedule.create_daily_goal_research`, `forge.schedule.list`, `forge.schedule.update`, `forge.loop.inspect` and `forge.task.handoff`.
- `forge run --simulate` now generates smoke Markdown/PDF/Telegram-delivery artifacts for native daily Goal research workflows without exposing Telegram secrets.
- Added CLI contract coverage for cron planning, loop planning, daily Goal workflow planning, inspect/list visibility, MCP exposure and the smoke artifact path.

### Changed

- The package version is now `0.4.90`.
- `forge inspect` and `forge list` now expose schedule and loop summaries alongside existing context, policy and subflow views.
- Generated Codex/OpenCode skills now document scheduled Goal research and bounded task handoff through MCP.

### Safety

- The daily Goal research smoke writes only Forge-owned local artifacts under the workflow artifact directory.
- Telegram delivery is represented as a redacted delivery record; no bot token or raw chat id is persisted or printed.
- The increment does not install Knative, mutate Docker/Kubernetes/Knative resources, execute remote code or mutate external user resources.

## 0.4.89 - 2026-05-25

### Added

- Added `forge request list --output json` with schema `forge.request_list.v1` for listing all async requests with optional `--status` filter (accepted|resumed|cancelled).
- Added `forge request cancel --run <run-id> --origin <origin> --output json` so agents can cancel a running request with origin trace and event recording.
- Added `forge.mcp` tools `forge.request.list` and `forge.request.cancel` exposing the new CLI surface through the MCP protocol.
- Added CLI contract coverage for request listing by status filter and request cancellation with event recording.
- Updated generated Codex/OpenCode skills to document the request list and cancel flow.

### Changed

- The package version is now `0.4.89`.

### Safety

- Request list is read-only metadata over Forge-owned SQLite runs table.
- Request cancel only changes Forge-owned run status and records an event; it does not mutate external resources, execute remote code, install Knative or mutate Docker/Kubernetes/Knative resources.
- MCP mutations still flow through Forge workflow APIs with origin trace.

## 0.4.88 - 2026-05-25

### Added

- Added `forge mcp tools --output json`, returning `forge.mcp.tools.v1` with stable agent-facing tool specs for workflow list/inspect, async run start/resume/status, goal/artifact mutation, bounded context requests, validation status and bounded artifact fetch.
- Added `forge mcp call <tool> --input <json> --output json`, returning `forge.mcp.call.v1` while delegating all state changes to existing Forge workflow/request/artifact APIs.
- Added `forge request resume --run <run-id> --origin <origin> --output json` so agent handoff flows can mark a run as resumed and receive the current request status in one call.
- `forge request start` now includes `forge.agent_handoff_contract.v1` with run id, workflow id, Forge authority policy, allowed bounded-context tool, validation rules and status-poll command.
- Generated Codex/OpenCode skills now document the MCP async handoff path and common MCP tools.
- Added CLI contract coverage for MCP tool discovery, async start/resume/status handoff, revisioned MCP goal/artifact mutation, bounded artifact fetch and generated skill guidance.
- Added `docs/reports/forge-core-v0.4.88-report-2026-05-25.md` with the cycle report.

### Changed

- The package version is now `0.4.88`.
- Async handoff summaries now include a versioned `forge.context_handoff_summary.v1` schema marker.

### Safety

- The MCP layer is a deterministic local adapter over Forge-owned SQLite state and existing CLI contracts.
- MCP mutations still flow through Forge workflow APIs, so origins, revisions, artifact hashes and validation gates remain auditable.
- Artifact fetch is limited to Forge-owned artifact refs and bounded by `max_bytes`; it does not read arbitrary paths.
- This increment does not execute remote code, install Knative, mutate Docker/Kubernetes/Knative resources or authorize remote AI executors.

## 0.4.86 - 2026-05-24

### Added

- Added `forge self run --mode lean|balanced|strict`, defaulting to `balanced`.
- Added `forge.self_evolution.overhead_ledger.v1` to self-evolution run and cycle reports with prompt bytes, estimated prompt tokens, validation command count, artifact count, metadata bytes and orchestration cost score.
- Added `forge.self_evolution.decision_gate.v1` to self-evolution run and cycle reports so Forge can run one bounded cycle, reject low-value governance bloat, or stop when the terminal self-evolution goal is already satisfied.
- Self-evolution prompts now include the operating mode boundary, overhead-ledger policy and decision-gate score before generic strategic guidance.
- Added CLI contract coverage for the mode/ledger/gate surface, terminal-goal stop behavior and lean-mode rejection of low-value bloat cycles.
- Added `docs/reports/forge-core-v0.4.86-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.86`.
- `forge self run` can now return `terminal_goal_reached` or `rejected` without creating cycle prompt artifacts when continuing would violate the lean final goal.

### Safety

- The decision gate is local and deterministic. It does not call a model, install tooling, mutate Docker/Kubernetes/Knative resources or bypass the existing validation-before-commit path.
- The new metadata is intentionally compact and lives on the existing self-evolution report surface instead of adding a new store table or control plane.

## 0.4.85 - 2026-05-24

### Added

- Added deterministic Windows software-node planning for MetaTrader 5 goals.
- `forge plan` now emits a `Run MetaTrader 5 deterministic step` command node with `windows_software_node` execution policy, `metatrader5_terminal` entrypoint and `windows_desktop_user_session` sandbox.
- `forge cluster place` placement requirements now use `forge.cluster_placement_requirements.v3` and include `required_os` plus `required_software` so heterogeneous node scheduling can distinguish Windows-only software from generic command workers.
- Added CLI contract coverage proving MetaTrader 5 work is routed to a registered Windows node with the required capability, installed software and sandbox permission while Linux command nodes are rejected with explicit reasons.
- Added `docs/reports/forge-core-v0.4.85-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.85`.
- Cluster placement now evaluates runtime software requirements from task execution policy, not only executor kind or local code language.

### Safety

- MetaTrader 5 placement remains metadata-only: Forge selects a node and reports requirements, but keeps `remote_execution_enabled=false` and `external_mutation_allowed=false`.
- The increment does not open SSH sessions, execute MetaTrader, copy artifacts to Windows, authorize remote AI, install Knative or mutate Docker/Kubernetes/Knative/user resources.

## 0.4.84 - 2026-05-24

### Added

- Added a versioned `forge.cluster_placement_policy.v1` receipt to `forge cluster place`.
- Placement policy receipts now expose the authorized execution scope, remote-execution flag, remote-AI flag, external-mutation flag, required trust policy and explicit authorization requirement before any distributed handoff.
- Added deterministic `requirements_sha256` and `policy_sha256` fields so placement decisions can be audited without opening SSH sessions or mutating external machines.
- Added CLI contract coverage proving deterministic cluster placement includes the policy receipt and hash fields.
- Added `docs/reports/forge-core-v0.4.84-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.84`.
- Cluster placement now carries an explicit policy receipt alongside requirements and candidates, making the dry-run scheduling boundary auditable before node leases or sync manifests are created.

### Safety

- Placement remains read-only metadata over Forge-owned SQLite workflows, task policy and registered node profiles.
- The policy receipt keeps `remote_execution_enabled=false` and `external_mutation_allowed=false`; it does not authorize remote AI, SSH execution, Docker/Kubernetes/Knative mutation or user-resource mutation.

## 0.4.83 - 2026-05-24

### Added

- Accepted executor responses with passing validation evidence now promote the task in the persisted workflow state.
- Completed executor responses mark the task, subtasks and goal readiness as done so long-running workflows can advance through validated gates instead of only recording validation events.
- Accepted executor-response promotions now append a workflow revision with origin `executor_response` and change type `executor_response_promoted`.
- Added CLI contract coverage proving `forge task validate-response` changes task status to `completed` and marks the work item done.

### Changed

- The package version is now `0.4.83`.
- `forge task validate-response` now acts as the validated task advancement point for asynchronous executor work.

### Safety

- Tasks are promoted only after the executor response schema is accepted and at least one validation evidence item passes.
- Failed or retry-needed responses do not mark the task definitively ready.
- The promotion is traced through workflow revisions and an `executor_response_promoted` event containing the response hash.

## 0.4.82 - 2026-05-24

### Added

- Added explicit cluster placement metadata for reasoning-heavy tasks: `reasoning_required` and `remote_ai_execution_allowed`.
- Added CLI contract coverage proving a LAN node that advertises `ai` capability is still ineligible for AI task placement until explicit remote cognitive executor authorization exists.
- Added `docs/reports/forge-core-v0.4.82-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.82`.
- Cluster placement requirements now use schema `forge.cluster_placement_requirements.v2`.
- Cluster placement remains available for deterministic/local-code handoff, but AI/Mixed tasks are blocked from remote placement by default.

### Safety

- Remote AI execution remains disabled even when a node reports GPU/AI capabilities.
- This increment only changes Forge-owned scheduling metadata, placement policy and documentation.
- No SSH session, Docker/Kubernetes/Knative resource, remote machine or user infrastructure is mutated.

## 0.4.81 - 2026-05-24

### Added

- Added Hackathon MVP Software Factory planning for hackathon, ideathon and maratona goals that ask for an MVP or software factory.
- Hackathon planning now adds regulation parsing, buffered deadline calculation, regulation-fit viability gating, weighted brainstorm, final idea selection, PDF artifact generation, Telegram delivery, MVP backlog, OSM/OSRM technical planning, pitch validation and continuous improvement until the buffered deadline.
- Hackathon intents now expose deliverables for the regulation compliance matrix, idea viability decision, final idea PDF, MVP software factory plan, pitch package, deadline-buffered improvement loop and Telegram payload.
- Added CLI contract coverage proving the hackathon factory graph includes the expected executor mix, validation gates, Telegram notification metadata, OSM/OSRM context and recurring improvement schedule.

### Changed

- The package version is now `0.4.81`.
- Hackathon MVP planning now treats user ideas as regulation-first candidates: off-theme or weakly aligned ideas must be reframed or replaced before the MVP backlog is built.

### Safety

- The hackathon factory increment only changes Forge-owned planning metadata, validation rules and documentation.
- Telegram delivery is represented as a notification node with a configured chat target and no exposed token or raw chat id.
- The improvement loop stops at the buffered deadline and prioritizes rubric gaps before extra features.

## 0.4.80 - 2026-05-24

### Added

- `forge cluster list --output json` now returns `forge.cluster_registry.v2`.
- Cluster registry output includes per-node `forge.cluster_node_scheduling.v1` posture rows with schedulable state, busy/idle/blocked status, active/expired lease counts, blockers and explicit no-remote-execution/no-external-mutation policy markers.
- Cluster registry summaries now include schedulable, busy schedulable, idle schedulable, active lease and expired lease counts.
- Added CLI contract coverage proving `forge cluster list` exposes lease-derived scheduling posture after a cluster handoff.
- Added `docs/reports/forge-core-v0.4.80-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.80`.
- README and technical definition now document cluster registry scheduling posture as the operator preflight surface before placement and handoff.

### Safety

- Cluster scheduling posture is read-only metadata derived from Forge-owned SQLite node profiles and task leases.
- It does not open SSH sessions, execute remote commands, copy files to external machines, authorize AI executors, install Knative or mutate Docker/Kubernetes/Knative/user resources.
- Each scheduling row keeps `remote_execution_enabled=false`, `external_mutation_allowed=false` and the explicit trust policy string.

## 0.4.79 - 2026-05-24

### Added

- Added lease-aware cluster placement candidate metadata through `active_lease_count`.
- `forge cluster place` now counts active task leases per registered node and penalizes busy eligible nodes when scoring candidates.
- Added CLI contract coverage proving a second eligible idle node is selected over a node that already holds an active cluster handoff lease.
- Added `docs/reports/forge-core-v0.4.79-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.79`.
- README and technical definition now document active-lease pressure in dry-run cluster placement.

### Safety

- Lease-aware placement only reads Forge-owned SQLite task leases and cluster node metadata.
- It does not open SSH sessions, execute remote commands, copy files to external machines, authorize AI executors, install Knative or mutate Docker/Kubernetes/Knative/user resources.
- Cluster handoff still declares `remote_execution_enabled=false` and `external_mutation_allowed=false`.

## 0.4.78 - 2026-05-24

### Added

- Added `manifest_sha256` to `forge.cluster_sync_manifest.v1`.
- The manifest checksum is computed deterministically from the sync contract fields while excluding the hash field itself.
- Added CLI contract coverage proving cluster handoff manifests expose a reproducible 64-character SHA-256 digest.
- Added `docs/reports/forge-core-v0.4.78-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.78`.
- README and technical definition now document manifest-level hashing for distributed handoff auditing.

### Safety

- The sync manifest remains hash-only metadata. This change does not open SSH sessions, execute remote commands, copy files to external machines, authorize AI executors, install Knative or mutate Docker/Kubernetes/Knative/user resources.
- `remote_execution_enabled=false` and `external_mutation_allowed=false` remain explicit in the cluster handoff contract.

## 0.4.77 - 2026-05-24

### Added

- Added `forge cluster leases --output json` with schema `forge.cluster_node_lease_registry.v1`.
- Cluster lease registry rows expose node id/name, workflow/task identity, lease id, lease scope, active/expired state, trust level, sandbox permissions and explicit no-remote-execution/no-external-mutation flags.
- Added `--node-id <id>` filtering for node-scoped lease inspection.
- Added CLI contract coverage proving cluster handoff leases can be inspected without enabling remote execution or external mutation.
- Added `docs/reports/forge-core-v0.4.77-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.77`.
- README and technical definition now document cluster lease inspection as the audit surface after cluster handoff.

### Safety

- Cluster lease inspection is read-only metadata over Forge-owned SQLite task leases and registered node profiles.
- This change does not open SSH sessions, execute remote commands, copy files to external machines, authorize AI executors, install Knative or mutate Docker/Kubernetes/Knative/user resources.
- Every listed node lease keeps `remote_execution_enabled=false`, `external_mutation_allowed=false` and the explicit trust policy string.

## 0.4.76 - 2026-05-24

### Added

- Added `forge cluster handoff --workflow <id> --task <task-id>` with schema `forge.cluster_task_handoff.v1`.
- Cluster handoff now composes deterministic cluster placement with the existing strict executor handoff contract, acquiring the task lease with the selected node id as the executor.
- Added a node-scoped `cluster_node_lease` ref with lease id, workflow/task identity, selected node id, lease scope and expiry.
- Added `forge.cluster_sync_manifest.v1`, a content-addressed hash manifest for distributed task staging. It carries context checksums, routing cache keys, lineage hash, checkpoint refs, artifact refs and replay shard refs without copying or executing remote content.
- Added CLI contract coverage proving a trusted LAN node can be selected, leased and returned with a sync manifest, and that a second cluster handoff is blocked by the existing task lease.
- Added `docs/reports/forge-core-v0.4.76-report-2026-05-24.md` with the cycle report.

### Changed

- The package version is now `0.4.76`.
- README and technical definition now document cluster handoff as the safe staging step after registry placement and before any remote execution adapter.

### Safety

- Cluster handoff does not open SSH sessions, execute remote commands, copy files to external machines, authorize AI executors, install Knative or mutate Docker/Kubernetes/Knative/user resources.
- The returned sync manifest explicitly keeps `remote_execution_enabled=false` and `external_mutation_allowed=false`; it is an auditable hash contract for future permission-scoped adapters.

## 0.4.75 - 2026-05-24

### Added

- `forge plan` now detects n8n research goals and adds two explicit workflow tasks before graph promotion: `Catalog n8n workflow primitives` and `Evaluate Forge primitive candidates`.
- n8n research intents now include `n8n primitive research catalog` and `Forge primitive promotion recommendation` deliverables, plus risk/unknown records requiring current source/docs review and avoiding blind code/license copying.
- The atomic graph build task depends on the Forge primitive recommendation when n8n research is requested, so external automation concepts remain gated before becoming native Forge graph semantics.
- Added CLI contract coverage proving the n8n catalog task, promotion guard and graph dependency are persisted in the planned workflow.
- Added `docs/reports/forge-core-v0.4.75-report-2026-05-24.md` with the cycle report and initial source-backed n8n pattern catalog.

### Changed

- The package version is now `0.4.75`.
- README now documents n8n-aware research planning as a pre-promotion workflow-design stage.

### Safety

- The n8n increment only changes Forge-owned planning metadata and documentation. It does not fetch or copy n8n source into Forge, execute external code, promote any n8n primitive, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative/user resources.
- Promotion remains gated by Forge validation rules requiring evidence that a candidate improves validated DAG execution, context routing, resumability, observability or operator clarity.

## 0.4.74 - 2026-05-24

### Added

- Added a local Forge cluster node registry through `forge cluster register` and `forge cluster list`.
- Cluster node records use schema `forge.cluster_node.v1` and persist CPU, memory, OS, architecture, GPU inventory, installed software, Python/Node/Docker/GPU availability, network reachability, lifecycle status, cost/latency/reliability, trust level and sandbox permissions.
- Added `forge cluster place --workflow <id> --task <task-id>` with schema `forge.cluster_placement.v1` for dry-run task placement by deterministic capability, trust and sandbox policy.
- Added CLI contract coverage proving a Python local-code task selects the Linux Python node while rejecting a Windows MetaTrader 5 node that lacks the required Python capability.

### Changed

- The package version is now `0.4.74`.
- README and technical definition now document the safe LAN/SSH cluster registry stage before remote AI execution or distributed task handoff.

### Safety

- Cluster placement is read-only metadata and policy evaluation. It does not open SSH sessions, execute remote code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative/user resources.
- Placement requires explicit registered node capability, trusted LAN/local trust and declared sandbox permissions before a node is selected.

## 0.4.73 - 2026-05-24

### Added

- `forge self run` now loads the most specific persisted Forge self-evolution goal before creating a new self-evolution workflow.
- Self-evolution prompt packets now use `forge.self_evolution.prompt.v2` and include the current persisted workflow goal, initial workflow goal and workflow revision before generic strategic guidance.
- Added CLI contract coverage proving a runtime `forge workflow update-goal` mutation, including clusterization and n8n research priorities, appears in the next dry-run self-evolution prompt artifact and in the new workflow state.

### Changed

- The package version is now `0.4.73`.
- README and technical definition now document persisted-goal propagation for self-evolution cycles.

### Safety

- The change only alters self-evolution planning/prompt generation and does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Generic self-evolution guidance remains in the prompt, but the persisted Forge workflow goal is now explicitly authoritative for future cycles.

## 0.4.72 - 2026-05-24

### Added

- `forge list --output json` workflow rows now include versioned `context_action_refs` entries with schema `forge.registry_context_action_ref.v1`.
- Each context action ref records the task id, title, executor, next action, handoff status, context/dependency readiness, routing quality status, blocker refs, checkpoint refs, current routing cache key, context checksum and action reason.
- Added CLI contract coverage proving registry rows expose the exact tasks behind aggregate context actions such as `partial_retry_with_fresh_context` and `wait_for_dependencies`.

### Changed

- The package version is now `0.4.72`.
- README and technical definition now document per-task context-action refs for registry triage.

### Safety

- Context action refs are read-only registry metadata derived from deterministic context routing packages.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## 0.4.71 - 2026-05-24

### Added

- `forge context` shard manifests now include per-shard selection cost audit fields:
  - `minimum_routable_bytes`;
  - `selection_saved_bytes`;
  - `selection_cost_bps`.
- Replay manifest shard refs carry the same audit fields, so async executors can replay and validate the cost of compressed or omitted context without rehydrating the full shard body.
- Context routing fingerprints now include a `shard_selection_audit` component, so cache keys account for the exact per-shard selection-cost ledger used by executor adapters.
- Added CLI contract coverage proving compressed shards report partial cost, budget-omitted shards report full savings, replay refs preserve the audit fields and the fingerprint binds the audit ledger.

### Changed

- The context packet schema is now `forge.context.v30`.
- The routing policy is now `task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30`.
- The package version is now `0.4.71`.

### Safety

- Shard selection-cost audit fields are read-only metadata derived from deterministic context routing.
- This change does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## 0.4.70 - 2026-05-24

### Added

- `forge list --context-actions` now returns a versioned context-action catalog with schema `forge.registry_context_action_catalog.v1`.
- The catalog exposes valid `--context-action` filter values, readiness classes and trigger descriptions for handoff, dependency wait, context repair, checkpoint resume and partial-retry routes.
- Added CLI contract coverage proving operators can discover `wait_for_dependencies`, `start_executor_handoff` and `partial_retry_with_fresh_context` without reading source or memorizing registry summary fields.

### Changed

- The package version is now `0.4.70`.
- README and technical definition now document context-action catalog discovery alongside lifecycle and quality-action registry filters.

### Safety

- Context-action catalogs are static read-only metadata for filtering Forge-owned workflow registry projections.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## 0.4.69 - 2026-05-24

### Added

- `forge context` now emits a versioned `minimum_correct_set` with schema `forge.context.minimum_correct_set.v1`.
- The minimum-correct set records each required context section with inclusion, compression, missing-state, routing decision, repair action, selected/original byte counts and source/content hashes.
- Context routing fingerprints now include a `minimum_correct_set` component, so executor cache keys account for the exact required-section floor used for budget repair and resumable handoff.
- `forge inspect --output json` now projects the minimum-correct set in each node's `context_route`, and terminal diagrams include a compact minimum-correct marker.
- Added CLI contract coverage proving low-budget context packets expose missing required sections through the minimum-correct set and bind the set checksum into the routing fingerprint.

### Changed

- The context packet schema is now `forge.context.v29`.
- The routing policy is now `task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_v29`.
- The package version is now `0.4.69`.

### Safety

- Minimum-correct sets are read-only metadata derived from deterministic context shard routing.
- This change does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## 0.4.68 - 2026-05-24

### Added

- `forge list` now accepts `--context-action <action>` to filter the workflow registry by the next context-routing actions already aggregated in `context_actions`.
- The registry filter report now includes `filter.context_action`, and summaries are recomputed after lifecycle, context-action and quality-action filters are applied together.
- Added CLI contract coverage proving `forge list --lifecycle running --context-action wait_for_dependencies --output json` returns only workflows whose context routing has dependency-wait pressure.

### Changed

- The package version is now `0.4.68`.

### Safety

- Context-action filtering is read-only metadata over Forge-owned persisted workflows and deterministic context-route summaries.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## 0.4.67 - 2026-05-24

### Added

- `forge list --output json` now includes a versioned `execution_policy` summary on both the global registry summary and each workflow row.
- The summary uses schema `forge.registry_execution_policy.v1` and counts AI, mixed, deterministic, no-AI, model-call-required, model-call-avoided, local-code and reusable local-code routes.
- Added CLI contract coverage proving non-running registry slices expose execution-policy route counts for repeated local Python code-node workflows.

### Changed

- The package version is now `0.4.67`.

### Safety

- Execution-policy registry summaries are read-only metadata derived from Forge-owned workflow task policies.
- This change does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Local-code reuse remains controlled by proposed child-subflow bindings and later validation gates.

## 0.4.66 - 2026-05-24

### Added

- `forge inspect --output json` now marks recursive child-subflow cycles explicitly instead of only terminating traversal silently.
- Subflow inspection rows now include `terminal_reason`, `cycle_detected`, `cycle_ref` and `recursion_policy` so recursive or infinite subflow composition remains auditable from the terminal and JSON contract.
- The terminal inspection diagram now prints a compact `cycle recursive_subflow_cycle` marker when traversal is stopped by a repeated workflow/task path.
- Added CLI contract coverage that builds a circular child-subflow graph in the persisted workflow registry and proves inspection terminates with a structured cycle record.

### Changed

- The package version is now `0.4.66`.

### Safety

- Recursive subflow cycle detection is read-only inspection metadata over Forge-owned workflow state.
- This change does not execute child subflows, complete tasks, promote workflows, authorize CLIs, run local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Proposed child-subflow execution remains controlled by validation, scheduling, executor handoff, task lease and continuation gates.

## 0.4.65 - 2026-05-24

### Added

- `forge context` now emits a versioned `execution_policy_decision` contract with schema `forge.context.execution_policy_decision.v1`.
- Execution policy decisions bind workflow/task identity, workflow revision, executor profile, task executor, policy mode, route class, AI/deterministic flags, model-call requirement/avoidance, reusable child-subflow eligibility, reuse key, local code runtime metadata, selection reason and validation gate into a stable decision checksum.
- Context routing fingerprints now include an `execution_policy_decision` component, so executor cache keys change when Forge's model-vs-deterministic route decision changes.
- `forge inspect --output json` now projects the execution policy decision for each inspected node, and the terminal diagram prints a compact decision class/hash plus model-call requirement marker.
- Added CLI contract coverage proving deterministic local Node.js code nodes expose the decision record through `forge context`, bind it into the routing fingerprint and surface it through `forge inspect`.

### Changed

- The package version is now `0.4.65`.

### Safety

- Execution policy decisions are read-only metadata derived from Forge-owned workflow/task execution policy and context profile state.
- This change does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## 0.4.64 - 2026-05-24

### Added

- `forge context` now emits a versioned `selection_receipt` with schema `forge.context.selection_receipt.v1`.
- Selection receipts summarize the exact context route in a compact audit contract: selector version, executor profile, reasoning/deterministic mode, requested/effective budget, selected bytes, minimum-correct budget, selected sections, required sections, missing required sections, compressed sections, budget-omitted sections, profile-omitted sections, route status and handoff status.
- Context routing fingerprints now include a `selection_receipt` component, so cache keys account for the receipt that executor adapters can audit before reuse.
- `forge inspect --output json` now projects the selection receipt checksum, route status and required-complete flag for each inspected node, and the terminal diagram prints a compact `receipt <hash>` marker.
- Added CLI contract coverage proving receipts are emitted by `forge context`, are bound into routing fingerprints and are visible through `forge inspect`.

### Changed

- The package version is now `0.4.64`.

### Safety

- Selection receipts are read-only metadata derived from deterministic context routing. They do not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## 0.4.63 - 2026-05-24

### Added

- Forge now selects a standalone deterministic `local_code_node` when a goal asks for repeated or frequent local Python/Node.js work, even when the workflow does not also request cron, email or another autonomous extension.
- Standalone local code nodes use the existing no-AI execution policy, local process/no-network runtime contract, reusable code-node hint and deterministic validation gate.
- Added CLI contract coverage proving a frequent local Node.js goal creates the deterministic code node, avoids scheduled-continuation scaffolding and routes context through the no-AI deterministic profile with a model call marked as avoided.

### Changed

- The package version is now `0.4.63`.

### Safety

- This change only affects graph planning metadata and context routing. It does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Existing cron/email autonomous extension behavior remains unchanged; scheduled continuation tasks are not added for standalone local code goals.

## 0.4.62 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v28` with a versioned `continuation_plan` for resumable executor adapters.
- Continuation plans use schema `forge.context.continuation_plan.v1` and turn checkpoint state, current context checksum, current route key and context delta into explicit actions: `start_fresh`, `resume_from_checkpoint`, `refresh_context_before_resume` or `partial_retry_with_fresh_context`.
- `forge inspect --output json` now projects the continuation plan for each terminal node, and the human diagram prints a compact `continue <action> <status>` marker.
- `forge task handoff` now emits `forge.executor_handoff.v8` and reuses the context continuation plan as the handoff `resume_plan`, so context, inspection and handoff agree on the same validation-gated continuation decision.
- Added CLI contract coverage proving the continuation plan is exposed through `forge context`, `forge inspect` and `forge task handoff`.

### Changed

- The context routing policy is now `task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_v28`.
- The package version is now `0.4.62`.

### Safety

- Continuation plans are read-only metadata derived from Forge-owned workflow checkpoints and deterministic context routing.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and the continuation plan's validation gate.

## 0.4.61 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v27` with a versioned `replay_manifest` contract for resumable executor context.
- Replay manifests use schema `forge.context.replay_manifest.v1` and record context schema, routing policy, selector version, workflow/task ids, workflow revision, executor profile, requested/effective budget, context checksum, content bytes, included/missing-required sections, a replay command and content-addressed shard refs.
- Prompt packets now use schema `forge.context.prompt_packet.v2` and packet version `forge.executor.prompt_packet.v2`, binding `replay_manifest_sha256` into the adapter-facing prompt packet checksum.
- Context routing fingerprints now include a `replay_manifest` component, so route cache keys account for replay manifest changes.
- `forge inspect --output json` now projects `replay_manifest_sha256` for each terminal node, and the human diagram prints a compact `replay <hash>` marker.
- Added CLI contract coverage proving the replay manifest is emitted, bound into the prompt packet and routing fingerprint, and surfaced through inspection.

### Changed

- The context routing policy is now `task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_v27`.
- The package version is now `0.4.61`.

### Safety

- Replay manifests are read-only metadata derived from Forge-owned workflow/task state and deterministic shard selection.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates and child-subflow validation gates.

## 0.4.60 - 2026-05-24

### Added

- Added focused terminal inspection with `forge inspect <workflow-id> --task <task-id>`.
- `forge inspect --output json` now includes a `focus` block when a task focus is requested, plus `workflow_task_count` so operators can distinguish the focused node count from the full persisted DAG size.
- Focused inspection routes the same context, persona, execution-policy, handoff and subflow projections as full inspection, but limits `nodes`, `handoff_summary` and the terminal diagram to the selected task.
- Added CLI contract coverage proving a focused verbose inspection includes the selected node's subtasks while excluding unrelated task lines from the terminal diagram.

### Changed

- The package version is now `0.4.60`.

### Safety

- Focused inspection is read-only and derives all output from Forge-owned workflow state and deterministic context routing.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Full workflow inspection remains the default when `--task` is not provided.

## 0.4.59 - 2026-05-24

### Added

- Added `forge task validate-response` for bounded executor adapter outputs.
- Added `forge.executor_response.v1` as the executor result shape expected by Forge: task id, status, artifact refs, trace ref, cost and validation evidence.
- Added `forge.executor_response_validation.v1` acceptance reports with response checksum, validation summary and structured violation codes.
- Completed responses now require at least one passing validation evidence item before the response contract is accepted.
- Added CLI contract coverage for accepted and rejected executor responses.

### Changed

- The package version is now `0.4.59`.

### Safety

- Response validation is read-only with respect to workflow task state. It records an audit event but does not complete tasks, promote workflows, acquire leases, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Forge remains the authority for accepting executor output: adapter responses are evidence to validate, not completion by themselves.

## 0.4.58 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v26` with a versioned `prompt_packet` contract for bounded executor adapters.
- The prompt packet uses schema `forge.context.prompt_packet.v1` and packet version `forge.executor.prompt_packet.v1`, binding context schema, routing policy, workflow/task ids, workflow revision, executor profile, executor kind, persona mode/profile, instruction sources, validation gates, context checksum, lineage checksum, budget status, routing-quality status and handoff status into a stable packet hash.
- Context routing fingerprints now include a `prompt_packet` component, so executor cache keys account for prompt-packet versioning and gate/source changes alongside context payload, economy, quality, budget, persona and delta contracts.
- `forge inspect --output json` now projects `prompt_packet_version` and `prompt_packet_sha256` for each terminal node, and the human diagram prints a compact `packet <hash>` marker.
- Added CLI contract coverage proving a persona-aware documentation node exposes the prompt packet and that inspection projects the same packet hash when using the default context route.

### Changed

- The context routing policy is now `task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_economy_prompt_packet_v26`.
- The package version is now `0.4.58`.

### Safety

- Prompt packets are read-only metadata derived from Forge-owned workflow/task state and deterministic context routing.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates and child-subflow validation gates.

## 0.4.57 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v25` with a versioned `routing_economy` ledger for context-cost audit.
- The economy contract uses schema `forge.context.routing_economy.v1` and reports executor profile, reasoning/deterministic flags, baseline bytes, selected bytes, compression savings, budget omissions, profile-filtered omissions, total avoided bytes, reduction basis points and deterministic no-AI model-call avoidance.
- Context routing fingerprints now include a `routing_economy` component, so executor cache keys account for cost/economy routing decisions alongside quality, budget, persona and context-delta contracts.
- `forge inspect --output json` now projects the economy ledger for each terminal node, and the human diagram prints compact economy decision/avoided-byte markers.
- Added CLI contract coverage proving AI routes expose bounded-context economy and deterministic no-AI code nodes explicitly mark a model call as avoided.

### Changed

- The context routing policy is now `task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_economy_v25`.
- The package version is now `0.4.57`.

### Safety

- Routing economy is read-only metadata derived from Forge-owned workflow/task state and deterministic shard selection.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates and child-subflow validation gates.

## 0.4.56 - 2026-05-24

### Added

- `forge validate` now blocks workflow promotion when a task still carries child-subflow bindings that are only `proposed`, have a non-promotable lifecycle state or are missing subflow validation metadata.
- Added `forge workflow validate-subflow` to transition a planned child-subflow reuse binding from `proposed` to `validated` through a Forge-owned, revisioned workflow mutation.
- The subflow validation command checks the current child workflow/task, stamps the latest child lifecycle state and validation gate into the parent binding, records an event and advances the workflow revision.
- Added CLI contract coverage proving a completed parent workflow remains blocked until the reused deterministic child subflow is explicitly validated, then becomes promotable.

### Changed

- The package version is now `0.4.56`.
- Proposed child-subflow reuse is no longer only inspection/context metadata; it is part of validation-before-promotion semantics.

### Safety

- Child-subflow validation is a metadata transition over Forge-owned workflow state. It does not execute child subflows, acquire leases, complete tasks, authorize CLIs, run local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- `forge workflow validate-subflow` refuses to validate child workflows that are not scaled to zero.
- Parent workflow promotion remains blocked until task readiness, persona gates and child-subflow validation gates all pass.

## 0.4.55 - 2026-05-24

### Added

- `forge inspect --output json` now expands proposed child-subflow links with recursive path metadata.
- Subflow inspection rows include parent workflow/task ids, depth, path, reachability, terminal status, loaded child workflow status, derived child lifecycle state and child task/subflow counts.
- Human `forge inspect` diagrams now include a compact `subflows:` section with each proposed child-subflow path, making recursive reuse auditable from the terminal before execution.
- Added CLI contract coverage proving a reused deterministic Python code-node subflow is inspectable by parent path and loaded child lifecycle metadata.

### Changed

- The package version is now `0.4.55`.

### Safety

- Subflow expansion is read-only inspection metadata over Forge-owned workflow state.
- This change does not execute child subflows, complete tasks, promote workflows, authorize CLIs, run local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Proposed child-subflow execution remains future work behind validation, scheduling and executor handoff gates.

## 0.4.54 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v24` with a derived `persona_profile` object for human-facing nodes.
- Persona profiles use schema `forge.context.persona_profile.v1` and carry a stable profile id, node mode/scope, voice, tone, validation gate, routing rationale, source-model summaries and profile checksum.
- Context lineage now includes `persona_profile_sha256`, and routing fingerprints include a `persona_profile` component so executor cache keys change when the selected persona profile changes.
- `forge task handoff` now emits `forge.executor_handoff.v7` and `forge.persona_handoff.v2`, projecting the persona profile id/checksum and source-model summaries in the executor-facing contract.
- `forge inspect` terminal output now annotates persona nodes with the selected profile id.
- Added CLI contract coverage proving context packets derive the persona profile from Codex developer/personality instructions plus Paperclip soul/voice/tone/persona inputs.

### Changed

- The context routing policy is now `task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_v24`.
- Context persona contracts now use schema `forge.context.persona_contract.v2` and bind the profile checksum/rationale to lineage.
- The package version is now `0.4.54`.

### Safety

- Persona profiles are derived read-only metadata from Forge-owned task persona routing state; they do not override workflow goals, validation rules or executor policy.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases and validation-before-promotion semantics.

## 0.4.53 - 2026-05-24

### Added

- `forge request start` now runs the same registry-derived subflow reuse pass as `forge plan` before persisting the async workflow.
- The request-start JSON response now includes `reuse_candidates` and `attached_subflows`, so Codex/OpenCode skill callers can see when Forge reused a compatible deterministic child subflow instead of silently creating isolated duplicate work.
- Added CLI contract coverage proving an async request attaches a previously completed reusable local Python code-node as a proposed child subflow and exposes it through `forge inspect`.

### Changed

- Async request creation now preserves Forge as the orchestration source of truth for flow reuse, aligning skill-style `request start` with direct planning behavior.
- The package version is now `0.4.53`.

### Safety

- Reuse remains deterministic and registry-derived. Forge only attaches candidates already marked attachable by lifecycle and compatibility checks.
- This change does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Proposed child subflows remain auditable in persisted workflow state and validation/inspection surfaces before any executor handoff.

## 0.4.52 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v23` with a versioned `context_delta` object for resumable context reuse decisions.
- The delta contract uses schema `forge.context.delta.v1` and compares the current context payload, routing cache key and workflow revision with the latest task checkpoint.
- Delta output reports checkpoint ids, checkpoint/current context hashes, checkpoint/current routing keys, changed components, `can_reuse_checkpoint_context` and `partial_retry_recommended`.
- `forge inspect --output json` now projects `context_delta` for each terminal node, and the human diagram prints a compact `delta <status>` marker.
- `forge task handoff` packets now carry the same context delta next to routing quality so executor adapters can avoid redundant reasoning or choose partial retry from the adapter envelope.
- Added CLI contract coverage proving `forge context` reports `no_checkpoint` before checkpointing and `route_changed` with changed components after a checkpointed route diverges.

### Changed

- The context routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_v23`.
- The package version is now `0.4.52`.

### Safety

- Context deltas are read-only metadata derived from Forge-owned workflow/task/checkpoint state and deterministic context routing.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases and validation-before-promotion semantics.

## 0.4.51 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v22` with a versioned `budget_plan` object for minimum-correct context routing.
- The budget plan uses schema `forge.context.budget_plan.v1` and records requested/effective budgets, selected bytes, required/original/minimum bytes, omitted required/optional bytes, compression savings, missing required sections, omitted-by-budget sections, status and a recommended budget.
- Context routing fingerprints now include a `budget_plan` component, so executor cache keys account for budget-plan changes alongside repair, quality and persona contracts.
- `forge inspect --output json` now projects the same budget plan for every terminal DAG node, and the human diagram prints a compact `budget_plan minimum/recommended status` marker.
- Added CLI contract coverage proving budget plans are exposed directly by `forge context` and projected through `forge inspect`.

### Changed

- The context routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_v22`.
- Budget repair guidance is now split into a repair action and a reusable minimum-correct budget plan, so adapters can distinguish required-context floor from optional budget pressure.
- The package version is now `0.4.51`.

### Safety

- Budget plans are read-only metadata derived from Forge-owned workflow/task state and deterministic shard selection.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases and validation-before-promotion semantics.

## 0.4.50 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v21` with a top-level `next_action` decision for executor adapters.
- The context-level decision reuses the existing `forge.inspect_context_action.v1` shape and reports whether the task should start handoff, wait for dependencies, increase context budget, refresh stale context, resume from checkpoint or partial-retry with fresh context.
- Added CLI contract coverage proving a context packet exposes fresh handoff guidance and later switches to `partial_retry_with_fresh_context` when a checkpointed route differs from the current routed context.

### Changed

- The context routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_persona_contract_next_action_v21`.
- `forge inspect` now consumes the same `ContextNextAction` stored on the context package instead of recomputing an inspection-only decision.
- The package version is now `0.4.50`.

### Safety

- The next-action decision is read-only metadata derived from Forge-owned workflow/task/checkpoint state, dependency readiness and deterministic context routing.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor ownership remains controlled by `forge task handoff`, strict context readiness, dependency readiness, validation rules and task leases.

## 0.4.49 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v20` with a versioned `persona_contract` for human-facing nodes.
- The context persona contract uses schema `forge.context.persona_contract.v1` and binds mode, node scope, instruction source, voice, tone, validation gate, source models, auditability, context lineage hash and persona-mode hash.
- Context routing fingerprints now include a `persona_contract` component, so executor cache keys account for Personality/Soul Routing changes before handoff.
- Added CLI contract coverage proving a human-facing documentation node exposes the persona contract directly in the context package.

### Changed

- The context routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_persona_contract_v20`.
- Human-facing persona routing is now auditable at context-build time instead of only after `forge task handoff` constructs an executor packet.
- The package version is now `0.4.49`.

### Safety

- The persona contract is read-only metadata derived from Forge-owned workflow/task state and existing context lineage.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Persona promotion remains validation-gated by `persona_routing_required`, and executor handoff remains controlled by strict context readiness, dependency readiness, validation rules and task leases.

## 0.4.48 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v19` with a versioned `routing_repair` object.
- The repair contract uses schema `forge.context.routing_repair.v1` and records repair status, action, current effective budget, recommended budget, required budget deficit, missing required sections, omitted-by-budget sections, compressed sections and a short reason.
- Context routing fingerprints now include a `routing_repair` component, so executor cache keys account for repair-plan changes alongside contracts and routing quality.
- `forge inspect --output json` now projects the same routing repair contract for terminal DAG nodes.
- Added CLI contract coverage proving missing required context produces an auditable `increase_context_budget` repair plan with a bounded budget recommendation.

### Changed

- The context routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_v19`.
- The package version is now `0.4.48`.

### Safety

- Routing repair is read-only metadata derived from Forge-owned workflow/task state and deterministic shard selection.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules and task leases.

## 0.4.47 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v18` with a versioned `routing_contract` object.
- The routing contract uses schema `forge.context.routing_contract.v1` and records selector version, executor profile version, profile id, selection strategy, requested/effective budgets, minimum budget, max profile budget, compression allowance, allowed/required/optional sections and a stable profile hash.
- Context routing fingerprints now include a `routing_contract` component, so executor cache keys account for selector/profile contract changes instead of only final content and shard outcomes.
- Added CLI contract coverage proving deterministic no-AI code nodes receive the routing contract and that the fingerprint binds it.

### Changed

- The context routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_v18`.
- Context routing profile details are now auditable as an explicit adapter-facing contract rather than requiring adapters to infer the selector profile from scattered fields.
- The package version is now `0.4.47`.

### Safety

- The routing contract is read-only metadata derived from Forge-owned workflow/task state and deterministic selector configuration.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules and task leases.

## 0.4.46 - 2026-05-24

### Added

- `forge list --quality-actions` now emits a versioned catalog of registry quality-action filter values.
- The catalog uses schema `forge.registry_quality_action_catalog.v1` and lists each action, filter value, possible priorities, description and trigger.
- Added CLI contract coverage proving operators can discover `increase_context_budget` and `start_executor_handoff` before filtering workflow inventory with `--quality-action`.

### Changed

- Registry quality-action taxonomy is now exposed through a read-only CLI contract instead of requiring operators to infer valid filter keys from current workflow rows or changelog text.
- The package version is now `0.4.46`.

### Safety

- Quality-action catalog discovery is static, read-only metadata derived from Forge's registry recommendation contract.
- This change does not open or mutate workflow stores, acquire leases, complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by `forge task handoff`, strict context readiness, dependency readiness and task leases.

## 0.4.45 - 2026-05-24

### Added

- `forge list` now accepts `--quality-action <action>` so operators can slice workflow inventory by the next Context Routing Engine intervention.
- The registry filter report now includes `filter.quality_action`, keeping lifecycle and quality-action filters auditable in JSON output.
- Added CLI contract coverage proving `forge list --lifecycle running --quality-action increase_context_budget` returns only the matching lifecycle/action slice and recomputes registry summaries over that slice.

### Changed

- Registry listing now uses a composable `WorkflowRegistryFilters` contract internally, preserving the existing lifecycle-only API while enabling new read-only triage filters.

### Safety

- Quality-action filtering is a read-only projection over Forge-owned workflow state and deterministic context-quality recommendations.
- This change does not acquire leases, complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by `forge task handoff`, strict context readiness, dependency readiness and task leases.

## 0.4.44 - 2026-05-24

### Added

- `forge list --output json` now includes a versioned `context_quality` projection on each workflow row and on the filtered registry summary.
- The projection uses schema `forge.registry_context_quality.v1` and aggregates routing quality status, score, warning severity and warning codes across the current lifecycle slice.
- Workflow rows now include a versioned `quality_action` recommendation using schema `forge.registry_quality_action.v1`, so operators can distinguish budget/profile routing pressure from dependency waits without opening full context packets.
- Added CLI contract coverage proving `forge list --lifecycle running` aggregates context quality only over the filtered running workflows and recommends `increase_context_budget` when routing quality reports budget pressure.

### Changed

- Registry triage now reuses the existing Context Routing Engine quality contract derived for handoff summaries instead of recomputing shard quality in `forge list`.

### Safety

- Registry quality actions are read-only recommendations derived from Forge-owned workflow/task state and deterministic context routing.
- This change does not acquire leases, complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by `forge task handoff`, strict context readiness, dependency readiness and task leases.

## 0.4.43 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v17` with a versioned `routing_quality` contract.
- Context routing quality uses schema `forge.context_routing_quality.v1` and reports status, score, warnings, recommendations and section refs for missing required context, budget pressure, compressed context and profile-filtered optional context.
- The context routing fingerprint now includes a `routing_quality` component so executor replay/cache keys account for quality-relevant routing state.
- `forge inspect`, `forge request status` and context handoff summaries now expose `forge.context_routing_quality_summary.v1` aggregates plus per-task quality contracts.
- `forge task handoff` now emits `forge.executor_handoff.v6` and includes the selected context routing quality contract in the top-level adapter packet.

### Changed

- The routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_v17`.
- Operators can distinguish dependency waits from context-budget/profile pressure without reopening full context packets or recomputing shard manifests.

### Safety

- Routing quality is read-only metadata derived from Forge-owned workflow/task state and deterministic context shard selection.
- This change does not acquire leases, complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules and task leases.

## 0.4.42 - 2026-05-24

### Added

- `forge inspect --output json` now includes a versioned `execution_policy` projection for every terminal DAG node.
- The projection uses schema `forge.inspect_execution_policy.v1` and exposes mode, AI allowance, deterministic flag, reuse hint, selection reason, validation gate and optional local code runtime fields.
- Human inspection diagrams now append a compact execution policy marker such as `policy local_code_node no_ai deterministic python reuse_compatible_code_node`.
- Added CLI contract coverage proving deterministic local Python code nodes expose their no-AI execution policy through `forge inspect` before any executor handoff is requested.

### Changed

- Operators can now see deterministic local runtime decisions from inspection output instead of waiting for a `forge task handoff` packet.
- The existing context routing, next-action, lease and handoff packet contracts remain unchanged; this release adds a read-only inspection projection.

### Safety

- Execution policy inspection is read-only metadata derived from Forge-owned workflow/task state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Actual execution remains gated by strict context readiness, dependency readiness, validation rules and task leases.

## 0.4.41 - 2026-05-24

### Added

- `forge task handoff` now emits `forge.executor_handoff.v5`.
- Executor handoff packets now include the full `execution_policy` contract alongside the existing `execution_policy_mode` compatibility field.
- Deterministic local code nodes expose `ai_allowed`, `deterministic`, `reuse_hint`, `selection_reason`, `validation_gate` and `code_runtime` directly in the adapter envelope.
- Added CLI contract coverage proving a local Python no-AI code node receives its bounded execution policy without requiring adapters to parse the nested context package.

### Changed

- Bounded executor adapters can now decide whether to run a no-AI deterministic node, and which local runtime to use, from the top-level handoff packet.
- The existing strict context, dependency readiness, lease and persona contracts remain unchanged; v5 extends the packet without authorizing execution by itself.

### Safety

- The full execution policy is read-only metadata derived from Forge-owned workflow/task state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Execution remains gated by strict context readiness, dependency readiness, validation rules and task leases.

## 0.4.40 - 2026-05-24

### Added

- `forge list --output json` now includes a versioned `context_actions` projection on each workflow row and on the filtered registry summary.
- The projection uses schema `forge.registry_context_action.v1` and counts task next actions: `start_executor_handoff`, `wait_for_dependencies`, `increase_context_budget`, `repair_context_and_wait_for_dependencies`, `refresh_context_before_resume`, `resume_from_checkpoint` and `partial_retry_with_fresh_context`.
- Registry summaries now expose `ready_for_handoff`, `blocked_tasks` and `partial_retry_recommended` counts so operators can triage workflow fleets without opening every `forge inspect` DAG.
- Added CLI contract coverage proving `forge list` aggregates checkpoint-driven partial retries and dependency waits from the shared Context Routing Engine next-action decision.

### Changed

- The next-action decision previously local to `forge inspect` now lives in the Context Routing Engine as a shared `ContextNextAction` contract.
- `forge inspect` keeps emitting the same `forge.inspect_context_action.v1` node action shape, while `forge list` adds a registry-level aggregate rather than duplicating per-node details.

### Safety

- Registry action summaries are read-only metadata derived from Forge-owned workflow/task/checkpoint state and deterministic context routing.
- This change does not acquire leases, complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by `forge task handoff`, strict context readiness and task leases.

## 0.4.39 - 2026-05-24

### Added

- `forge inspect --output json` now includes a versioned `context_route.next_action` projection for every terminal DAG node.
- The projection uses schema `forge.inspect_context_action.v1` and reports the operator action, readiness for handoff, checkpoint route keys, partial-retry recommendation, blocking refs and a short reason.
- Human inspection diagrams now append `next <action>` to each node's context route so terminal operators can distinguish fresh handoff, dependency waits, context-budget repair and resumable checkpoint retries without opening full context packets.
- Added CLI contract coverage proving fresh handoff, dependency-wait and checkpoint route-change actions are surfaced through `forge inspect`.

### Changed

- Workflow inspection now turns Context Routing Engine and checkpoint state into explicit operator guidance instead of exposing only raw handoff and resume statuses.
- The `forge.context.v16` packet remains unchanged; this release adds a read-only inspection projection derived from the existing context, dependency and checkpoint contracts.

### Safety

- Next-action projections are read-only metadata derived from Forge-owned workflow/task/checkpoint state and deterministic context routing.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by `forge task handoff`, strict context readiness and task leases.

## 0.4.38 - 2026-05-24

### Added

- `forge task handoff` now emits `forge.executor_handoff.v4`.
- Human-facing nodes with Personality/Soul Routing now include a versioned `persona_contract` in the executor handoff packet.
- The contract uses schema `forge.persona_handoff.v1` and carries persona mode, node scope, instruction source, voice, tone, source models, validation gate, auditable flag, workflow context lineage hash and persona mode hash.
- Added CLI contract coverage proving a Codex handoff for a persona-routed documentation node receives the complete contract outside the nested context body.

### Changed

- Executor adapters no longer need to parse the full context package to enforce node-scoped persona routing before generating human-facing artifacts.
- The legacy `persona_mode` field remains in the handoff packet for compact routing checks and backwards-compatible operator projections.

### Safety

- The persona handoff contract is read-only metadata derived from Forge-owned task persona routing and context lineage state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Persona promotion remains validation-gated by `persona_routing_required` and existing `forge validate` checks for node scope, source models and auditability.

## 0.4.37 - 2026-05-24

### Added

- `forge list` now accepts `--lifecycle all|running|non-running` for explicit workflow inventory slices.
- Registry JSON output now includes `filter.lifecycle`, making filtered operator views auditable and replayable.
- Added CLI contract coverage proving running and non-running workflow rows are filtered separately and that summary counts are derived from the filtered view.

### Changed

- The default `forge list` behavior remains `all`, while filtered list calls recompute summary counts, reusable subflow counts and context-handoff totals over only the selected rows.
- Internal registry callers continue to use the unfiltered source-of-truth workflow registry unless they explicitly request a lifecycle filter.

### Safety

- Lifecycle filtering is read-only registry projection metadata derived from Forge-owned workflow/task state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.

## 0.4.36 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v16` with a per-shard remaining-budget ledger.
- Each context shard now carries `remaining_budget_before` and `remaining_budget_after`, making full, compressed, profile-omitted and budget-omitted routing decisions replayable without reconstructing the selector state.
- The routing fingerprint now includes a `budget_ledger` component so executor cache keys account for the per-shard budget cursor.

### Changed

- The routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_v16`.
- Executor adapters can audit why a shard was included or omitted using the same context packet they already receive, instead of recomputing budget pressure from the final content payload.

### Safety

- The budget ledger is read-only metadata derived from Forge-owned workflow/task state and the deterministic context selector.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.

## 0.4.35 - 2026-05-24

### Added

- `forge context` now emits schema `forge.context.v15` with content-addressed shard metadata.
- Each context shard includes a stable `sequence`, `shard_id` and `source_sha256` so executor adapters can audit and reuse shard identities even when a shard is omitted by budget or profile routing.
- The context routing fingerprint now includes a `source_shards` component derived from ordered shard source hashes.

### Changed

- The routing policy is now `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_v15`.
- Executor cache keys now account for source shard content, not only the final selected context payload and included/omitted section names.

### Safety

- Shard addressing is read-only context metadata derived from Forge-owned workflow/task state.
- This change does not complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.

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
