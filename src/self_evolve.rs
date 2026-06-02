use crate::artifact::{hex_sha256, write_json_artifact};
use crate::graph::{
    create_workflow, task, ExecutorKind, LoopSpec, ProductDecision, Workflow, WorkflowRevision,
};
use crate::intent::parse_intent;
use crate::request::{create_run_record, heartbeat_request, save_run_record, update_run_status};
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const OPENCODE_TIMEOUT_SECONDS: u64 = 180;
const EXECUTOR_TIMEOUT_SECONDS: u64 = 180;

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
const DEFAULT_SELF_EXECUTORS: [&str; 3] = ["opencode", "gemini", "codex"];

#[derive(Debug, Clone)]
pub struct SelfRunOptions {
    pub repo: PathBuf,
    pub until: String,
    pub max_cycles: u32,
    pub sleep_seconds: u64,
    pub executors: Vec<String>,
    pub fallback_executors: Vec<String>,
    pub goal: Option<String>,
    pub validation_commands: Vec<String>,
    pub mode: String,
    pub skip_self_update: bool,
    pub self_update_command: Option<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_fallbacks: Vec<String>,
    pub operating_mode: String,
    pub max_cycles: u32,
    pub dry_run: bool,
    pub push: bool,
    pub internal_loop: SelfEvolutionLoopReport,
    pub overhead_ledger: SelfOverheadLedger,
    pub decision_gate: SelfDecisionGateReport,
    pub cycle_reports: Vec<SelfCycleReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfCycleReport {
    pub cycle: u32,
    pub requested_executor: String,
    pub executor: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_fallbacks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_attempts: Vec<SelfExecutorAttempt>,
    pub status: String,
    pub executor_policy: SelfExecutorPolicyReport,
    pub prompt_path: String,
    pub prompt_packet_version: String,
    pub prompt_sha256: String,
    pub validation_report_path: String,
    pub validation_report_sha256: String,
    pub report_path: String,
    pub markdown_report_path: String,
    pub validation_passed: bool,
    pub overhead_ledger: SelfOverheadLedger,
    pub decision_gate: SelfDecisionGateReport,
    pub self_update: SelfUpdateReport,
    pub committed: bool,
    pub commit: Option<String>,
    pub public_project_update: PublicProjectUpdateReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfExecutorAttempt {
    pub executor: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub local: bool,
    pub quota_model: String,
    pub cost_model: String,
    pub remaining_quota: String,
    pub rate_limit_risk: String,
    pub monetary_or_token_cost: String,
    pub expected_quality: String,
    pub fallback_risk: String,
    pub selection_tier: u32,
    pub status: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

struct SelfExecutorExecution {
    executor: String,
    status: String,
    attempts: Vec<SelfExecutorAttempt>,
}

#[derive(Debug, Clone)]
struct SelfExecutorStrategy {
    executor: String,
    provider: Option<String>,
    model: Option<String>,
    local: bool,
    quota_model: String,
    cost_model: String,
    remaining_quota: String,
    rate_limit_risk: String,
    monetary_or_token_cost: String,
    expected_quality: String,
    fallback_risk: String,
    selection_tier: u32,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfExecutorPolicyReport {
    pub schema_version: String,
    pub selection_principle: String,
    pub decision_factors: Vec<String>,
    pub requested_chain: Vec<String>,
    pub candidates: Vec<SelfExecutorPolicyCandidate>,
    pub skipped_to_preserve_quota: Vec<String>,
    pub repair_goals: Vec<String>,
    pub active_repair_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfExecutorPolicyCandidate {
    pub executor: String,
    pub provider: String,
    pub model: Option<String>,
    pub local_vs_non_local: String,
    pub free_vs_paid_if_known: String,
    pub quota_model: String,
    pub remaining_quota: String,
    pub rate_limit_risk: String,
    pub monetary_or_token_cost: String,
    pub latency: String,
    pub expected_quality: String,
    pub suitability_for_product_business_reasoning: String,
    pub fallback_risk: String,
    pub non_interactive_requirement: String,
    pub selection_tier: u32,
    pub selection_status: String,
    pub reason: String,
    pub capability_evidence: Vec<String>,
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
pub struct SelfEvolutionLoopReport {
    pub schema_version: String,
    pub loop_count: u32,
    pub loop_task_id: String,
    pub loop_control_kind: String,
    pub execution_shape: String,
    pub sleep_seconds: u64,
    pub sleep_policy: String,
    pub next_goal_decision: SelfNextGoalDecisionReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfNextGoalDecisionReport {
    pub schema_version: String,
    pub decision_id: String,
    pub selected_goal: String,
    pub source: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
    pub trade_offs: Vec<String>,
    pub success_metrics: Vec<String>,
    pub backlog_mutation: String,
    pub workflow_revision: u64,
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
    executor_fallbacks: Vec<String>,
    workflow_id: String,
    run_id: String,
    workflow_goal: String,
    initial_workflow_goal: String,
    workflow_revision: u64,
    stop_at: String,
    repo: String,
    operating_mode: String,
    decision_gate: SelfDecisionGateReport,
    internal_loop: SelfEvolutionLoopReport,
    validation_commands: Vec<String>,
    /// Structured breakdown of forge 0.5 capabilities detected in the goal text,
    /// each with a prioritised maturity target.
    capability_analysis: Vec<ForgeCapability>,
}

#[derive(Debug, Clone, Serialize)]
struct ForgeCapability {
    name: String,
    priority: String,
    present_in_goal: bool,
    description: String,
    guidance: String,
}

fn analyze_goal_capabilities(goal: &str) -> Vec<ForgeCapability> {
    let normalized = goal.to_ascii_lowercase();
    // Each capability has a keyword set, priority, and human-readable guidance.
    // Priority reflects how close we are to 0.5: "critical" means a 0.5 blocker,
    // "high" means strongly desired this cycle, "medium" means valuable but not blocking.
    let candidates: Vec<(&str, &str, &str, &str)> = vec![
        (
            "cron / schedule",
            "critical",
            "cron, schedule, scheduled, due_workflows, scan_due, next_run_at",
            "Complete cron+loop+schedule for reliable periodic and continuous workflows in production.",
        ),
        (
            "interactive forge CLI",
            "critical",
            "interactive forge cli, no-argument interactive, tui, home_screen, slash_command, conversational",
            "Ship the full terminal interactive mode with slash commands, conversational routing, and workflow retention decisions.",
        ),
        (
            "creative runtime",
            "critical",
            "creative runtime, whiteboard, document, slide, screen, component_manifest, design_token",
            "Activate the creative artifact runtime for screens, whiteboards, documents/slides, and component manifests with design token resolution.",
        ),
        (
            "live collaboration",
            "high",
            "live collaboration, presence, co-edit, human+ai, decision, audit, shared_context",
            "Enable human+AI collaborative editing with presence tracking, decision nodes, and audit trails.",
        ),
        (
            "context routing engine",
            "high",
            "context routing, compress, summarize, select, version, shard, context routing engine",
            "Build the context routing engine to compress, summarize, select, version, and shard the correct context per executor.",
        ),
        (
            "MCP / skill integration",
            "high",
            "mcp, skill, tool, agent integration, codex, opencode, executor adapter",
            "Strengthen MCP tool integration, skill installation, and executor adapter contracts.",
        ),
        (
            "scheduler / loop / subflow",
            "high",
            "scheduler, loop, subflow, recursive, infinite, scale_to_zero, flow composition",
            "Complete recursive/infinite subflow support, scale-to-zero lifecycle, and flow composition/reuse.",
        ),
        (
            "quota-aware executor policy",
            "high",
            "quota, executor policy, fallback, selection tier, non-interactive, model selection",
            "Implement explicit quota-aware executor selection and fallback policy to avoid interactive timeouts and manage costs.",
        ),
        (
            "durable decision artifacts",
            "high",
            "decision artifact, rationale, alternatives, trade-offs, success metrics, backlog mutation",
            "Add durable decision artifacts for product choices, rationale, alternatives, trade-offs, and success metrics.",
        ),
        (
            "validation & milestone gates",
            "medium",
            "validation, milestone, promote, gate, block, promotable, rework",
            "Evolve the validation framework to block promotion when constraints are breached and produce promotable milestone evidence.",
        ),
        (
            "workflow listing & inspect",
            "medium",
            "list, inspect, workflow_registry, lifecycle, running, non-running",
            "Build workflow inspection tools for terminal DAG/subflow visualization and a registry that can reuse compatible flows.",
        ),
        (
            "telegram / notification",
            "medium",
            "telegram, notification, notify, webhook, alert",
            "Add notification channels (Telegram or webhook) for workflow events and human decision prompts.",
        ),
        (
            "design tokens",
            "medium",
            "design token, design system, semantic resolution, inheritance, patch_by_intent",
            "Complete design token schema with semantic resolution, inheritance, and AI patch-by-intent support.",
        ),
        (
            "componentization",
            "medium",
            "componentization, component manifest, variant, state, action, dependency",
            "Complete componentization with variants, states, actions, and token dependency tracking.",
        ),
        (
            "execution policy",
            "medium",
            "execution policy, deterministic, ai_allowed, no-ai, python node, node.js node",
            "Add execution policy that can choose no-AI deterministic nodes for repeated work instead of model calls.",
        ),
    ];

    candidates
        .into_iter()
        .map(|(name, priority, keywords, description)| {
            let present = keywords.split(',').any(|kw| {
                let kw = kw.trim();
                normalized.contains(kw)
            });
            let guidance = if present {
                format!(
                    "Goal mentions {}. Prioritise work that advances this toward 0.5-ready state.",
                    name
                )
            } else {
                format!(
                    "Goal does not explicitly mention {}. Include structural scaffolding if safe.",
                    name
                )
            };
            ForgeCapability {
                name: name.to_string(),
                priority: priority.to_string(),
                present_in_goal: present,
                description: description.to_string(),
                guidance,
            }
        })
        .collect()
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
        DEFAULT_SELF_EXECUTORS
            .iter()
            .map(|executor| executor.to_string())
            .collect()
    } else {
        options.executors.clone()
    };
    let primary_executor = executors.first().map(String::as_str).unwrap_or("opencode");
    let executor_fallbacks = executor_fallback_chain_for_requested(
        primary_executor,
        &executors,
        &options.fallback_executors,
    );

    let persisted_self_evolution_goal = load_persisted_self_evolution_goal(store)?;
    let self_evolution_goal = options
        .goal
        .clone()
        .or(persisted_self_evolution_goal)
        .unwrap_or_else(|| BASE_SELF_EVOLUTION_GOAL.to_string());
    let mut workflow = create_workflow(parse_intent(&self_evolution_goal));
    ensure_self_evolution_loop_control(&mut workflow);
    let internal_loop = persist_self_evolution_loop_state(
        &mut workflow,
        options.sleep_seconds,
        options.goal.as_deref(),
    )?;

    let loop_count = workflow
        .tasks
        .iter()
        .filter(|t| t.loop_control.is_some())
        .count();
    if loop_count == 0 {
        bail!("Self-evolution workflow has loop_count == 0. The planned workflow must include persisted loop_control tasks.");
    }

    let mut run = create_run_record(&workflow, "forge_cli", "planned");
    run.executor_fallbacks = executor_fallbacks.clone();
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
            executor_fallbacks,
            operating_mode: operating_mode.as_str().to_string(),
            max_cycles: options.max_cycles,
            dry_run: options.dry_run,
            push: options.push,
            internal_loop,
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
        let requested_executor = executors[((cycle - 1) as usize) % executors.len()].clone();
        let cycle_fallback_executors = executor_fallback_chain_for_requested(
            &requested_executor,
            &executors,
            &options.fallback_executors,
        );
        let executor_policy = build_executor_policy(
            store,
            &requested_executor,
            &cycle_fallback_executors,
            &cycle_reports,
        );
        let mut executor = requested_executor.clone();
        let mut executor_attempts = Vec::new();
        let current_workflow = store
            .load_workflow(&workflow.id)
            .unwrap_or_else(|_| workflow.clone());
        let prompt_packet = SelfEvolutionPromptPacket::new(SelfEvolutionPromptPacketParams {
            cycle,
            executor: &requested_executor,
            executor_fallbacks: &cycle_fallback_executors,
            workflow: &current_workflow,
            run_id: &run.run_id,
            options: &options,
            operating_mode: &operating_mode,
            decision_gate: &decision_gate,
            internal_loop: &internal_loop,
        });
        let prompt = render_prompt(&prompt_packet);
        let prompt_sha256 = hex_sha256(prompt.as_bytes());
        let cycle_overhead_ledger = SelfOverheadLedger::for_cycle(
            &operating_mode,
            prompt.len() as u64,
            prompt_packet.validation_commands.len() as u32,
            4,
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
        let markdown_report_path = format!(
            "artifacts/{}/self-evolution-cycle-{:03}-report.md",
            workflow.id, cycle
        );
        let validation_report_path = format!(
            "artifacts/{}/self-evolution-cycle-{:03}-validation.json",
            workflow.id, cycle
        );
        write_text_artifact(&store.base_dir(), &prompt_path, &prompt)?;

        let mut status = "planned".to_string();
        let mut validation_report = SelfValidationEvidenceReport::planned(&prompt_packet);
        let mut self_update =
            SelfUpdateReport::planned_for(effective_self_update_command(&options));
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
            let execution = match execute_cycle_with_fallback(
                store,
                &options.repo,
                &requested_executor,
                &cycle_fallback_executors,
                &cycle_reports,
                &prompt,
            ) {
                Ok(execution) => execution,
                Err(e) => {
                    let _ = update_run_status(store, &run.run_id, "failed", "forge_cli");
                    if let Ok(mut wf) = store.load_workflow(&workflow.id) {
                        wf.status = "failed".to_string();
                        let _ = store.save_workflow(&wf);
                    }
                    return Err(e.context(format!("executor cycle {cycle} failed")));
                }
            };
            executor = execution.executor;
            executor_attempts = execution.attempts;
            status = execution.status;
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
                self_update = run_self_update(&options.repo, &options)?;
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
                self_update = SelfUpdateReport::skipped_for(
                    effective_self_update_command(&options),
                    "validation failed",
                );
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
            requested_executor,
            executor,
            executor_fallbacks: cycle_fallback_executors,
            executor_attempts,
            status,
            executor_policy,
            prompt_path: prompt_path.clone(),
            prompt_packet_version: prompt_packet.version,
            prompt_sha256,
            validation_report_path: validation_report_path.clone(),
            validation_report_sha256,
            report_path: report_path.clone(),
            markdown_report_path: markdown_report_path.clone(),
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
        write_text_artifact(
            &store.base_dir(),
            &markdown_report_path,
            &render_cycle_markdown_report(&cycle_report),
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
        executor_fallbacks,
        operating_mode: operating_mode.as_str().to_string(),
        max_cycles: options.max_cycles,
        dry_run: options.dry_run,
        push: options.push,
        internal_loop,
        overhead_ledger,
        decision_gate,
        cycle_reports,
    })
}

struct SelfEvolutionPromptPacketParams<'a> {
    cycle: u32,
    executor: &'a str,
    executor_fallbacks: &'a [String],
    workflow: &'a Workflow,
    run_id: &'a str,
    options: &'a SelfRunOptions,
    operating_mode: &'a SelfOperatingMode,
    decision_gate: &'a SelfDecisionGateReport,
    internal_loop: &'a SelfEvolutionLoopReport,
}

impl SelfEvolutionPromptPacket {
    fn new(params: SelfEvolutionPromptPacketParams<'_>) -> Self {
        let capability_analysis = analyze_goal_capabilities(&params.workflow.goal);
        Self {
            version: SELF_EVOLUTION_PROMPT_PACKET_VERSION.to_string(),
            cycle: params.cycle,
            executor: params.executor.to_string(),
            executor_fallbacks: params.executor_fallbacks.to_vec(),
            workflow_id: params.workflow.id.clone(),
            run_id: params.run_id.to_string(),
            workflow_goal: params.workflow.goal.clone(),
            initial_workflow_goal: params
                .workflow
                .initial_goal
                .clone()
                .unwrap_or_else(|| params.workflow.goal.clone()),
            workflow_revision: params.workflow.revisions.len() as u64,
            stop_at: params.options.until.clone(),
            repo: params.options.repo.display().to_string(),
            operating_mode: params.operating_mode.as_str().to_string(),
            decision_gate: params.decision_gate.clone(),
            internal_loop: params.internal_loop.clone(),
            validation_commands: effective_validation_commands(params.options),
            capability_analysis,
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
    #[cfg(test)]
    fn planned() -> Self {
        Self::planned_for(self_update_command())
    }

    fn planned_for(command: Vec<String>) -> Self {
        Self {
            status: "planned".to_string(),
            command,
            reason: None,
        }
    }

    #[cfg(test)]
    fn completed() -> Self {
        Self::completed_for(self_update_command())
    }

    fn completed_for(command: Vec<String>) -> Self {
        Self {
            status: "completed".to_string(),
            command,
            reason: None,
        }
    }

    #[cfg(test)]
    fn skipped(reason: &str) -> Self {
        Self::skipped_for(self_update_command(), reason)
    }

    fn skipped_for(command: Vec<String>, reason: &str) -> Self {
        Self {
            status: "skipped".to_string(),
            command,
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

fn ensure_self_evolution_loop_control(workflow: &mut Workflow) {
    if let Some(task) = workflow.tasks.iter_mut().find(|task| {
        task.title == "Continue self-evolution while product goal is not definitively ready"
    }) {
        if task.loop_control.is_some() {
            ensure_loop_decision_inputs(task);
            return;
        }
        task.loop_control = Some(self_evolution_loop_spec());
        ensure_loop_decision_inputs(task);
        return;
    }

    let id = format!("task-{:03}", workflow.tasks.len() + 1);
    let dependency = workflow
        .tasks
        .last()
        .map(|task| task.id.as_str())
        .unwrap_or("task-001")
        .to_string();
    let mut loop_task = task(
        &id,
        "Continue self-evolution while product goal is not definitively ready",
        &[dependency.as_str()],
        &[
            "current product-evolution goal",
            "validation evidence",
            "human pause/stop/mutation state",
            "next goal decision with product/business rationale",
        ],
        Vec::new(),
        "Self-evolution loop-control trace",
        (ExecutorKind::Command, 0.0002),
    );
    loop_task.loop_control = Some(self_evolution_loop_spec());
    ensure_loop_decision_inputs(&mut loop_task);
    workflow.tasks.push(loop_task);
}

fn self_evolution_loop_spec() -> LoopSpec {
    LoopSpec {
        schema_version: "forge.loop.v1".to_string(),
        kind: "while_until".to_string(),
        items: vec!["product_evolution_goal".to_string()],
        max_iterations: None,
        condition: Some(
            "continue until goal ready, validation passes, or human pauses/stops/mutates workflow"
                .to_string(),
        ),
        backoff_policy: None,
        subflow_mode: "ordinary_forge_workflow".to_string(),
        stop_policy: "human_pause_stop_or_revisioned_mutation".to_string(),
        state: "active".to_string(),
    }
}

fn ensure_loop_decision_inputs(task: &mut crate::graph::AtomicTask) {
    let required_input = "next goal decision with product/business rationale".to_string();
    if !task
        .context_requirements
        .iter()
        .any(|input| input == &required_input)
    {
        task.context_requirements.push(required_input);
    }
    if !task
        .expected_output
        .contains("next goal decision with product/business rationale")
    {
        task.expected_output = format!(
            "{} including next goal decision with product/business rationale",
            task.expected_output
        );
    }
}

fn persist_self_evolution_loop_state(
    workflow: &mut Workflow,
    sleep_seconds: u64,
    explicit_human_goal: Option<&str>,
) -> Result<SelfEvolutionLoopReport> {
    let (loop_task_id, loop_control_kind) = workflow
        .tasks
        .iter()
        .find_map(|task| {
            if task.title == "Continue self-evolution while product goal is not definitively ready" {
                task.loop_control
                    .as_ref()
                    .map(|loop_control| (task.id.clone(), loop_control.kind.clone()))
            } else {
                None
            }
        })
        .context("Self-evolution workflow must include persisted loop_control before loop state can be reported")?;
    let loop_count = workflow
        .tasks
        .iter()
        .filter(|task| task.loop_control.is_some())
        .count() as u32;
    if loop_count == 0 {
        bail!("Self-evolution workflow has loop_count == 0. The planned workflow must include persisted loop_control tasks.");
    }

    let selected_goal = explicit_human_goal
        .map(str::trim)
        .filter(|goal| !goal.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            "Make the Product/PM CLI-TUI the main entry point for human-guided product/workflow creation."
                .to_string()
        });
    let source = if explicit_human_goal
        .map(str::trim)
        .is_some_and(|goal| !goal.is_empty())
    {
        "human_goal".to_string()
    } else {
        "autonomous_v0_5_priority_selection".to_string()
    };
    let rationale = if source == "human_goal" {
        "A fresh human goal outranks generic self-evolution guidance and preserves Forge as the source of truth for runtime steering.".to_string()
    } else {
        "This improves the product and business outcome by making Forge easier to adopt for product managers and founders before deeper runtime automation, while preserving validation evidence and low implementation leverage risk.".to_string()
    };
    let alternatives = vec![
        "Add durable decision artifacts before the PM/TUI entry point".to_string(),
        "Improve executor non-interactive policy before human product workflow creation"
            .to_string(),
        "Start visual workflow editing before terminal product decisions are durable".to_string(),
    ];
    let trade_offs = vec![
        "Prioritizes user-facing product workflow value over lower-level executor polish for this cycle".to_string(),
        "Keeps the increment small enough for validation while delaying richer visual surfaces".to_string(),
    ];
    let success_metrics = vec![
        "self run report exposes internal recurring loop evidence".to_string(),
        "workflow inspect shows a loop_control node tied to next-goal selection".to_string(),
        "next selected goal includes product/business rationale and revision lineage".to_string(),
    ];
    let revision = workflow
        .revisions
        .last()
        .map(|item| item.revision + 1)
        .unwrap_or(1);
    let decision_id = format!("decision-self-evolution-next-goal-r{revision}");
    workflow.product_decisions.push(ProductDecision {
        id: decision_id.clone(),
        title: "Self-evolution next goal selection".to_string(),
        rationale: rationale.clone(),
        author: "forge_self_evolution".to_string(),
        status: "approved".to_string(),
        revision,
        created_at: Utc::now(),
        affected_goals: vec![selected_goal.clone()],
        affected_tasks: vec![loop_task_id.clone()],
        affected_artifacts: Vec::new(),
    });
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: "forge_self_evolution".to_string(),
        change_type: "self_evolution_next_goal_decision".to_string(),
        summary: format!(
            "selected next self-evolution goal `{selected_goal}` with product/business rationale"
        ),
        created_at: Utc::now(),
    });

    Ok(SelfEvolutionLoopReport {
        schema_version: "forge.self_evolution.loop.v1".to_string(),
        loop_count,
        loop_task_id,
        loop_control_kind,
        execution_shape: "ordinary_forge_workflow_internal_recurring_loop".to_string(),
        sleep_seconds,
        sleep_policy: "rest_between_iterations".to_string(),
        next_goal_decision: SelfNextGoalDecisionReport {
            schema_version: "forge.self_evolution.next_goal_decision.v1".to_string(),
            decision_id,
            selected_goal,
            source,
            rationale,
            alternatives,
            trade_offs,
            success_metrics,
            backlog_mutation: "persisted_as_product_decision_for_next_iteration".to_string(),
            workflow_revision: revision,
        },
    })
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
    let is_forge_core_goal = packet
        .workflow_goal
        .to_ascii_lowercase()
        .contains("forge core");
    let strategic_guidance = if is_forge_core_goal {
        "- Improve Forge Core itself in a small, validated, production-quality increment.\n- The persisted Forge workflow goal above is runtime state. If a human updates that goal with `forge workflow update-goal`, future self-evolution cycles must honor it before generic guidance.\n- Prefer structural improvements over cosmetic changes.\n- Good candidates: async run records, task leases, executor adapter contracts, prompt packet versioning, runtime mutation propagation, changelog/report quality, validation gates.\n- Strategic runtime goals now include workflow listing, terminal inspection, recursive subflows, infinite subflows, scale-to-zero lifecycle state and flow composition/reuse.\n- Prefer increments that move toward `forge list` for running and non-running workflows, `forge inspect` for terminal DAG/subflow visualization, and a workflow registry that can reuse compatible existing flows as child subflows before creating new work.\n- Prioritize the Context Routing Engine: compress, summarize, select, version and shard the minimum correct context for each executor to reduce irrelevant context, redundant reasoning and cost.\n- Add Personality/Soul Routing for human-facing artifacts: inspect how Codex handles developer/personality instructions and how Paperclip models soul, voice, tone or persona, then allow specific workflow moments to switch persona mode explicitly, scoped to the node, auditable in lineage and validation-gated.\n- Preserve deterministic + AI hybrid graph semantics: AI tasks, deterministic code tasks, waits, cron, approvals, validation, rollback and deployment should coexist in the same graph.\n- Improve long-running cognition: pause/resume, async continuation, durable execution, checkpointing, partial retry and resumable context.\n- Add execution policy that can choose no-AI deterministic nodes for repeated or frequent work, including local Python or Node.js code nodes, instead of spending model calls."
    } else {
        "- Improve the target repository named in this prompt, not Forge Core itself.\n- Treat the persisted workflow goal above as authoritative over generic Forge guidance.\n- Keep changes scoped to the target repository and current project objective.\n- Prefer small, validated increments with clear artifact evidence.\n- If the goal asks for a migration, preserve the current working baseline and add parity tests before replacing behavior.\n- Keep workflow state portable when possible by writing resumable project manifests under a versionable `.forge/project-state/` folder while leaving local databases, logs and executor scratch files ignored."
    };
    let scope_rule = if is_forge_core_goal {
        "- Keep changes scoped to Forge Core.\n- After validation passes, update the local Forge installation with `cargo install --path . --force`."
    } else {
        "- Keep changes scoped to the target repository.\n- Do not run Forge Core self-update unless explicitly configured for this project."
    };
    format!(
        r#"# Forge Self-Run Cycle

Prompt packet version: `{}`

You are executing Forge self-evolution cycle {}.

Run id: `{}`
Workflow id: `{}`
Executor: `{}`
Executor fallback chain: `{}`
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

Internal recurring loop state:
- Schema: `{}`
- Execution shape: `{}`
- Loop count: `{}`
- Loop control task: `{}`
- Loop control kind: `{}`
- Rest between iterations: `{}s`
- Next goal decision: `{}`
- Next goal source: `{}`
- Next goal rationale: {}
- Next goal workflow revision: `{}`

Strategic goal guidance:
{}

Capability analysis (prioritised from goal text):
{}


Constraints:
- Use the repository at `{}`.
- Do not mutate external Docker/Kubernetes/Knative resources.
- Do not install Knative or modify user infrastructure.
{}
- Use tests first when adding behavior.
- Run the required validation commands listed in this prompt packet.
- If validation fails, fix or report the blocker without pretending the cycle completed.
- Generate or update a strong changelog/report artifact when the version behavior changes.
- Codex/OpenCode should treat Forge as the source of truth: update goals/artifacts through Forge CLI if runtime state changes.
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
        render_executor_fallbacks(&packet.executor_fallbacks),
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
        packet.internal_loop.schema_version,
        packet.internal_loop.execution_shape,
        packet.internal_loop.loop_count,
        packet.internal_loop.loop_task_id,
        packet.internal_loop.loop_control_kind,
        packet.internal_loop.sleep_seconds,
        packet.internal_loop.next_goal_decision.selected_goal,
        packet.internal_loop.next_goal_decision.source,
        packet.internal_loop.next_goal_decision.rationale,
        packet.internal_loop.next_goal_decision.workflow_revision,
        strategic_guidance,
        render_capability_breakdown(packet),
        packet.repo,
        scope_rule,
        packet
            .validation_commands
            .iter()
            .map(|command| format!("- `{command}`"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn render_cycle_markdown_report(report: &SelfCycleReport) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "# Forge self-evolution cycle {}\n\n",
        report.cycle
    ));
    output.push_str(&format!("- Status: {}\n", report.status));
    output.push_str(&format!(
        "- Requested executor: {}\n",
        report.requested_executor
    ));
    output.push_str(&format!("- Selected executor: {}\n", report.executor));
    output.push_str(&format!(
        "- Validation: {}\n",
        if report.validation_passed {
            "passed"
        } else {
            "not passed"
        }
    ));
    output.push_str(&format!("- JSON report: {}\n", report.report_path));
    output.push_str(&format!(
        "- Validation report: {}\n\n",
        report.validation_report_path
    ));

    output.push_str("## Quota-aware executor policy\n\n");
    output.push_str(&format!(
        "- Policy: {}\n",
        report.executor_policy.selection_principle
    ));
    output.push_str(&format!(
        "- Repair status: {}\n\n",
        report.executor_policy.active_repair_status
    ));
    output
        .push_str("| Executor | Provider | Model | Locality | Quota | Cost | Status | Reason |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for candidate in &report.executor_policy.candidates {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            markdown_cell(&candidate.executor),
            markdown_cell(&candidate.provider),
            markdown_cell(candidate.model.as_deref().unwrap_or("default")),
            markdown_cell(&candidate.local_vs_non_local),
            markdown_cell(&candidate.quota_model),
            markdown_cell(&candidate.monetary_or_token_cost),
            markdown_cell(&candidate.selection_status),
            markdown_cell(&candidate.reason),
        ));
    }

    if !report.executor_attempts.is_empty() {
        output.push_str("\n## Executor attempts\n\n");
        output
            .push_str("| Executor | Provider | Model | Local | Quota | Cost | Status | Error |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
        for attempt in &report.executor_attempts {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                markdown_cell(&attempt.executor),
                markdown_cell(attempt.provider.as_deref().unwrap_or("default")),
                markdown_cell(attempt.model.as_deref().unwrap_or("default")),
                attempt.local,
                markdown_cell(&attempt.quota_model),
                markdown_cell(&attempt.monetary_or_token_cost),
                markdown_cell(&attempt.status),
                markdown_cell(attempt.error.as_deref().unwrap_or("none")),
            ));
        }
    }

    if !report.executor_policy.skipped_to_preserve_quota.is_empty() {
        output.push_str("\n## Quota preservation\n\n");
        for item in &report.executor_policy.skipped_to_preserve_quota {
            output.push_str(&format!("- {}\n", item));
        }
    }

    if !report.executor_policy.repair_goals.is_empty() {
        output.push_str("\n## Repair goals\n\n");
        for goal in &report.executor_policy.repair_goals {
            output.push_str(&format!("- {}\n", goal));
        }
    }

    output
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "/").replace('\n', " ")
}

fn render_capability_breakdown(packet: &SelfEvolutionPromptPacket) -> String {
    let critical: Vec<_> = packet
        .capability_analysis
        .iter()
        .filter(|c| c.priority == "critical")
        .collect();
    let high: Vec<_> = packet
        .capability_analysis
        .iter()
        .filter(|c| c.priority == "high")
        .collect();
    let medium: Vec<_> = packet
        .capability_analysis
        .iter()
        .filter(|c| c.priority == "medium")
        .collect();

    let mut buf = String::new();
    if !critical.is_empty() {
        buf.push_str("\n### Critical (0.5 blockers)\n\n");
        for cap in &critical {
            buf.push_str(&format!(
                "- **{}** {} — {}\n",
                cap.name,
                if cap.present_in_goal { "✓" } else { "○" },
                cap.guidance
            ));
        }
    }
    if !high.is_empty() {
        buf.push_str("\n### High priority\n\n");
        for cap in &high {
            buf.push_str(&format!(
                "- **{}** {} — {}\n",
                cap.name,
                if cap.present_in_goal { "✓" } else { "○" },
                cap.guidance
            ));
        }
    }
    if !medium.is_empty() {
        buf.push_str("\n### Medium priority\n\n");
        for cap in &medium {
            buf.push_str(&format!(
                "- **{}** {} — {}\n",
                cap.name,
                if cap.present_in_goal { "✓" } else { "○" },
                cap.guidance
            ));
        }
    }
    buf
}

fn render_executor_fallbacks(fallback_executors: &[String]) -> String {
    if fallback_executors.is_empty() {
        "none".to_string()
    } else {
        fallback_executors.join(", ")
    }
}

fn write_text_artifact(base_dir: &Path, relative_path: &str, content: &str) -> Result<()> {
    let full_path = base_dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(full_path, content)?;
    Ok(())
}

fn select_executor_strategies(
    store: &ForgeStore,
    primary_executor: &str,
    fallback_executors: &[String],
    previous_reports: &[SelfCycleReport],
) -> Vec<SelfExecutorStrategy> {
    build_executor_policy(
        store,
        primary_executor,
        fallback_executors,
        previous_reports,
    )
    .candidates
    .into_iter()
    .filter(|candidate| candidate.selection_status == "eligible")
    .map(|candidate| SelfExecutorStrategy {
        executor: candidate.executor,
        provider: Some(candidate.provider).filter(|provider| provider != "configured_cli"),
        model: candidate.model,
        local: candidate.local_vs_non_local == "local",
        quota_model: candidate.quota_model,
        cost_model: candidate.free_vs_paid_if_known,
        remaining_quota: candidate.remaining_quota,
        rate_limit_risk: candidate.rate_limit_risk,
        monetary_or_token_cost: candidate.monetary_or_token_cost,
        expected_quality: candidate.expected_quality,
        fallback_risk: candidate.fallback_risk,
        selection_tier: candidate.selection_tier,
        reason: candidate.reason,
    })
    .collect()
}

fn build_executor_policy(
    store: &ForgeStore,
    primary_executor: &str,
    fallback_executors: &[String],
    previous_reports: &[SelfCycleReport],
) -> SelfExecutorPolicyReport {
    let requested_chain = normalize_executor_chain(primary_executor, fallback_executors);
    let mut candidates = Vec::new();

    if requested_chain
        .iter()
        .any(|executor| executor == "opencode")
    {
        candidates.push(opencode_free_non_local_candidate(&requested_chain));
    }
    if requested_chain.iter().any(|executor| executor == "gemini") {
        candidates.push(gemini_non_local_candidate(&requested_chain));
    }
    if requested_chain.iter().any(|executor| executor == "codex") {
        candidates.push(codex_non_local_candidate(&requested_chain));
    }
    if requested_chain
        .iter()
        .any(|executor| executor == "opencode")
    {
        candidates.push(opencode_paid_non_local_candidate(&requested_chain));
    }
    if requested_chain
        .iter()
        .any(|executor| executor == "opencode")
    {
        candidates.push(opencode_local_candidate(&requested_chain));
    }

    // Apply previous failure status to candidates
    for candidate in &mut candidates {
        for report in previous_reports {
            for attempt in &report.executor_attempts {
                if attempt.executor == candidate.executor
                    && attempt.provider.as_deref() == Some(&candidate.provider)
                    && attempt.model == candidate.model
                {
                    if let Some(error) = &attempt.error {
                        if error.contains("timed out") || error.contains("interactive") {
                            candidate.selection_status = "failed_timeout".to_string();
                            candidate.reason = format!(
                                "Previously failed due to timeout/interaction in cycle {}. Repair needed.",
                                report.cycle
                            );
                        } else {
                            candidate.selection_status = "failed_previous".to_string();
                        }
                    }
                }
            }
        }
    }

    // Augment with persisted quota observations from store
    let observations = store.load_executor_quotas().unwrap_or_default();
    for candidate in &mut candidates {
        let matching = observations.iter().find_map(|v| {
            let obs: crate::executor::ExecutorQuotaObservation =
                serde_json::from_value(v.clone()).ok()?;
            if obs.executor == candidate.executor
                && (obs.provider == candidate.provider || candidate.provider == "configured_cli")
            {
                Some(obs)
            } else {
                None
            }
        });
        if let Some(obs) = matching {
            candidate.capability_evidence.push(format!(
                "quota_observation:{}:{}:{}:{}",
                obs.source, obs.remaining_quota, obs.rate_limit_risk, obs.observed_at
            ));
            candidate.remaining_quota = obs.remaining_quota;
            candidate.rate_limit_risk = obs.rate_limit_risk;
            candidate.monetary_or_token_cost = obs.monetary_or_token_cost;
            candidate.latency = obs.latency;
        }
    }

    candidates.sort_by_key(|candidate| candidate.selection_tier);

    let mut repair_goals = vec![
        "Gemini non-interactive repair: detect auth/model/approval prompts before handoff and create a repair goal instead of repeated timeouts.".to_string(),
        "OpenCode model repair: record provider/model availability and distinguish non-local quota-bound choices from local Ollama fallback.".to_string(),
    ];

    let active_repair_status = if candidates
        .iter()
        .any(|c| c.selection_status == "failed_timeout")
    {
        "repair_needed_timeout_detected".to_string()
    } else {
        "stable".to_string()
    };

    if active_repair_status == "repair_needed_timeout_detected" {
        repair_goals.push("Urgent: Fix interactive timeout in primary executor chain.".to_string());
    }

    SelfExecutorPolicyReport {
        schema_version: "forge.self_evolution.executor_policy.v1".to_string(),
        selection_principle:
            "maximize useful progress under expected value, quota, cost, latency, quality and fallback risk constraints"
                .to_string(),
        decision_factors: vec![
            "provider".to_string(),
            "model".to_string(),
            "local_vs_non_local".to_string(),
            "free_vs_paid_if_known".to_string(),
            "remaining_quota_if_available".to_string(),
            "rate_limit_risk".to_string(),
            "monetary_or_token_cost".to_string(),
            "latency".to_string(),
            "expected_quality".to_string(),
            "suitability_for_product_business_reasoning".to_string(),
            "fallback_risk".to_string(),
            "non_interactive_requirement".to_string(),
        ],
        requested_chain,
        candidates,
        skipped_to_preserve_quota: vec![
            "Use deterministic validation commands directly instead of spending Gemini/Codex quota."
                .to_string(),
            "Prefer local OpenCode/Ollama for cheap repetitive or low-value work when non-local quota value is low."
                .to_string(),
        ],
        repair_goals,
        active_repair_status,
    }
}

fn normalize_executor_chain(primary_executor: &str, fallback_executors: &[String]) -> Vec<String> {
    let mut chain = Vec::new();
    for executor in
        std::iter::once(primary_executor.to_string()).chain(fallback_executors.iter().cloned())
    {
        let executor = executor.trim().to_lowercase();
        if !executor.is_empty() && !chain.iter().any(|existing| existing == &executor) {
            chain.push(executor);
        }
    }
    chain
}

fn requested_rank(chain: &[String], executor: &str) -> u32 {
    chain
        .iter()
        .position(|candidate| candidate == executor)
        .map(|rank| rank as u32)
        .unwrap_or(99)
}

fn quota_aware_selection_tier(chain: &[String], executor: &str, capability_rank: u32) -> u32 {
    capability_rank * 10 + requested_rank(chain, executor).min(9)
}

fn opencode_free_non_local_candidate(chain: &[String]) -> SelfExecutorPolicyCandidate {
    let model = std::env::var("OPENCODE_FREE_MODEL")
        .ok()
        .or_else(|| Some("google/gemini-2.5-pro".to_string()));
    let provider = model
        .as_deref()
        .and_then(|model| model.split('/').next())
        .unwrap_or("configured_cli");
    SelfExecutorPolicyCandidate {
        executor: "opencode".to_string(),
        provider: provider.to_string(),
        model,
        local_vs_non_local: "non_local".to_string(),
        free_vs_paid_if_known: "unknown_or_configured_free_non_local".to_string(),
        quota_model: "quota_or_rate_limit_bound".to_string(),
        remaining_quota: "unknown_until_provider_probe".to_string(),
        rate_limit_risk: "medium".to_string(),
        monetary_or_token_cost: "provider_configured_no_cost".to_string(),
        latency: "medium".to_string(),
        expected_quality: "high_when_configured_provider_is_strong".to_string(),
        suitability_for_product_business_reasoning: "high_for_high_value_pm_business_or_creative_reasoning".to_string(),
        fallback_risk: "may fail when provider auth, quota or model configuration is missing".to_string(),
        non_interactive_requirement: "must run through opencode run without model/auth prompts".to_string(),
        selection_tier: quota_aware_selection_tier(chain, "opencode", 1),
        selection_status: "eligible".to_string(),
        reason: "OpenCode non-local free/configured provider path is first choice when expected value justifies configured no-cost capacity.".to_string(),
        capability_evidence: vec![
            "Supports --model override".to_string(),
            "Non-interactive --dangerously-skip-permissions mode available".to_string(),
        ],
    }
}

fn gemini_non_local_candidate(chain: &[String]) -> SelfExecutorPolicyCandidate {
    let model = std::env::var("GEMINI_MODEL")
        .ok()
        .or_else(|| Some("gemini-2.5-pro".to_string()));
    SelfExecutorPolicyCandidate {
        executor: "gemini".to_string(),
        provider: "google".to_string(),
        model,
        local_vs_non_local: "non_local".to_string(),
        free_vs_paid_if_known: "not_free_quota_bound".to_string(),
        quota_model: "quota_bound".to_string(),
        remaining_quota: "unknown_until_gemini_probe".to_string(),
        rate_limit_risk: "medium".to_string(),
        monetary_or_token_cost: "quota_or_paid_usage_if_configured".to_string(),
        latency: "medium".to_string(),
        expected_quality: "high".to_string(),
        suitability_for_product_business_reasoning: "high_for_product_pm_and_business_decision_tasks".to_string(),
        fallback_risk: "interactive auth, approval or model selection must be classified as configuration failure".to_string(),
        non_interactive_requirement: "Gemini CLI must not wait for approval, model selection or auth prompts.".to_string(),
        selection_tier: quota_aware_selection_tier(chain, "gemini", 2),
        selection_status: "eligible".to_string(),
        reason: "Gemini is a non-local quota-bound capability for high-value reasoning when non-interactive mode works.".to_string(),
        capability_evidence: vec![
            "Supports --approval-mode yolo".to_string(),
            "Non-interactive --skip-trust flag available".to_string(),
        ],
    }
}

fn codex_non_local_candidate(chain: &[String]) -> SelfExecutorPolicyCandidate {
    SelfExecutorPolicyCandidate {
        executor: "codex".to_string(),
        provider: "openai".to_string(),
        model: None,
        local_vs_non_local: "non_local".to_string(),
        free_vs_paid_if_known: "not_free_quota_bound".to_string(),
        quota_model: "quota_bound".to_string(),
        remaining_quota: "unknown_until_codex_runtime".to_string(),
        rate_limit_risk: "medium".to_string(),
        monetary_or_token_cost: "quota_or_paid_usage".to_string(),
        latency: "medium".to_string(),
        expected_quality: "high".to_string(),
        suitability_for_product_business_reasoning: "high_as_reliable_fallback_for_complex_reasoning".to_string(),
        fallback_risk: "may consume scarce non-local quota and should be reserved for work where value justifies it".to_string(),
        non_interactive_requirement: "codex exec must run with approval disabled and workspace-write sandbox.".to_string(),
        selection_tier: quota_aware_selection_tier(chain, "codex", 3),
        selection_status: "eligible".to_string(),
        reason: "Codex is a reliable non-local quota-bound fallback when expected value justifies quota.".to_string(),
        capability_evidence: vec![
            "Supports --ask-for-approval never".to_string(),
            "Non-interactive --sandbox workspace-write available".to_string(),
        ],
    }
}

fn opencode_paid_non_local_candidate(_chain: &[String]) -> SelfExecutorPolicyCandidate {
    let model = std::env::var("OPENCODE_MODEL")
        .ok()
        .or_else(|| {
            std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .map(|_| "anthropic/claude-3-7-sonnet-20250219".to_string())
        })
        .or_else(|| {
            std::env::var("OPENAI_API_KEY")
                .ok()
                .map(|_| "openai/gpt-4o".to_string())
        });
    let provider = model
        .as_deref()
        .and_then(|model| model.split('/').next())
        .unwrap_or("configured_cli");
    SelfExecutorPolicyCandidate {
        executor: "opencode".to_string(),
        provider: provider.to_string(),
        model,
        local_vs_non_local: "non_local".to_string(),
        free_vs_paid_if_known: "unknown_or_paid".to_string(),
        quota_model: "quota_or_rate_limit_bound".to_string(),
        remaining_quota: "unknown_until_provider_probe".to_string(),
        rate_limit_risk: "medium".to_string(),
        monetary_or_token_cost: "provider_config_dependent".to_string(),
        latency: "medium".to_string(),
        expected_quality: "medium_high_when_configured_provider_is_strong".to_string(),
        suitability_for_product_business_reasoning:
            "medium_high_for_product_and_code_when_configured".to_string(),
        fallback_risk:
            "may consume paid quota or fail when provider auth/model configuration is missing"
                .to_string(),
        non_interactive_requirement:
            "must run through opencode run with explicit provider/model and no auth prompts"
                .to_string(),
        selection_tier: 35,
        selection_status: "eligible".to_string(),
        reason: "OpenCode non-local paid-or-unknown provider path is used after configured no-cost options and stronger non-local fallbacks are unsuitable.".to_string(),
        capability_evidence: vec![
            "Supports --model override".to_string(),
            "Requires provider/model classification before handoff".to_string(),
        ],
    }
}

fn opencode_local_candidate(chain: &[String]) -> SelfExecutorPolicyCandidate {
    SelfExecutorPolicyCandidate {
        executor: "opencode".to_string(),
        provider: "ollama".to_string(),
        model: Some(
            std::env::var("OPENCODE_LOCAL_MODEL")
                .unwrap_or_else(|_| "ollama/qwen3:14b".to_string()),
        ),
        local_vs_non_local: "local".to_string(),
        free_vs_paid_if_known: "local_runtime_no_remote_token_cost".to_string(),
        quota_model: "local_capacity_bound".to_string(),
        remaining_quota: "bounded_by_local_runtime_capacity".to_string(),
        rate_limit_risk: "low_remote_rate_limit_risk".to_string(),
        monetary_or_token_cost: "local_compute_cost".to_string(),
        latency: "variable".to_string(),
        expected_quality: "medium".to_string(),
        suitability_for_product_business_reasoning: "medium_for_repetitive_or_low_value_work_low_for_hard_pm_strategy".to_string(),
        fallback_risk: "local model may be weaker or unavailable and should not displace high-value non-local reasoning when quota is justified".to_string(),
        non_interactive_requirement: "opencode must run with explicit Ollama model and no interactive provider prompt.".to_string(),
        selection_tier: quota_aware_selection_tier(chain, "opencode", 4),
        selection_status: "eligible".to_string(),
        reason: "OpenCode local/Ollama is efficient when quotas are low, work is cheap or privacy/locality matters.".to_string(),
        capability_evidence: vec![
            "Supports local Ollama provider".to_string(),
            "Non-interactive --model override available".to_string(),
        ],
    }
}

fn execute_cycle_with_fallback(
    store: &ForgeStore,
    repo: &Path,
    primary_executor: &str,
    fallback_executors: &[String],
    previous_reports: &[SelfCycleReport],
    prompt: &str,
) -> Result<SelfExecutorExecution> {
    let mut attempts = Vec::new();
    let strategies = select_executor_strategies(
        store,
        primary_executor,
        fallback_executors,
        previous_reports,
    );
    let mut errors = Vec::new();

    if strategies.is_empty() {
        bail!(
            "no suitable executor strategy found for {} with fallbacks {}",
            primary_executor,
            fallback_executors.join(", ")
        );
    }

    for strategy in strategies {
        match execute_cycle(repo, &strategy, prompt) {
            Ok(status) => {
                attempts.push(SelfExecutorAttempt {
                    executor: strategy.executor.clone(),
                    provider: strategy.provider.clone(),
                    model: strategy.model.clone(),
                    local: strategy.local,
                    quota_model: strategy.quota_model.clone(),
                    cost_model: strategy.cost_model.clone(),
                    remaining_quota: strategy.remaining_quota.clone(),
                    rate_limit_risk: strategy.rate_limit_risk.clone(),
                    monetary_or_token_cost: strategy.monetary_or_token_cost.clone(),
                    expected_quality: strategy.expected_quality.clone(),
                    fallback_risk: strategy.fallback_risk.clone(),
                    selection_tier: strategy.selection_tier,
                    status: "completed".to_string(),
                    reason: strategy.reason.clone(),
                    error: None,
                });
                return Ok(SelfExecutorExecution {
                    executor: strategy.executor,
                    status,
                    attempts,
                });
            }
            Err(error) => {
                let error_msg = error.to_string();
                errors.push(format!("{}: {}", strategy.executor, error_msg));
                attempts.push(SelfExecutorAttempt {
                    executor: strategy.executor.clone(),
                    provider: strategy.provider.clone(),
                    model: strategy.model.clone(),
                    local: strategy.local,
                    quota_model: strategy.quota_model.clone(),
                    cost_model: strategy.cost_model.clone(),
                    remaining_quota: strategy.remaining_quota.clone(),
                    rate_limit_risk: strategy.rate_limit_risk.clone(),
                    monetary_or_token_cost: strategy.monetary_or_token_cost.clone(),
                    expected_quality: strategy.expected_quality.clone(),
                    fallback_risk: strategy.fallback_risk.clone(),
                    selection_tier: strategy.selection_tier,
                    status: "failed".to_string(),
                    reason: strategy.reason.clone(),
                    error: Some(error_msg),
                });
            }
        }
    }

    bail!("all self-evolution executors failed: {}", errors.join("; "))
}

fn execute_cycle(repo: &Path, strategy: &SelfExecutorStrategy, prompt: &str) -> Result<String> {
    match strategy.executor.as_str() {
        "codex" => {
            let output = execute_command_capture(
                "codex",
                &[
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
                ],
                repo,
                None,
            )?;
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
            let mut args = vec![
                "run".to_string(),
                "--dir".to_string(),
                repo.to_str().unwrap_or(".").to_string(),
                "--title".to_string(),
                "Forge self evolution".to_string(),
                "--dangerously-skip-permissions".to_string(),
            ];

            if let Some(model) = &strategy.model {
                args.push("--model".to_string());
                args.push(model.clone());
            }

            args.push(prompt.to_string());
            let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

            let output = execute_command_capture(
                "opencode",
                &args_ref,
                repo,
                Some(OPENCODE_TIMEOUT_SECONDS),
            )?;
            if output.status.success() {
                Ok("executor_completed".to_string())
            } else {
                bail!(
                    "opencode executor failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            }
        }
        "gemini" => {
            let mut args = vec![
                "-p".to_string(),
                prompt.to_string(),
                "--approval-mode".to_string(),
                "yolo".to_string(),
                "--skip-trust".to_string(),
                "--output-format".to_string(),
                "text".to_string(),
            ];

            if let Some(model) = &strategy.model {
                args.push("--model".to_string());
                args.push(model.clone());
            }

            let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

            let output =
                execute_command_capture("gemini", &args_ref, repo, Some(EXECUTOR_TIMEOUT_SECONDS))?;
            fs::create_dir_all(repo.join(".forge"))?;
            let response = String::from_utf8_lossy(&output.stdout);
            fs::write(
                repo.join(".forge/last-gemini-self-evolution.md"),
                response.as_bytes(),
            )?;
            if output.status.success() {
                Ok("executor_completed".to_string())
            } else {
                bail!(
                    "gemini executor failed: {}{}",
                    String::from_utf8_lossy(&output.stderr),
                    response
                )
            }
        }
        other => bail!("unsupported self-evolution executor: {other}"),
    }
}

fn execute_command_capture(
    program: &str,
    args: &[&str],
    repo: &Path,
    timeout_seconds: Option<u64>,
) -> Result<std::process::Output> {
    if !command_available(program) {
        bail!("executor `{program}` binary not found in PATH");
    }

    let (cmd, command_args): (&str, Vec<String>) = if let Some(timeout_seconds) = timeout_seconds {
        if command_available("timeout") {
            let mut command_args = Vec::with_capacity(args.len() + 2);
            command_args.push(format!("{timeout_seconds}s"));
            command_args.push(program.to_string());
            command_args.extend(args.iter().map(|arg| arg.to_string()));
            ("timeout", command_args)
        } else {
            (program, args.iter().map(|arg| arg.to_string()).collect())
        }
    } else {
        (program, args.iter().map(|arg| arg.to_string()).collect())
    };

    let mut command = Command::new(cmd);
    command.current_dir(repo);
    command.args(command_args.iter().map(String::as_str));
    command.stdin(std::process::Stdio::null());
    let output = command.output().map_err(|error| {
        if error.kind() == ErrorKind::NotFound && cmd == "timeout" {
            anyhow::anyhow!("`timeout` command not found in PATH")
        } else {
            error.into()
        }
    })?;
    if let Some(timeout_seconds) = timeout_seconds {
        if !output.status.success() && output.status.code() == Some(124) {
            bail!(
                "executor `{program}` timed out after {timeout_seconds}s (likely waiting for interactive completion)"
            );
        }
    }
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!("{program} command failed: {stderr}{stdout}")
    }
}

fn command_available(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }

    if command.contains('/') {
        return fs::metadata(command).is_ok_and(|meta| meta.is_file());
    }

    if let Some(path) = std::env::var_os("PATH") {
        return std::env::split_paths(&path).any(|dir| {
            let candidate = dir.join(command);
            candidate.exists()
        });
    }
    false
}

fn normalize_self_executor_fallbacks(
    primary_executors: &[String],
    fallback_executors: &[String],
) -> Vec<String> {
    let mut normalized = Vec::new();
    for fallback in fallback_executors {
        let fallback = fallback.trim();
        if fallback.is_empty()
            || primary_executors
                .iter()
                .any(|executor| executor.trim() == fallback)
            || normalized.iter().any(|existing| existing == fallback)
        {
            continue;
        }
        normalized.push(fallback.to_string());
    }
    normalized
}

fn executor_fallback_chain_for_requested(
    requested_executor: &str,
    all_executors: &[String],
    explicit_fallbacks: &[String],
) -> Vec<String> {
    if !explicit_fallbacks.is_empty() {
        return normalize_self_executor_fallbacks(
            &[requested_executor.to_string()],
            explicit_fallbacks,
        );
    }

    all_executors
        .iter()
        .filter(|executor| executor.as_str() != requested_executor)
        .cloned()
        .collect()
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

fn run_self_update(repo: &Path, options: &SelfRunOptions) -> Result<SelfUpdateReport> {
    let command = effective_self_update_command(options);
    if options.skip_self_update {
        return Ok(SelfUpdateReport::skipped_for(
            command,
            "self update disabled",
        ));
    }

    let (program, args) = split_command(&command)?;
    run_program(repo, program, &args).context("failed to run self-update command")?;
    Ok(SelfUpdateReport::completed_for(command))
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

fn effective_validation_commands(options: &SelfRunOptions) -> Vec<String> {
    if options.validation_commands.is_empty() {
        VALIDATION_COMMANDS
            .iter()
            .map(|command| command.to_string())
            .collect()
    } else {
        options.validation_commands.clone()
    }
}

fn effective_self_update_command(options: &SelfRunOptions) -> Vec<String> {
    options
        .self_update_command
        .as_deref()
        .map(shell_words)
        .unwrap_or_else(self_update_command)
}

fn split_command(command: &[String]) -> Result<(&str, Vec<&str>)> {
    let Some((program, args)) = command.split_first() else {
        bail!("self-update command cannot be empty");
    };
    Ok((program.as_str(), args.iter().map(String::as_str).collect()))
}

fn shell_words(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|part| part.to_string())
        .collect()
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

    fn test_store() -> (tempfile::TempDir, ForgeStore) {
        let temp = tempfile::tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        (temp, store)
    }

    fn test_internal_loop_report() -> SelfEvolutionLoopReport {
        SelfEvolutionLoopReport {
            schema_version: "forge.self_evolution.loop.v1".to_string(),
            loop_count: 1,
            loop_task_id: "task-loop".to_string(),
            loop_control_kind: "while_until".to_string(),
            execution_shape: "ordinary_forge_workflow_internal_recurring_loop".to_string(),
            sleep_seconds: 180,
            sleep_policy: "rest_between_iterations".to_string(),
            next_goal_decision: SelfNextGoalDecisionReport {
                schema_version: "forge.self_evolution.next_goal_decision.v1".to_string(),
                decision_id: "decision-test".to_string(),
                selected_goal: "Make the Product/PM CLI-TUI the main entry point.".to_string(),
                source: "autonomous_v0_5_priority_selection".to_string(),
                rationale: "Test product/business rationale.".to_string(),
                alternatives: Vec::new(),
                trade_offs: Vec::new(),
                success_metrics: Vec::new(),
                backlog_mutation: "persisted_as_product_decision_for_next_iteration".to_string(),
                workflow_revision: 1,
            },
        }
    }

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
        let (_temp, store) = test_store();
        let r1 = SelfCycleReport {
            cycle: 1,
            requested_executor: "test".to_string(),
            executor: "test".to_string(),
            executor_fallbacks: Vec::new(),
            executor_attempts: Vec::new(),
            status: "completed".to_string(),
            executor_policy: build_executor_policy(&store, "test", &[], &[]),
            prompt_path: "p1.md".to_string(),
            prompt_packet_version: "v1".to_string(),
            prompt_sha256: "a".to_string(),
            validation_report_path: "v1.json".to_string(),
            validation_report_sha256: "b".to_string(),
            report_path: "r1.json".to_string(),
            markdown_report_path: "r1.md".to_string(),
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
            requested_executor: "test".to_string(),
            executor: "test".to_string(),
            executor_fallbacks: Vec::new(),
            executor_attempts: Vec::new(),
            status: "completed".to_string(),
            executor_policy: build_executor_policy(&store, "test", &[], &[]),
            prompt_path: "p2.md".to_string(),
            prompt_packet_version: "v1".to_string(),
            prompt_sha256: "c".to_string(),
            validation_report_path: "v2.json".to_string(),
            validation_report_sha256: "d".to_string(),
            report_path: "r2.json".to_string(),
            markdown_report_path: "r2.md".to_string(),
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
            product_decisions: vec![],
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
            product_decisions: vec![],
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

    #[test]
    fn test_analyze_goal_capabilities_empty_goal() {
        let caps = analyze_goal_capabilities("");
        assert!(!caps.is_empty());
        for c in &caps {
            assert!(!c.present_in_goal);
        }
    }

    #[test]
    fn test_analyze_goal_capabilities_detects_critical() {
        let goal =
            "cron schedule interactive forge cli creative runtime whiteboard scan_due next_run_at";
        let caps = analyze_goal_capabilities(goal);
        let cron = caps.iter().find(|c| c.name == "cron / schedule").unwrap();
        assert!(cron.present_in_goal);
        assert_eq!(cron.priority, "critical");
        let interactive = caps
            .iter()
            .find(|c| c.name == "interactive forge CLI")
            .unwrap();
        assert!(interactive.present_in_goal);
        assert_eq!(interactive.priority, "critical");
        let creative = caps.iter().find(|c| c.name == "creative runtime").unwrap();
        assert!(creative.present_in_goal);
    }

    #[test]
    fn test_analyze_goal_capabilities_detects_high() {
        let goal = "context routing engine mcp skill integration scheduler loop subflow";
        let caps = analyze_goal_capabilities(goal);
        let ctx = caps
            .iter()
            .find(|c| c.name == "context routing engine")
            .unwrap();
        assert!(ctx.present_in_goal);
        assert_eq!(ctx.priority, "high");
        let mcp = caps
            .iter()
            .find(|c| c.name == "MCP / skill integration")
            .unwrap();
        assert!(mcp.present_in_goal);
        let scheduler = caps
            .iter()
            .find(|c| c.name == "scheduler / loop / subflow")
            .unwrap();
        assert!(scheduler.present_in_goal);
    }

    #[test]
    fn test_analyze_goal_capabilities_detects_medium() {
        let goal = "design token design system list inspect milestone promote telegram notification execution policy";
        let caps = analyze_goal_capabilities(goal);
        let tokens = caps.iter().find(|c| c.name == "design tokens").unwrap();
        assert!(tokens.present_in_goal);
        assert_eq!(tokens.priority, "medium");
        let listing = caps
            .iter()
            .find(|c| c.name == "workflow listing & inspect")
            .unwrap();
        assert!(listing.present_in_goal);
        let telegram = caps
            .iter()
            .find(|c| c.name == "telegram / notification")
            .unwrap();
        assert!(telegram.present_in_goal);
        let policy = caps.iter().find(|c| c.name == "execution policy").unwrap();
        assert!(policy.present_in_goal);
    }

    #[test]
    fn test_executor_fallback_chain_uses_explicit_fallback_executors() {
        let executors = vec![
            "opencode".to_string(),
            "gemini".to_string(),
            "codex".to_string(),
        ];
        let explicit = vec!["codex".to_string(), "opencode".to_string()];
        let resolved = executor_fallback_chain_for_requested("gemini", &executors, &explicit);
        assert_eq!(resolved, vec!["codex", "opencode"]);
    }

    #[test]
    fn test_executor_fallback_chain_defaults_to_remaining_executors_in_schedule_order() {
        let executors = vec![
            "opencode".to_string(),
            "gemini".to_string(),
            "codex".to_string(),
        ];
        let resolved = executor_fallback_chain_for_requested("gemini", &executors, &[]);
        assert_eq!(resolved, vec!["opencode", "codex"]);
    }

    #[test]
    fn test_executor_policy_prefers_non_local_quota_aware_capabilities_for_self_evolution() {
        let (_temp, store) = test_store();
        let policy = build_executor_policy(
            &store,
            "codex",
            &["opencode".to_string(), "gemini".to_string()],
            &[],
        );

        let ordered: Vec<_> = policy
            .candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.executor.as_str(),
                    candidate.provider.as_str(),
                    candidate.local_vs_non_local.as_str(),
                )
            })
            .collect();

        assert_eq!(
            ordered,
            vec![
                ("opencode", "google", "non_local"),
                ("gemini", "google", "non_local"),
                ("codex", "openai", "non_local"),
                ("opencode", "configured_cli", "non_local"),
                ("opencode", "ollama", "local"),
            ]
        );
        assert!(policy.selection_principle.contains("expected value"));
        assert!(policy
            .decision_factors
            .iter()
            .any(|factor| factor == "remaining_quota_if_available"));
        assert!(policy
            .decision_factors
            .iter()
            .any(|factor| factor == "monetary_or_token_cost"));
        assert!(policy
            .skipped_to_preserve_quota
            .iter()
            .any(|entry| entry.contains("low-value")));
    }

    #[test]
    fn test_executor_strategy_preserves_quota_cost_fields_for_attempt_reports() {
        let (_temp, store) = test_store();
        let strategies = select_executor_strategies(
            &store,
            "codex",
            &["opencode".to_string(), "gemini".to_string()],
            &[],
        );

        let opencode = strategies
            .iter()
            .find(|strategy| strategy.executor == "opencode" && !strategy.local)
            .unwrap();

        assert_eq!(opencode.remaining_quota, "unknown_until_provider_probe");
        assert_eq!(opencode.rate_limit_risk, "medium");
        assert_eq!(
            opencode.monetary_or_token_cost,
            "provider_configured_no_cost"
        );
        assert!(opencode.expected_quality.contains("high"));
        assert!(opencode.fallback_risk.contains("provider auth"));
    }

    #[test]
    fn test_render_capability_breakdown_empty_is_empty() {
        let caps: Vec<ForgeCapability> = Vec::new();
        let packet = SelfEvolutionPromptPacket {
            version: "v2".to_string(),
            cycle: 1,
            executor: "test".to_string(),
            executor_fallbacks: Vec::new(),
            workflow_id: "wf-1".to_string(),
            run_id: "run-1".to_string(),
            workflow_goal: "".to_string(),
            initial_workflow_goal: "".to_string(),
            workflow_revision: 0,
            stop_at: "2030-01-01T00:00:00Z".to_string(),
            repo: "/tmp".to_string(),
            operating_mode: "balanced".to_string(),
            decision_gate: SelfDecisionGateReport::evaluate("", &SelfOperatingMode::Balanced),
            internal_loop: test_internal_loop_report(),
            validation_commands: vec![],
            capability_analysis: caps,
        };
        let rendered = render_capability_breakdown(&packet);
        assert!(rendered.is_empty());
    }

    #[test]
    fn test_executor_policy_detects_timeout_and_requires_repair() {
        let (_temp, store) = test_store();
        let timeout_report = SelfCycleReport {
            cycle: 1,
            requested_executor: "gemini".to_string(),
            executor: "gemini".to_string(),
            executor_fallbacks: Vec::new(),
            executor_attempts: vec![SelfExecutorAttempt {
                executor: "gemini".to_string(),
                provider: Some("google".to_string()),
                model: Some("gemini-2.5-pro".to_string()),
                local: false,
                quota_model: "quota_bound".to_string(),
                cost_model: "paid".to_string(),
                remaining_quota: "unknown_until_gemini_probe".to_string(),
                rate_limit_risk: "medium".to_string(),
                monetary_or_token_cost: "quota_or_paid_usage_if_configured".to_string(),
                expected_quality: "high".to_string(),
                fallback_risk: "interactive auth/model prompt".to_string(),
                selection_tier: 20,
                status: "failed".to_string(),
                reason: "interactive timeout".to_string(),
                error: Some("executor `gemini` timed out after 180s (likely waiting for interactive completion)".to_string()),
            }],
            status: "failed".to_string(),
            executor_policy: build_executor_policy(&store, "gemini", &[], &[]),
            prompt_path: "p1.md".to_string(),
            prompt_packet_version: "v2".to_string(),
            prompt_sha256: "sha".to_string(),
            validation_report_path: "v1.json".to_string(),
            validation_report_sha256: "sha".to_string(),
            report_path: "r1.json".to_string(),
            markdown_report_path: "r1.md".to_string(),
            validation_passed: false,
            overhead_ledger: SelfOverheadLedger::empty(&SelfOperatingMode::Balanced),
            decision_gate: SelfDecisionGateReport::evaluate("", &SelfOperatingMode::Balanced),
            self_update: SelfUpdateReport::skipped_for(vec![], "test"),
            committed: false,
            commit: None,
            public_project_update: PublicProjectUpdateReport::skipped(false, "test"),
        };

        let policy = build_executor_policy(&store, "gemini", &[], &[timeout_report]);

        assert_eq!(
            policy.active_repair_status,
            "repair_needed_timeout_detected"
        );
        assert!(policy
            .repair_goals
            .iter()
            .any(|g| g.contains("Urgent: Fix interactive timeout")));

        let gemini_candidate = policy
            .candidates
            .iter()
            .find(|c| c.executor == "gemini")
            .unwrap();
        assert_eq!(gemini_candidate.selection_status, "failed_timeout");
        assert!(gemini_candidate
            .reason
            .contains("Previously failed due to timeout"));
    }
}
