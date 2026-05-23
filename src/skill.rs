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
7. Use `forge context --workflow <id> --task <task-id> --budget <bytes> --output json` before giving an agent task-specific context.
8. Run `forge validate --workflow <id> --output json` before promotion. If `rework_tasks` is not empty, return those tasks to work.
9. Run `forge improve --workflow <id> --target-version <version> --output json` only to generate a controlled experiment and changelog. Do not auto-promote without benchmark and validation evidence.

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
forge request status --run <run-id> --output json
forge sync all --home "$HOME" --allow codex --allow opencode --output json
forge executors --output json
forge runtimes --output json
forge workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
forge workflow attach-artifact --workflow <workflow-id> --path ./artifact.md --kind report --origin opencode --output json
forge runtime guard --substrate knative --resource service/forge-node --namespace forge --action update --owner forge --output json
forge status --workflow <workflow-id> --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
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
