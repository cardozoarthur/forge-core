use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const SKILL_NAME: &str = "forge-core";

pub const SKILL_MD: &str = r#"---
name: forge-core
description: Use Forge Core to run operational and strategic assisted AI/non-AI workflows with goal-oriented DAGs, executor/runtime sync, live goal/node mutation, mutable artifacts, validation gates, persistence, rework loops, and controlled self-improvement.
license: MIT
compatibility: codex, opencode, gemini, claude
metadata:
  runtime: rust
  cli: forge
---

## What Forge Core Does

Forge Core is an operational, strategic and visual assisted-operations runtime, not a chatbot wrapper and not a human-flow builder. Use it when an objective needs to become a persistent execution graph that can mix AI steps, deterministic non-AI steps, scheduled waits/cron, notifications, code/subworkflow execution, live human/AI modification, visual tasks/subtasks and Forge-owned creative artifacts such as whiteboards, screens, components, wireframes, flows and design tokens.

## Required Workflow

1. Run `forge plan --goal "<human objective>" --output json`.
2. For skill-style use, prefer `forge request start --goal "<objective>" --origin codex|opencode|gemini|claude|skill --output json` and return the `run_id` to the caller.
3. Run `forge sync all --home "$HOME" --output json` when executor or runtime availability may have changed. If Forge-first CLI shims are installed, include `--shim-dir "$HOME/.forge/bin"` or the project shim directory so executor readiness includes harness status.
4. Inspect the generated atomic tasks, task goals, subtasks, impediments, async policy and validation rules.
5. Use `forge workflow update-goal ... --origin codex|opencode|gemini|forge_cli|skill` when the human changes direction during execution.
6. Use `forge workflow attach-artifact ... --origin codex|opencode|gemini|forge_cli|skill` when new artifacts appear during execution.
7. Use `forge context --workflow <id> --task <task-id> --project-root <project-root> --budget <bytes> --strict --output json` before giving an agent task-specific context; include `--project-root` whenever project `.forge/memory-governance.json` should affect the context memory policy.
8. Use `forge memory policy --project-root <project-root> --output json` and `forge memory search --workflow <workflow-id> --query "<query>" --memory-level none|session|short_term|standard|full|admin --scope global|organization|project|processing --organization <organization-id> --audience public|internal|manager|private --output json` before loading broad historical context. Forge memory is file-first, level-scoped, workflow/tenant-bound when a workflow is supplied, and visibility-gated; search returns snippets and line ranges, not whole files. Configure project defaults explicitly with `forge memory configure --project-root <project-root> ... --approved-by <operator> --reason "<reason>"` or MCP `forge.memory.configure`; when `memory_level`, `scope` and `audience` are omitted, search uses `.forge/memory-governance.json` defaults for that project.
9. Run `forge validate --workflow <id> --output json` before promotion. If `rework_tasks` is not empty, return those tasks to work.
10. Run `forge improve candidates --output json` or `forge.improve.candidates` before choosing a workflow to mutate; use its run/event/outcome/parallelization/cost evidence to decide whether to recover a stale run, parallelize ready handoffs, replace avoidable AI work with command nodes, or generate a controlled experiment.
11. Run `forge improve --workflow <id> --target-version <version> --output json` only to generate a controlled experiment and changelog. Use `forge improve apply-event-policy --workflow <id> --policy <policy> --apply --approved-by <operator> --output json` or `--recommendation <recommendation-id>` only for approved event-policy revisions; recommendations may target node, Addon or workflow scope. Then run `forge improve benchmark-event-policy --workflow <id> --policy <policy> --output json` to validate rollback, equivalence and workflow validation evidence, and only then `forge improve promote-event-policy --workflow <id> --policy <policy> --approved-by <operator> --output json` to record governed acceptance. Do not auto-promote without benchmark, validation and explicit approval evidence.
12. Run `forge milestone status --version 0.5 --output json` and `forge milestone manifest --version 0.5 --output json` before claiming Forge 0.5 creative-runtime readiness; planned or groundwork capabilities block promotion unless their complete operator-approved required attached-evidence set is present. Use `forge milestone evidence-plan --capability <capability-id> --project-root <project-root> --output json` or MCP `forge.milestone.evidence_plan` to inspect project manifests, secret-free `manifest_templates` such as `.forge/connected-brain-runtimes.json`, `.forge/multimodal.json` and `.forge/multimodal-runtimes.json`, and collection commands before running real provider/runtime evidence. When `manifest_templates` are present, use `forge milestone prepare-evidence-inputs --capability <capability-id> --project-root <project-root> --apply --approved-by <operator> --output json` or MCP `forge.milestone.prepare_evidence_inputs` to materialize only secret-free templates; it prepares inputs only and never counts as release evidence. Use `forge milestone collect-evidence --capability <capability-id> --kind <evidence-kind> --project-root <project-root> --approved-by <operator> --output json` or MCP `forge.milestone.collect_evidence` to run a ready provider/runtime/demo source, persist the receipt and attach it; omit `--kind` only when the capability default receipt is intended. Use `forge milestone attach-evidence --capability <capability-id> --kind <kind> --artifact <path> --approved-by <operator> --output json` or MCP `forge.milestone.attach_evidence` only to attach already-reviewed external release evidence; a single receipt stays audit-only, while the complete required receipt set makes that capability promotion-ready in the manifest.

## MCP Agent Surface

- Use `forge mcp tools --output json` to discover stable agent-facing tools before wiring a Codex/OpenCode workflow.
- Treat Codex, OpenCode, Gemini CLI, Claude CLI and future CLIs as replaceable execution brains only. Forge owns and routes workflow state, memory, skills, MCP servers/tools, credential-vault references, context packets, shell/session lifecycle, permissions, cost policy, validation gates and self-improvement decisions. Inspect this boundary with `forge brains --output json` or MCP `forge.brain_router`, inspect provider/session state with `forge sessions --output json` or MCP `forge.sessions`, record ordered shell session lifecycle with `forge sessions lifecycle` or MCP `forge.session.lifecycle`, and inspect per-session audit history with `forge sessions history --session <id>` or MCP `forge.session.history`, before handing work to a brain.
- Distinguish the orchestrator brain from node brains. Forge is the orchestrator brain/control plane; each AI or mixed workflow node can carry its own `node_brain_routing` contract with one or more agent slots, different brains per slot, multiple agents on the same brain, Forge-owned memory/skills/MCP routing, parallel execution when leases/quota/context allow it, node-level routing mutation through `forge workflow update-node-brain`, and run-level hot-swap through `forge request switch-executor` without stopping the workflow.
- In `forge.context.request` / `forge.task.handoff`, treat `prompt_packet.organization_context`, `prompt_packet.personality_decision` and `prompt_packet.company_work_decision` as required executor context. `organization_context` carries organization, brand, product, user, channel, memory/personality scope, tenant policy, brand voice/tone/values, terminology, design-system sources and operating policy. `personality_decision` carries the Forge-owned routing owner, selected persona mode/profile, selected voice/tone, brand voice/tone/values, style sources, fallback mode and audit flag. `company_work_decision` carries the compact multidisciplinary operating checklist for product, technical, financial, administrative, marketing, communication and delivery work. Their checksums plus `organization_context_required`, `personality_decision_required` and `company_work_decision_required` validation gates make agent output accountable to organizational context, personality routing and company operating discipline.
- Before reading historical memory for a task, inspect the `memory_policy` object returned by `forge context --project-root <project-root>` or task handoff with `project_root`, or call `forge memory policy --project-root <project-root> --output json` / MCP `forge.memory.policy` when operating on a project. It carries the workflow/tenant-derived memory level, allowed scopes, tenant boundary, default audience and governed `forge memory search --workflow <workflow-id>` command; project policy also reports `.forge/memory-governance.json` status, source fields and effective defaults. Do not inline broad memory into executor prompts.
- Promote memory only through `forge memory promote --workflow <workflow-id>` or MCP `forge.memory.promote` with `workflow_id`, using a curated summary, source path, approver, reason, visibility and compatible shareability. Never copy raw private processing memory into project, organization or global memory.
- Clean up processing memory only through `forge memory cleanup --workflow <workflow-id>` or MCP `forge.memory.cleanup`. Non-dry-run cleanup requires an approver, reason and confirm, and only archives/deletes files that `forge memory retention` classified as `delete_after_final_packaging`.
- Treat memory as scoped, classified files. `global` memory lives across projects, `organization` memory belongs to a tenant/org root, `project` memory belongs under the workflow/project `.forge` area, and `processing` memory is run-lived scratch that may be deleted after final packaging. Classify each memory as `public`, `internal` or `private`, and as `global_shared`, `organization_shared`, `project_shared`, `manager_shared`, `thread_private` or `non_shareable`. A customer suggestion starts private/thread-scoped, but a curated suggestion can be `manager_shared` for a manager or product owner without becoming public or globally reusable.
- Treat every customer request as company work, not only technical work. Even small tasks should decide what will be done, how it will be done, how delivery will be accepted, how it will be communicated, and whether product, technical, financial, administrative, marketing, communication or delivery concerns apply.
- Use `forge improve candidates --output json` or MCP `forge.improve.candidates` as the orchestrator's first improvement scan. It ranks live/degraded workflows with workflow events, run heartbeats, outcome evidence, parallel-ready handoffs and cost-efficiency signals for repetitive/deterministic tasks that are still using AI.
- Use `forge interactive home --project-root <project-root> --output json` or MCP `forge.interactive.home` with `project_root` when the home dashboard must render project-scoped release gates, harness mode and identity context without relying on process cwd.
- Inspect the no-argument interactive dashboard through `forge.interactive.home`, discover slash commands through `forge.interactive.slash_commands`, and route conversational input through `forge.interactive.route` when an agent needs the same command/chat classification as the TUI without launching a local terminal. Read `dashboard.navigation_panel` for `forge.interactive.navigation.v1` keyboard bindings, themes and compact/detailed/focus display modes. Read `dashboard.ui_composition_panel` for `forge.interactive.ui_composition.v1` regions, Core widgets, Addon widgets from enabled Addon views including `tui` surfaces, renderer families and refresh/inspection commands before composing a TUI, web dashboard or agent-side visual surface. Read `dashboard.release_gates_panel`, call `forge interactive release-gates --version 0.5 --project-root <project-root> --output json`, or use MCP `forge.interactive.release_gates` with `project_root` before claiming milestone readiness; it exposes promotion decision, blocked capabilities, required evidence, current evidence, per-capability evidence-plan status, secret-free manifest templates and next commands without mutating state. Read `dashboard.harness_panel` or call `forge interactive harness --output json` / MCP `forge.interactive.harness` for `forge.interactive.harness.v1` harness mode, doctor, shim status, CLI wrap-plan, `headroom_plan`, `adoption_plan`, `headroom_stats`, `session_lifecycle_plan`, token-headroom preview and guarded `bootstrap_project_harness` command before rendering Forge-first CLI controls. Read `dashboard.sessions_panel` or call `forge interactive sessions --output json` / MCP `forge.interactive.sessions` for `forge.interactive.sessions.v1` provider/session readiness, lifecycle state, per-session `operation_plan`, shell history commands and next lifecycle controls before opening or attaching brain shells. Read `dashboard.patch_workbench_panel` or call `forge interactive patch-workbench --output json` / MCP `forge.interactive.patch_workbench` for `forge.interactive.patch_workbench.v1` `addon_contract`, Git status, `diff_preview`, `diff_review_queue`, per-file `action_hint` next actions, `edit_intake` required inputs/forms, ordered `operation_plan`, diff stat/check, changed-file lanes, `approval_flow` review/approval/rollback gates and permission-gated `patch plan/review/diff/apply/restore` commands before presenting file editing or diff review UI. Read `dashboard.identity_panel` or call `forge interactive identity --output json` / MCP `forge.interactive.identity` for `forge.interactive.identity.v1` operating context, identity registry, channel aliases, memberships and tenant audit before rendering identity or tenant context operations. Read `dashboard.permissions_panel` or call `forge interactive permissions --output json` / MCP `forge.interactive.permissions` for `forge.interactive.permissions.v1` tenant memberships, Addon permission authorizations and pending human approvals before rendering permission or approval operations. Read `dashboard.structured_logs_panel` or call `forge interactive structured-logs --output json` / MCP `forge.interactive.structured_logs` for `forge.interactive.structured_logs.v1` recent event logs with store sequence, workflow, category, severity, origin, source, correlation, observability and payload preview before building timeline drill-downs. Before opening brain shells, call `forge interactive readiness --output json` or MCP `forge.interactive.readiness` for `forge.interactive.readiness.v1` executor, brain, shell, Forge-controlled surface and harness readiness with next corrective commands; also read `dashboard.harness_panel`, `dashboard.sessions_panel`, `dashboard.harness_mode_panel` and `dashboard.harness_doctor_panel`. Before operational handoff decisions, call `forge interactive operational-cockpit --output json` or MCP `forge.interactive.operational_cockpit` for `forge.interactive.operational_cockpit.v1`, read `dashboard.digital_twin_panel` for `forge.ops.operational_digital_twin.v1` state, `dashboard.dag_panel` or call `forge interactive workflow-dag --output json` / MCP `forge.interactive.workflow_dag` for `forge.interactive.workflow_dag.v1` dependency nodes/edges, readiness, human waits and drill-down commands, and read `dashboard.task_board_panel` or call `forge interactive task-board --output json` / MCP `forge.interactive.task_board`; these expose workflow lanes, attention, handoffs, checkpoint resume candidates, pending human waits, artifacts, selected brain, observability and direct next commands.
- Read `dashboard.command_palette_panel` or call `forge interactive command-palette --output json` / MCP `forge.interactive.command_palette` for `forge.interactive.command_palette.v1` grouped contextual actions before building command palettes, quick-open menus or agent-side action pickers; it is read-only and exposes navigation, workflow, Addon actions, permission, harness, session and observability actions with mutation and approval flags. CLI actions declared by enabled TUI Addon views can provide `palette_group`, `source_panel`, `description`, `risk_level`, `mutates_workflow`, `command_template` and `keywords`; the palette preserves `addon_contract` using `forge.interactive.addon_action_contract.v1`, plus `addon_view_id` and `addon_view_action_id`, and each entry carries `enabled`, `blocked_reason` and `operation_plan` using `forge.interactive.command_palette_action_plan.v1` so clients can show source Addon, Addon version/lifecycle, capability, permission, permission-gate status, execution/diagnostic status and concrete view action before offering domain-specific commands. When an Addon action is blocked by permission readiness, the entry remains visible for diagnosis, exposes no executable command template and points to the Addon view inspection command as the next diagnostic step.
- Read `dashboard.action_registry_panel`, type `/actions [query]` in the interactive REPL, or call `forge interactive action-registry --output json` / MCP `forge.interactive.action_registry` for `forge.interactive.action_registry.v1` when a TUI, web dashboard or agent needs a stable read-only list of actions independent of command-palette layout. It derives from the same governed action contracts but returns strict query filtering, per-group counts, enabled/blocked/diagnostic/mutation/approval totals, action `operation_plan` data and next diagnostic commands.
- Type `/action <action-id>` in the interactive REPL, call `forge interactive action-invocation --action <action-id> --output json`, or use MCP `forge.interactive.action_invocation` when a TUI, web dashboard or agent has selected one action and needs a safe invocation plan. It resolves the action from the registry into `forge.interactive.action_invocation.v1`, returns `can_execute`, `selected_command`, `operation_plan`, diagnostics and `not_executed=true`; clients must still execute explicit commands themselves or route future mutations through Forge.
- Read `dashboard.autocomplete_panel` or call `forge interactive autocomplete --input "<partial input>" --output json` / MCP `forge.interactive.autocomplete` for `forge.interactive.autocomplete.v1` slash-command, command-palette and `/action <partial>` action-id suggestions before building autocomplete, inline command completion or agent-side quick input; it is read-only and returns score, source panel, mutation, approval, `enabled`, `blocked_reason` and `operation_plan` flags. `/action` suggestions use source `action_registry`, insert `/action <action-id>`, and point equivalent commands to `forge interactive action-invocation --action <action-id> --output json`, so clients can complete selected action IDs without executing them. Command-palette suggestions preserve any `addon_contract` using `forge.interactive.addon_action_contract.v1`, plus `addon_view_id` and `addon_view_action_id`, so autocomplete can keep Addon/capability/permission/action lineage visible instead of flattening domain-specific actions into Core commands; blocked Addon actions remain discoverable but have empty insert/equivalent commands and diagnostic-only operation plans.
- Use the interactive `/brains`, `/sessions`, `/sessions history`, `/sessions lifecycle`, `/shells`, `/harness` and `/harness doctor` commands to show Forge-controlled brain routing, provider/session readiness, auditable shell lifecycle, effective Forge-first harness mode and full CLI readiness before opening a brain shell. Use `forge sessions --output json` or MCP `forge.sessions` to list providers, session readiness, lifecycle state, `lifecycle_policy.allowed_next_states`, per-session `operation_plan`, next lifecycle commands and recorded shell launch events; pass `--provider <brain>`, `--state <state>` or `--readiness <readiness>` on the CLI, or `provider_id`, `lifecycle_state` and `readiness` in MCP input, when the operator/UI only needs one provider or lifecycle lane. Use `forge sessions history --session <id> --output json` or MCP `forge.session.history` when an operator/agent needs one session's chronological shell-launch and lifecycle audit, counts, current state and next lifecycle commands without filtering global events in the client. Use `forge sessions lifecycle --session <id> --state opened|attached|closed --origin <origin>` or MCP `forge.session.lifecycle` to record audit-only shell lifecycle events without starting child processes; Forge returns `previous_state`, `lifecycle_sequence` and a `transition` policy, and rejects invalid ordered transitions such as detaching before attachment. Use `forge shells --executor <executor> --workflow <workflow-id> --task <task-id> --run <run-id> --output json` or MCP `forge.shell.launch_plan` when an operator/agent needs a plan-only launch report with readiness, preflight commands, concrete context/handoff/heartbeat commands, `prompt_packet_gate_policy` and handoff gates before starting an external brain shell; the policy lists `organization_context_required`, `personality_decision_required` and `company_work_decision_required` so shells cannot ignore prompt-packet decisions. Use `forge shells --record-session` or MCP `forge.shell.record_plan` when the shell intent should be auditable as a `shell_launch_planned` event. Directly opening `codex`, `opencode`, `gemini` or `claude` should be treated as inspection/debugging; production handoff should go through Forge context, leases and validation.
- Use `forge harness doctor --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` or MCP `forge.harness.doctor` for a consolidated read-only readiness audit before relying on Forge-first CLI operation. Use `forge harness mode --output json` or MCP `forge.harness.mode` to audit the effective Forge-first default, source, project config status, exec policy and precedence before relying on a wrapper or shim. Use `forge harness headroom-plan --executor <executor> --project-root <project-root> --output json` or MCP `forge.harness.headroom_plan` to inspect the effective context budget, token-headroom source, wrapper env, `session_lifecycle_plan`, compression pipeline, reserve strategy and next commands before handing large logs, tool output or CLI stdout to a brain. Use `forge harness headroom-stats --source <source> --output json` or MCP `forge.harness.headroom_stats` to inspect persisted token savings by source/content kind and top reversible retrieval refs before expanding noisy outputs back into context. Use `forge harness adoption-plan --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` or MCP `forge.harness.adoption_plan` to get the ordered read-only adoption plan for project harness config, Forge-first shims, executor sync, headroom verification and lineage-required execution without writing config, installing shims or launching child CLIs. Use `forge harness bootstrap --executor <executor> --shim-dir <dir> --project-root <project-root> --output json` or MCP `forge.harness.bootstrap` for a safe dry-run bootstrap; add `--apply --approved-by <operator>` only after review to write `.forge/harness.json` and install Forge-owned shims. Use `forge harness wrap-plan` / MCP `forge.harness.wrap_plan` before running Codex, Claude, Gemini or OpenCode under Forge-first control; it returns `forge.harness.session_lifecycle_plan.v1` with record-launch/opened/attached/closed commands so shell state is auditable before and after handoff. Pass `--project-root <project-root>` or MCP `project_root` when planning for another project's `.forge/harness.json`, and pass `--workflow`, `--task` and `--run` whenever the external CLI belongs to a workflow node. When a shell should prefer Forge infrastructure by default, set `FORGE_HARNESS_DEFAULT_MODE=forge_first`, add project `.forge/harness.json` with `{"default_mode":"forge_first"}`, or use `forge harness install-shims --shim-dir <dir> --executor <executor> --project-root <project-root>` / MCP `forge.harness.install_shims` with `project_root`; `--observe-only` disables those defaults for one CLI invocation. Project `.forge/harness.json` may also set `context_budget` or `default_context_budget`, `default_token_headroom` or `token_headroom`, and `require_token_headroom_for_forge_first`; `forge harness mode`, `headroom-plan`, `headroom-stats`, `adoption-plan`, `bootstrap`, `wrap-plan`, `doctor`, `install-shims` and `exec` report the resolved `context_budget_source`, `token_headroom_source` and `require_token_headroom_for_forge_first`. Add `"require_lineage_for_exec": true` to the same project file when real child execution must be blocked unless `workflow`, `task` and `run` lineage are present. Harness reports expose `forge_first_source`, `project_exec_policy_status`, `require_lineage_for_exec`, context budget and token-headroom sources, and the child overlay includes `FORGE_HARNESS_MODE_SOURCE`, so operators can tell whether Forge-first came from an explicit flag, env default, project config, observe-only override, MCP input or MCP default. Forge resolves the native CLI from PATH while excluding the shim directory, refuses to overwrite existing non-Forge files unless forced and records the resolution source/status. After installing or changing PATH, use `forge harness shim-status --shim-dir <dir> --executor <executor>` or MCP `forge.harness.shim_status` to audit existence, Forge ownership, executable bit, PATH precedence, parsed real CLI/store/Forge binary and recursion risk before relying on the shim. Then run `forge sync executors --shim-dir <dir>` or `forge sync all --shim-dir <dir>` so `forge executors`, `forge brains`, `/brains`, `/shells` and `forge shells` expose `forge.executor_harness_status.v1`, `forge_first_ready` and Forge-first shell entrypoints. Use `--real-cmd` only when an explicit native CLI path is required. Use `forge harness exec --project-root <project-root>` / MCP `forge.harness.exec` with `project_root` for guarded CLI invocation receipts; with `require_lineage_for_exec`, missing workflow/task/run returns `harness_exec_blocked_by_project_policy` instead of launching the child process. When token headroom is enabled, real child stdout/stderr get `stdout_headroom`/`stderr_headroom` reports with reversible retrieval refs, so compressed output can flow to a brain while Forge preserves the original stream. When a store plus workflow, task or run lineage is present, the receipt records a `forge.harness.exec_event.v1` global timeline event and returns `event_recorded` plus `global_event_id`.
- For human+AI assisted operation, use `forge ops snapshot --project-root <project-root> --output json` or MCP `forge.ops.snapshot` for an operational registry view, or `forge ops serve --project-root <project-root> --host 127.0.0.1 --port 8765` to open the local web console. Include `--project-root`/`project_root` whenever project `.forge/memory-governance.json` should be visible to operators and modifier AIs. The snapshot includes `forge.ops.operational_digital_twin.v1`, showing each workflow's current activity, completed work, remaining work, validated work, rejected work, pending approvals and direct inspect/task-board/validate/events commands. The console is local-only by default and lets operators observe workflows, drive runs, step deterministic work, complete tasks with evidence and update workflow goals or task nodes in real time. Its modifier lane lets a separate strategic AI or human propose goal/node mutations and apply them through Forge-owned events while execution continues. Its memory/context governance panel exposes project governance, workflow tenant/personality context and ready-to-run governed `forge context` / `forge memory search` commands. Its visual surface shows tasks/subtasks and lets operators create whiteboards, screens, wireframes, flows, components, documents, token collections, token patches and collaboration events through Forge-owned workflow revisions. Addon renderer interactions are validated against `allowed_client_events` and can be recorded through `/api/addon-renderer/event`, `forge ops renderer-event` or MCP `forge.ops.addon_renderer_event`, then projected back into snapshot runtime state.
- Treat `outcome_status` from `forge request status`, `forge request drive` and `forge list` as the final-result gate. If it says `support_only`, update the goal or tasks with explicit user-facing deliverables. If it says `needs_user_delivery_evidence` or `needs_final_outcome_audit`, continue the workflow instead of claiming completion.
- If `forge improve candidates` reports `missing_final_outcome_audit` for a workflow without a driveable run, use `forge request ensure-final-audit --workflow <workflow-id> --executor codex --origin codex --output json` or MCP `forge.workflow.ensure_final_audit` to create or surface the final audit task before packaging.
- For async handoff, call `forge mcp call forge.run.start --input '{"goal":"<objective>","origin":"codex"}' --output json`, return `result.run_id` quickly, and let Forge remain the source of truth.
- While an executor is alive, refresh observability with `forge request heartbeat --run <run-id> --executor codex --summary "<short progress>" --ttl-seconds 300 --pid <executor-pid> --origin codex --output json` or `forge.run.heartbeat`; this keeps `forge request status`, `forge request list` and `forge inspect` honest about active self-runs, including long runs whose heartbeat TTL expires while the recorded process is still alive.
- Before polling passively or starting another task handoff, call `forge request drive --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json` or `forge.run.drive`; it refreshes the heartbeat and returns `rework_required`, `ready_for_handoff`, `complete` or `blocked` with the next safe command.
- When `forge request drive` returns a ready deterministic task, prefer `forge request step --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json` or `forge.run.step` before manual handoff; it auto-promotes one command/wait/notification task through the normal executor-response validation path and refuses AI or external-command tasks instead of faking work.
- When an AI or mixed executor has actually done the ready handoff work, close it with `forge request complete-task --run <run-id> --task <task-id> --executor codex --summary "<result>" --origin codex --output json` or `forge.run.complete_task`; Forge writes a replayable execution trace, builds the executor response, validates it, promotes the task and immediately drives the next action.
- When `forge request drive` returns `complete`, inspect its `final_delivery_package`; Forge attaches Markdown and JSON summaries automatically at completion. Before handing an older or in-progress run back to the user, create or refresh the same package with `forge request final-package --run <run-id> --origin codex --output json` or `forge.run.final_package`; it reports `ready_for_user`, `in_progress` or `not_ready_for_user` so a support artifact is not mistaken for the requested final result.
- If the current executor is about to hit a model limit, becomes unavailable, or should hand off work, use `forge request switch-executor --run <run-id> --executor opencode --fallback-executor codex --summary "<takeover summary>" --origin codex --output json` or `forge.run.switch_executor`. This hot-swap changes the execution brain for the workflow run while preserving the same `run_id`, workflow id, checkpoints, artifacts and explicit user directives; it does not require shutting the workflow down. Use fallback executors to keep a run recoverable when the primary executor fails or loses model capacity.
- To change one AI/mixed node's brain routing while the workflow remains active, use `forge workflow update-node-brain --workflow <workflow-id> --task <task-id> --default-brain gemini --agent-slot agent-001=gemini:primary_node_agent:node-default --max-parallel-agents 1 --origin codex --output json` or MCP `forge.workflow.update_node_brain`. Use repeated `--agent-slot` values for multiple agents, including multiple agents on the same brain.
- If a heartbeat becomes stale, use `forge request recover-stale --run <run-id> --origin codex --output json` or `forge.run.recover_stale` to move the run to `needs_attention` without losing workflow/run lineage.
- Poll later with `forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json`.
- List active requests with `forge mcp call forge.request.list --input '{"status":"accepted"}' --output json`.
- Cancel a request with `forge mcp call forge.request.cancel --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Resume a paused async handoff with `forge mcp call forge.run.resume --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Create scheduled Goal research through `forge.schedule.create_daily_goal_research`; inspect/list/summarize/mutate schedules through `forge.schedule.list`, `forge.schedule.summary`, `forge.schedule.loop_summary`, `forge.schedule.worker_status`, `forge.workflow.inspect`, `forge.loop.inspect` and `forge.schedule.update`.
- Use `forge.schedule.update` or `forge schedule update --next-run-at <RFC3339>` for explicit due timestamp mutation, `forge.schedule.run_due` for one workflow, and `forge.schedule.scan_due` when Forge should scan all scheduled workflows, lease due nodes locally and record idle scale-to-zero decisions. Paused/stopped loop nodes must not advance.
- Use `forge schedule worker-status` or `forge.schedule.worker_status` to inspect next wakeup, scale-to-zero, bounded worker-pool capacity, cancellation safe points and backpressure before relying on tmux/systemd sleeps.
- Use `forge.credential_vault.describe` and `forge.credential_vault.records` to inspect local credential-vault contracts without resolving secrets. Use `forge credential-vault exec --contract <contract> --data <data> --record <record> -- <command>` when an executor needs credentials injected into a child process.
- Use `forge aws check`, `forge aws inventory` and MCP tools `forge.aws.check`, `forge.aws.inventory`, `forge.aws.raw` when a workflow needs an AWS API account configured through credential-vault. These commands delegate to `~/plugins/aws-ops/scripts/aws-ops`, use the AWS credential-vault defaults and keep mutation gating in the aws-ops wrapper.
- Inspect or route work through `forge.workflow.inspect`, `forge.context.request`, `forge.task.handoff`, `forge.patch.plan`, `forge.patch.diff`, `forge.patch.review`, `forge.patch.apply`, `forge.patch.revert`, `forge.patch.restore`, `forge.workflow.attach_artifact`, `forge.workflow.update_goal`, `forge.validation.status` and `forge.artifact.fetch`.
- In the interactive `forge` REPL, use `/context --workflow <id> --task <task-id> --budget 1200 --strict` for bounded context inspection and `/handoff --workflow <id> --task <task-id> --executor codex` only after approving lease acquisition.
- Treat source-code editing as a software-development Addon capability, not as a universal Core claim. Before presenting file-editing or patch-workbench UI, inspect `forge addons resolve --goal "<editing goal>" --output json` or `forge addons catalog --output json` and verify `forge.addon.software_development`, capability `source_code_patch_lifecycle`, permission `source_code.patch`, view `software.patch_workbench` and runtime contract `source_code_patch_lifecycle.executor`. The current contract uses `forge_core_builtin`/`forge.patch.lifecycle` only as a compatibility executor while the Addon boundary matures.
- Use `forge patch plan` or MCP tool `forge.patch.plan` before agent file editing to create a bounded patch plan with repo-relative target paths, file snapshots, permission gates, diff-review commands, validation commands and a Forge artifact; this command does not apply changes.
- Use `forge interactive patch-workbench` or MCP `forge.interactive.patch_workbench` to inspect the current repo's bounded inline `diff_preview`, `diff_review_queue`, per-file `action_hint`, `edit_intake` and ordered `operation_plan` before presenting a TUI/web review panel; each file `action_hint` tells whether the UI should offer review, plan creation or diff inspection and why apply remains blocked, `edit_intake` declares required inputs, missing fields, form readiness and safe command templates for `patch plan/review/diff/apply/revert/restore`, while `operation_plan` sequences the lifecycle steps, dependencies and human-approval gates. Then use `forge patch diff` or MCP tool `forge.patch.diff` to build a read-only multi-file diff navigation model with selectable file/hunk indexes before asking for human approval.
- Use `forge patch review` or MCP tool `forge.patch.review` after file edits and before apply approval to persist current diff/status/check evidence without modifying files.
- Use `forge patch apply` or MCP tool `forge.patch.apply` after a bounded executor edits files to record current file snapshots, validation output and a rollback artifact under workflow lineage.
- Use `forge patch revert` or MCP tool `forge.patch.revert` to record a guarded rollback proposal. It does not run `git checkout` or restore files automatically; human approval must precede destructive restore execution.
- Use `forge patch restore --confirm-restore --approved-by <operator>` or MCP `forge.patch.restore` only after a human-approved revert artifact exists; this is the explicit execution path that restores repo-local files and records `forge.patch_restore.v1` evidence.
- Inspect Forge 0.5 release readiness through `forge.milestone.status`, the full release-gate manifest through `forge.milestone.manifest`, the export/demo baseline through `forge.milestone.export_demo`, and replacement-grade CLI demo evidence through `forge.milestone.cli_demo`; the CLI demo includes `forge.milestone.patch_lifecycle_demo.v1` with patch plan/review/diff/apply/revert/restore artifact lineage in an isolated fixture repo, `forge.milestone.executor_project_demo.v1` for governed harness execution in an isolated project, `forge.milestone.brain_handoff_demo.v1` for Forge-owned context, node-brain routing, task handoff, plan-only shell launch and audit-only session lifecycle without child CLI/model execution, `forge.milestone.real_project_workflow_demo.v1` for multi-file code plus research artifacts through project-root handoff, brain routing, harness execution, validation and stdout headroom in an isolated project, and `forge.milestone.connected_external_brain_demo.v1` plus `forge.milestone.connected_external_brain_provider.v1` for a connected external-brain adapter process with provider-output schema validation, command/stdout hashes, handoff, harness lineage, event recording and validation while still declaring no live model/provider execution unless a project explicitly provides `.forge/connected-brain-runtimes.json` and the caller selects it with `forge milestone cli-demo --project-root <project-root> --connected-brain <provider-id>`. `groundwork`, `planned` and `blocked` capabilities prevent promotion.
- When using `forge.milestone.cli_demo`, inspect top-level `headroom_stats` for aggregated replacement-CLI token savings and reversible retrieval commands across the executor, real-project and connected-brain harness flows.
- Treat multimodal execution as the first-party `forge.addon.multimodal` capability `multimodal_runtime`, not as a universal Core claim. Before presenting multimodal benchmark UI or workflow steps, inspect `forge addons resolve --goal "<multimodal goal>" --output json` or `forge addons catalog --output json` and verify permission `multimodal.runtime_benchmark`, view `multimodal.benchmark_center` and runtime contract `multimodal_runtime_benchmark.executor`; queue it with `forge addons dispatch-contract` or MCP `forge.addons.dispatch_contract`, then execute the guarded Addon path with `forge addons run-dispatch` or MCP `forge.addons.run_dispatch`. The current contract uses `forge_core_builtin`/`forge.multimodal.runtime_benchmark` only as guarded compatibility while production Addon runtime workers mature.
- Inspect the experimental multimodal track through `forge.multimodal.status`; use approved `.forge/multimodal.json` plus CLI `--project-root` or MCP `project_root` to make feature-flag state explicit without relying on process cwd; generate plan-only model/runtime install manifests through `forge.multimodal.install_plan`; inspect runtime/model readiness through `forge multimodal readiness` or MCP `forge.multimodal.readiness` without installs, model execution, device access, network access or automation; generate benchmark/report templates through `forge.multimodal.benchmark_template`; record approval-gated fixture-only benchmark results through `forge.multimodal.benchmark_result` without installs, model execution, device access or network access; run guarded deterministic local runtime benchmark evidence through `forge multimodal runtime-benchmark` or MCP `forge.multimodal.runtime_benchmark` only after experimental opt-in, `--approved-by`, `--confirm-runtime-execution` and `--allow-model`; when a project declares approved connected runtimes in `.forge/multimodal-runtimes.json`, select one explicitly with `--connected-runtime <runtime-id>` or MCP `connected_runtime` so Forge loads the project manifest, runs only the declared probe command, records hashes/measurements and still performs no installs, network access or device access by default; connected runtimes may include a `production` evidence block with approval, model manifest hash, evidence artifacts and quality/latency thresholds, and Forge marks `promotion_ready=true` only when the probe measurements satisfy that contract; generate guarded local image/audio/Blender demo plans through `forge.multimodal.demo_plan`; record guarded local fixture demo receipts after opt-in through `forge.multimodal.demo_receipt` while proving camera, microphone, screen, input and filesystem access stay blocked without guard approval; evaluate camera, microphone, screen, input and peripheral access through `forge.multimodal.guard` before any device or automation action.
- MCP mutations must still go through Forge so revisions, artifact hashes, origins and validation gates are persisted.

## Safety Rules

- Never mark an execution step complete without validation evidence.
- Never treat task output as enough by itself. The task goal must be definitively ready.
- Do not use detected CLIs until `forge sync executors` has persisted human authorization for them.
- Treat Docker/Kubernetes/Knative as run substrates. Do not install or mutate them without explicit authorization.
- Only mutate Forge-owned runtime resources by default. External resources require a positive `forge runtime guard` decision with explicit authorization.
- Runtime goal/artifact changes must go through Forge so revisions and origins are persisted.
- When Codex/OpenCode use Forge as a skill, they should not wait for long work inline. They should start a request, return `run_id`, and let Forge continue asynchronously.
- Do not expose full project history to a task when `forge context` can produce bounded local context.
- Do not expose private or internal memory to public audiences. Customer suggestions may be shared with a manager only after classification as `manager_shared`; public/global memory writes require explicit approval.
- Treat model providers as interchangeable execution resources and keep non-AI steps independent from live model calls.
- Do not resolve or print credential-vault secret values. Prefer `forge credential-vault exec` so secrets only enter the child process environment.
- A notification step can generate an email payload with final workflow costs when that was part of the user's objective.
- Keep self-improvement controlled: experiment, benchmark, compare, then promote only after validation.

## Useful Commands

```bash
forge plan --goal "Create a delivery platform" --output json
forge request start --goal "Improve Forge Core" --origin codex --output json
forge request heartbeat --run <run-id> --executor codex --summary "executor applying bounded patch" --ttl-seconds 300 --pid <executor-pid> --origin codex --output json
forge request drive --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json
forge request step --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json
forge request complete-task --run <run-id> --task <task-id> --executor codex --summary "executor finished the ready task with passing evidence" --origin codex --output json
forge request final-package --run <run-id> --origin codex --output json
forge request ensure-final-audit --workflow <workflow-id> --executor codex --origin codex --output json
forge request switch-executor --run <run-id> --executor opencode --fallback-executor codex --summary "codex limit approaching; opencode continuing from Forge state" --origin codex --output json
forge workflow update-node-brain --workflow <workflow-id> --task task-001 --default-brain gemini --agent-slot agent-001=gemini:primary_node_agent:node-default --max-parallel-agents 1 --origin codex --output json
forge request status --run <run-id> --output json
forge request resume --run <run-id> --origin codex --output json
forge request list --status stale --output json
forge request recover-stale --run <run-id> --origin codex --output json
forge ops snapshot --project-root <project-root> --output json
forge ops serve --project-root <project-root> --host 127.0.0.1 --port 8765
forge mcp call forge.ops.snapshot --input '{"project_root":"<project-root>"}' --output json
forge ops renderer-event --workflow <workflow-id> --addon <addon-id> --view <view-id> --event-kind hover_changed --payload '{"point":"series.current"}' --output json
forge improve candidates --output json
forge events improvement-policy --workflow <workflow-id> --output json
forge improve apply-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --apply --approved-by <operator> --output json
forge improve apply-event-policy --workflow <workflow-id> --recommendation <recommendation-id> --apply --approved-by <operator> --output json
forge improve benchmark-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --output json
forge improve promote-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --approved-by <operator> --output json
forge cost incremental --project-root . --after-sequence <global-event-id> --output json
forge cost maintain --project-root . --workflow <workflow-id> --bucket day --group-by source_kind --retention-days 31 --output json
forge cost daemon --project-root . --workflow <workflow-id> --bucket day --group-by workflow --max-cycles 2 --interval-seconds 300 --retention-days 31 --output json
forge cost retention --project-root . --organization <organization-id> --retention-days 31 --apply --approved-by <operator> --reason "Validated retention window." --confirm --output json
forge mcp tools --output json
forge mcp call forge.improve.candidates --input '{"limit":10}' --output json
forge mcp call forge.improve.benchmark_event_policy --input '{"workflow_id":"<workflow-id>","recommended_policy":"prefer_deterministic_node"}' --output json
forge mcp call forge.improve.promote_event_policy --input '{"workflow_id":"<workflow-id>","recommended_policy":"prefer_deterministic_node","approved_by":"<operator>"}' --output json
forge mcp call forge.ops.addon_renderer_event --input '{"workflow_id":"<workflow-id>","addon_id":"<addon-id>","view_id":"<view-id>","event_kind":"refresh_requested","payload":{"refresh":true}}' --output json
forge mcp call forge.cost.incremental --input '{"project_root":".","after_sequence":0}' --output json
forge mcp call forge.cost.daemon --input '{"project_root":".","workflow_id":"<workflow-id>","max_cycles":1,"interval_seconds":0}' --output json
forge mcp call forge.cost.retention --input '{"project_root":".","organization_id":"<organization-id>","retention_days":31,"apply":true,"approved_by":"<operator>","reason":"Validated retention window.","confirm":true}' --output json
forge mcp call forge.interactive.home --input '{"project_root":"<project-root>"}' --output json
forge interactive command-palette --output json
forge mcp call forge.interactive.command_palette --input '{"query":"patch"}' --output json
forge interactive action-registry --output json
forge mcp call forge.interactive.action_registry --input '{"query":"patch"}' --output json
forge interactive action-invocation --action patch.diff --output json
forge mcp call forge.interactive.action_invocation --input '{"action_id":"patch.diff"}' --output json
forge interactive autocomplete --input "/patch r" --output json
forge mcp call forge.interactive.autocomplete --input '{"input":"/ha"}' --output json
forge interactive operational-cockpit --output json
forge mcp call forge.interactive.operational_cockpit --output json
forge interactive readiness --output json
forge mcp call forge.interactive.readiness --output json
forge interactive release-gates --version 0.5 --project-root . --output json
forge mcp call forge.interactive.release_gates --input '{"version":"0.5","project_root":"."}' --output json
forge interactive harness --output json
forge mcp call forge.interactive.harness --output json
forge interactive sessions --output json
forge mcp call forge.interactive.sessions --output json
forge interactive patch-workbench --output json
forge mcp call forge.interactive.patch_workbench --output json
forge interactive permissions --output json
forge mcp call forge.interactive.permissions --output json
forge interactive identity --output json
forge mcp call forge.interactive.identity --output json
forge interactive task-board --output json
forge mcp call forge.interactive.task_board --output json
forge interactive workflow-dag --output json
forge mcp call forge.interactive.workflow_dag --output json
forge interactive structured-logs --output json
forge mcp call forge.interactive.structured_logs --output json
forge brains --output json
forge mcp call forge.brain_router --output json
forge sessions --output json
forge sessions --provider codex --state opened --output json
forge sessions history --session codex-shell --output json
forge mcp call forge.sessions --output json
forge mcp call forge.sessions --input '{"provider_id":"codex","lifecycle_state":"opened"}' --output json
forge mcp call forge.session.history --input '{"session_id":"codex-shell"}' --output json
forge sessions lifecycle --session codex-shell --state opened --workflow <workflow-id> --task <task-id> --run <run-id> --origin forge_cli --output json
forge sessions lifecycle --session codex-shell --state attached --workflow <workflow-id> --task <task-id> --run <run-id> --origin forge_cli --output json
forge mcp call forge.session.lifecycle --input '{"session_id":"codex-shell","state":"closed","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","origin":"mcp"}' --output json
/harness doctor --executor codex --shim-dir <dir> --project-root <project-root>
forge harness doctor --executor codex --shim-dir <dir> --project-root <project-root> --output json
forge mcp call forge.harness.doctor --input '{"executor":"codex","shim_dir":"<dir>","project_root":"<project-root>"}' --output json
forge harness mode --project-root <project-root> --output json
forge harness headroom-plan --executor codex --project-root <project-root> --output json
forge harness headroom-stats --source <source> --output json
forge mcp call forge.harness.headroom_plan --input '{"executor":"codex","project_root":"<project-root>"}' --output json
forge mcp call forge.harness.headroom_stats --input '{"source":"<source>","limit":5}' --output json
forge harness adoption-plan --executor codex --shim-dir <dir> --project-root <project-root> --output json
forge mcp call forge.harness.adoption_plan --input '{"executor":"codex","shim_dir":"<dir>","project_root":"<project-root>"}' --output json
forge harness bootstrap --executor codex --shim-dir <dir> --project-root <project-root> --output json
forge harness bootstrap --executor codex --shim-dir <dir> --project-root <project-root> --apply --approved-by <operator> --output json
forge mcp call forge.harness.bootstrap --input '{"executor":"codex","shim_dir":"<dir>","project_root":"<project-root>"}' --output json
forge harness wrap-plan --executor codex --cmd codex --project-root <project-root> --output json
forge harness install-shims --shim-dir <dir> --executor codex --project-root <project-root> --output json
forge harness exec --executor codex --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --execute --allow-exec -- <cmd>
forge shells --executor codex --workflow <workflow-id> --task <task-id> --run <run-id> --context-budget 1200 --ttl-seconds 900 --output json
forge mcp call forge.shell.launch_plan --input '{"executor":"codex","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":1200,"ttl_seconds":900}' --output json
forge shells --executor codex --workflow <workflow-id> --task <task-id> --run <run-id> --record-session --origin forge_cli --output json
forge mcp call forge.shell.record_plan --input '{"executor":"codex","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":1200,"ttl_seconds":900,"origin":"mcp"}' --output json
forge identity membership-update --subject <user-id> --organization <org-id> --brand <brand-id> --product <product-id> --grant workflow:mutate --source codex --output json
forge mcp call forge.identity.membership_update --input '{"subject_id":"<user-id>","organization_id":"<org-id>","brand_id":"<brand-id>","product_id":"<product-id>","grant_permissions":["workflow:mutate"],"source":"codex"}' --output json
forge memory configure --project-root <project-root> --memory-level MEMORY_SHORT_TERM --default-scope project --default-scope processing --default-audience manager --privacy-mode private_by_default --retention-mode processing_auto_archive --approved-by arthur --reason "Project memory defaults" --output json
forge memory policy --project-root <project-root> --output json
forge memory search --workflow <workflow-id> --query "customer suggestion operations" --scope project --scope processing --audience manager --memory-level short_term --output json
forge memory search --workflow <workflow-id> --query "operating decisions" --scope organization --organization digital-directive --audience internal --memory-level standard --output json
forge memory promote --workflow <workflow-id> --from-scope processing --to-scope organization --source-path ./run-memory.md --summary "Curated product signal without private data." --approved-by arthur --reason "Useful operating memory for the organization." --organization digital-directive --output json
forge memory promotions --workflow <workflow-id> --to-scope organization --approved-by arthur --output json
forge memory retention --workflow <workflow-id> --scope processing --scope project --output json
forge memory cleanup --workflow <workflow-id> --scope processing --dry-run --output json
forge memory cleanup --workflow <workflow-id> --scope processing --mode archive --approved-by arthur --reason "Final packaging complete." --confirm --output json
forge mcp call forge.memory.configure --input '{"project_root":"<project-root>","memory_level":"MEMORY_SHORT_TERM","default_scopes":["project","processing"],"default_audience":"manager","privacy_mode":"private_by_default","retention_mode":"processing_auto_archive","approved_by":"arthur","reason":"Project memory defaults"}' --output json
forge mcp call forge.memory.policy --input '{"project_root":"<project-root>"}' --output json
forge mcp call forge.memory.search --input '{"workflow_id":"<workflow-id>","query":"operating decisions","scopes":["organization"],"organization_id":"digital-directive","audience":"internal","limit":5}' --output json
forge mcp call forge.memory.promote --input '{"workflow_id":"<workflow-id>","from_scope":"processing","to_scope":"project","source_path":"./run-memory.md","summary":"Curated project memory without private data.","approved_by":"arthur","reason":"Useful for this project","visibility":"internal","shareability":"project_shared","dry_run":true}' --output json
forge mcp call forge.memory.promotions --input '{"workflow_id":"<workflow-id>","to_scope":"organization","approved_by":"arthur"}' --output json
forge mcp call forge.memory.retention --input '{"workflow_id":"<workflow-id>","scopes":["processing","project"]}' --output json
forge mcp call forge.memory.cleanup --input '{"workflow_id":"<workflow-id>","scopes":["processing"],"dry_run":true}' --output json
forge mcp call forge.interactive.slash_commands --output json
forge mcp call forge.interactive.route --input '{"input":"What is the current Forge status?","origin":"codex"}' --output json
forge mcp call forge.run.start --input '{"goal":"Improve Forge Core","origin":"codex"}' --output json
forge mcp call forge.run.heartbeat --input '{"run_id":"<run-id>","executor":"codex","summary":"executor alive","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.drive --input '{"run_id":"<run-id>","executor":"codex","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.step --input '{"run_id":"<run-id>","executor":"codex","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.complete_task --input '{"run_id":"<run-id>","task_id":"<task-id>","executor":"codex","summary":"executor finished the ready task with passing evidence","origin":"codex"}' --output json
forge mcp call forge.run.final_package --input '{"run_id":"<run-id>","origin":"codex"}' --output json
forge mcp call forge.run.switch_executor --input '{"run_id":"<run-id>","executor":"opencode","fallback_executors":["codex"],"summary":"take over without stopping workflow","origin":"codex"}' --output json
forge mcp call forge.workflow.update_node_brain --input '{"workflow_id":"<workflow-id>","task_id":"task-001","default_brain":"gemini","agent_slots":["agent-001=gemini:primary_node_agent:node-default"],"max_parallel_agents":1,"origin":"codex"}' --output json
forge mcp call forge.run.recover_stale --input '{"run_id":"<run-id>","origin":"codex"}' --output json
forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json
forge request list --output json
forge request list --status accepted --output json
forge request list --status needs_attention --output json
forge request cancel --run <run-id> --origin codex --output json
forge credential-vault records --contract /path/to/vault.contract.yaml --data /path/to/vault.data.yaml --output json
forge credential-vault exec --contract /path/to/vault.contract.yaml --data /path/to/vault.data.yaml --record login -- command-that-needs-secrets
forge mcp call forge.credential_vault.describe --input '{"contract":"/path/to/vault.contract.yaml","data":"/path/to/vault.data.yaml"}' --output json
forge aws check --output json
forge aws inventory --regions us-east-1,sa-east-1 --output json
forge aws raw -- sts get-caller-identity
forge mcp call forge.aws.check --input '{}' --output json
forge mcp call forge.aws.inventory --input '{"regions":"us-east-1,sa-east-1"}' --output json
forge sync all --home "$HOME" --shim-dir "$HOME/.forge/bin" --allow codex --allow opencode --output json
forge executors --output json
forge runtimes --output json
forge workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
forge workflow attach-artifact --workflow <workflow-id> --path ./artifact.md --kind report --origin opencode --output json
forge mcp call forge.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"./artifact.md","kind":"report","origin":"codex"}' --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"task-001","budget":1200,"project_root":"<project-root>"}' --output json
forge mcp call forge.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","executor":"codex","budget":1200,"project_root":"<project-root>"}' --output json
/context --workflow <workflow-id> --task task-001 --budget 1200 --strict
/handoff --workflow <workflow-id> --task task-001 --executor codex --budget 1200
forge patch plan --workflow <workflow-id> --task task-001 --intent "Patch selected files with human diff review" --path Cargo.toml --origin codex --output json
forge mcp call forge.patch.plan --input '{"workflow_id":"<workflow-id>","task_id":"task-001","intent":"Patch selected files with human diff review","paths":["Cargo.toml"],"origin":"codex"}' --output json
forge patch review --workflow <workflow-id> --task task-001 --path Cargo.toml --origin codex --output json
forge patch diff --workflow <workflow-id> --task task-001 --path Cargo.toml --file-index 0 --hunk-index 0 --origin codex --output json
forge mcp call forge.patch.diff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","paths":["Cargo.toml"],"file_index":0,"hunk_index":0,"origin":"codex"}' --output json
forge mcp call forge.patch.review --input '{"workflow_id":"<workflow-id>","task_id":"task-001","paths":["Cargo.toml"],"origin":"codex"}' --output json
forge patch apply --workflow <workflow-id> --task task-001 --path Cargo.toml --origin codex --output json
forge patch revert --workflow <workflow-id> --task task-001 --apply-artifact <attached-patch_apply.json> --origin codex --output json
forge patch restore --workflow <workflow-id> --task task-001 --revert-artifact <attached-patch_revert.json> --approved-by <operator> --confirm-restore --origin codex --output json
forge mcp call forge.patch.apply --input '{"workflow_id":"<workflow-id>","task_id":"task-001","paths":["Cargo.toml"],"origin":"codex"}' --output json
forge mcp call forge.patch.revert --input '{"workflow_id":"<workflow-id>","task_id":"task-001","apply_artifact":"<attached-patch_apply.json>","origin":"codex"}' --output json
forge mcp call forge.patch.restore --input '{"workflow_id":"<workflow-id>","task_id":"task-001","revert_artifact":"<attached-patch_revert.json>","approved_by":"<operator>","confirm_restore":true,"origin":"codex"}' --output json
forge schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json
forge mcp call forge.schedule.create_daily_goal_research --input '{"goals":["hackathon"],"timezone":"America/Sao_Paulo","cron":"0 8 * * *","origin":"codex"}' --output json
forge schedule summary --output json
forge schedule loop-summary --output json
forge schedule worker-status --executor forge-scheduler --max-workers 1 --ttl-seconds 300 --output json
forge mcp call forge.schedule.summary --output json
forge mcp call forge.schedule.loop_summary --output json
forge mcp call forge.schedule.worker_status --input '{"executor":"mcp-scheduler","max_workers":1,"ttl_seconds":300}' --output json
forge schedule update --workflow <workflow-id> --task task-009 --next-run-at 2026-05-26T11:00:00Z --origin codex --output json
forge mcp call forge.schedule.update --input '{"workflow_id":"<workflow-id>","task_id":"task-009","next_run_at":"2026-05-26T11:00:00Z","origin":"codex"}' --output json
forge schedule pause --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule resume --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule run-due --workflow <workflow-id> --output json
forge schedule scan-due --executor forge-scheduler --ttl-seconds 300 --output json
forge mcp call forge.schedule.scan_due --input '{"executor":"mcp-scheduler","ttl_seconds":300}' --output json
forge runtime guard --substrate knative --resource service/forge-node --namespace forge --action update --owner forge --output json
forge list --output json
forge status --workflow <workflow-id> --output json
forge context --workflow <workflow-id> --task task-001 --project-root <project-root> --budget 1200 --strict --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
forge milestone status --version 0.5 --output json
forge milestone manifest --version 0.5 --output json
forge milestone prepare-evidence-inputs --version 0.5 --capability replacement_grade_cli --project-root . --connected-brain <provider-id> --apply --approved-by arthur --output json
forge milestone prepare-evidence-inputs --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --apply --approved-by arthur --output json
forge milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root . --connected-brain <provider-id> --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind broader_project_coding_research_workflow --project-root . --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind terminal_file_editing_ux --project-root . --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --approved-by arthur --output json
forge milestone attach-evidence --version 0.5 --capability experimental_multimodal_runtime --kind production_runtime_benchmark --summary "Operator-approved runtime receipt." --artifact ./runtime-receipt.json --approved-by arthur --output json
forge milestone export-demo --origin codex --output json
forge milestone cli-demo --origin codex --output json
forge multimodal status --output json
forge multimodal status --project-root . --output json
forge multimodal install-plan --capability audio_transcription --output json
forge multimodal readiness --capability image_understanding --output json
forge multimodal benchmark-template --capability audio_transcription --output json
forge multimodal benchmark-result --capability image_understanding --fixture static_image_labels --approved-by <operator> --confirm-fixture-only --output json
forge multimodal runtime-benchmark --capability image_understanding --fixture static_image_labels --approved-by <operator> --confirm-runtime-execution --allow-model --output json
forge multimodal runtime-benchmark --capability image_understanding --fixture static_image_labels --project-root <project-root> --connected-runtime <runtime-id> --approved-by <operator> --confirm-runtime-execution --allow-model --output json
forge multimodal demo-plan --demo local_image_recognition --output json
forge multimodal demo-receipt --demo local_image_recognition --fixture static_image_labels --approved-by <operator> --confirm-local-fixture --allow-model --output json
forge multimodal guard --capability camera --action access --output json
forge mcp call forge.multimodal.status --output json
forge mcp call forge.multimodal.status --input '{"project_root":"."}' --output json
forge mcp call forge.multimodal.install_plan --input '{"capability_id":"audio_transcription"}' --output json
forge mcp call forge.multimodal.readiness --input '{"capability_id":"image_understanding"}' --output json
forge mcp call forge.multimodal.benchmark_template --input '{"capability_id":"audio_transcription"}' --output json
forge mcp call forge.multimodal.benchmark_result --input '{"capability_id":"image_understanding","fixture_id":"static_image_labels","approved_by":"<operator>","confirm_fixture_only":true}' --output json
forge mcp call forge.multimodal.runtime_benchmark --input '{"project_root":".","capability_id":"image_understanding","fixture_id":"static_image_labels","approved_by":"<operator>","confirm_runtime_execution":true,"allow_model":true}' --output json
forge mcp call forge.multimodal.runtime_benchmark --input '{"project_root":".","capability_id":"image_understanding","fixture_id":"static_image_labels","connected_runtime":"<runtime-id>","approved_by":"<operator>","confirm_runtime_execution":true,"allow_model":true}' --output json
forge mcp call forge.multimodal.demo_plan --input '{"demo_id":"local_image_recognition"}' --output json
forge mcp call forge.multimodal.demo_receipt --input '{"project_root":".","demo_id":"local_image_recognition","fixture_id":"static_image_labels","approved_by":"<operator>","confirm_local_fixture":true,"allow_model":true}' --output json
forge mcp call forge.multimodal.guard --input '{"capability":"camera","action":"access","enable_experimental":false,"allow":false}' --output json
forge mcp call forge.milestone.status --input '{"version":"0.5"}' --output json
forge mcp call forge.milestone.manifest --input '{"version":"0.5"}' --output json
forge mcp call forge.milestone.export_demo --output json
forge mcp call forge.milestone.cli_demo --output json
forge improve --workflow <workflow-id> --target-version 0.3.0 --output json
forge self run --repo /home/arthur/projects/forge-core --until 2026-05-25T10:00:00-03:00 --executor opencode --fallback-executor codex --max-cycles 1 --output json
```
"#;

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
                write_skill(&path)?;
                installed.push(path.display().to_string());
            }
            "opencode" => {
                let path = home
                    .join(".config/opencode/skills")
                    .join(SKILL_NAME)
                    .join("SKILL.md");
                write_skill(&path)?;
                installed.push(path.display().to_string());
            }
            "agents" => {
                let path = home
                    .join(".agents/skills")
                    .join(SKILL_NAME)
                    .join("SKILL.md");
                write_skill(&path)?;
                installed.push(path.display().to_string());
            }
            other => anyhow::bail!("unsupported skill target: {other}"),
        }
    }

    let shared_path = home
        .join(".agents/skills")
        .join(SKILL_NAME)
        .join("SKILL.md");
    write_skill(&shared_path)?;
    let shared_display = shared_path.display().to_string();
    if !installed.iter().any(|path| path == &shared_display) {
        installed.push(shared_display);
    }

    Ok(SkillInstallReport {
        skill: SKILL_NAME.to_string(),
        installed,
    })
}

pub fn write_repo_skill(path: impl Into<PathBuf>) -> Result<()> {
    write_skill(&path.into())
}

fn write_skill(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create skill directory {}", parent.display()))?;
    }
    fs::write(path, SKILL_MD).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
