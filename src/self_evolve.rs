use crate::artifact::{hex_sha256, write_json_artifact};
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
use std::time::Instant;

const SELF_EVOLUTION_PROMPT_PACKET_VERSION: &str = "forge.self_evolution.prompt.v1";
const SELF_EVOLUTION_VALIDATION_REPORT_VERSION: &str = "forge.self_evolution.validation.v1";
const GH_AUTH_TIMEOUT_SECONDS: &str = "20";
const GIT_PUSH_TIMEOUT_SECONDS: &str = "300";
const VALIDATION_COMMANDS: [&str; 4] = [
    "cargo fmt --check",
    "cargo clippy --all-targets --all-features -- -D warnings",
    "cargo test",
    "cargo build --release",
];

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
    pub prompt_packet_version: String,
    pub prompt_sha256: String,
    pub validation_report_path: String,
    pub validation_report_sha256: String,
    pub report_path: String,
    pub validation_passed: bool,
    pub self_update: SelfUpdateReport,
    pub committed: bool,
    pub commit: Option<String>,
    pub public_project_update: PublicProjectUpdateReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfUpdateReport {
    pub status: String,
    pub command: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicProjectUpdateReport {
    pub status: String,
    pub uses_gh: bool,
    pub gh_auth_command: Vec<String>,
    pub repo_view_command: Vec<String>,
    pub push_command: Vec<String>,
    pub url: Option<String>,
    pub visibility: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SelfEvolutionPromptPacket {
    version: String,
    cycle: u32,
    executor: String,
    workflow_id: String,
    run_id: String,
    stop_at: String,
    repo: String,
    validation_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SelfValidationEvidenceReport {
    schema_version: String,
    prompt_packet_version: String,
    workflow_id: String,
    run_id: String,
    cycle: u32,
    executor: String,
    repo: String,
    status: String,
    validation_passed: bool,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    commands: Vec<SelfValidationCommandEvidence>,
}

#[derive(Debug, Clone, Serialize)]
struct SelfValidationCommandEvidence {
    command: String,
    status: String,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    stdout: String,
    stderr: String,
    reason: Option<String>,
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
        let prompt_packet =
            SelfEvolutionPromptPacket::new(cycle, &executor, &workflow.id, &run.run_id, &options);
        let prompt = render_prompt(&prompt_packet);
        let prompt_sha256 = hex_sha256(prompt.as_bytes());
        let prompt_path = format!(
            "artifacts/{}/self-evolution-cycle-{:03}-prompt.md",
            workflow.id, cycle
        );
        let report_path = format!(
            "artifacts/{}/self-evolution-cycle-{:03}-report.json",
            workflow.id, cycle
        );
        let validation_report_path = format!(
            "artifacts/{}/self-evolution-cycle-{:03}-validation.json",
            workflow.id, cycle
        );
        write_text_artifact(&store.base_dir(), &prompt_path, &prompt)?;

        let mut status = "planned".to_string();
        let mut validation_report = SelfValidationEvidenceReport::planned(&prompt_packet);
        let mut self_update = SelfUpdateReport::planned();
        let mut committed = false;
        let mut commit = None;
        let mut public_project_update = PublicProjectUpdateReport::planned(options.push);

        if !options.dry_run {
            status = execute_cycle(&options.repo, &executor, &prompt)?;
            validation_report = run_validation(&options.repo, &prompt_packet)?;
            if !validation_report.validation_passed {
                emit_validation_failure_logs(&validation_report);
            }
            let validation_passed = validation_report.validation_passed;
            if validation_passed {
                self_update = run_self_update(&options.repo)?;
                if has_changes(&options.repo)? {
                    commit = commit_changes(&options.repo, cycle)?;
                    committed = commit.is_some();
                    if committed && options.push {
                        public_project_update = publish_public_project_with_gh(&options.repo)?;
                    } else if !options.push {
                        public_project_update = PublicProjectUpdateReport::skipped(
                            options.push,
                            "push flag not requested",
                        );
                    }
                } else {
                    public_project_update =
                        PublicProjectUpdateReport::skipped(options.push, "no changes to publish");
                }
            } else {
                self_update = SelfUpdateReport::skipped("validation failed");
                public_project_update =
                    PublicProjectUpdateReport::skipped(options.push, "validation failed");
            }
        }
        let (_validation_full_path, validation_report_sha256) = write_json_artifact(
            &store.base_dir(),
            &validation_report_path,
            &serde_json::to_value(&validation_report)?,
        )?;

        let cycle_report = SelfCycleReport {
            cycle,
            executor,
            status,
            prompt_path: prompt_path.clone(),
            prompt_packet_version: prompt_packet.version,
            prompt_sha256,
            validation_report_path: validation_report_path.clone(),
            validation_report_sha256,
            report_path: report_path.clone(),
            validation_passed: validation_report.validation_passed,
            self_update,
            committed,
            commit,
            public_project_update,
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

impl SelfEvolutionPromptPacket {
    fn new(
        cycle: u32,
        executor: &str,
        workflow_id: &str,
        run_id: &str,
        options: &SelfRunOptions,
    ) -> Self {
        Self {
            version: SELF_EVOLUTION_PROMPT_PACKET_VERSION.to_string(),
            cycle,
            executor: executor.to_string(),
            workflow_id: workflow_id.to_string(),
            run_id: run_id.to_string(),
            stop_at: options.until.clone(),
            repo: options.repo.display().to_string(),
            validation_commands: vec![
                VALIDATION_COMMANDS[0].to_string(),
                VALIDATION_COMMANDS[1].to_string(),
                VALIDATION_COMMANDS[2].to_string(),
                VALIDATION_COMMANDS[3].to_string(),
            ],
        }
    }
}

impl SelfValidationEvidenceReport {
    fn planned(packet: &SelfEvolutionPromptPacket) -> Self {
        Self {
            schema_version: SELF_EVOLUTION_VALIDATION_REPORT_VERSION.to_string(),
            prompt_packet_version: packet.version.clone(),
            workflow_id: packet.workflow_id.clone(),
            run_id: packet.run_id.clone(),
            cycle: packet.cycle,
            executor: packet.executor.clone(),
            repo: packet.repo.clone(),
            status: "planned".to_string(),
            validation_passed: false,
            started_at: None,
            finished_at: None,
            commands: packet
                .validation_commands
                .iter()
                .map(|command| SelfValidationCommandEvidence::planned(command))
                .collect(),
        }
    }
}

impl SelfValidationCommandEvidence {
    fn planned(command: &str) -> Self {
        Self {
            command: command.to_string(),
            status: "planned".to_string(),
            exit_code: None,
            duration_ms: None,
            stdout: String::new(),
            stderr: String::new(),
            reason: None,
        }
    }

    fn skipped(command: &str, reason: &str) -> Self {
        Self {
            command: command.to_string(),
            status: "skipped".to_string(),
            exit_code: None,
            duration_ms: None,
            stdout: String::new(),
            stderr: String::new(),
            reason: Some(reason.to_string()),
        }
    }
}

impl SelfUpdateReport {
    fn planned() -> Self {
        Self {
            status: "planned".to_string(),
            command: self_update_command(),
            reason: None,
        }
    }

    fn completed() -> Self {
        Self {
            status: "completed".to_string(),
            command: self_update_command(),
            reason: None,
        }
    }

    fn skipped(reason: &str) -> Self {
        Self {
            status: "skipped".to_string(),
            command: self_update_command(),
            reason: Some(reason.to_string()),
        }
    }
}

impl PublicProjectUpdateReport {
    fn planned(push: bool) -> Self {
        if !push {
            return Self::skipped(false, "push flag not requested");
        }
        Self {
            status: "planned".to_string(),
            uses_gh: true,
            gh_auth_command: gh_auth_command(),
            repo_view_command: gh_repo_view_command(),
            push_command: git_push_command(),
            url: None,
            visibility: None,
            reason: None,
        }
    }

    fn completed(remote_url: String) -> Self {
        Self {
            status: "completed".to_string(),
            uses_gh: true,
            gh_auth_command: gh_auth_command(),
            repo_view_command: gh_repo_view_command(),
            push_command: git_push_command(),
            url: Some(remote_url),
            visibility: None,
            reason: None,
        }
    }

    fn skipped(push: bool, reason: &str) -> Self {
        Self {
            status: "skipped".to_string(),
            uses_gh: push,
            gh_auth_command: gh_auth_command(),
            repo_view_command: gh_repo_view_command(),
            push_command: git_push_command(),
            url: None,
            visibility: None,
            reason: Some(reason.to_string()),
        }
    }
}

fn render_prompt(packet: &SelfEvolutionPromptPacket) -> String {
    format!(
        r#"# Improve Forge Core

Prompt packet version: `{}`

You are executing Forge self-evolution cycle {}.

Run id: `{}`
Workflow id: `{}`
Executor: `{}`
Stop date: `{}`

Goal:
- Improve Forge Core itself in a small, validated, production-quality increment.
- Prefer structural improvements over cosmetic changes.
- Good candidates: async run records, task leases, executor adapter contracts, prompt packet versioning, runtime mutation propagation, changelog/report quality, validation gates.
- Strategic runtime goals now include workflow listing, terminal inspection, recursive subflows, infinite subflows, scale-to-zero lifecycle state and flow composition/reuse.
- Prefer increments that move toward `forge list` for running and non-running workflows, `forge inspect` for terminal DAG/subflow visualization, and a workflow registry that can reuse compatible existing flows as child subflows before creating new work.
- Prioritize the Context Routing Engine: compress, summarize, select, version and shard the minimum correct context for each executor to reduce irrelevant context, redundant reasoning and cost.
- Preserve deterministic + AI hybrid graph semantics: AI tasks, deterministic code tasks, waits, cron, approvals, validation, rollback and deployment should coexist in the same graph.
- Improve long-running cognition: pause/resume, async continuation, durable execution, checkpointing, partial retry and resumable context.
- Add execution policy that can choose no-AI deterministic nodes for repeated or frequent work, including local Python or Node.js code nodes, instead of spending model calls.

Constraints:
- Use the repository at `{}`.
- Do not mutate external Docker/Kubernetes/Knative resources.
- Do not install Knative or modify user infrastructure.
- Keep changes scoped to Forge Core.
- Use tests first when adding behavior.
- Run the required validation commands listed in this prompt packet.
- If validation fails, fix or report the blocker without pretending the cycle completed.
- Generate or update a strong changelog/report artifact when the version behavior changes.
- Codex/OpenCode should treat Forge as the source of truth: update goals/artifacts through Forge CLI if runtime state changes.
- After validation passes, update the local Forge installation with `cargo install --path . --force`.
- Publish validated commits through the GitHub CLI contract: `gh auth token`, `git remote get-url origin`, then `git push`.

Required validation commands:
{}

Return a concise final report with:
- files changed;
- tests run;
- validation result;
- next recommended cycle.
"#,
        packet.version,
        packet.cycle,
        packet.run_id,
        packet.workflow_id,
        packet.executor,
        packet.stop_at,
        packet.repo,
        packet
            .validation_commands
            .iter()
            .map(|command| format!("- `{command}`"))
            .collect::<Vec<_>>()
            .join("\n")
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
                    "--ask-for-approval",
                    "never",
                    "exec",
                    "--cd",
                    repo.to_str().unwrap_or("."),
                    "--sandbox",
                    "workspace-write",
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

fn run_validation(
    repo: &Path,
    packet: &SelfEvolutionPromptPacket,
) -> Result<SelfValidationEvidenceReport> {
    let started_at = Utc::now();
    let mut commands = Vec::new();
    let mut validation_passed = true;
    let mut skip_remaining = false;

    for command in &packet.validation_commands {
        if skip_remaining {
            commands.push(SelfValidationCommandEvidence::skipped(
                command,
                "previous validation command failed",
            ));
            continue;
        }

        let started = Instant::now();
        let output = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .current_dir(repo)
            .output()
            .with_context(|| format!("failed to run validation command `{command}`"))?;
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let passed = output.status.success();
        if !passed {
            validation_passed = false;
            skip_remaining = true;
        }
        commands.push(SelfValidationCommandEvidence {
            command: command.clone(),
            status: if passed { "passed" } else { "failed" }.to_string(),
            exit_code: output.status.code(),
            duration_ms: Some(duration_ms),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            reason: None,
        });
    }

    Ok(SelfValidationEvidenceReport {
        schema_version: SELF_EVOLUTION_VALIDATION_REPORT_VERSION.to_string(),
        prompt_packet_version: packet.version.clone(),
        workflow_id: packet.workflow_id.clone(),
        run_id: packet.run_id.clone(),
        cycle: packet.cycle,
        executor: packet.executor.clone(),
        repo: packet.repo.clone(),
        status: if validation_passed {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        validation_passed,
        started_at: Some(started_at),
        finished_at: Some(Utc::now()),
        commands,
    })
}

fn emit_validation_failure_logs(report: &SelfValidationEvidenceReport) {
    for command in &report.commands {
        if command.status != "failed" {
            continue;
        }
        eprintln!("validation command failed: {}", command.command);
        if !command.stdout.is_empty() {
            eprintln!("{}", command.stdout);
        }
        if !command.stderr.is_empty() {
            eprintln!("{}", command.stderr);
        }
    }
}

fn run_self_update(repo: &Path) -> Result<SelfUpdateReport> {
    run_program(repo, "cargo", &["install", "--path", ".", "--force"])
        .context("failed to update local Forge installation")?;
    Ok(SelfUpdateReport::completed())
}

fn publish_public_project_with_gh(repo: &Path) -> Result<PublicProjectUpdateReport> {
    run_program(
        repo,
        "timeout",
        &[GH_AUTH_TIMEOUT_SECONDS, "gh", "auth", "token"],
    )
    .context("failed to validate GitHub CLI authentication")?;
    let remote_url = run_git(repo, &["remote", "get-url", "origin"])
        .context("failed to inspect git origin before public project update")?;
    run_program(repo, "timeout", &[GIT_PUSH_TIMEOUT_SECONDS, "git", "push"])
        .context("failed to push validated Forge update")?;
    Ok(PublicProjectUpdateReport::completed(
        remote_url.trim().to_string(),
    ))
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
    run_program(repo, "git", args)
}

fn run_program(repo: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(repo)
        .output()?;
    if !output.status.success() {
        bail!(
            "{} {:?} failed: {}{}",
            program,
            args,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn self_update_command() -> Vec<String> {
    ["cargo", "install", "--path", ".", "--force"]
        .iter()
        .map(|part| part.to_string())
        .collect()
}

fn gh_auth_command() -> Vec<String> {
    ["timeout", GH_AUTH_TIMEOUT_SECONDS, "gh", "auth", "token"]
        .iter()
        .map(|part| part.to_string())
        .collect()
}

fn gh_repo_view_command() -> Vec<String> {
    ["git", "remote", "get-url", "origin"]
        .iter()
        .map(|part| part.to_string())
        .collect()
}

fn git_push_command() -> Vec<String> {
    ["timeout", GIT_PUSH_TIMEOUT_SECONDS, "git", "push"]
        .iter()
        .map(|part| part.to_string())
        .collect()
}
