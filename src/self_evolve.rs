use crate::artifact::{hex_sha256, write_json_artifact};
use crate::graph::{create_workflow, Workflow};
use crate::intent::parse_intent;
use crate::request::{create_run_record, heartbeat_request, save_run_record, update_run_status};
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const SELF_EVOLUTION_PROMPT_PACKET_VERSION: &str = "forge.self_evolution.prompt.v2";
const SELF_EVOLUTION_VALIDATION_REPORT_VERSION: &str = "forge.self_evolution.validation.v1";
const BASE_SELF_EVOLUTION_GOAL: &str =
    "Improve Forge Core autonomously with bounded executor cycles, validation gates, artifacts and changelog";
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
    pub mode: String,
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
    pub operating_mode: String,
    pub max_cycles: u32,
    pub dry_run: bool,
    pub push: bool,
    pub overhead_ledger: SelfOverheadLedger,
    pub decision_gate: SelfDecisionGateReport,
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
    pub overhead_ledger: SelfOverheadLedger,
    pub decision_gate: SelfDecisionGateReport,
    pub self_update: SelfUpdateReport,
    pub committed: bool,
    pub commit: Option<String>,
    pub public_project_update: PublicProjectUpdateReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfOverheadLedger {
    pub schema_version: String,
    pub operating_mode: String,
    pub cycle_count: u32,
    pub prompt_bytes: u64,
    pub estimated_prompt_tokens: u64,
    pub validation_command_count: u32,
    pub artifact_count: u32,
    pub metadata_bytes: u64,
    pub orchestration_cost_score: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfDecisionGateReport {
    pub schema_version: String,
    pub operating_mode: String,
    pub mode_boundary: String,
    pub decision: String,
    pub stop_loop: bool,
    pub terminal_goal_reached: bool,
    pub expected_value_score: u32,
    pub orchestration_cost_score: u32,
    pub reason: String,
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
    workflow_goal: String,
    initial_workflow_goal: String,
    workflow_revision: u64,
    stop_at: String,
    repo: String,
    operating_mode: String,
    decision_gate: SelfDecisionGateReport,
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

#[derive(Debug, Clone)]
enum SelfOperatingMode {
    Lean,
    Balanced,
    Strict,
}

impl SelfOperatingMode {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "balanced" => Ok(Self::Balanced),
            "lean" => Ok(Self::Lean),
            "strict" => Ok(Self::Strict),
            other => bail!("unsupported self-evolution mode: {other}"),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Lean => "lean",
            Self::Balanced => "balanced",
            Self::Strict => "strict",
        }
    }

    fn boundary(&self) -> &'static str {
        match self {
            Self::Lean => {
                "minimal governance; run only when expected throughput, cost, retry or artifact value clearly exceeds orchestration cost"
            }
            Self::Balanced => {
                "default bounded governance; allow small validated increments with explicit value evidence and measured overhead"
            }
            Self::Strict => {
                "high auditability; tolerate more overhead only for real failure prevention, audit, safety or distributed execution needs"
            }
        }
    }

    fn base_cost_score(&self) -> u32 {
        match self {
            Self::Lean => 2,
            Self::Balanced => 3,
            Self::Strict => 5,
        }
    }
}

impl SelfOverheadLedger {
    fn empty(mode: &SelfOperatingMode) -> Self {
        Self {
            schema_version: "forge.self_evolution.overhead_ledger.v1".to_string(),
            operating_mode: mode.as_str().to_string(),
            cycle_count: 0,
            prompt_bytes: 0,
            estimated_prompt_tokens: 0,
            validation_command_count: 0,
            artifact_count: 0,
            metadata_bytes: 0,
            orchestration_cost_score: mode.base_cost_score(),
        }
    }

    fn for_cycle(
        mode: &SelfOperatingMode,
        prompt_bytes: u64,
        validation_command_count: u32,
        artifact_count: u32,
        metadata_bytes: u64,
    ) -> Self {
        let estimated_prompt_tokens = estimate_tokens(prompt_bytes);
        Self {
            schema_version: "forge.self_evolution.overhead_ledger.v1".to_string(),
            operating_mode: mode.as_str().to_string(),
            cycle_count: 1,
            prompt_bytes,
            estimated_prompt_tokens,
            validation_command_count,
            artifact_count,
            metadata_bytes,
            orchestration_cost_score: mode.base_cost_score()
                + (estimated_prompt_tokens / 2_000) as u32
                + artifact_count,
        }
    }

    fn aggregate(mode: &SelfOperatingMode, reports: &[SelfCycleReport]) -> Self {
        let mut ledger = Self::empty(mode);
        ledger.cycle_count = reports.len() as u32;
        for report in reports {
            ledger.prompt_bytes += report.overhead_ledger.prompt_bytes;
            ledger.estimated_prompt_tokens += report.overhead_ledger.estimated_prompt_tokens;
            ledger.validation_command_count += report.overhead_ledger.validation_command_count;
            ledger.artifact_count += report.overhead_ledger.artifact_count;
            ledger.metadata_bytes += report.overhead_ledger.metadata_bytes;
            ledger.orchestration_cost_score += report.overhead_ledger.orchestration_cost_score;
        }
        ledger
    }
}

impl SelfDecisionGateReport {
    fn evaluate(goal: &str, mode: &SelfOperatingMode) -> Self {
        let expected_value_score = expected_value_score(goal);
        let orchestration_cost_score = mode.base_cost_score() + bloat_score(goal);
        let terminal_goal_reached = terminal_goal_contract_satisfied(goal);
        let (decision, stop_loop, reason) = if terminal_goal_reached {
            (
                "stop_terminal_goal_reached",
                true,
                "terminal self-evolution goal is already satisfied by the mode boundary, overhead ledger and decision gate",
            )
        } else if expected_value_score < orchestration_cost_score {
            (
                "reject_low_value_cycle",
                true,
                "expected value is lower than orchestration cost under the selected operating mode",
            )
        } else {
            (
                "run_cycle",
                false,
                "expected value is high enough to justify one bounded self-evolution cycle",
            )
        };

        Self {
            schema_version: "forge.self_evolution.decision_gate.v1".to_string(),
            operating_mode: mode.as_str().to_string(),
            mode_boundary: mode.boundary().to_string(),
            decision: decision.to_string(),
            stop_loop,
            terminal_goal_reached,
            expected_value_score,
            orchestration_cost_score,
            reason: reason.to_string(),
        }
    }
}

pub fn run_self_evolution(store: &ForgeStore, options: SelfRunOptions) -> Result<SelfRunReport> {
    let operating_mode = SelfOperatingMode::parse(&options.mode)?;
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

    let self_evolution_goal = load_persisted_self_evolution_goal(store)?
        .unwrap_or_else(|| BASE_SELF_EVOLUTION_GOAL.to_string());
    let workflow = create_workflow(parse_intent(&self_evolution_goal));
    let run = create_run_record(&workflow, "forge_cli", "planned");
    store.save_workflow(&workflow)?;
    save_run_record(store, &run)?;

    let decision_gate = SelfDecisionGateReport::evaluate(&self_evolution_goal, &operating_mode);
    if decision_gate.stop_loop {
        let overhead_ledger = SelfOverheadLedger::empty(&operating_mode);
        return Ok(SelfRunReport {
            status: if decision_gate.terminal_goal_reached {
                "terminal_goal_reached".to_string()
            } else {
                "rejected".to_string()
            },
            run_id: run.run_id,
            workflow_id: workflow.id,
            stop_at: options.until,
            repo: options.repo.display().to_string(),
            executors,
            operating_mode: operating_mode.as_str().to_string(),
            max_cycles: options.max_cycles,
            dry_run: options.dry_run,
            push: options.push,
            overhead_ledger,
            decision_gate,
            cycle_reports: Vec::new(),
        });
    }

    let mut cycle_reports = Vec::new();
    for cycle in 1..=options.max_cycles {
        if Utc::now() >= stop_at.with_timezone(&Utc) {
            break;
        }
        let executor = executors[((cycle - 1) as usize) % executors.len()].clone();
        let current_workflow = store
            .load_workflow(&workflow.id)
            .unwrap_or_else(|_| workflow.clone());
        let prompt_packet = SelfEvolutionPromptPacket::new(
            cycle,
            &executor,
            &current_workflow,
            &run.run_id,
            &options,
            &operating_mode,
            &decision_gate,
        );
        let prompt = render_prompt(&prompt_packet);
        let prompt_sha256 = hex_sha256(prompt.as_bytes());
        let cycle_overhead_ledger = SelfOverheadLedger::for_cycle(
            &operating_mode,
            prompt.len() as u64,
            prompt_packet.validation_commands.len() as u32,
            3,
            serde_json::to_vec(&prompt_packet)?.len() as u64,
        );
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
            heartbeat_request(
                store,
                &run.run_id,
                &executor,
                &format!("Self-evolution cycle {cycle}: preparing"),
                300,
                std::process::id().into(),
                "forge_cli",
            )?;
            if let Ok(mut wf) = store.load_workflow(&workflow.id) {
                wf.status = "running".to_string();
                let _ = store.save_workflow(&wf);
            }
            status = match execute_cycle(&options.repo, &executor, &prompt) {
                Ok(s) => s,
                Err(e) => {
                    let _ = update_run_status(store, &run.run_id, "failed", "forge_cli");
                    if let Ok(mut wf) = store.load_workflow(&workflow.id) {
                        wf.status = "failed".to_string();
                        let _ = store.save_workflow(&wf);
                    }
                    return Err(e.context(format!("executor cycle {cycle} failed")));
                }
            };
            validation_report = run_validation(&options.repo, &prompt_packet)?;
            if !validation_report.validation_passed {
                emit_validation_failure_logs(&validation_report);
            }
            let validation_passed = validation_report.validation_passed;
            let cycle_workflow_status = if validation_passed {
                "completed"
            } else {
                "failed"
            };
            heartbeat_request(
                store,
                &run.run_id,
                &executor,
                &format!("Self-evolution cycle {cycle}: {cycle_workflow_status}"),
                300,
                std::process::id().into(),
                "forge_cli",
            )?;
            if let Ok(mut wf) = store.load_workflow(&workflow.id) {
                wf.status = cycle_workflow_status.to_string();
                let _ = store.save_workflow(&wf);
            }
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
            overhead_ledger: cycle_overhead_ledger,
            decision_gate: decision_gate.clone(),
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
    let overhead_ledger = SelfOverheadLedger::aggregate(&operating_mode, &cycle_reports);

    let has_failures = cycle_reports.iter().any(|r| !r.validation_passed);
    if !options.dry_run {
        let final_status = if has_failures { "failed" } else { "completed" };
        update_run_status(store, &run.run_id, final_status, "forge_cli")?;
        if let Ok(mut wf) = store.load_workflow(&workflow.id) {
            wf.status = final_status.to_string();
            let _ = store.save_workflow(&wf);
        }
    }

    Ok(SelfRunReport {
        status: if options.dry_run {
            "planned".to_string()
        } else if has_failures {
            "failed".to_string()
        } else {
            "completed".to_string()
        },
        run_id: run.run_id,
        workflow_id: workflow.id,
        stop_at: options.until,
        repo: options.repo.display().to_string(),
        executors,
        operating_mode: operating_mode.as_str().to_string(),
        max_cycles: options.max_cycles,
        dry_run: options.dry_run,
        push: options.push,
        overhead_ledger,
        decision_gate,
        cycle_reports,
    })
}

impl SelfEvolutionPromptPacket {
    fn new(
        cycle: u32,
        executor: &str,
        workflow: &Workflow,
        run_id: &str,
        options: &SelfRunOptions,
        operating_mode: &SelfOperatingMode,
        decision_gate: &SelfDecisionGateReport,
    ) -> Self {
        Self {
            version: SELF_EVOLUTION_PROMPT_PACKET_VERSION.to_string(),
            cycle,
            executor: executor.to_string(),
            workflow_id: workflow.id.clone(),
            run_id: run_id.to_string(),
            workflow_goal: workflow.goal.clone(),
            initial_workflow_goal: workflow
                .initial_goal
                .clone()
                .unwrap_or_else(|| workflow.goal.clone()),
            workflow_revision: workflow.revisions.len() as u64,
            stop_at: options.until.clone(),
            repo: options.repo.display().to_string(),
            operating_mode: operating_mode.as_str().to_string(),
            decision_gate: decision_gate.clone(),
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

fn load_persisted_self_evolution_goal(store: &ForgeStore) -> Result<Option<String>> {
    let workflows = store.load_workflows()?;
    Ok(workflows
        .into_iter()
        .filter(is_self_evolution_workflow)
        .filter_map(|workflow| {
            let goal = workflow.goal.trim().to_string();
            if goal.is_empty() {
                return None;
            }
            let changed_at = workflow
                .revisions
                .iter()
                .map(|revision| revision.created_at)
                .max()
                .unwrap_or(workflow.created_at);
            Some((changed_at, goal))
        })
        .max_by_key(|(changed_at, _)| *changed_at)
        .map(|(_, goal)| goal))
}

fn is_self_evolution_workflow(workflow: &Workflow) -> bool {
    workflow.goal.contains(BASE_SELF_EVOLUTION_GOAL)
        || workflow
            .initial_goal
            .as_deref()
            .is_some_and(|goal| goal.contains(BASE_SELF_EVOLUTION_GOAL))
}

fn estimate_tokens(bytes: u64) -> u64 {
    bytes.saturating_add(3) / 4
}

fn terminal_goal_contract_satisfied(goal: &str) -> bool {
    let normalized = goal.to_ascii_lowercase();
    let explicit_continuation = normalized.contains("do not stop")
        || normalized.contains("continue until")
        || normalized.contains("forge 0.5")
        || normalized.contains("creative runtime")
        || normalized.contains("first-class no-argument interactive forge cli")
        || normalized.contains("live human+ai collaboration")
        || normalized.contains("version-boundary");
    if explicit_continuation {
        return false;
    }

    normalized.contains("validated lean/balanced/strict mode boundary")
        && normalized.contains("measurable overhead ledger")
        && normalized.contains("automated self-evolution decision gate")
        && normalized.contains("expected value is lower than orchestration cost")
}

fn expected_value_score(goal: &str) -> u32 {
    let normalized = goal.to_ascii_lowercase();
    let no_value_clause = normalized.contains("without changing")
        || normalized.contains("without improving")
        || normalized.contains("does not improve");
    if no_value_clause && bloat_score(goal) > 0 {
        return 1;
    }

    let value_terms = [
        "throughput",
        "reduces",
        "reduce",
        "cost",
        "retries",
        "retry",
        "deterministic",
        "artifact delivery",
        "validation",
        "useful artifact",
        "prevents",
        "failure",
        "context routing",
        "bounded executor",
    ];
    let strategic_terms = [
        "forge 0.5",
        "mcp",
        "skill",
        "agent integration",
        "creative runtime",
        "interactive forge cli",
        "no-argument interactive",
        "slash command",
        "slash-command",
        "tui",
        "direct-chat routing",
        "human decision",
        "form",
        "live collaboration",
        "whiteboard",
        "design token",
        "design system",
        "componentization",
        "creative artifact",
        "milestone manifest",
        "telegram",
    ];
    let base_score = value_terms
        .iter()
        .filter(|term| normalized.contains(**term))
        .count() as u32;
    let strategic_score = strategic_terms
        .iter()
        .filter(|term| normalized.contains(**term))
        .count() as u32;
    let score = base_score + strategic_score.saturating_mul(2);
    score.max(4)
}

fn bloat_score(goal: &str) -> u32 {
    let normalized = goal.to_ascii_lowercase();
    [
        "governance",
        "schema",
        "schemas",
        "receipt",
        "receipts",
        "hash",
        "hashes",
        "manifest",
        "manifests",
        "projection",
        "projections",
        "metadata",
    ]
    .iter()
    .filter(|term| normalized.contains(**term))
    .count() as u32
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

Persisted Forge workflow goal (authoritative):
{}

Initial workflow goal:
{}

Workflow revision: `{}`

Operating mode: `{}`

Mode boundary:
- {}

Lean overhead ledger:
- Record prompt bytes, estimated prompt tokens, validation command count, artifact count and metadata bytes for each cycle.
- Use the ledger to compare orchestration cost against useful artifact delivery, retries avoided, deterministic execution and validation value.

Automated self-evolution decision gate:
- Schema: `{}`
- Decision: `{}`
- Expected value score: `{}`
- Orchestration cost score: `{}`
- Reason: {}

Strategic goal guidance:
- Improve Forge Core itself in a small, validated, production-quality increment.
- The persisted Forge workflow goal above is runtime state. If a human updates that goal with `forge workflow update-goal`, future self-evolution cycles must honor it before generic guidance.
- Prefer structural improvements over cosmetic changes.
- Good candidates: async run records, task leases, executor adapter contracts, prompt packet versioning, runtime mutation propagation, changelog/report quality, validation gates.
- Strategic runtime goals now include workflow listing, terminal inspection, recursive subflows, infinite subflows, scale-to-zero lifecycle state and flow composition/reuse.
- Prefer increments that move toward `forge list` for running and non-running workflows, `forge inspect` for terminal DAG/subflow visualization, and a workflow registry that can reuse compatible existing flows as child subflows before creating new work.
- Prioritize the Context Routing Engine: compress, summarize, select, version and shard the minimum correct context for each executor to reduce irrelevant context, redundant reasoning and cost.
- Add Personality/Soul Routing for human-facing artifacts: inspect how Codex handles developer/personality instructions and how Paperclip models soul, voice, tone or persona, then allow specific workflow moments to switch persona mode explicitly, scoped to the node, auditable in lineage and validation-gated.
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
        packet.workflow_goal,
        packet.initial_workflow_goal,
        packet.workflow_revision,
        packet.operating_mode,
        packet.decision_gate.mode_boundary,
        packet.decision_gate.schema_version,
        packet.decision_gate.decision,
        packet.decision_gate.expected_value_score,
        packet.decision_gate.orchestration_cost_score,
        packet.decision_gate.reason,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Workflow;
    use chrono::Utc;

    #[test]
    fn test_operating_mode_parse_valid() {
        assert!(matches!(
            SelfOperatingMode::parse("").unwrap(),
            SelfOperatingMode::Balanced
        ));
        assert!(matches!(
            SelfOperatingMode::parse("balanced").unwrap(),
            SelfOperatingMode::Balanced
        ));
        assert!(matches!(
            SelfOperatingMode::parse("Balanced").unwrap(),
            SelfOperatingMode::Balanced
        ));
        assert!(matches!(
            SelfOperatingMode::parse("lean").unwrap(),
            SelfOperatingMode::Lean
        ));
        assert!(matches!(
            SelfOperatingMode::parse("strict").unwrap(),
            SelfOperatingMode::Strict
        ));
    }

    #[test]
    fn test_operating_mode_parse_invalid() {
        assert!(SelfOperatingMode::parse("invalid").is_err());
        assert!(SelfOperatingMode::parse("ultra").is_err());
    }

    #[test]
    fn test_operating_mode_as_str() {
        assert_eq!(SelfOperatingMode::Lean.as_str(), "lean");
        assert_eq!(SelfOperatingMode::Balanced.as_str(), "balanced");
        assert_eq!(SelfOperatingMode::Strict.as_str(), "strict");
    }

    #[test]
    fn test_operating_mode_boundary() {
        assert!(SelfOperatingMode::Lean
            .boundary()
            .contains("minimal governance"));
        assert!(SelfOperatingMode::Balanced
            .boundary()
            .contains("default bounded governance"));
        assert!(SelfOperatingMode::Strict
            .boundary()
            .contains("high auditability"));
    }

    #[test]
    fn test_operating_mode_base_cost_score() {
        assert_eq!(SelfOperatingMode::Lean.base_cost_score(), 2);
        assert_eq!(SelfOperatingMode::Balanced.base_cost_score(), 3);
        assert_eq!(SelfOperatingMode::Strict.base_cost_score(), 5);
    }

    #[test]
    fn test_overhead_ledger_empty() {
        let ledger = SelfOverheadLedger::empty(&SelfOperatingMode::Balanced);
        assert_eq!(
            ledger.schema_version,
            "forge.self_evolution.overhead_ledger.v1"
        );
        assert_eq!(ledger.operating_mode, "balanced");
        assert_eq!(ledger.cycle_count, 0);
        assert_eq!(ledger.prompt_bytes, 0);
        assert_eq!(ledger.estimated_prompt_tokens, 0);
        assert_eq!(ledger.validation_command_count, 0);
        assert_eq!(ledger.artifact_count, 0);
        assert_eq!(ledger.metadata_bytes, 0);
        assert_eq!(ledger.orchestration_cost_score, 3);
    }

    #[test]
    fn test_overhead_ledger_for_cycle() {
        let ledger = SelfOverheadLedger::for_cycle(&SelfOperatingMode::Lean, 1024, 4, 3, 512);
        assert_eq!(ledger.operating_mode, "lean");
        assert_eq!(ledger.prompt_bytes, 1024);
        assert_eq!(ledger.estimated_prompt_tokens, 256);
        assert_eq!(ledger.validation_command_count, 4);
        assert_eq!(ledger.artifact_count, 3);
        assert_eq!(ledger.metadata_bytes, 512);
        assert_eq!(ledger.orchestration_cost_score, 5);
    }

    #[test]
    fn test_overhead_ledger_aggregate() {
        let r1 = SelfCycleReport {
            cycle: 1,
            executor: "test".to_string(),
            status: "completed".to_string(),
            prompt_path: "p1.md".to_string(),
            prompt_packet_version: "v1".to_string(),
            prompt_sha256: "a".to_string(),
            validation_report_path: "v1.json".to_string(),
            validation_report_sha256: "b".to_string(),
            report_path: "r1.json".to_string(),
            validation_passed: true,
            overhead_ledger: SelfOverheadLedger::for_cycle(
                &SelfOperatingMode::Balanced,
                500,
                2,
                1,
                100,
            ),
            decision_gate: SelfDecisionGateReport {
                schema_version: String::new(),
                operating_mode: "balanced".to_string(),
                mode_boundary: String::new(),
                decision: "run_cycle".to_string(),
                stop_loop: false,
                terminal_goal_reached: false,
                expected_value_score: 10,
                orchestration_cost_score: 5,
                reason: String::new(),
            },
            self_update: SelfUpdateReport::completed(),
            committed: false,
            commit: None,
            public_project_update: PublicProjectUpdateReport::skipped(false, "test"),
        };
        let r2 = SelfCycleReport {
            cycle: 2,
            executor: "test".to_string(),
            status: "completed".to_string(),
            prompt_path: "p2.md".to_string(),
            prompt_packet_version: "v1".to_string(),
            prompt_sha256: "c".to_string(),
            validation_report_path: "v2.json".to_string(),
            validation_report_sha256: "d".to_string(),
            report_path: "r2.json".to_string(),
            validation_passed: true,
            overhead_ledger: SelfOverheadLedger::for_cycle(
                &SelfOperatingMode::Balanced,
                700,
                2,
                2,
                200,
            ),
            decision_gate: SelfDecisionGateReport {
                schema_version: String::new(),
                operating_mode: "balanced".to_string(),
                mode_boundary: String::new(),
                decision: "run_cycle".to_string(),
                stop_loop: false,
                terminal_goal_reached: false,
                expected_value_score: 10,
                orchestration_cost_score: 5,
                reason: String::new(),
            },
            self_update: SelfUpdateReport::completed(),
            committed: true,
            commit: Some("abc123".to_string()),
            public_project_update: PublicProjectUpdateReport::skipped(false, "test"),
        };
        let aggregated = SelfOverheadLedger::aggregate(&SelfOperatingMode::Balanced, &[r1, r2]);
        assert_eq!(aggregated.cycle_count, 2);
        assert_eq!(aggregated.prompt_bytes, 1200);
        assert_eq!(aggregated.estimated_prompt_tokens, 125 + 175);
        assert_eq!(aggregated.validation_command_count, 4);
        assert_eq!(aggregated.artifact_count, 3);
        assert_eq!(aggregated.metadata_bytes, 300);
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(1), 1);
        assert_eq!(estimate_tokens(4), 1);
        assert_eq!(estimate_tokens(5), 2);
        assert_eq!(estimate_tokens(100), 25);
    }

    #[test]
    fn test_is_self_evolution_workflow() {
        let wf_evolution = Workflow {
            id: "wf_test".to_string(),
            goal: BASE_SELF_EVOLUTION_GOAL.to_string(),
            initial_goal: None,
            status: "running".to_string(),
            created_at: Utc::now(),
            intent: crate::intent::IntentSpec {
                goal: BASE_SELF_EVOLUTION_GOAL.to_string(),
                constraints: vec![],
                deliverables: vec![],
                risks: vec![],
                unknowns: vec![],
            },
            tasks: vec![],
            artifacts: vec![],
            creative_artifacts: vec![],
            token_collection: None,
            revisions: vec![],
        };
        assert!(is_self_evolution_workflow(&wf_evolution));

        let wf_other = Workflow {
            id: "wf_other".to_string(),
            goal: "Build a web app".to_string(),
            initial_goal: None,
            status: "pending".to_string(),
            created_at: Utc::now(),
            intent: crate::intent::IntentSpec {
                goal: "Build a web app".to_string(),
                constraints: vec![],
                deliverables: vec![],
                risks: vec![],
                unknowns: vec![],
            },
            tasks: vec![],
            artifacts: vec![],
            creative_artifacts: vec![],
            token_collection: None,
            revisions: vec![],
        };
        assert!(!is_self_evolution_workflow(&wf_other));
    }

    #[test]
    fn test_terminal_goal_contract_satisfied_true() {
        let goal = "validated lean/balanced/strict mode boundary and measurable overhead ledger and automated self-evolution decision gate and expected value is lower than orchestration cost";
        assert!(terminal_goal_contract_satisfied(goal));
    }

    #[test]
    fn test_terminal_goal_explicit_continuation_prevents_satisfied() {
        assert!(!terminal_goal_contract_satisfied(
            "do not stop and validated lean/balanced/strict mode boundary"
        ));
        assert!(!terminal_goal_contract_satisfied(
            "continue until forge 0.5 and measurable overhead ledger"
        ));
        assert!(!terminal_goal_contract_satisfied(
            "forge 0.5 creative runtime"
        ));
        assert!(!terminal_goal_contract_satisfied(
            "first-class no-argument interactive forge cli"
        ));
        assert!(!terminal_goal_contract_satisfied(
            "live human+ai collaboration"
        ));
        assert!(!terminal_goal_contract_satisfied(
            "version-boundary milestone"
        ));
    }

    #[test]
    fn test_terminal_goal_not_satisfied_for_unrelated_goal() {
        assert!(!terminal_goal_contract_satisfied("improve test coverage"));
        assert!(!terminal_goal_contract_satisfied(""));
    }

    #[test]
    fn test_expected_value_score_has_minimum() {
        assert!(expected_value_score("") >= 4);
        assert!(expected_value_score("unrelated text without value terms") >= 4);
    }

    #[test]
    fn test_expected_value_score_scales_with_terms() {
        let basic = expected_value_score("validation throughput");
        assert!(basic >= 4);
        let strategic =
            expected_value_score("forge 0.5 mcp skill creative runtime interactive forge cli");
        assert!(strategic > basic);
    }

    #[test]
    fn test_bloat_score_counts_matching_terms() {
        assert_eq!(bloat_score("governance metadata receipt"), 3);
        assert_eq!(bloat_score("schema hash manifest projection"), 4);
        assert_eq!(bloat_score("no bloat here"), 0);
        assert_eq!(bloat_score(""), 0);
    }

    #[test]
    fn test_decision_gate_evaluate_terminal_reached() {
        let mode = SelfOperatingMode::Balanced;
        let goal = "validated lean/balanced/strict mode boundary and measurable overhead ledger and automated self-evolution decision gate and expected value is lower than orchestration cost";
        let gate = SelfDecisionGateReport::evaluate(goal, &mode);
        assert!(gate.stop_loop);
        assert!(gate.terminal_goal_reached);
        assert_eq!(gate.decision, "stop_terminal_goal_reached");
    }

    #[test]
    fn test_decision_gate_evaluate_run_cycle() {
        let mode = SelfOperatingMode::Lean;
        let goal = "forge 0.5 creative runtime with validation and artifact delivery";
        let gate = SelfDecisionGateReport::evaluate(goal, &mode);
        assert!(!gate.stop_loop);
        assert!(!gate.terminal_goal_reached);
        assert_eq!(gate.decision, "run_cycle");
        assert!(gate.expected_value_score >= gate.orchestration_cost_score);
    }

    #[test]
    fn test_self_update_report() {
        let planned = SelfUpdateReport::planned();
        assert_eq!(planned.status, "planned");
        assert!(planned.reason.is_none());

        let completed = SelfUpdateReport::completed();
        assert_eq!(completed.status, "completed");

        let skipped = SelfUpdateReport::skipped("validation failed");
        assert_eq!(skipped.status, "skipped");
        assert_eq!(skipped.reason.unwrap(), "validation failed");
    }

    #[test]
    fn test_self_update_command_format() {
        let cmd = self_update_command();
        assert_eq!(cmd, vec!["cargo", "install", "--path", ".", "--force"]);
    }
}
