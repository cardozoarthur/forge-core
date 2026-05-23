use crate::artifact::write_json_artifact;
use crate::graph::create_workflow;
use crate::intent::parse_intent;
use crate::request::{create_run_record, save_run_record};
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct SelfRunOptions {
    pub repo: PathBuf,
    pub until: String,
    pub max_cycles: u32,
    pub sleep_seconds: u64,
    pub executors: Vec<String>,
    pub dry_run: bool,
    pub push: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfRunReport {
    pub status: String,
    pub run_id: String,
    pub workflow_id: String,
    pub stop_at: String,
    pub repo: String,
    pub executors: Vec<String>,
    pub max_cycles: u32,
    pub dry_run: bool,
    pub push: bool,
    pub cycle_reports: Vec<SelfCycleReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfCycleReport {
    pub cycle: u32,
    pub executor: String,
    pub status: String,
    pub prompt_path: String,
    pub report_path: String,
    pub validation_passed: bool,
    pub committed: bool,
    pub commit: Option<String>,
}

pub fn run_self_evolution(store: &ForgeStore, options: SelfRunOptions) -> Result<SelfRunReport> {
    let stop_at = DateTime::parse_from_rfc3339(&options.until)
        .with_context(|| format!("invalid --until value: {}", options.until))?;
    if stop_at.with_timezone(&Utc) <= Utc::now() {
        bail!("stop date is in the past");
    }
    if options.max_cycles == 0 {
        bail!("max cycles must be greater than zero");
    }
    if !options.repo.exists() {
        bail!("repo does not exist: {}", options.repo.display());
    }

    let executors = if options.executors.is_empty() {
        vec!["codex".to_string(), "opencode".to_string()]
    } else {
        options.executors.clone()
    };

    let workflow = create_workflow(parse_intent(
        "Improve Forge Core autonomously with bounded executor cycles, validation gates, artifacts and changelog",
    ));
    let run = create_run_record(&workflow, "forge_cli", "planned");
    store.save_workflow(&workflow)?;
    save_run_record(store, &run)?;

    let mut cycle_reports = Vec::new();
    for cycle in 1..=options.max_cycles {
        if Utc::now() >= stop_at.with_timezone(&Utc) {
            break;
        }
        let executor = executors[((cycle - 1) as usize) % executors.len()].clone();
        let prompt = render_prompt(cycle, &executor, &workflow.id, &run.run_id, &options);
        let prompt_path = format!(
            "artifacts/{}/self-evolution-cycle-{:03}-prompt.md",
            workflow.id, cycle
        );
        let report_path = format!(
            "artifacts/{}/self-evolution-cycle-{:03}-report.json",
            workflow.id, cycle
        );
        write_text_artifact(&store.base_dir(), &prompt_path, &prompt)?;

        let mut status = "planned".to_string();
        let mut validation_passed = false;
        let mut committed = false;
        let mut commit = None;

        if !options.dry_run {
            status = execute_cycle(&options.repo, &executor, &prompt)?;
            validation_passed = run_validation(&options.repo)?;
            if validation_passed && has_changes(&options.repo)? {
                commit = commit_changes(&options.repo, cycle)?;
                committed = commit.is_some();
                if committed && options.push {
                    run_git(&options.repo, &["push"])?;
                }
            }
        }

        let cycle_report = SelfCycleReport {
            cycle,
            executor,
            status,
            prompt_path: prompt_path.clone(),
            report_path: report_path.clone(),
            validation_passed,
            committed,
            commit,
        };
        write_json_artifact(
            &store.base_dir(),
            &report_path,
            &serde_json::to_value(&cycle_report)?,
        )?;
        cycle_reports.push(cycle_report);

        if !options.dry_run
            && cycle < options.max_cycles
            && Utc::now() < stop_at.with_timezone(&Utc)
        {
            std::thread::sleep(std::time::Duration::from_secs(options.sleep_seconds));
        }
    }

    Ok(SelfRunReport {
        status: if options.dry_run {
            "planned".to_string()
        } else {
            "started".to_string()
        },
        run_id: run.run_id,
        workflow_id: workflow.id,
        stop_at: options.until,
        repo: options.repo.display().to_string(),
        executors,
        max_cycles: options.max_cycles,
        dry_run: options.dry_run,
        push: options.push,
        cycle_reports,
    })
}

fn render_prompt(
    cycle: u32,
    executor: &str,
    workflow_id: &str,
    run_id: &str,
    options: &SelfRunOptions,
) -> String {
    format!(
        r#"# Improve Forge Core

You are executing Forge self-evolution cycle {cycle}.

Run id: `{run_id}`
Workflow id: `{workflow_id}`
Executor: `{executor}`
Stop date: `{}`

Goal:
- Improve Forge Core itself in a small, validated, production-quality increment.
- Prefer structural improvements over cosmetic changes.
- Good candidates: async run records, task leases, executor adapter contracts, prompt packet versioning, runtime mutation propagation, changelog/report quality, validation gates.

Constraints:
- Use the repository at `{}`.
- Do not mutate external Docker/Kubernetes/Knative resources.
- Do not install Knative or modify user infrastructure.
- Keep changes scoped to Forge Core.
- Use tests first when adding behavior.
- Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `cargo build --release`.
- If validation fails, fix or report the blocker without pretending the cycle completed.
- Generate or update a strong changelog/report artifact when the version behavior changes.
- Codex/OpenCode should treat Forge as the source of truth: update goals/artifacts through Forge CLI if runtime state changes.

Return a concise final report with:
- files changed;
- tests run;
- validation result;
- next recommended cycle.
"#,
        options.until,
        options.repo.display()
    )
}

fn write_text_artifact(base_dir: &Path, relative_path: &str, content: &str) -> Result<()> {
    let full_path = base_dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(full_path, content)?;
    Ok(())
}

fn execute_cycle(repo: &Path, executor: &str, prompt: &str) -> Result<String> {
    match executor {
        "codex" => {
            let output = Command::new("codex")
                .args([
                    "exec",
                    "--cd",
                    repo.to_str().unwrap_or("."),
                    "--sandbox",
                    "workspace-write",
                    "--ask-for-approval",
                    "never",
                    "--output-last-message",
                    ".forge/last-codex-self-evolution.md",
                    prompt,
                ])
                .current_dir(repo)
                .output()?;
            if output.status.success() {
                Ok("executor_completed".to_string())
            } else {
                bail!(
                    "codex executor failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            }
        }
        "opencode" => {
            let output = Command::new("opencode")
                .args([
                    "run",
                    "--dir",
                    repo.to_str().unwrap_or("."),
                    "--title",
                    "Forge self evolution",
                    prompt,
                ])
                .current_dir(repo)
                .output()?;
            if output.status.success() {
                Ok("executor_completed".to_string())
            } else {
                bail!(
                    "opencode executor failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            }
        }
        other => bail!("unsupported self-evolution executor: {other}"),
    }
}

fn run_validation(repo: &Path) -> Result<bool> {
    let status = Command::new("sh")
        .arg("-lc")
        .arg("cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test && cargo build --release")
        .current_dir(repo)
        .status()?;
    Ok(status.success())
}

fn has_changes(repo: &Path) -> Result<bool> {
    let output = run_git(repo, &["status", "--short"])?;
    Ok(!output.trim().is_empty())
}

fn commit_changes(repo: &Path, cycle: u32) -> Result<Option<String>> {
    run_git(repo, &["add", "."])?;
    run_git(
        repo,
        &[
            "commit",
            "-m",
            &format!("chore: forge self evolution cycle {cycle}"),
        ],
    )?;
    let commit = run_git(repo, &["rev-parse", "--short", "HEAD"])?;
    Ok(Some(commit.trim().to_string()))
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
