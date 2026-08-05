use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const SKILL_NAME: &str = "foundry-core";

pub const SKILL_MD: &str = include_str!("../.agents/skills/foundry-core/SKILL.md");

pub const AGENT_REFERENCE_MD: &str = r#"---
name: foundry-core
description: Use Foundry Core to run operational and strategic assisted AI/non-AI workflows with goal-oriented DAGs, executor/runtime sync, live goal/node mutation, mutable artifacts, validation gates, persistence, rework loops, and controlled self-improvement.
license: MIT
compatibility: codex, opencode, agy, claude
metadata:
  runtime: rust
  cli: foundry
---

## What Foundry Core Does

Foundry Core is an operational, strategic and visual assisted-operations runtime, not a chatbot wrapper and not a human-flow builder. Use it when an objective needs to become a persistent execution graph that can mix AI steps, deterministic non-AI steps, scheduled waits/cron, notifications, code/subworkflow execution, live human/AI modification, visual tasks/subtasks and Foundry-owned creative artifacts such as whiteboards, screens, components, wireframes, flows and design tokens.

The internal Rust layer that integrates external brain CLIs is `cli_integration`. The existing `foundry harness` commands, `foundry.harness.*` schemas/MCP tools and Rust alias remain stable compatibility contracts; they are not a second orchestration authority.

## Required Workflow

1. Run `foundry plan --goal "<human objective>" --output json`.
2. For skill-style use, prefer `foundry request start --goal "<objective>" --origin codex|opencode|agy|claude|skill --output json` and return the `run_id` to the caller.
3. Run `foundry sync all --home "$HOME" --output json` when executor or runtime availability may have changed. If Foundry-first CLI shims are installed, include `--shim-dir "$HOME/.foundry/bin"` or the project shim directory so executor readiness includes harness status.
4. Inspect the generated atomic tasks, task goals, subtasks, impediments, async policy and validation rules.
5. Use `foundry workflow update-goal ... --origin codex|opencode|agy|foundry_cli|skill` when the human changes direction during execution.
6. Use `foundry workflow attach-artifact ... --tag <tag> --origin codex|opencode|agy|foundry_cli|skill` when new artifacts appear during execution; tags should describe artifact kind, domain, customer/account, workflow stage and search intent.
7. Use `foundry context --workflow <id> --task <task-id> --project-root <project-root> --budget <bytes> --strict --view compact --output json` before giving an agent task-specific context; include `--project-root` whenever project `.foundry/memory-governance.json` should affect the context memory policy. Read the compact `selected_source_ids`, `deferred_source_ids`, `expand_commands` and `guardrail` before discovering MCP servers, skills, memory or CRM records: only router-selected sources belong in the current node packet, and deferred sources must wait for a later node or an explicit Foundry expansion command. When blocked, work only the bounded predecessor frontier in `guardrail.next_commands`, then request compact context again after state changes; never hand off the still-blocked current task. For full-view routing audits, inspect `context_router` (`foundry.context.router.v1`) and `deferred_discovery` (`foundry.context.deferred_discovery.v1`), including its `deferred_sources`; request the full context view only for routing audit or replay diagnostics.
8. Use `foundry memory policy --project-root <project-root> --output json` and `foundry memory search --workflow <workflow-id> --query "<query>" --memory-level none|session|short_term|standard|full|admin --scope global|organization|project|processing --organization <organization-id> --audience public|internal|manager|private --output json` before loading broad historical context. Foundry memory is file-first, level-scoped, workflow/tenant-bound when a workflow is supplied, and visibility-gated; search returns snippets and line ranges, not whole files. Configure project defaults explicitly with `foundry memory configure --project-root <project-root> ... --approved-by <operator> --reason "<reason>"` or MCP `foundry.memory.configure`; when `memory_level`, `scope` and `audience` are omitted, search uses `.foundry/memory-governance.json` defaults for that project.
9. Run `foundry validate --workflow <id> --output json` before promotion. If `rework_tasks` is not empty, return those tasks to work.
10. Run `foundry improve candidates --output json` or `foundry.improve.candidates` before choosing a workflow to mutate; use its run/event/outcome/parallelization/cost evidence to decide whether to recover a stale run, parallelize ready handoffs, replace avoidable AI work with command nodes, or generate a controlled experiment.
11. Run `foundry improve --workflow <id> --target-version <version> --output json` only to generate a controlled experiment and changelog. Use `foundry improve apply-event-policy --workflow <id> --policy <policy> --apply --approved-by <operator> --output json` or `--recommendation <recommendation-id>` only for approved event-policy revisions; recommendations may target node, Addon or workflow scope. Then run `foundry improve benchmark-event-policy --workflow <id> --policy <policy> --output json` to validate rollback, equivalence and workflow validation evidence, and only then `foundry improve promote-event-policy --workflow <id> --policy <policy> --approved-by <operator> --output json` to record governed acceptance. Do not auto-promote without benchmark, validation and explicit approval evidence.
12. Run `foundry milestone status --version 0.5 --output json` and `foundry milestone manifest --version 0.5 --output json` before claiming Foundry 0.5 creative-runtime readiness; planned or groundwork capabilities block promotion unless their complete operator-approved required attached-evidence set is present. Use `foundry milestone evidence-plan --capability <capability-id> --project-root <project-root> --output json` or MCP `foundry.milestone.evidence_plan` to inspect project manifests, secret-free `manifest_templates` such as `.foundry/connected-brain-runtimes.json`, `.foundry/multimodal.json` and `.foundry/multimodal-runtimes.json`, static wrapper/runtime readiness checks and collection commands before running real provider/runtime evidence; replacement-grade CLI plans block provider evidence when the selected connected-brain provider adapter path is missing or not executable. When `manifest_templates` are present, use `foundry milestone prepare-evidence-inputs --capability <capability-id> --project-root <project-root> --apply --approved-by <operator> --output json` or MCP `foundry.milestone.prepare_evidence_inputs` to materialize only secret-free templates; add `--connected-brain <provider-id> --provider-command <absolute-provider-adapter-path> --model-id <approved-model-id> --approval-ref <approval-ref>` only after the operator has approved a local provider adapter command for replacement-grade CLI preflight. It prepares inputs only and never counts as release evidence. Use `foundry milestone collect-evidence --capability <capability-id> --kind <evidence-kind> --project-root <project-root> --approved-by <operator> --output json` or MCP `foundry.milestone.collect_evidence` to run a ready provider/runtime/demo source, persist the receipt and attach it; omit `--kind` only when the capability default receipt is intended. Use `foundry milestone collect-ready-evidence --project-root <project-root> --approved-by <operator> --output json` or MCP `foundry.milestone.collect_ready_evidence` to attempt every required evidence kind, attach only receipts whose inputs and gates are ready, and report skipped/failed kinds without auto-promoting. Use `foundry milestone attach-evidence --capability <capability-id> --kind <kind> --artifact <path> --approved-by <operator> --output json` or MCP `foundry.milestone.attach_evidence` only to attach already-reviewed external release evidence; a single receipt stays audit-only, while the complete required receipt set makes that capability promotion-ready in the manifest.

## MCP Agent Surface

- Use `foundry mcp tools --output json` to discover stable agent-facing tools before wiring a Codex/OpenCode workflow.
- Treat Codex, OpenCode, Antigravity CLI, Claude CLI and future CLIs as replaceable execution brains only. Foundry owns and routes workflow state, memory, skills, MCP servers/tools, credential-vault references, context packets, shell/session lifecycle, permissions, cost policy, validation gates and self-improvement decisions. Inspect this boundary with `foundry brains --output json` or MCP `foundry.brain_router`, inspect provider/session state with `foundry sessions --output json` or MCP `foundry.sessions`, record ordered shell session lifecycle with `foundry sessions lifecycle` or MCP `foundry.session.lifecycle`, and inspect per-session audit history with `foundry sessions history --session <id>` or MCP `foundry.session.history`, before handing work to a brain.
- Distinguish the orchestrator brain from node brains. Foundry is the orchestrator brain/control plane; each AI or mixed workflow node can carry its own `node_brain_routing` contract with one or more agent slots, different brains per slot, multiple agents on the same brain, Foundry-owned memory/skills/MCP routing, parallel execution when leases/quota/context allow it, node-level routing mutation through `foundry workflow update-node-brain`, and run-level hot-swap through `foundry request switch-executor` without stopping the workflow.
- In `foundry.context.request` / `foundry.task.handoff`, treat `prompt_packet.organization_context`, `prompt_packet.personality_decision` and `prompt_packet.company_work_decision` as required executor context. `organization_context` carries organization, brand, product, user, channel, memory/personality scope, tenant policy, brand voice/tone/values, terminology, design-system sources and operating policy. `personality_decision` carries the Foundry-owned routing owner, selected persona mode/profile, selected voice/tone, brand voice/tone/values, style sources, fallback mode and audit flag. `company_work_decision` carries the compact multidisciplinary operating checklist for product, technical, financial, administrative, marketing, communication and delivery work. Their checksums plus `organization_context_required`, `personality_decision_required` and `company_work_decision_required` validation gates make agent output accountable to organizational context, personality routing and company operating discipline.
- Treat `context_router` (`foundry.context.router.v1`) as the LiteLLM-inspired node context router and `deferred_discovery` as its source manifest. Foundry does not need to rediscover every MCP server or skill for every node; `context_router` exposes `routing_strategy=node_requirement_tag_routing_with_budget_fallbacks`, `route_groups`, `node_tags`, `pre_call_checks` such as `bound_subject_or_defer`, `selected_source_ids`, `deferred_source_ids`, `global_discovery_allowed=false`, `avoided_global_discovery=true` and a plan-only fallback policy, while `deferred_discovery` keeps `selected_sources` and `deferred_sources` for the source-level audit. Agents must not load MCP servers, skill catalogs, memory or CRM subject timelines unless the current node's route group selected that source or Foundry returned an explicit expansion command; CRM profile/timeline sources require a concrete bound subject marker such as `bound_crm_subject`, `crm_subject`, `user_id`, `lead_id`, `contact_id` or `account_id`.
- Before reading historical memory for a task, inspect the `memory_policy` object returned by `foundry context --project-root <project-root>` or task handoff with `project_root`, or call `foundry memory policy --project-root <project-root> --output json` / MCP `foundry.memory.policy` when operating on a project. It carries the workflow/tenant-derived memory level, allowed scopes, tenant boundary, default audience and governed `foundry memory search --workflow <workflow-id>` command; project policy also reports `.foundry/memory-governance.json` status, source fields and effective defaults. Do not inline broad memory into executor prompts.
- Promote memory only through `foundry memory promote --workflow <workflow-id>` or MCP `foundry.memory.promote` with `workflow_id`, using a curated summary, source path, approver, reason, visibility and compatible shareability. Never copy raw private processing memory into project, organization or global memory.
- Clean up processing memory only through `foundry memory cleanup --workflow <workflow-id>` or MCP `foundry.memory.cleanup`. Non-dry-run cleanup requires an approver, reason and confirm, and only archives/deletes files that `foundry memory retention` classified as `delete_after_final_packaging`.
- Treat memory as scoped, classified files. `global` memory lives across projects, `organization` memory belongs to a tenant/org root, `project` memory belongs under the workflow/project `.foundry` area, and `processing` memory is run-lived scratch that may be deleted after final packaging. Classify each memory as `public`, `internal` or `private`, and as `global_shared`, `organization_shared`, `project_shared`, `manager_shared`, `thread_private` or `non_shareable`. A customer suggestion starts private/thread-scoped, but a curated suggestion can be `manager_shared` for a manager or product owner without becoming public or globally reusable.
- Treat every customer request as company work, not only technical work. Even small tasks should decide what will be done, how it will be done, how delivery will be accepted, how it will be communicated, and whether product, technical, financial, administrative, marketing, communication or delivery concerns apply.
- Use `foundry improve candidates --output json` or MCP `foundry.improve.candidates` as the orchestrator's first improvement scan. It ranks live/degraded workflows with workflow events, run heartbeats, outcome evidence, parallel-ready handoffs and cost-efficiency signals for repetitive/deterministic tasks that are still using AI.
- Read `dashboard.improvement_loop_panel`, call `foundry interactive improvement-loop --output json`, or use MCP `foundry.interactive.improvement_loop` before self-improvement or workflow mutation. It exposes `foundry.interactive.improvement_loop.v1` with ranked candidates, critical/high counts, parallel-ready handoffs, avoidable AI cost, final-outcome gaps, stale/attention signals, structured-log counts, cost totals, validation failures, context quality and governed next commands without applying mutations.
- Read `dashboard.workflow_mutation_panel`, call `foundry interactive workflow-mutation --output json`, or use MCP `foundry.interactive.workflow_mutation` before replanning a live workflow. It exposes `foundry.interactive.workflow_mutation.v1` with DAG, task-board, modifier lane proposals, handoffs, costs, events and safe `workflow update-goal`, `workflow update-node-brain` and `workflow attach-artifact` commands without applying mutations from the read-only panel.
- Typing only `foundry` must open the OpenCode-style orchestrator-first Foundry TUI by default. Normal input talks to the Foundry orchestrator, which decides whether to answer directly or create a Foundry workflow. Plan, build, agents and subagents are Foundry workflows or workflow nodes; each node may have its own agent/brain routing. Use `foundry tui --output json` for the same compact entrypoint model in scripts; it exposes orchestrator prompt operation, `!<cmd>` local shell execution, `!` shell-mode toggle, workflow/agent/subagent/node-agent/event/Addon/cost/handoff/approval capabilities and quick commands while keeping the legacy detailed cockpit under `foundry interactive guided-cockpit --output json` or MCP `foundry.interactive.guided_cockpit`.
- Read `dashboard.core_boundary_panel`, call `foundry interactive core-boundary --project-root <project-root> --output json`, or use MCP `foundry.interactive.core_boundary` with `project_root` for `foundry.interactive.core_boundary.v1` before claiming Core/Addons architectural alignment. It audits universal Core responsibilities, domain-specific Core leaks, Addon-owned capability boundaries, visible compatibility executors and goal3 acceptance gates without mutating state.
- Read `dashboard.architecture_compass_panel`, call `foundry interactive architecture --project-root <project-root> --output json`, or use MCP `foundry.interactive.architecture` with `project_root` before choosing the next architectural increment. It exposes `foundry.interactive.architecture_compass.v1` with goal1/goal2/goal3 hashes, project operating context, architecture tracks, benchmark boundaries, conflicts, dependencies, reuse opportunities and an incremental execution plan without mutating state.
- Use `foundry interactive home --project-root <project-root> --output json` or MCP `foundry.interactive.home` with `project_root` when the home dashboard must render project-scoped release gates, harness mode, identity context, workflow sidebar navigation, replacement-grade CLI readiness, Addon-owned multimodal runtime readiness and read-only event-runtime state without relying on process cwd. Read `dashboard.workflow_sidebar_panel`, call `foundry interactive workflow-sidebar --output json`, or use MCP `foundry.interactive.workflow_sidebar` for `foundry.interactive.workflow_sidebar.v1` before composing a TUI/sidebar; it groups active, attention, event-driven, scheduled and completed workflows, marks the selected workflow and exposes inspect/task-board/DAG/events/validate commands without mutating state. Read `dashboard.replacement_cli_panel`, call `foundry interactive replacement-cli --project-root <project-root> --output json`, or use MCP `foundry.interactive.replacement_cli` with `project_root` for `foundry.interactive.replacement_cli.v1` before claiming `foundry` is replacing daily CLI work; it aggregates operator home, workflow operations, patch editing UX, action discovery, project-scoped harness/session controls, observability, approvals, milestone evidence, `external_brain_evidence_plan`, provider-level `provider_readiness` rows, plan-only `provider_wrapper_plans` and static `provider_wrapper_manifest_audit` for the required connected-brain provider adapter path while keeping promotion false until required evidence is attached; provider adapter plans and manifest audit do not launch child CLIs, execute models, mutate project files or count as release evidence. Read `dashboard.multimodal_runtime_panel`, call `foundry interactive multimodal-runtime --project-root <project-root> --output json`, or use MCP `foundry.interactive.multimodal_runtime` with `project_root` for `foundry.interactive.multimodal_runtime.v1` before presenting multimodal runtime work; it exposes the `foundry.addon.multimodal` boundary, feature flag, guard state, safe templates, demo plans, project-scoped Addon commands and `production_runtime_evidence_plan` blockers without installing models, executing models, accessing devices or mutating workflows. Read `dashboard.event_runtime_panel` (`foundry.interactive.event_runtime.v1`) for pending inbound events, worker/service status, wakeable persistent workflows, recommendation and explicit inbox/reconcile/supervisor/webhook commands; rendering it must not route events or start workers.
- Use `foundry smoke operational-tui --output json` before claiming the operational TUI is ready. The smoke creates a demonstrable local run, scheduled workflow, event, approval signal, workflow mutation proposal and dashboard evidence, then checks that `foundry` opens the OpenCode-style orchestrator-first TUI by default and exposes workflows ativos, agentes/subagentes/node agents, eventos/schedules, Addons/capabilities, custos, replanejamento, core boundary and handoffs/approvals plus the README five-minute intro. Use `foundry smoke foundry-first-harness --output json` before claiming Foundry-first CLI harness readiness; it proves persisted reversible headroom, read-only adoption-plan, approval-gated bootstrap dry-run, isolated Foundry-owned shim installation, shim audit, one-shot PATH activation and `harness exec` dry-run without executing or mutating the external CLI. Use `foundry smoke replacement-cli-evidence --output json` before claiming replacement-grade CLI evidence collection is operational; it proves ready replacement CLI evidence is attached while provider/runtime evidence remains skipped until approved project manifests exist. Use `foundry smoke multimodal-runtime-evidence --output json` before claiming Addon-owned multimodal runtime evidence collection is operational; it proves missing project manifests stay planning-only and approved connected runtime manifests attach `production_runtime_benchmark` without auto-promotion.
- Use `foundry cli create --name <name> --goal "<objective>" --source <api-or-site> --command <command> --compound-command <insight> --output json` or MCP `foundry.cli.create` when the deliverable is a generated CLI rather than only a workflow plan. Foundry creates a persistent `foundry.cli_factory.creation_plan.v1` workflow whose state owner is `foundry_workflow_runtime`, keeps generated CLI/MCP/skill/Addons as workflow artifacts/runtime contracts, defaults to local-first SQLite, exposes agent-native commands (`sync`, `search`, `sql`, `insight`, JSON/compact/dry-run) and requires scorecard, dogfood, proof and smoke checks before promotion. Generated CLIs are products built on Foundry, not separate orchestration authorities.
- Read `dashboard.context_memory_panel`, call `foundry interactive context-memory --project-root <project-root> --output json`, or use MCP `foundry.interactive.context_memory` before rendering context/memory governance operations. It exposes `foundry.interactive.context_memory.v1` with context handoff readiness, context routing quality, project memory policy, governed memory/context commands and next actions without building a task packet or mutating workflows.
- Read `dashboard.operating_context_panel`, call `foundry interactive operating-context --project-root <project-root> --output json`, or use MCP `foundry.interactive.operating_context` before presenting executor handoff controls. It exposes `foundry.interactive.operating_context.v1` with tenant identity, memory policy, personality routing, brand/design context, prompt-packet gates, company-work checklist and handoff readiness without mutating workflows.
- Inspect the no-argument interactive dashboard through `foundry.interactive.home`, discover slash commands through `foundry.interactive.slash_commands`, and route conversational input through `foundry.interactive.route` when an agent needs the same command/chat classification as the TUI without launching a local terminal. Read `dashboard.navigation_panel` for `foundry.interactive.navigation.v1` keyboard bindings, themes and compact/detailed/focus display modes. Read `dashboard.ui_composition_panel`, call `foundry interactive ui-composition --project-root <project-root> --output json`, or use MCP `foundry.interactive.ui_composition` for `foundry.interactive.ui_composition.v1` regions, Core widgets, Addon widgets from enabled Addon views including `tui` surfaces, renderer families and refresh/inspection commands before composing a TUI, web dashboard or agent-side visual surface. Read `dashboard.release_gates_panel`, call `foundry interactive release-gates --version 0.5 --project-root <project-root> --output json`, or use MCP `foundry.interactive.release_gates` with `project_root` before claiming milestone readiness; it exposes promotion decision, blocked capabilities, required evidence, current evidence, per-capability evidence-plan status, secret-free manifest templates and next commands without mutating state. Read `dashboard.harness_panel` or call `foundry interactive harness --output json` / MCP `foundry.interactive.harness` for `foundry.interactive.harness.v1` harness mode, doctor, shim status, CLI wrap-plan, `headroom_plan`, `adoption_plan`, `headroom_stats`, `session_lifecycle_plan`, `executor_compatibility`, token-headroom preview and guarded `bootstrap_project_harness` command before rendering Foundry-first CLI controls. `executor_compatibility` uses `foundry.harness.executor_compatibility.v1` to expose canonical Codex, Claude, Antigravity and OpenCode adapter families plus readiness for env overlay, PATH shim, guarded exec, token headroom, session lifecycle, context/memory/skill/MCP routing and credential-vault boundaries. Read `dashboard.sessions_panel` or call `foundry interactive sessions --output json` / MCP `foundry.interactive.sessions` for `foundry.interactive.sessions.v1` provider/session readiness, lifecycle state, per-session `operation_plan`, shell history commands and next lifecycle controls before opening or attaching brain shells. Read `dashboard.patch_workbench_panel` or call `foundry interactive patch-workbench --output json` / MCP `foundry.interactive.patch_workbench` for `foundry.interactive.patch_workbench.v1` `addon_contract`, Git status, `diff_preview`, `diff_review_queue`, per-file `action_hint` next actions, `edit_intake` required inputs/forms, ordered `operation_plan`, diff stat/check, changed-file lanes, `approval_flow` review/approval/rollback gates and permission-gated `patch plan/review/diff/apply/restore` commands before presenting file editing or diff review UI. Read `dashboard.identity_panel` or call `foundry interactive identity --output json` / MCP `foundry.interactive.identity` for `foundry.interactive.identity.v1` operating context, identity registry, channel aliases, memberships and tenant audit before rendering identity or tenant context operations. Read `dashboard.permissions_panel` or call `foundry interactive permissions --output json` / MCP `foundry.interactive.permissions` for `foundry.interactive.permissions.v1` tenant memberships, Addon permission authorizations and pending human approvals before rendering permission or approval operations. Read `dashboard.structured_logs_panel` or call `foundry interactive structured-logs --output json` / MCP `foundry.interactive.structured_logs` for `foundry.interactive.structured_logs.v1` recent event logs with store sequence, workflow, category, severity, origin, source, correlation, observability and payload preview before building timeline drill-downs. Before opening brain shells, call `foundry interactive readiness --output json` or MCP `foundry.interactive.readiness` for `foundry.interactive.readiness.v1` executor, brain, shell, Foundry-controlled surface and harness readiness with next corrective commands; also read `dashboard.harness_panel`, `dashboard.sessions_panel`, `dashboard.harness_mode_panel` and `dashboard.harness_doctor_panel`. Before operational handoff decisions, call `foundry interactive operational-cockpit --output json` or MCP `foundry.interactive.operational_cockpit` for `foundry.interactive.operational_cockpit.v1`; inspect its `modifier_lane` (`foundry.interactive.operational_modifier_lane.v1`) for pending/applied human+AI strategic runtime mutations before applying proposals through the ops console/API, then read `dashboard.digital_twin_panel` for `foundry.ops.operational_digital_twin.v1` state, `dashboard.dag_panel` or call `foundry interactive workflow-dag --output json` / MCP `foundry.interactive.workflow_dag` for `foundry.interactive.workflow_dag.v1` dependency nodes/edges, readiness, human waits and drill-down commands, and read `dashboard.task_board_panel` or call `foundry interactive task-board --output json` / MCP `foundry.interactive.task_board`; these expose workflow lanes, attention, handoffs, checkpoint resume candidates, pending human waits, artifacts, selected brain, observability and direct next commands.
- Read `dashboard.command_palette_panel` or call `foundry interactive command-palette --output json` / MCP `foundry.interactive.command_palette` for `foundry.interactive.command_palette.v1` grouped contextual actions before building command palettes, quick-open menus or agent-side action pickers; it is read-only and exposes navigation, workflow, Addon actions, permission, harness, session and observability actions with mutation and approval flags. CLI actions declared by enabled TUI Addon views can provide `palette_group`, `source_panel`, `description`, `risk_level`, `mutates_workflow`, `command_template`, `keywords` and optional action hooks. The palette preserves `addon_contract` using `foundry.interactive.addon_action_contract.v1`, generic `hook_contract` using `foundry.interactive.action_hook_contract.v1`, workflow dispatch `workflow_hook_contract` using `foundry.interactive.workflow_hook_contract.v1`, and CLI-brain `brain_hook_contract` using `foundry.interactive.brain_hook_contract.v1`; each workflow hook is plan-only, `state_owner=foundry_workflow_runtime`, `hook_execution_owner=foundry_workflow_runtime`, `not_executed=true`, and uses `route_action_hook_to_foundry_workflow` so CRM actions such as list/tag/email operations can route to another Foundry workflow without bypassing runtime lineage. Each brain hook is plan-only, `state_owner=foundry_workflow_runtime`, `hook_execution_owner=foundry_harness`, and `not_executed=true`, so clients can route Codex/OpenCode/Antigravity/Claude hooks through Foundry instead of launching brains directly. Each entry also carries `addon_view_id`, `addon_view_action_id`, `enabled`, `blocked_reason` and `operation_plan` using `foundry.interactive.command_palette_action_plan.v1` so clients can show source Addon, Addon version/lifecycle, capability, permission, permission-gate status, execution/diagnostic status and concrete view action before offering domain-specific commands. When an Addon action is blocked by permission readiness, the entry remains visible for diagnosis, exposes no executable command template and points to the Addon view inspection command as the next diagnostic step.
- Read `dashboard.action_registry_panel`, type `/actions [query]` in the interactive REPL, or call `foundry interactive action-registry --output json` / MCP `foundry.interactive.action_registry` for `foundry.interactive.action_registry.v1` when a TUI, web dashboard or agent needs a stable read-only list of actions independent of command-palette layout. It derives from the same governed action contracts but returns strict query filtering, per-group counts, enabled/blocked/diagnostic/mutation/approval totals, action `operation_plan`, generic hook contracts, workflow dispatch plans, CLI-brain dispatch plans and next diagnostic commands.
- Type `/action <action-id>` in the interactive REPL, call `foundry interactive action-invocation --action <action-id> --output json`, or use MCP `foundry.interactive.action_invocation` when a TUI, web dashboard or agent has selected one action and needs a safe invocation plan. It resolves the action from the registry into `foundry.interactive.action_invocation.v1`, returns `can_execute`, `selected_command`, `operation_plan`, diagnostics, `hook_contract`, `workflow_hook_contract`, `brain_hook_contract` and `not_executed=true`; clients must still execute explicit commands themselves or route workflow hooks through Foundry runtime and CLI-brain hooks through Foundry Harness.
- Read `dashboard.autocomplete_panel` or call `foundry interactive autocomplete --input "<partial input>" --output json` / MCP `foundry.interactive.autocomplete` for `foundry.interactive.autocomplete.v1` slash-command, command-palette and `/action <partial>` action-id suggestions before building autocomplete, inline command completion or agent-side quick input; it is read-only and returns score, source panel, mutation, approval, `enabled`, `blocked_reason` and `operation_plan` flags. `/action` suggestions use source `action_registry`, insert `/action <action-id>`, and point equivalent commands to `foundry interactive action-invocation --action <action-id> --output json`, so clients can complete selected action IDs without executing them. Command-palette suggestions preserve any `addon_contract` using `foundry.interactive.addon_action_contract.v1`, plus `addon_view_id` and `addon_view_action_id`, so autocomplete can keep Addon/capability/permission/action lineage visible instead of flattening domain-specific actions into Core commands; blocked Addon actions remain discoverable but have empty insert/equivalent commands and diagnostic-only operation plans.
- Use the interactive `/brains`, `/sessions`, `/sessions history`, `/sessions lifecycle`, `/shells`, `/harness` and `/harness doctor` commands to show Foundry-controlled brain routing, provider/session readiness, auditable shell lifecycle, effective Foundry-first harness mode and full CLI readiness before opening a brain shell. Use `foundry sessions --output json` or MCP `foundry.sessions` to list providers, session readiness, lifecycle state, `lifecycle_policy.allowed_next_states`, per-session `operation_plan`, next lifecycle commands and recorded shell launch events; pass `--provider <brain>`, `--state <state>` or `--readiness <readiness>` on the CLI, or `provider_id`, `lifecycle_state` and `readiness` in MCP input, when the operator/UI only needs one provider or lifecycle lane. Use `foundry sessions history --session <id> --output json` or MCP `foundry.session.history` when an operator/agent needs one session's chronological shell-launch and lifecycle audit, counts, current state and next lifecycle commands without filtering global events in the client. Use `foundry sessions lifecycle --session <id> --state opened|attached|closed --origin <origin>` or MCP `foundry.session.lifecycle` to record audit-only shell lifecycle events without starting child processes; Foundry returns `previous_state`, `lifecycle_sequence` and a `transition` policy, and rejects invalid ordered transitions such as detaching before attachment. Use `foundry shells --executor <executor> --workflow <workflow-id> --task <task-id> --run <run-id> --output json` or MCP `foundry.shell.launch_plan` when an operator/agent needs a plan-only launch report with readiness, preflight commands, concrete context/handoff/heartbeat commands, `prompt_packet_gate_policy` and handoff gates before starting an external brain shell; the policy lists `organization_context_required`, `personality_decision_required` and `company_work_decision_required` so shells cannot ignore prompt-packet decisions. Use `foundry shells --record-session` or MCP `foundry.shell.record_plan` when the shell intent should be auditable as a `shell_launch_planned` event. Directly opening `codex`, `opencode`, `antigravity` or `claude` should be treated as inspection/debugging; production handoff should go through Foundry context, leases and validation.
- Use `foundry harness doctor --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` or MCP `foundry.harness.doctor` for a consolidated read-only readiness audit before relying on Foundry-first CLI operation. Use `foundry harness mode --output json` or MCP `foundry.harness.mode` to audit the effective Foundry-first default, source, project config status, exec policy and precedence before relying on a wrapper or shim. Use `foundry harness headroom-plan --executor <executor> --project-root <project-root> --output json` or MCP `foundry.harness.headroom_plan` to inspect the effective context budget, token-headroom source, wrapper env, `session_lifecycle_plan`, compression pipeline, reserve strategy and next commands before handing large logs, tool output or CLI stdout to a brain. Use `foundry harness headroom-stats --source <source> --output json` or MCP `foundry.harness.headroom_stats` to inspect persisted token savings by source/content kind and top reversible retrieval refs before expanding noisy outputs back into context. Use `foundry harness adoption-plan --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` or MCP `foundry.harness.adoption_plan` to get the ordered read-only adoption plan for project harness config, Foundry-first shims, activation profile, executor sync, headroom verification, lineage-required dry-run and lineage-required real execution without writing config, installing shims or launching child CLIs. Use the returned `commands.exec_with_lineage_dry_run` first; only then use `commands.exec_with_lineage`, which adds `--execute --allow-exec` and starts the external child process. Use `foundry harness activation-profile --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` or MCP `foundry.harness.activation_profile` to generate reversible shell activation/deactivation commands that prepend Foundry-owned shims to `PATH` and export Foundry-first/headroom defaults; add `--shell-rc <path> --apply --approved-by <operator>` only after review to write a reversible Foundry-managed shell startup block. Use `foundry harness bootstrap --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` or MCP `foundry.harness.bootstrap` for a safe dry-run bootstrap; add `--apply --approved-by <operator>` only after review to write `.foundry/harness.json` and install Foundry-owned shims. Use `foundry harness install-provider-adapter` / MCP `foundry.harness.install_provider_adapter` to create the separate `.foundry/bin/<executor>-provider` command for milestone provider evidence after operator approval. Use `foundry harness wrap-plan` / MCP `foundry.harness.wrap_plan` before running Codex, Claude, Antigravity or OpenCode under Foundry-first control; it returns `foundry.harness.session_lifecycle_plan.v1` with record-launch/opened/attached/closed commands so shell state is auditable before and after handoff, and returns `connected_brain_provider_wrapper` (`foundry.harness.connected_brain_provider_wrapper.v1`) so `.foundry/connected-brain-runtimes.json` can point provider commands at the absolute Foundry-owned `.foundry/bin/<executor>-provider` adapter without counting that preparation as release evidence. Pass `--project-root <project-root>` or MCP `project_root` when planning for another project's `.foundry/harness.json`, and pass `--workflow`, `--task` and `--run` whenever the external CLI belongs to a workflow node. When a shell should prefer Foundry infrastructure by default, set `FOUNDRY_HARNESS_DEFAULT_MODE=foundry_first`, add project `.foundry/harness.json` with `{"default_mode":"foundry_first"}`, or use `foundry harness install-shims --shim-dir <dir> --executor <executor> --project-root <project-root>` / MCP `foundry.harness.install_shims` with `project_root`; `--observe-only` disables those defaults for one CLI invocation. Project `.foundry/harness.json` may also set `context_budget` or `default_context_budget`, `default_token_headroom` or `token_headroom`, and `require_token_headroom_for_foundry_first`; `foundry harness mode`, `headroom-plan`, `headroom-stats`, `adoption-plan`, `activation-profile`, `bootstrap`, `wrap-plan`, `doctor`, `install-shims` and `exec` report the resolved `context_budget_source`, `token_headroom_source` and `require_token_headroom_for_foundry_first`. Add `"require_lineage_for_exec": true` to the same project file when real child execution must be blocked unless `workflow`, `task` and `run` lineage are present. Harness reports expose `foundry_first_source`, `project_exec_policy_status`, `require_lineage_for_exec`, context budget and token-headroom sources, and the child overlay includes `FOUNDRY_HARNESS_MODE_SOURCE`, so operators can tell whether Foundry-first came from an explicit flag, env default, project config, observe-only override, MCP input or MCP default. Foundry resolves the native CLI from PATH while excluding the shim directory, refuses to overwrite existing non-Foundry files unless forced and records the resolution source/status. After installing or changing PATH, use `foundry harness shim-status --shim-dir <dir> --executor <executor>` or MCP `foundry.harness.shim_status` to audit existence, Foundry ownership, executable bit, PATH precedence, parsed real CLI/store/Foundry binary and recursion risk before relying on the shim. Then run `foundry sync executors --shim-dir <dir>` or `foundry sync all --shim-dir <dir>` so `foundry executors`, `foundry brains`, `/brains`, `/shells` and `foundry shells` expose `foundry.executor_harness_status.v1`, `foundry_first_ready` and Foundry-first shell entrypoints. Use `--real-cmd` only when an explicit native CLI path is required. Use `foundry harness exec --project-root <project-root>` / MCP `foundry.harness.exec` with `project_root` for guarded CLI invocation receipts; with `require_lineage_for_exec`, missing workflow/task/run returns `harness_exec_blocked_by_project_policy` instead of launching the child process. When token headroom is enabled, real child stdout/stderr get `stdout_headroom`/`stderr_headroom` reports with reversible retrieval refs, so compressed output can flow to a brain while Foundry preserves the original stream. When a store plus workflow, task or run lineage is present, the receipt records a `foundry.harness.exec_event.v1` global timeline event and returns `event_recorded` plus `global_event_id`.
- For human+AI assisted operation, use `foundry ops snapshot --project-root <project-root> --output json` or MCP `foundry.ops.snapshot` for an operational registry view, or `foundry ops serve --project-root <project-root> --host 127.0.0.1 --port 8765` to open the local web console. Include `--project-root`/`project_root` whenever project `.foundry/memory-governance.json` should be visible to operators and modifier AIs. The snapshot includes `foundry.ops.operational_digital_twin.v1`, showing each workflow's current activity, completed work, remaining work, validated work, rejected work, pending approvals and direct inspect/task-board/validate/events commands. The console is local-only by default and lets operators observe workflows, drive runs, route deterministic work for explicit executor receipts, complete tasks with evidence and update workflow goals or task nodes in real time. Its modifier lane lets a separate strategic AI or human propose goal/node mutations and apply them through Foundry-owned events while execution continues. Its memory/context governance panel exposes project governance, workflow tenant/personality context and ready-to-run governed `foundry context` / `foundry memory search` commands. Its visual surface shows tasks/subtasks and lets operators create whiteboards, screens, wireframes, flows, components, documents, token collections, token patches and collaboration events through Foundry-owned workflow revisions. Addon renderer interactions are validated against `allowed_client_events` and can be recorded through `/api/addon-renderer/event`, `foundry ops renderer-event` or MCP `foundry.ops.addon_renderer_event`, then projected back into snapshot runtime state.
- Treat `outcome_status` from `foundry request status`, `foundry request drive` and `foundry list` as the final-result gate. If it says `support_only`, update the goal or tasks with explicit user-facing deliverables. If it says `needs_user_delivery_evidence` or `needs_final_outcome_audit`, continue the workflow instead of claiming completion.
- If `foundry improve candidates` reports `missing_final_outcome_audit` for a workflow without a driveable run, use `foundry request ensure-final-audit --workflow <workflow-id> --executor codex --origin codex --output json` or MCP `foundry.workflow.ensure_final_audit` to create or surface the final audit task before packaging.
- For async handoff, call `foundry mcp call foundry.run.start --input '{"goal":"<objective>","origin":"codex"}' --output json`, return `result.run_id` quickly, and let Foundry remain the source of truth.
- While an executor is alive, refresh observability with `foundry request heartbeat --run <run-id> --executor codex --summary "<short progress>" --ttl-seconds 300 --pid <executor-pid> --origin codex --output json` or `foundry.run.heartbeat`; this keeps `foundry request status`, `foundry request list` and `foundry inspect` honest about active self-runs, including long runs whose heartbeat TTL expires while the recorded process is still alive.
- Before polling passively or starting another task handoff, call `foundry request drive --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json` or `foundry.run.drive`; it refreshes the heartbeat and returns `rework_required`, `ready_for_handoff`, `complete` or `blocked` with the next safe command.
- `foundry request step` and `foundry.run.step` never stand in for task execution. Every ready task requires the executor to perform the real work and close it through `foundry request complete-task ... --evidence-command "<passing gate>" --evidence-exit-code <observed-exit-code>`; the exit code must be the value actually observed by the caller, and Foundry never fabricates `0`. Without that receipt Foundry returns `handoff_required` instead of promoting metadata as work.
- When an AI or mixed executor has actually done the ready handoff work, close it with `foundry request complete-task --run <run-id> --task <task-id> --executor codex --summary "<result>" --evidence-command "<passing gate>" --evidence-exit-code <observed-exit-code> --origin codex --output json` or `foundry.run.complete_task`; Foundry writes a replayable execution trace, builds the executor response, validates it, promotes the task and immediately drives the next action.
- When `foundry request drive` returns `complete`, inspect its `final_delivery_package`; Foundry attaches Markdown and JSON summaries automatically at completion. Before handing an older or in-progress run back to the user, create or refresh the same package with `foundry request final-package --run <run-id> --origin codex --output json` or `foundry.run.final_package`; it reports `ready_for_user`, `in_progress` or `not_ready_for_user` so a support artifact is not mistaken for the requested final result.
- If the current executor is about to hit a model limit, becomes unavailable, or should hand off work, use `foundry request switch-executor --run <run-id> --executor opencode --fallback-executor codex --summary "<takeover summary>" --origin codex --output json` or `foundry.run.switch_executor`. This hot-swap changes the execution brain for the workflow run while preserving the same `run_id`, workflow id, checkpoints, artifacts and explicit user directives; it does not require shutting the workflow down. Use fallback executors to keep a run recoverable when the primary executor fails or loses model capacity.
- To change one AI/mixed node's brain routing while the workflow remains active, use `foundry workflow update-node-brain --workflow <workflow-id> --task <task-id> --default-brain agy --agent-slot agent-001=agy:primary_node_agent:node-default --max-parallel-agents 1 --origin codex --output json` or MCP `foundry.workflow.update_node_brain`. Use repeated `--agent-slot` values for multiple agents, including multiple agents on the same brain.
- If a heartbeat becomes stale, use `foundry request recover-stale --run <run-id> --origin codex --output json` or `foundry.run.recover_stale` to move the run to `needs_attention` without losing workflow/run lineage.
- Poll later with `foundry mcp call foundry.run.status --input '{"run_id":"<run-id>"}' --output json`.
- List active requests with `foundry mcp call foundry.request.list --input '{"status":"accepted"}' --output json`.
- Cancel a request with `foundry mcp call foundry.request.cancel --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Resume a paused async handoff with `foundry mcp call foundry.run.resume --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Create scheduled Goal research through `foundry.schedule.create_daily_goal_research`; inspect/list/summarize/mutate schedules through `foundry.schedule.list`, `foundry.schedule.summary`, `foundry.schedule.loop_summary`, `foundry.schedule.worker_status`, `foundry.workflow.inspect`, `foundry.loop.inspect` and `foundry.schedule.update`.
- Use `foundry.schedule.update` or `foundry schedule update --next-run-at <RFC3339>` for explicit due timestamp mutation, `foundry.schedule.run_due` for one workflow, and `foundry.schedule.scan_due` when Foundry should scan all scheduled workflows, lease due nodes locally and record idle scale-to-zero decisions. Paused/stopped loop nodes must not advance.
- Use `foundry schedule worker-status` or `foundry.schedule.worker_status` to inspect next wakeup, scale-to-zero, bounded worker-pool capacity, cancellation safe points and backpressure before relying on tmux/systemd sleeps.
- Use `foundry.credential_vault.describe` and `foundry.credential_vault.records` to inspect local credential-vault contracts without resolving secrets. Use `foundry credential-vault exec --contract <contract> --data <data> --record <record> -- <command>` when an executor needs credentials injected into a child process.
- AWS is not a Foundry Core runtime, build, installation, backup, or production-gate dependency. Only when an operator explicitly selects the optional AWS adoption integration, use `foundry aws check`, `foundry aws inventory` or MCP tools `foundry.aws.check`, `foundry.aws.inventory`, `foundry.aws.raw`. These commands delegate to `~/plugins/aws-ops/scripts/aws-ops`, use the AWS credential-vault defaults and keep mutation gating in the aws-ops wrapper.
- Inspect or route work through `foundry.workflow.inspect`, `foundry.context.request`, `foundry.task.handoff`, `foundry.patch.plan`, `foundry.patch.diff`, `foundry.patch.review`, `foundry.patch.apply`, `foundry.patch.revert`, `foundry.patch.restore`, `foundry.workflow.attach_artifact`, `foundry.workflow.update_goal`, `foundry.validation.status` and `foundry.artifact.fetch`.
- In the interactive `foundry` REPL, use `j`/`k` to move panel focus, `enter` to open the focused panel, `r` to refresh the advanced cockpit frame, `m` to cycle compact/detailed/focus display modes, and `t` to cycle themes. Use `/cockpit`, `/task-board`, `/workflow-mutation`, `/operating-context`, `/improvement-loop`, `/readiness`, `/schedules`, `/addons`, `/sessions`, `/harness`, `/logs`, `/permissions`, `/identity` and `/workflow-dag` to render operational panels in place. Use `/context --workflow <id> --task <task-id> --budget 1200 --strict` for bounded context inspection and `/handoff --workflow <id> --task <task-id> --executor codex` only after approving lease acquisition.
- Treat source-code editing as a software-development Addon capability, not as a universal Core claim. Before presenting file-editing or patch-workbench UI, inspect `foundry addons resolve --goal "<editing goal>" --output json` or `foundry addons catalog --output json` and verify `foundry.addon.software_development`, capability `source_code_patch_lifecycle`, permission `source_code.patch`, view `software.patch_workbench` and runtime contract `source_code_patch_lifecycle.executor`. The current contract uses `foundry_core_builtin`/`foundry.patch.lifecycle` only as a compatibility executor while the Addon boundary matures.
- Use `foundry patch plan` or MCP tool `foundry.patch.plan` before agent file editing to create a bounded patch plan with repo-relative target paths, file snapshots, permission gates, diff-review commands, validation commands and a Foundry artifact; this command does not apply changes.
- Use `foundry interactive patch-workbench` or MCP `foundry.interactive.patch_workbench` to inspect the current repo's bounded inline `diff_preview`, `diff_review_queue`, per-file `action_hint`, `edit_intake` and ordered `operation_plan` before presenting a TUI/web review panel; each file `action_hint` tells whether the UI should offer review, plan creation or diff inspection and why apply remains blocked, `edit_intake` declares required inputs, missing fields, form readiness and safe command templates for `patch plan/review/diff/apply/revert/restore`, while `operation_plan` sequences the lifecycle steps, dependencies and human-approval gates. Then use `foundry patch diff` or MCP tool `foundry.patch.diff` to build a read-only multi-file diff navigation model with selectable file/hunk indexes before asking for human approval.
- Use `foundry patch review` or MCP tool `foundry.patch.review` after file edits and before apply approval to persist current diff/status/check evidence without modifying files.
- Use `foundry patch apply` or MCP tool `foundry.patch.apply` after a bounded executor edits files to record current file snapshots, validation output and a rollback artifact under workflow lineage.
- Use `foundry patch revert` or MCP tool `foundry.patch.revert` to record a guarded rollback proposal. It does not run `git checkout` or restore files automatically; human approval must precede destructive restore execution.
- Use `foundry patch restore --confirm-restore --approved-by <operator>` or MCP `foundry.patch.restore` only after a human-approved revert artifact exists; this is the explicit execution path that restores repo-local files and records `foundry.patch_restore.v1` evidence.
- Inspect Foundry 0.5 release readiness through `foundry.milestone.status`, the full release-gate manifest through `foundry.milestone.manifest`, the export/demo baseline through `foundry.milestone.export_demo`, and replacement-grade CLI demo evidence through `foundry.milestone.cli_demo`; the CLI demo includes `foundry.milestone.patch_lifecycle_demo.v1` with patch plan/review/diff/apply/revert/restore artifact lineage in an isolated fixture repo, `foundry.milestone.executor_project_demo.v1` for governed harness execution in an isolated project, `foundry.milestone.brain_handoff_demo.v1` for Foundry-owned context, node-brain routing, task handoff, plan-only shell launch and audit-only session lifecycle without child CLI/model execution, `foundry.milestone.headroom_runtime_wrapper_demo.v1` for the non-executing Foundry-first Headroom runtime wrapper contract reused from `foundry harness wrap-plan`, `foundry.milestone.real_project_workflow_demo.v1` for multi-file code plus research artifacts through project-root handoff, brain routing, harness execution, validation and stdout headroom in an isolated project, and `foundry.milestone.connected_external_brain_demo.v1` plus `foundry.milestone.connected_external_brain_provider.v1` for a connected external-brain adapter process with provider-output schema validation, command/stdout hashes, handoff, harness lineage, event recording and validation while still declaring no live model/provider execution unless a project explicitly provides `.foundry/connected-brain-runtimes.json` and the caller selects it with `foundry milestone cli-demo --project-root <project-root> --connected-brain <provider-id>`. `groundwork`, `planned` and `blocked` capabilities prevent promotion.
- When using `foundry.milestone.cli_demo`, inspect top-level `headroom_stats` for aggregated replacement-CLI token savings and reversible retrieval commands across the executor, real-project and connected-brain harness flows; inspect top-level `headroom_runtime_wrapper` for the structured wrapper runtime contract, including interception points, content routes, reversible store URI, retrieval MCP tools and env overlay.
- Treat multimodal execution as the first-party `foundry.addon.multimodal` capability `multimodal_runtime`, not as a universal Core claim. Before presenting multimodal benchmark UI or workflow steps, inspect `foundry addons resolve --goal "<multimodal goal>" --output json` or `foundry addons catalog --output json` and verify permission `multimodal.runtime_benchmark`, view `multimodal.benchmark_center` and runtime contract `multimodal_runtime_benchmark.executor`; queue it with `foundry addons dispatch-contract` or MCP `foundry.addons.dispatch_contract`, then execute the guarded Addon path with `foundry addons run-dispatch` or MCP `foundry.addons.run_dispatch`. The current contract uses `foundry_core_builtin`/`foundry.multimodal.runtime_benchmark` only as guarded compatibility while production Addon runtime workers mature.
- Inspect the experimental multimodal track through `foundry.multimodal.status`; use approved `.foundry/multimodal.json` plus CLI `--project-root` or MCP `project_root` to make feature-flag state explicit without relying on process cwd; generate plan-only model/runtime install manifests through `foundry.multimodal.install_plan`; inspect runtime/model readiness through `foundry multimodal readiness` or MCP `foundry.multimodal.readiness` without installs, model execution, device access, network access or automation; generate benchmark/report templates through `foundry.multimodal.benchmark_template`; record approval-gated fixture-only benchmark results through `foundry.multimodal.benchmark_result` without installs, model execution, device access or network access; run guarded deterministic local runtime benchmark evidence through `foundry multimodal runtime-benchmark` or MCP `foundry.multimodal.runtime_benchmark` only after experimental opt-in, `--approved-by`, `--confirm-runtime-execution` and `--allow-model`; when a project declares approved connected runtimes in `.foundry/multimodal-runtimes.json`, select one explicitly with `--connected-runtime <runtime-id>` or MCP `connected_runtime` so Foundry loads the project manifest, runs only the declared probe command, records hashes/measurements and still performs no installs, network access or device access by default; connected runtimes may include a `production` evidence block with approval, model manifest hash, evidence artifacts and quality/latency thresholds, and Foundry marks `promotion_ready=true` only when the probe measurements satisfy that contract; generate guarded local image/audio/Blender demo plans through `foundry.multimodal.demo_plan`; record guarded local fixture demo receipts after opt-in through `foundry.multimodal.demo_receipt` while proving camera, microphone, screen, input and filesystem access stay blocked without guard approval; evaluate camera, microphone, screen, input and peripheral access through `foundry.multimodal.guard` before any device or automation action.
- MCP mutations must still go through Foundry so revisions, artifact hashes, origins and validation gates are persisted.

- Read `dashboard.addon_capability_panel`, call `foundry interactive addon-capabilities --project-root <project-root> --output json`, or use MCP `foundry.interactive.addon_capabilities` with `project_root` before rendering Addon/capability operations. It exposes `foundry.interactive.addon_capability.v1` lifecycle counts, capability registry totals, permission gates, runtime contracts, TUI views and dispatch state without mutating workflows.
- Read `dashboard.schedule_panel`, call `foundry interactive schedules --output json`, or use MCP `foundry.interactive.schedules` before rendering scheduled workflow operations. It exposes `foundry.interactive.schedules.v1` due workflows, scheduler worker capacity, deterministic assignment queues, sleep/backpressure/cancellation state and observed scheduled workflow rows without mutating workflows.

## Safety Rules

- Never mark an execution step complete without validation evidence.
- Never treat task output as enough by itself. The task goal must be definitively ready.
- Do not use detected CLIs until `foundry sync executors` has persisted human authorization for them.
- Treat Docker/Kubernetes/Knative as run substrates. Do not install or mutate them without explicit authorization.
- Only mutate Foundry-owned runtime resources by default. External resources require a positive `foundry runtime guard` decision with explicit authorization.
- Runtime goal/artifact changes must go through Foundry so revisions and origins are persisted.
- When Codex/OpenCode use Foundry as a skill, they should not wait for long work inline. They should start a request, return `run_id`, and let Foundry continue asynchronously.
- Do not expose full project history to a task when `foundry context` can produce bounded local context.
- Do not expose private or internal memory to public audiences. Customer suggestions may be shared with a manager only after classification as `manager_shared`; public/global memory writes require explicit approval.
- Treat model providers as interchangeable execution resources and keep non-AI steps independent from live model calls.
- Do not resolve or print credential-vault secret values. Prefer `foundry credential-vault exec` so secrets only enter the child process environment.
- A notification step can generate an email payload with final workflow costs when that was part of the user's objective.
- Keep self-improvement controlled: experiment, benchmark, compare, then promote only after validation.

## Useful Commands

```bash
foundry plan --goal "Create a delivery platform" --output json
foundry request start --goal "Improve Foundry Core" --origin codex --output json
foundry cli create --name hubspot --goal "Create a relationship intelligence CLI over HubSpot" --source https://developers.hubspot.com --command deals-stale --compound-command forecast-health --output json
foundry request heartbeat --run <run-id> --executor codex --summary "executor applying bounded patch" --ttl-seconds 300 --pid <executor-pid> --origin codex --output json
foundry request drive --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json
foundry request step --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json
foundry request complete-task --run <run-id> --task <task-id> --executor codex --summary "executor finished the ready task with passing evidence" --evidence-command "<passing gate>" --evidence-exit-code <observed-exit-code> --origin codex --output json
foundry request final-package --run <run-id> --origin codex --output json
foundry request ensure-final-audit --workflow <workflow-id> --executor codex --origin codex --output json
foundry request switch-executor --run <run-id> --executor opencode --fallback-executor codex --summary "codex limit approaching; opencode continuing from Foundry state" --origin codex --output json
foundry workflow update-node-brain --workflow <workflow-id> --task task-001 --default-brain agy --agent-slot agent-001=agy:primary_node_agent:node-default --max-parallel-agents 1 --origin codex --output json
foundry request status --run <run-id> --output json
foundry request resume --run <run-id> --origin codex --output json
foundry request list --status stale --output json
foundry request recover-stale --run <run-id> --origin codex --output json
foundry ops snapshot --project-root <project-root> --output json
foundry ops serve --project-root <project-root> --host 127.0.0.1 --port 8765
foundry mcp call foundry.ops.snapshot --input '{"project_root":"<project-root>"}' --output json
foundry mcp call foundry.cli.create --input '{"name":"hubspot","goal":"Create a relationship intelligence CLI","source":"https://developers.hubspot.com","commands":["deals-stale"],"compound_commands":["forecast-health"]}' --output json
foundry ops renderer-event --workflow <workflow-id> --addon <addon-id> --view <view-id> --event-kind hover_changed --payload '{"point":"series.current"}' --output json
foundry improve candidates --output json
foundry events improvement-policy --workflow <workflow-id> --output json
foundry improve apply-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --apply --approved-by <operator> --output json
foundry improve apply-event-policy --workflow <workflow-id> --recommendation <recommendation-id> --apply --approved-by <operator> --output json
foundry improve benchmark-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --output json
foundry improve promote-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --approved-by <operator> --output json
foundry cost incremental --project-root . --after-sequence <global-event-id> --output json
foundry cost maintain --project-root . --workflow <workflow-id> --bucket day --group-by source_kind --retention-days 31 --output json
foundry cost daemon --project-root . --workflow <workflow-id> --bucket day --group-by workflow --max-cycles 2 --interval-seconds 300 --retention-days 31 --output json
foundry cost retention --project-root . --organization <organization-id> --retention-days 31 --apply --approved-by <operator> --reason "Validated retention window." --confirm --output json
foundry mcp tools --output json
foundry mcp call foundry.improve.candidates --input '{"limit":10}' --output json
foundry mcp call foundry.improve.benchmark_event_policy --input '{"workflow_id":"<workflow-id>","recommended_policy":"prefer_deterministic_node"}' --output json
foundry mcp call foundry.improve.promote_event_policy --input '{"workflow_id":"<workflow-id>","recommended_policy":"prefer_deterministic_node","approved_by":"<operator>"}' --output json
foundry mcp call foundry.ops.addon_renderer_event --input '{"workflow_id":"<workflow-id>","addon_id":"<addon-id>","view_id":"<view-id>","event_kind":"refresh_requested","payload":{"refresh":true}}' --output json
foundry mcp call foundry.cost.incremental --input '{"project_root":".","after_sequence":0}' --output json
foundry mcp call foundry.cost.daemon --input '{"project_root":".","workflow_id":"<workflow-id>","max_cycles":1,"interval_seconds":0}' --output json
foundry mcp call foundry.cost.retention --input '{"project_root":".","organization_id":"<organization-id>","retention_days":31,"apply":true,"approved_by":"<operator>","reason":"Validated retention window.","confirm":true}' --output json
foundry
foundry interactive guided-cockpit --output json
foundry interactive ui-composition --project-root . --output json
foundry mcp call foundry.interactive.guided_cockpit --output json
foundry mcp call foundry.interactive.ui_composition --input '{"project_root":"."}' --output json
foundry interactive core-boundary --project-root . --output json
foundry mcp call foundry.interactive.core_boundary --input '{"project_root":"."}' --output json
foundry mcp call foundry.interactive.home --input '{"project_root":"<project-root>"}' --output json
foundry interactive command-palette --output json
foundry mcp call foundry.interactive.command_palette --input '{"query":"patch"}' --output json
foundry interactive action-registry --output json
foundry mcp call foundry.interactive.action_registry --input '{"query":"patch"}' --output json
foundry interactive action-invocation --action patch.diff --output json
foundry mcp call foundry.interactive.action_invocation --input '{"action_id":"patch.diff"}' --output json
foundry interactive autocomplete --input "/patch r" --output json
foundry mcp call foundry.interactive.autocomplete --input '{"input":"/ha"}' --output json
foundry interactive operational-cockpit --output json
foundry mcp call foundry.interactive.operational_cockpit --output json
foundry interactive schedules --output json
foundry mcp call foundry.interactive.schedules --output json
foundry interactive context-memory --project-root . --output json
foundry mcp call foundry.interactive.context_memory --input '{"project_root":"."}' --output json
foundry interactive operating-context --project-root . --output json
foundry mcp call foundry.interactive.operating_context --input '{"project_root":"."}' --output json
foundry interactive architecture --project-root . --output json
foundry mcp call foundry.interactive.architecture --input '{"project_root":"."}' --output json
foundry interactive improvement-loop --output json
foundry mcp call foundry.interactive.improvement_loop --output json
foundry interactive workflow-mutation --output json
foundry mcp call foundry.interactive.workflow_mutation --output json
foundry interactive addon-capabilities --project-root . --output json
foundry mcp call foundry.interactive.addon_capabilities --input '{"project_root":"."}' --output json
foundry smoke operational-tui --output json
foundry smoke foundry-first-harness --output json
foundry smoke replacement-cli-evidence --output json
foundry smoke multimodal-runtime-evidence --output json
foundry interactive readiness --output json
foundry mcp call foundry.interactive.readiness --output json
foundry interactive release-gates --version 0.5 --project-root . --output json
foundry mcp call foundry.interactive.release_gates --input '{"version":"0.5","project_root":"."}' --output json
foundry interactive harness --output json
foundry mcp call foundry.interactive.harness --output json
foundry interactive sessions --output json
foundry mcp call foundry.interactive.sessions --output json
foundry interactive patch-workbench --output json
foundry mcp call foundry.interactive.patch_workbench --output json
foundry interactive permissions --output json
foundry mcp call foundry.interactive.permissions --output json
foundry interactive identity --output json
foundry mcp call foundry.interactive.identity --output json
foundry interactive task-board --output json
foundry mcp call foundry.interactive.task_board --output json
foundry interactive workflow-dag --output json
foundry mcp call foundry.interactive.workflow_dag --output json
foundry interactive structured-logs --output json
foundry mcp call foundry.interactive.structured_logs --output json
foundry brains --output json
foundry mcp call foundry.brain_router --output json
foundry sessions --output json
foundry sessions --provider codex --state opened --output json
foundry sessions history --session codex-shell --output json
foundry mcp call foundry.sessions --output json
foundry mcp call foundry.sessions --input '{"provider_id":"codex","lifecycle_state":"opened"}' --output json
foundry mcp call foundry.session.history --input '{"session_id":"codex-shell"}' --output json
foundry sessions lifecycle --session codex-shell --state opened --workflow <workflow-id> --task <task-id> --run <run-id> --origin foundry_cli --output json
foundry sessions lifecycle --session codex-shell --state attached --workflow <workflow-id> --task <task-id> --run <run-id> --origin foundry_cli --output json
foundry mcp call foundry.session.lifecycle --input '{"session_id":"codex-shell","state":"closed","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","origin":"mcp"}' --output json
/harness doctor --executor codex --shim-dir <dir> --project-root <project-root>
foundry harness doctor --executor codex --shim-dir <dir> --project-root <project-root> --output json
foundry mcp call foundry.harness.doctor --input '{"executor":"codex","shim_dir":"<dir>","project_root":"<project-root>"}' --output json
foundry harness mode --project-root <project-root> --output json
foundry harness headroom-plan --executor codex --project-root <project-root> --output json
foundry harness headroom-stats --source <source> --output json
foundry mcp call foundry.harness.headroom_plan --input '{"executor":"codex","project_root":"<project-root>"}' --output json
foundry mcp call foundry.harness.headroom_stats --input '{"source":"<source>","limit":5}' --output json
foundry harness adoption-plan --executor codex --shim-dir <dir> --project-root <project-root> --output json
foundry mcp call foundry.harness.adoption_plan --input '{"executor":"codex","shim_dir":"<dir>","project_root":"<project-root>"}' --output json
foundry harness activation-profile --executor codex --shim-dir <dir> --project-root <project-root> --output json
foundry harness activation-profile --executor codex --shim-dir <dir> --project-root <project-root> --shell-rc <path> --apply --approved-by <operator> --output json
foundry mcp call foundry.harness.activation_profile --input '{"executor":"codex","shim_dir":"<dir>","project_root":"<project-root>"}' --output json
foundry harness bootstrap --executor codex --shim-dir <dir> --project-root <project-root> --output json
foundry harness bootstrap --executor codex --shim-dir <dir> --project-root <project-root> --apply --approved-by <operator> --output json
foundry mcp call foundry.harness.bootstrap --input '{"executor":"codex","shim_dir":"<dir>","project_root":"<project-root>"}' --output json
foundry harness wrap-plan --executor codex --cmd codex --project-root <project-root> --output json
foundry harness install-shims --shim-dir <dir> --executor codex --project-root <project-root> --output json
foundry harness exec --executor codex --foundry-first --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --output json -- <cmd>
foundry harness exec --executor codex --foundry-first --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --execute --allow-exec --output json -- <cmd>
foundry shells --executor codex --workflow <workflow-id> --task <task-id> --run <run-id> --context-budget 1200 --ttl-seconds 900 --output json
foundry mcp call foundry.shell.launch_plan --input '{"executor":"codex","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":1200,"ttl_seconds":900}' --output json
foundry shells --executor codex --workflow <workflow-id> --task <task-id> --run <run-id> --record-session --origin foundry_cli --output json
foundry mcp call foundry.shell.record_plan --input '{"executor":"codex","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":1200,"ttl_seconds":900,"origin":"mcp"}' --output json
foundry identity membership-update --subject <user-id> --organization <org-id> --brand <brand-id> --product <product-id> --grant workflow:mutate --source codex --output json
foundry mcp call foundry.identity.membership_update --input '{"subject_id":"<user-id>","organization_id":"<org-id>","brand_id":"<brand-id>","product_id":"<product-id>","grant_permissions":["workflow:mutate"],"source":"codex"}' --output json
foundry memory configure --project-root <project-root> --memory-level MEMORY_SHORT_TERM --default-scope project --default-scope processing --default-audience manager --privacy-mode private_by_default --retention-mode processing_auto_archive --approved-by arthur --reason "Project memory defaults" --output json
foundry memory policy --project-root <project-root> --output json
foundry memory search --workflow <workflow-id> --query "customer suggestion operations" --scope project --scope processing --audience manager --memory-level short_term --output json
foundry memory search --workflow <workflow-id> --query "operating decisions" --scope organization --organization digital-directive --audience internal --memory-level standard --output json
foundry memory promote --workflow <workflow-id> --from-scope processing --to-scope organization --source-path ./run-memory.md --summary "Curated product signal without private data." --approved-by arthur --reason "Useful operating memory for the organization." --organization digital-directive --output json
foundry memory promotions --workflow <workflow-id> --to-scope organization --approved-by arthur --output json
foundry memory retention --workflow <workflow-id> --scope processing --scope project --output json
foundry memory cleanup --workflow <workflow-id> --scope processing --dry-run --output json
foundry memory cleanup --workflow <workflow-id> --scope processing --mode archive --approved-by arthur --reason "Final packaging complete." --confirm --output json
foundry mcp call foundry.memory.configure --input '{"project_root":"<project-root>","memory_level":"MEMORY_SHORT_TERM","default_scopes":["project","processing"],"default_audience":"manager","privacy_mode":"private_by_default","retention_mode":"processing_auto_archive","approved_by":"arthur","reason":"Project memory defaults"}' --output json
foundry mcp call foundry.memory.policy --input '{"project_root":"<project-root>"}' --output json
foundry mcp call foundry.memory.search --input '{"workflow_id":"<workflow-id>","query":"operating decisions","scopes":["organization"],"organization_id":"digital-directive","audience":"internal","limit":5}' --output json
foundry mcp call foundry.memory.promote --input '{"workflow_id":"<workflow-id>","from_scope":"processing","to_scope":"project","source_path":"./run-memory.md","summary":"Curated project memory without private data.","approved_by":"arthur","reason":"Useful for this project","visibility":"internal","shareability":"project_shared","dry_run":true}' --output json
foundry mcp call foundry.memory.promotions --input '{"workflow_id":"<workflow-id>","to_scope":"organization","approved_by":"arthur"}' --output json
foundry mcp call foundry.memory.retention --input '{"workflow_id":"<workflow-id>","scopes":["processing","project"]}' --output json
foundry mcp call foundry.memory.cleanup --input '{"workflow_id":"<workflow-id>","scopes":["processing"],"dry_run":true}' --output json
foundry mcp call foundry.interactive.slash_commands --output json
foundry mcp call foundry.interactive.route --input '{"input":"What is the current Foundry status?","origin":"codex"}' --output json
foundry mcp call foundry.run.start --input '{"goal":"Improve Foundry Core","origin":"codex"}' --output json
foundry mcp call foundry.run.heartbeat --input '{"run_id":"<run-id>","executor":"codex","summary":"executor alive","ttl_seconds":300,"origin":"codex"}' --output json
foundry mcp call foundry.run.drive --input '{"run_id":"<run-id>","executor":"codex","ttl_seconds":300,"origin":"codex"}' --output json
foundry mcp call foundry.run.step --input '{"run_id":"<run-id>","executor":"codex","ttl_seconds":300,"origin":"codex"}' --output json
foundry mcp call foundry.run.complete_task --input '{"run_id":"<run-id>","task_id":"<task-id>","executor":"codex","summary":"executor finished the ready task with passing evidence","origin":"codex"}' --output json
foundry mcp call foundry.run.final_package --input '{"run_id":"<run-id>","origin":"codex"}' --output json
foundry mcp call foundry.run.switch_executor --input '{"run_id":"<run-id>","executor":"opencode","fallback_executors":["codex"],"summary":"take over without stopping workflow","origin":"codex"}' --output json
foundry mcp call foundry.workflow.update_node_brain --input '{"workflow_id":"<workflow-id>","task_id":"task-001","default_brain":"agy","agent_slots":["agent-001=agy:primary_node_agent:node-default"],"max_parallel_agents":1,"origin":"codex"}' --output json
foundry mcp call foundry.run.recover_stale --input '{"run_id":"<run-id>","origin":"codex"}' --output json
foundry mcp call foundry.run.status --input '{"run_id":"<run-id>"}' --output json
foundry request list --output json
foundry request list --status accepted --output json
foundry request list --status needs_attention --output json
foundry request cancel --run <run-id> --origin codex --output json
foundry credential-vault records --contract /path/to/vault.contract.yaml --data /path/to/vault.data.yaml --output json
foundry credential-vault exec --contract /path/to/vault.contract.yaml --data /path/to/vault.data.yaml --record login -- command-that-needs-secrets
foundry mcp call foundry.credential_vault.describe --input '{"contract":"/path/to/vault.contract.yaml","data":"/path/to/vault.data.yaml"}' --output json
foundry aws check --output json
foundry aws inventory --regions us-east-1,sa-east-1 --output json
foundry aws raw -- sts get-caller-identity
foundry mcp call foundry.aws.check --input '{}' --output json
foundry mcp call foundry.aws.inventory --input '{"regions":"us-east-1,sa-east-1"}' --output json
foundry sync all --home "$HOME" --shim-dir "$HOME/.foundry/bin" --allow codex --allow opencode --output json
foundry executors --output json
foundry runtimes --output json
foundry workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
foundry workflow attach-artifact --workflow <workflow-id> --path ./artifact.md --kind report --tag <tag> --origin opencode --output json
foundry mcp call foundry.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"./artifact.md","kind":"report","tags":["report","crm"],"origin":"codex"}' --output json
foundry mcp call foundry.context.request --input '{"workflow_id":"<workflow-id>","task_id":"task-001","budget":1200,"project_root":"<project-root>","view":"compact"}' --output json
foundry mcp call foundry.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","executor":"codex","budget":1200,"project_root":"<project-root>"}' --output json
/context --workflow <workflow-id> --task task-001 --budget 1200 --strict
/handoff --workflow <workflow-id> --task task-001 --executor codex --budget 1200
foundry patch plan --workflow <workflow-id> --task task-001 --intent "Patch selected files with human diff review" --path Cargo.toml --origin codex --output json
foundry mcp call foundry.patch.plan --input '{"workflow_id":"<workflow-id>","task_id":"task-001","intent":"Patch selected files with human diff review","paths":["Cargo.toml"],"origin":"codex"}' --output json
foundry patch review --workflow <workflow-id> --task task-001 --path Cargo.toml --origin codex --output json
foundry patch diff --workflow <workflow-id> --task task-001 --path Cargo.toml --file-index 0 --hunk-index 0 --origin codex --output json
foundry mcp call foundry.patch.diff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","paths":["Cargo.toml"],"file_index":0,"hunk_index":0,"origin":"codex"}' --output json
foundry mcp call foundry.patch.review --input '{"workflow_id":"<workflow-id>","task_id":"task-001","paths":["Cargo.toml"],"origin":"codex"}' --output json
foundry patch apply --workflow <workflow-id> --task task-001 --path Cargo.toml --origin codex --output json
foundry patch revert --workflow <workflow-id> --task task-001 --apply-artifact <attached-patch_apply.json> --origin codex --output json
foundry patch restore --workflow <workflow-id> --task task-001 --revert-artifact <attached-patch_revert.json> --approved-by <operator> --confirm-restore --origin codex --output json
foundry mcp call foundry.patch.apply --input '{"workflow_id":"<workflow-id>","task_id":"task-001","paths":["Cargo.toml"],"origin":"codex"}' --output json
foundry mcp call foundry.patch.revert --input '{"workflow_id":"<workflow-id>","task_id":"task-001","apply_artifact":"<attached-patch_apply.json>","origin":"codex"}' --output json
foundry mcp call foundry.patch.restore --input '{"workflow_id":"<workflow-id>","task_id":"task-001","revert_artifact":"<attached-patch_revert.json>","approved_by":"<operator>","confirm_restore":true,"origin":"codex"}' --output json
foundry schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json
foundry mcp call foundry.schedule.create_daily_goal_research --input '{"goals":["hackathon"],"timezone":"America/Sao_Paulo","cron":"0 8 * * *","origin":"codex"}' --output json
foundry schedule summary --output json
foundry schedule loop-summary --output json
foundry schedule worker-status --executor foundry-scheduler --max-workers 1 --ttl-seconds 300 --output json
foundry mcp call foundry.schedule.summary --output json
foundry mcp call foundry.schedule.loop_summary --output json
foundry mcp call foundry.schedule.worker_status --input '{"executor":"mcp-scheduler","max_workers":1,"ttl_seconds":300}' --output json
foundry schedule update --workflow <workflow-id> --task task-009 --next-run-at 2026-05-26T11:00:00Z --origin codex --output json
foundry mcp call foundry.schedule.update --input '{"workflow_id":"<workflow-id>","task_id":"task-009","next_run_at":"2026-05-26T11:00:00Z","origin":"codex"}' --output json
foundry schedule pause --workflow <workflow-id> --task task-010 --origin codex --output json
foundry schedule resume --workflow <workflow-id> --task task-010 --origin codex --output json
foundry schedule run-due --workflow <workflow-id> --output json
foundry schedule scan-due --executor foundry-scheduler --ttl-seconds 300 --output json
foundry mcp call foundry.schedule.scan_due --input '{"executor":"mcp-scheduler","ttl_seconds":300}' --output json
foundry runtime guard --substrate knative --resource service/foundry-node --namespace foundry --action update --owner foundry --output json
foundry list --output json
foundry status --workflow <workflow-id> --output json
foundry context --workflow <workflow-id> --task task-001 --project-root <project-root> --budget 1200 --strict --view compact --output json
foundry run --workflow <workflow-id> --simulate --output json
foundry validate --workflow <workflow-id> --output json
foundry artifacts --workflow <workflow-id> --output json
foundry milestone status --version 0.5 --output json
foundry milestone manifest --version 0.5 --output json
foundry interactive multimodal-runtime --project-root . --output json
foundry milestone prepare-evidence-inputs --version 0.5 --capability replacement_grade_cli --project-root . --connected-brain <provider-id> --provider-command <absolute-provider-adapter-path> --model-id <approved-model-id> --approval-ref <approval-ref> --apply --approved-by arthur --output json
foundry milestone prepare-evidence-inputs --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --apply --approved-by arthur --output json
foundry milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --output json
foundry milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root . --connected-brain <provider-id> --approved-by arthur --origin codex --output json
foundry milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind broader_project_coding_research_workflow --project-root . --approved-by arthur --origin codex --output json
foundry milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind terminal_file_editing_ux --project-root . --approved-by arthur --origin codex --output json
foundry milestone collect-evidence --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --approved-by arthur --output json
foundry milestone attach-evidence --version 0.5 --capability experimental_multimodal_runtime --kind production_runtime_benchmark --summary "Operator-approved runtime receipt." --artifact ./runtime-receipt.json --approved-by arthur --output json
foundry milestone export-demo --origin codex --output json
foundry milestone cli-demo --origin codex --output json
foundry multimodal status --output json
foundry multimodal status --project-root . --output json
foundry multimodal install-plan --capability audio_transcription --output json
foundry multimodal readiness --capability image_understanding --output json
foundry multimodal benchmark-template --capability audio_transcription --output json
foundry multimodal benchmark-result --capability image_understanding --fixture static_image_labels --approved-by <operator> --confirm-fixture-only --output json
foundry multimodal runtime-benchmark --capability image_understanding --fixture static_image_labels --approved-by <operator> --confirm-runtime-execution --allow-model --output json
foundry multimodal runtime-benchmark --capability image_understanding --fixture static_image_labels --project-root <project-root> --connected-runtime <runtime-id> --approved-by <operator> --confirm-runtime-execution --allow-model --output json
foundry multimodal demo-plan --demo local_image_recognition --output json
foundry multimodal demo-receipt --demo local_image_recognition --fixture static_image_labels --approved-by <operator> --confirm-local-fixture --allow-model --output json
foundry multimodal guard --capability camera --action access --output json
foundry mcp call foundry.multimodal.status --output json
foundry mcp call foundry.multimodal.status --input '{"project_root":"."}' --output json
foundry mcp call foundry.multimodal.install_plan --input '{"capability_id":"audio_transcription"}' --output json
foundry mcp call foundry.multimodal.readiness --input '{"capability_id":"image_understanding"}' --output json
foundry mcp call foundry.multimodal.benchmark_template --input '{"capability_id":"audio_transcription"}' --output json
foundry mcp call foundry.multimodal.benchmark_result --input '{"capability_id":"image_understanding","fixture_id":"static_image_labels","approved_by":"<operator>","confirm_fixture_only":true}' --output json
foundry mcp call foundry.multimodal.runtime_benchmark --input '{"project_root":".","capability_id":"image_understanding","fixture_id":"static_image_labels","approved_by":"<operator>","confirm_runtime_execution":true,"allow_model":true}' --output json
foundry mcp call foundry.multimodal.runtime_benchmark --input '{"project_root":".","capability_id":"image_understanding","fixture_id":"static_image_labels","connected_runtime":"<runtime-id>","approved_by":"<operator>","confirm_runtime_execution":true,"allow_model":true}' --output json
foundry mcp call foundry.multimodal.demo_plan --input '{"demo_id":"local_image_recognition"}' --output json
foundry mcp call foundry.multimodal.demo_receipt --input '{"project_root":".","demo_id":"local_image_recognition","fixture_id":"static_image_labels","approved_by":"<operator>","confirm_local_fixture":true,"allow_model":true}' --output json
foundry mcp call foundry.multimodal.guard --input '{"capability":"camera","action":"access","enable_experimental":false,"allow":false}' --output json
foundry mcp call foundry.milestone.status --input '{"version":"0.5"}' --output json
foundry mcp call foundry.milestone.manifest --input '{"version":"0.5"}' --output json
foundry mcp call foundry.milestone.export_demo --output json
foundry mcp call foundry.milestone.cli_demo --output json
foundry improve --workflow <workflow-id> --target-version 0.3.0 --output json
foundry self run --repo /home/arthur/projects/foundry-core --until 2026-05-25T10:00:00-03:00 --executor opencode --fallback-executor codex --max-cycles 1 --output json
```
"#;

const RUNTIME_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-runtime/SKILL.md");

const CONTEXT_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-context/SKILL.md");
const MISSIONS_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-missions/SKILL.md");
const DOCUMENTATION_SKILL_MD: &str =
    include_str!("../.agents/skills/foundry-core-documentation/SKILL.md");
const AGENT_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-agent/SKILL.md");
const WORKFLOW_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-workflow/SKILL.md");

const ARTIFACT_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-artifacts/SKILL.md");

const EXECUTOR_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-executors/SKILL.md");

const WORKSPACES_SKILL_MD: &str =
    include_str!("../.agents/skills/foundry-core-workspaces/SKILL.md");

const ADDONS_UI_SKILL_MD: &str = include_str!("../.agents/skills/foundry-core-addons-ui/SKILL.md");

#[derive(Debug, Clone, Copy)]
struct SkillModule {
    name: &'static str,
    markdown: &'static str,
}

const SKILL_MODULES: &[SkillModule] = &[
    SkillModule {
        name: "foundry-core-runtime",
        markdown: RUNTIME_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-missions",
        markdown: MISSIONS_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-context",
        markdown: CONTEXT_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-artifacts",
        markdown: ARTIFACT_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-executors",
        markdown: EXECUTOR_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-workspaces",
        markdown: WORKSPACES_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-addons-ui",
        markdown: ADDONS_UI_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-documentation",
        markdown: DOCUMENTATION_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-agent",
        markdown: AGENT_SKILL_MD,
    },
    SkillModule {
        name: "foundry-core-workflow",
        markdown: WORKFLOW_SKILL_MD,
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct SkillInstallReport {
    pub skill: String,
    pub installed: Vec<String>,
}

pub fn install_skill(home: &Path, targets: &[String]) -> Result<SkillInstallReport> {
    let mut installed = Vec::new();
    let mut effective_targets = targets.to_vec();
    if effective_targets.is_empty() {
        effective_targets.push("codex".to_string());
        effective_targets.push("opencode".to_string());
    }

    for target in &effective_targets {
        match target.as_str() {
            "codex" => {
                let path = home.join(".codex/skills").join(SKILL_NAME).join("SKILL.md");
                push_installed_paths(&mut installed, write_skill_bundle(&path)?);
            }
            "opencode" => {
                let path = home
                    .join(".config/opencode/skills")
                    .join(SKILL_NAME)
                    .join("SKILL.md");
                push_installed_paths(&mut installed, write_skill_bundle(&path)?);
            }
            "agents" => {
                let path = home
                    .join(".agents/skills")
                    .join(SKILL_NAME)
                    .join("SKILL.md");
                push_installed_paths(&mut installed, write_skill_bundle(&path)?);
            }
            other => anyhow::bail!("unsupported skill target: {other}"),
        }
    }

    let shared_path = home
        .join(".agents/skills")
        .join(SKILL_NAME)
        .join("SKILL.md");
    push_installed_paths(&mut installed, write_skill_bundle(&shared_path)?);

    Ok(SkillInstallReport {
        skill: SKILL_NAME.to_string(),
        installed,
    })
}

pub fn write_repo_skill(path: impl Into<PathBuf>) -> Result<()> {
    write_skill_bundle(&path.into()).map(|_| ())
}

fn write_skill_bundle(path: &Path) -> Result<Vec<PathBuf>> {
    write_skill_file(path, SKILL_MD)?;
    let skill_dir = path
        .parent()
        .with_context(|| format!("skill path has no parent: {}", path.display()))?;
    let skills_root = skill_dir
        .parent()
        .with_context(|| format!("skill directory has no parent: {}", skill_dir.display()))?;
    let mut written = vec![path.to_path_buf()];

    for module in SKILL_MODULES {
        let module_path = skills_root.join(module.name).join("SKILL.md");
        write_skill_file(&module_path, module.markdown)?;
        written.push(module_path);
    }

    Ok(written)
}

fn write_skill_file(path: &Path, markdown: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create skill directory {}", parent.display()))?;
    }
    fs::write(path, markdown).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn push_installed_paths(installed: &mut Vec<String>, paths: Vec<PathBuf>) {
    for path in paths {
        let display = path.display().to_string();
        if !installed
            .iter()
            .any(|installed_path| installed_path == &display)
        {
            installed.push(display);
        }
    }
}
