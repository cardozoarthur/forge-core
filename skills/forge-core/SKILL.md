---
name: forge-core
description: Use Forge Core to run operational and strategic assisted AI/non-AI workflows with goal-oriented atomic DAGs, executor sync, live goal/node mutation, validation gates, cost reports, notifications, persistence, rework loops, and controlled self-improvement.
license: MIT
compatibility: codex, opencode
metadata:
  runtime: rust
  cli: forge
---

## What Forge Core Does

Forge Core is an operational, strategic and visual assisted-operations runtime, not a chatbot wrapper and not a human-flow builder. Use it when an objective needs to become a persistent execution graph that can mix AI steps, deterministic non-AI steps, scheduled waits/cron, notifications, code/subworkflow execution, live human/AI modification, visual tasks/subtasks and Forge-owned creative artifacts such as whiteboards, screens, components, wireframes, flows and design tokens.

## Required Workflow

1. Run `forge plan --goal "<human objective>" --output json`.
2. For skill-style use, prefer `forge request start --goal "<objective>" --origin codex|opencode|skill --output json` and return the `run_id` to the caller.
3. Run `forge sync all --home "$HOME" --output json` when executor or runtime availability may have changed.
4. Inspect the generated atomic tasks, task goals, subtasks, impediments, async policy and validation rules.
5. Use `forge workflow update-goal ... --origin codex|opencode|forge_cli|skill` when the human changes direction during execution.
6. Use `forge workflow attach-artifact ... --origin codex|opencode|forge_cli|skill` when new artifacts appear during execution.
7. Use `forge context --workflow <id> --task <task-id> --budget <bytes> --strict --output json` before giving an agent task-specific context.
8. Run `forge validate --workflow <id> --output json` before promotion. If `rework_tasks` is not empty, return those tasks to work.
9. Run `forge improve --workflow <id> --target-version <version> --output json` only to generate a controlled experiment and changelog. Do not auto-promote without benchmark and validation evidence.
10. Run `forge milestone status --version 0.5 --output json` and `forge milestone manifest --version 0.5 --output json` before claiming Forge 0.5 creative-runtime readiness; planned or groundwork capabilities block promotion unless their complete operator-approved required attached-evidence set is present. Use `forge milestone evidence-plan --capability <capability-id> --project-root <project-root> --output json` or MCP `forge.milestone.evidence_plan` to inspect project manifests and collection commands before running real provider/runtime evidence. Use `forge milestone collect-evidence --capability <capability-id> --kind <evidence-kind> --project-root <project-root> --approved-by <operator> --output json` or MCP `forge.milestone.collect_evidence` to run a ready provider/runtime/demo source, persist the receipt and attach it; omit `--kind` only when the capability default receipt is intended. Use `forge milestone attach-evidence --capability <capability-id> --kind <kind> --artifact <path> --approved-by <operator> --output json` or MCP `forge.milestone.attach_evidence` only to attach already-reviewed external release evidence; a single receipt stays audit-only, while the complete required receipt set makes that capability promotion-ready in the manifest.

## MCP Agent Surface

- Use `forge mcp tools --output json` to discover stable agent-facing tools before wiring a Codex/OpenCode workflow.
- Inspect the no-argument interactive dashboard through `forge.interactive.home`, discover slash commands through `forge.interactive.slash_commands`, and route conversational input through `forge.interactive.route` when an agent needs the same command/chat classification as the TUI without launching a local terminal.
- Read `dashboard.identity_panel` or call `forge interactive identity --output json` / MCP `forge.interactive.identity` for `forge.interactive.identity.v1` operating context, identity registry, channel aliases, memberships and tenant audit before rendering identity or tenant context operations.
- For human+AI assisted operation, use `forge ops snapshot --output json` for an operational registry view or `forge ops serve --host 127.0.0.1 --port 8765` to open the local web console. The console is local-only by default and lets operators observe workflows, drive runs, step deterministic work, complete tasks with evidence and update workflow goals or task nodes in real time. Its modifier lane lets a separate strategic AI or human propose goal/node mutations and apply them through Forge-owned events while execution continues. Its visual surface shows tasks/subtasks and lets operators create whiteboards, screens, wireframes, flows, components, documents, token collections, token patches and collaboration events through Forge-owned workflow revisions. Addon renderer interactions are validated against `allowed_client_events` and can be recorded through `/api/addon-renderer/event`, `forge ops renderer-event` or MCP `forge.ops.addon_renderer_event`, then projected back into snapshot runtime state.
- Treat `outcome_status` from `forge request status`, `forge request drive` and `forge list` as the final-result gate. If it says `support_only`, update the goal or tasks with explicit user-facing deliverables. If it says `needs_user_delivery_evidence` or `needs_final_outcome_audit`, continue the workflow instead of claiming completion.
- For async handoff, call `forge mcp call forge.run.start --input '{"goal":"<objective>","origin":"codex"}' --output json`, return `result.run_id` quickly, and let Forge remain the source of truth.
- While an executor is alive, refresh observability with `forge request heartbeat --run <run-id> --executor codex --summary "<short progress>" --ttl-seconds 300 --pid <executor-pid> --origin codex --output json` or `forge.run.heartbeat`; this keeps `forge request status`, `forge request list` and `forge inspect` honest about active self-runs, including long runs whose heartbeat TTL expires while the recorded process is still alive.
- Before polling passively or starting another task handoff, call `forge request drive --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json` or `forge.run.drive`; it refreshes the heartbeat and returns `rework_required`, `ready_for_handoff`, `complete` or `blocked` with the next safe command.
- When `forge request drive` returns a ready deterministic task, prefer `forge request step --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json` or `forge.run.step` before manual handoff; it auto-promotes one command/wait/notification task through the normal executor-response validation path and refuses AI or external-command tasks instead of faking work.
- When an AI or mixed executor has actually done the ready handoff work, close it with `forge request complete-task --run <run-id> --task <task-id> --executor codex --summary "<result>" --origin codex --output json` or `forge.run.complete_task`; Forge writes a replayable execution trace, builds the executor response, validates it, promotes the task and immediately drives the next action.
- When `forge request drive` returns `complete`, inspect its `final_delivery_package`; Forge attaches Markdown and JSON summaries automatically at completion. Before handing an older or in-progress run back to the user, create or refresh the same package with `forge request final-package --run <run-id> --origin codex --output json` or `forge.run.final_package`; it reports `ready_for_user`, `in_progress` or `not_ready_for_user` so a support artifact is not mistaken for the requested final result.
- If the current executor is about to hit a model limit, becomes unavailable, or should hand off work, use `forge request switch-executor --run <run-id> --executor opencode --summary "<takeover summary>" --origin codex --output json` or `forge.run.switch_executor`. This hot-swap preserves the same `run_id`, workflow id, checkpoints, artifacts and explicit user directives; it does not require shutting the workflow down.
- If a heartbeat becomes stale, use `forge request recover-stale --run <run-id> --origin codex --output json` or `forge.run.recover_stale` to move the run to `needs_attention` without losing workflow/run lineage.
- Poll later with `forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json`.
- Resume a paused async handoff with `forge mcp call forge.run.resume --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Create scheduled Goal research through `forge.schedule.create_daily_goal_research`; inspect/list/summarize/mutate schedules through `forge.schedule.list`, `forge.schedule.summary`, `forge.schedule.loop_summary`, `forge.workflow.inspect`, `forge.loop.inspect` and `forge.schedule.update`.
- Use `forge.schedule.update` or `forge schedule update --next-run-at <RFC3339>` for explicit due timestamp mutation, `forge.schedule.run_due` for one workflow, and `forge.schedule.scan_due` when Forge should scan all scheduled workflows, lease due nodes locally and record idle scale-to-zero decisions. With `max_workers > 1`, the parallel scanner still reconciles idle workflows and includes a `forge.worker_pool.v1` execution report. Paused/stopped loop nodes must not advance. If cron work is stale, read `missed_run_reconciliation` plus list/inspect schedule summaries before deciding whether a run was skipped, caught up or executed once.
- Use `forge.schedule.worker_status` or `forge schedule worker-status --max-workers <n>` before scheduler handoff when concurrency matters. Its `worker_pool.assignment_plan` is deterministic and separates due workflows that fit the bounded worker pool from queued work under backpressure.
- Use `forge.interaction.create_choice`, `forge.interaction.create_form`, `forge.interaction.answer`, `forge.interaction.expire` and `forge.interaction.list` for agent-facing human approval/form bridges. These MCP tools must be preferred over ad hoc chat decisions when a workflow is paused on a human interaction node.
- Use `forge.credential_vault.describe` and `forge.credential_vault.records` to inspect local credential-vault contracts without resolving secrets. Use `forge credential-vault exec --contract <contract> --data <data> --record <record> -- <command>` when an executor needs credentials injected into a child process.
- Use `forge aws check`, `forge aws inventory` and MCP tools `forge.aws.check`, `forge.aws.inventory`, `forge.aws.raw` when a workflow needs an AWS API account. These commands delegate to `~/plugins/aws-ops/scripts/aws-ops`, use the AWS credential-vault defaults and keep mutation gating in the aws-ops wrapper.
- Inspect or route work through `forge.workflow.inspect`, `forge.context.request`, `forge.task.handoff`, `forge.patch.plan`, `forge.patch.apply`, `forge.patch.revert`, `forge.workflow.attach_artifact`, `forge.workflow.update_goal`, `forge.validation.status` and `forge.artifact.fetch`.
- In the interactive `forge` REPL, use `/context --workflow <id> --task <task-id> --budget 1200 --strict` for bounded context inspection and `/handoff --workflow <id> --task <task-id> --executor codex` only after approving lease acquisition.
- Use `forge patch plan` or MCP tool `forge.patch.plan` before agent file editing to create a bounded patch plan with repo-relative target paths, file snapshots, permission gates, diff-review commands, validation commands and a Forge artifact; this command does not apply changes.
- Use `forge patch apply` or MCP tool `forge.patch.apply` after a bounded executor edits files to record current file snapshots, validation output and a rollback artifact under workflow lineage.
- Use `forge patch revert` or MCP tool `forge.patch.revert` to record a guarded rollback proposal. It does not run `git checkout` or restore files automatically; human approval must precede destructive restore execution.
- Inspect Forge 0.5 release readiness through `forge.milestone.status`, the full release-gate manifest through `forge.milestone.manifest`, the export/demo baseline through `forge.milestone.export_demo`, and replacement-grade CLI demo evidence through `forge.milestone.cli_demo`; `groundwork`, `planned` and `blocked` capabilities prevent promotion.
- Inspect the experimental multimodal track through `forge.multimodal.status`; generate plan-only model/runtime install manifests through `forge.multimodal.install_plan`; generate benchmark/report templates through `forge.multimodal.benchmark_template`; generate guarded local image/audio/Blender demo plans through `forge.multimodal.demo_plan`; evaluate camera, microphone, screen, input and peripheral access through `forge.multimodal.guard` before any device or automation action.
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
forge request switch-executor --run <run-id> --executor opencode --summary "codex limit approaching; opencode continuing from Forge state" --origin codex --output json
forge request status --run <run-id> --output json
forge request resume --run <run-id> --origin codex --output json
forge request list --status stale --output json
forge request recover-stale --run <run-id> --origin codex --output json
forge ops snapshot --output json
forge ops serve --host 127.0.0.1 --port 8765
forge ops renderer-event --workflow <workflow-id> --addon <addon-id> --view <view-id> --event-kind hover_changed --payload '{"point":"series.current"}' --output json
forge mcp tools --output json
forge credential-vault records --contract /path/to/vault.contract.yaml --data /path/to/vault.data.yaml --output json
forge credential-vault exec --contract /path/to/vault.contract.yaml --data /path/to/vault.data.yaml --record login -- command-that-needs-secrets
forge mcp call forge.credential_vault.describe --input '{"contract":"/path/to/vault.contract.yaml","data":"/path/to/vault.data.yaml"}' --output json
forge aws check --output json
forge aws inventory --regions us-east-1,sa-east-1 --output json
forge aws raw -- sts get-caller-identity
forge mcp call forge.aws.check --input '{}' --output json
forge mcp call forge.aws.inventory --input '{"regions":"us-east-1,sa-east-1"}' --output json
forge mcp call forge.interactive.home --output json
forge interactive identity --output json
forge mcp call forge.interactive.identity --output json
forge mcp call forge.interactive.slash_commands --output json
forge mcp call forge.interactive.route --input '{"input":"What is the current Forge status?","origin":"codex"}' --output json
forge mcp call forge.ops.addon_renderer_event --input '{"workflow_id":"<workflow-id>","addon_id":"<addon-id>","view_id":"<view-id>","event_kind":"refresh_requested","payload":{"refresh":true}}' --output json
forge mcp call forge.run.start --input '{"goal":"Improve Forge Core","origin":"codex"}' --output json
forge mcp call forge.run.heartbeat --input '{"run_id":"<run-id>","executor":"codex","summary":"executor alive","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.drive --input '{"run_id":"<run-id>","executor":"codex","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.step --input '{"run_id":"<run-id>","executor":"codex","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.complete_task --input '{"run_id":"<run-id>","task_id":"<task-id>","executor":"codex","summary":"executor finished the ready task with passing evidence","origin":"codex"}' --output json
forge mcp call forge.run.final_package --input '{"run_id":"<run-id>","origin":"codex"}' --output json
forge mcp call forge.run.switch_executor --input '{"run_id":"<run-id>","executor":"opencode","summary":"take over without stopping workflow","origin":"codex"}' --output json
forge mcp call forge.run.recover_stale --input '{"run_id":"<run-id>","origin":"codex"}' --output json
forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json
forge request list --status needs_attention --output json
forge sync all --home "$HOME" --allow codex --allow opencode --output json
forge executors --output json
forge runtimes --output json
forge workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
forge workflow attach-artifact --workflow <workflow-id> --path ./artifact.md --kind report --origin opencode --output json
forge mcp call forge.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"./artifact.md","kind":"report","origin":"codex"}' --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"task-001","budget":1200}' --output json
forge mcp call forge.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","executor":"codex","budget":1200}' --output json
/context --workflow <workflow-id> --task task-001 --budget 1200 --strict
/handoff --workflow <workflow-id> --task task-001 --executor codex --budget 1200
forge patch plan --workflow <workflow-id> --task task-001 --intent "Patch selected files with human diff review" --path Cargo.toml --origin codex --output json
forge mcp call forge.patch.plan --input '{"workflow_id":"<workflow-id>","task_id":"task-001","intent":"Patch selected files with human diff review","paths":["Cargo.toml"],"origin":"codex"}' --output json
forge patch apply --workflow <workflow-id> --task task-001 --path Cargo.toml --origin codex --output json
forge patch revert --workflow <workflow-id> --task task-001 --apply-artifact <attached-patch_apply.json> --origin codex --output json
forge mcp call forge.patch.apply --input '{"workflow_id":"<workflow-id>","task_id":"task-001","paths":["Cargo.toml"],"origin":"codex"}' --output json
forge mcp call forge.patch.revert --input '{"workflow_id":"<workflow-id>","task_id":"task-001","apply_artifact":"<attached-patch_apply.json>","origin":"codex"}' --output json
forge schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json
forge mcp call forge.schedule.create_daily_goal_research --input '{"goals":["hackathon"],"timezone":"America/Sao_Paulo","cron":"0 8 * * *","origin":"codex"}' --output json
forge schedule summary --output json
forge schedule loop-summary --output json
forge mcp call forge.schedule.summary --output json
forge mcp call forge.schedule.loop_summary --output json
forge schedule update --workflow <workflow-id> --task task-009 --next-run-at 2026-05-26T11:00:00Z --origin codex --output json
forge schedule update --workflow <workflow-id> --task task-009 --missed-run-policy skip_missed --origin codex --output json
forge mcp call forge.schedule.update --input '{"workflow_id":"<workflow-id>","task_id":"task-009","next_run_at":"2026-05-26T11:00:00Z","origin":"codex"}' --output json
forge schedule pause --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule resume --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule run-due --workflow <workflow-id> --output json
forge schedule worker-status --executor forge-scheduler --max-workers 3 --ttl-seconds 300 --output json
forge schedule scan-due --executor forge-scheduler --ttl-seconds 300 --output json
forge mcp call forge.schedule.worker_status --input '{"executor":"mcp-scheduler","max_workers":3,"ttl_seconds":300}' --output json
forge mcp call forge.schedule.scan_due --input '{"executor":"mcp-scheduler","ttl_seconds":300}' --output json
forge interaction create-choice --workflow <workflow-id> --task task-002 --kind approve_reject_refine_combine --prompt "Choose direction" --choice approve=Approve --choice refine=Refine --origin codex --output json
forge mcp call forge.interaction.create_choice --input '{"workflow_id":"<workflow-id>","task_id":"task-002","kind":"approve_reject_refine_combine","prompt":"Choose direction","choices":["approve=Approve","refine=Refine"],"origin":"codex"}' --output json
forge mcp call forge.interaction.answer --input '{"workflow_id":"<workflow-id>","task_id":"task-002","selected_options":["approve"],"origin":"codex"}' --output json
forge runtime guard --substrate knative --resource service/forge-node --namespace forge --action update --owner forge --output json
forge list --output json
forge status --workflow <workflow-id> --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --strict --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
forge milestone status --version 0.5 --output json
forge milestone manifest --version 0.5 --output json
forge milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root . --connected-brain <provider-id> --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind broader_project_coding_research_workflow --project-root . --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind terminal_file_editing_ux --project-root . --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --approved-by arthur --output json
forge milestone attach-evidence --version 0.5 --capability experimental_multimodal_runtime --kind production_runtime_benchmark --summary "Operator-approved runtime receipt." --artifact ./runtime-receipt.json --approved-by arthur --output json
forge milestone export-demo --origin codex --output json
forge milestone cli-demo --origin codex --output json
forge multimodal status --output json
forge multimodal install-plan --capability audio_transcription --output json
forge multimodal benchmark-template --capability audio_transcription --output json
forge multimodal demo-plan --demo local_image_recognition --output json
forge multimodal guard --capability camera --action access --output json
forge mcp call forge.milestone.status --input '{"version":"0.5"}' --output json
forge mcp call forge.milestone.manifest --input '{"version":"0.5"}' --output json
forge mcp call forge.milestone.export_demo --output json
forge mcp call forge.milestone.cli_demo --output json
forge mcp call forge.multimodal.status --output json
forge mcp call forge.multimodal.install_plan --input '{"capability_id":"audio_transcription"}' --output json
forge mcp call forge.multimodal.benchmark_template --input '{"capability_id":"audio_transcription"}' --output json
forge mcp call forge.multimodal.demo_plan --input '{"demo_id":"local_image_recognition"}' --output json
forge mcp call forge.multimodal.guard --input '{"capability":"camera","action":"access","enable_experimental":false,"allow":false}' --output json
forge improve --workflow <workflow-id> --target-version 0.3.0 --output json
forge self run --repo /home/arthur/projects/forge-core --until 2026-05-25T10:00:00-03:00 --executor codex --executor opencode --max-cycles 1 --output json
```
