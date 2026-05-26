use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const SKILL_NAME: &str = "forge-core";

pub const SKILL_MD: &str = r#"---
name: forge-core
description: Use Forge Core to run autonomous or mixed AI/non-AI workflows with goal-oriented DAGs, executor/runtime sync, mutable goals/artifacts, validation gates, persistence, rework loops, and controlled self-improvement.
license: MIT
compatibility: codex, opencode
metadata:
  runtime: rust
  cli: forge
---

## What Forge Core Does

Forge Core is an operational runtime, not a chatbot wrapper and not a human-flow builder. Use it when an objective needs to become a persistent execution graph that can mix AI steps, deterministic non-AI steps, scheduled waits/cron and notifications.

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
10. Run `forge milestone status --version 0.5 --output json` and `forge milestone manifest --version 0.5 --output json` before claiming Forge 0.5 creative-runtime readiness; planned or groundwork capabilities block promotion.

## MCP Agent Surface

- Use `forge mcp tools --output json` to discover stable agent-facing tools before wiring a Codex/OpenCode workflow.
- For async handoff, call `forge mcp call forge.run.start --input '{"goal":"<objective>","origin":"codex"}' --output json`, return `result.run_id` quickly, and let Forge remain the source of truth.
- While an executor is alive, refresh observability with `forge request heartbeat --run <run-id> --executor codex --summary "<short progress>" --ttl-seconds 300 --pid <executor-pid> --origin codex --output json` or `forge.run.heartbeat`; this keeps `forge request status`, `forge request list` and `forge inspect` honest about active self-runs, including long runs whose heartbeat TTL expires while the recorded process is still alive.
- If a heartbeat becomes stale, use `forge request recover-stale --run <run-id> --origin codex --output json` or `forge.run.recover_stale` to move the run to `needs_attention` without losing workflow/run lineage.
- Poll later with `forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json`.
- List active requests with `forge mcp call forge.request.list --input '{"status":"accepted"}' --output json`.
- Cancel a request with `forge mcp call forge.request.cancel --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Resume a paused async handoff with `forge mcp call forge.run.resume --input '{"run_id":"<run-id>","origin":"opencode"}' --output json`.
- Create scheduled Goal research through `forge.schedule.create_daily_goal_research`; inspect/list/summarize/mutate schedules through `forge.schedule.list`, `forge.schedule.summary`, `forge.schedule.loop_summary`, `forge.schedule.worker_status`, `forge.workflow.inspect`, `forge.loop.inspect` and `forge.schedule.update`.
- Use `forge.schedule.update` or `forge schedule update --next-run-at <RFC3339>` for explicit due timestamp mutation, `forge.schedule.run_due` for one workflow, and `forge.schedule.scan_due` when Forge should scan all scheduled workflows, lease due nodes locally and record idle scale-to-zero decisions. Paused/stopped loop nodes must not advance.
- Use `forge schedule worker-status` or `forge.schedule.worker_status` to inspect next wakeup, scale-to-zero, bounded worker-pool capacity, cancellation safe points and backpressure before relying on tmux/systemd sleeps.
- Inspect or route work through `forge.workflow.inspect`, `forge.context.request`, `forge.task.handoff`, `forge.patch.plan`, `forge.patch.apply`, `forge.patch.revert`, `forge.workflow.attach_artifact`, `forge.workflow.update_goal`, `forge.validation.status` and `forge.artifact.fetch`.
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
- A notification step can generate an email payload with final workflow costs when that was part of the user's objective.
- Keep self-improvement controlled: experiment, benchmark, compare, then promote only after validation.

## Useful Commands

```bash
forge plan --goal "Create a delivery platform" --output json
forge request start --goal "Improve Forge Core" --origin codex --output json
forge request heartbeat --run <run-id> --executor codex --summary "executor applying bounded patch" --ttl-seconds 300 --pid <executor-pid> --origin codex --output json
forge request status --run <run-id> --output json
forge request resume --run <run-id> --origin codex --output json
forge request list --status stale --output json
forge request recover-stale --run <run-id> --origin codex --output json
forge mcp tools --output json
forge mcp call forge.run.start --input '{"goal":"Improve Forge Core","origin":"codex"}' --output json
forge mcp call forge.run.heartbeat --input '{"run_id":"<run-id>","executor":"codex","summary":"executor alive","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.recover_stale --input '{"run_id":"<run-id>","origin":"codex"}' --output json
forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json
forge request list --output json
forge request list --status accepted --output json
forge request list --status needs_attention --output json
forge request cancel --run <run-id> --origin codex --output json
forge sync all --home "$HOME" --allow codex --allow opencode --output json
forge executors --output json
forge runtimes --output json
forge workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
forge workflow attach-artifact --workflow <workflow-id> --path ./artifact.md --kind report --origin opencode --output json
forge mcp call forge.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"./artifact.md","kind":"report","origin":"codex"}' --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"task-001","budget":1200}' --output json
forge mcp call forge.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","executor":"codex","budget":1200}' --output json
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
forge context --workflow <workflow-id> --task task-001 --budget 1200 --strict --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
forge milestone status --version 0.5 --output json
forge milestone manifest --version 0.5 --output json
forge milestone export-demo --origin codex --output json
forge milestone cli-demo --origin codex --output json
forge multimodal status --output json
forge multimodal install-plan --capability audio_transcription --output json
forge multimodal benchmark-template --capability audio_transcription --output json
forge multimodal demo-plan --demo local_image_recognition --output json
forge multimodal guard --capability camera --action access --output json
forge mcp call forge.multimodal.status --output json
forge mcp call forge.multimodal.install_plan --input '{"capability_id":"audio_transcription"}' --output json
forge mcp call forge.multimodal.benchmark_template --input '{"capability_id":"audio_transcription"}' --output json
forge mcp call forge.multimodal.demo_plan --input '{"demo_id":"local_image_recognition"}' --output json
forge mcp call forge.multimodal.guard --input '{"capability":"camera","action":"access","enable_experimental":false,"allow":false}' --output json
forge mcp call forge.milestone.status --input '{"version":"0.5"}' --output json
forge mcp call forge.milestone.manifest --input '{"version":"0.5"}' --output json
forge mcp call forge.milestone.export_demo --output json
forge mcp call forge.milestone.cli_demo --output json
forge improve --workflow <workflow-id> --target-version 0.3.0 --output json
forge self run --repo /home/arthur/projects/forge-core --until 2026-05-25T10:00:00-03:00 --executor codex --executor opencode --max-cycles 1 --output json
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
