use crate::artifact::write_json_artifact;
use crate::graph::{ExecutorKind, TaskStatus, Workflow};
use crate::outcome::{assess_workflow_outcome_metadata, OutcomeStatusReport};
use crate::request::{build_run_activity, RunActivity, RunRecord};
use crate::scheduler::{plan_parallel_execution, ParallelSchedulePlan};
use crate::storage::ForgeStore;
use crate::validation::validate_workflow;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

const IMPROVEMENT_CANDIDATES_SCHEMA_VERSION: &str = "forge.orchestrator_improvement_candidates.v1";

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementProposal {
    pub workflow_id: String,
    pub status: String,
    pub auto_promoted: bool,
    pub promotion_gate: String,
    pub target_version: String,
    pub artifact_path: String,
    pub changelog_path: String,
    pub candidate_changes: Vec<String>,
    pub evolution_domains: Vec<String>,
    pub metrics_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrchestratorImprovementCandidatesReport {
    pub status: String,
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub total_workflows: usize,
    pub candidate_count: usize,
    pub selection_policy: Vec<String>,
    pub candidates: Vec<OrchestratorImprovementCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrchestratorImprovementCandidate {
    pub workflow_id: String,
    pub goal: String,
    pub workflow_status: String,
    pub score: i64,
    pub priority: String,
    pub recommended_action: String,
    pub reasons: Vec<ImprovementCandidateReason>,
    pub evidence: ImprovementCandidateEvidence,
    pub parallelization: ParallelizationOpportunityReport,
    pub cost_efficiency: CostEfficiencyReport,
    pub outcome_status: OutcomeStatusReport,
    pub active_runs: Vec<ImprovementRunEvidence>,
    pub latest_events: Vec<ImprovementEventSummary>,
    pub suggested_commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementCandidateReason {
    pub code: String,
    pub severity: String,
    pub score: i64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementCandidateEvidence {
    pub event_count: usize,
    pub latest_event_at: Option<String>,
    pub rework_event_count: usize,
    pub heartbeat_event_count: usize,
    pub final_delivery_package_count: usize,
    pub pending_task_count: usize,
    pub running_task_count: usize,
    pub blocked_task_count: usize,
    pub failed_task_count: usize,
    pub completed_task_count: usize,
    pub run_count: usize,
    pub active_run_count: usize,
    pub stale_run_count: usize,
    pub needs_attention_run_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParallelizationOpportunityReport {
    pub schema_version: String,
    pub parallel_opportunity: bool,
    pub ready_parallel_task_count: usize,
    pub ready_parallel_task_ids: Vec<String>,
    pub ready_parallel_task_titles: Vec<String>,
    pub total_waves: usize,
    pub parallel_wave_count: usize,
    pub max_parallel_width: usize,
    pub latency_reduction_bps: u32,
    pub recommended_max_parallelism: usize,
    pub policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostEfficiencyReport {
    pub schema_version: String,
    pub ai_task_count: usize,
    pub repetitive_or_deterministic_ai_task_count: usize,
    pub repetitive_or_deterministic_ai_task_ids: Vec<String>,
    pub repetitive_or_deterministic_ai_task_titles: Vec<String>,
    pub estimated_ai_cost_total_usd: f64,
    pub estimated_ai_cost_average_usd: f64,
    pub observed_ai_cost_total_usd: Option<f64>,
    pub observed_ai_cost_average_usd: Option<f64>,
    pub avoidable_estimated_cost_usd: f64,
    pub avoidable_estimated_cost_average_usd: f64,
    pub avoidable_observed_cost_total_usd: Option<f64>,
    pub avoidable_observed_cost_average_usd: Option<f64>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementRunEvidence {
    pub run_id: String,
    pub status: String,
    pub heartbeat_status: String,
    pub active: bool,
    pub executor: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub summary: Option<String>,
    pub recovery_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementEventSummary {
    pub kind: String,
    pub created_at: String,
}

pub fn rank_improvement_candidates(
    store: &ForgeStore,
    limit: usize,
) -> Result<OrchestratorImprovementCandidatesReport> {
    let workflows = store.load_workflows()?;
    let total_workflows = workflows.len();
    let runs = store
        .load_runs()?
        .into_iter()
        .filter_map(|value| serde_json::from_value::<RunRecord>(value).ok())
        .collect::<Vec<_>>();
    let mut runs_by_workflow: BTreeMap<String, Vec<RunRecord>> = BTreeMap::new();
    for run in runs {
        runs_by_workflow
            .entry(run.workflow_id.clone())
            .or_default()
            .push(run);
    }

    let mut candidates = Vec::new();
    for workflow in workflows {
        let workflow_runs = runs_by_workflow
            .get(&workflow.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if let Some(candidate) = build_improvement_candidate(store, &workflow, workflow_runs)? {
            candidates.push(candidate);
        }
    }

    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then(left.workflow_id.cmp(&right.workflow_id))
    });
    if limit > 0 && candidates.len() > limit {
        candidates.truncate(limit);
    }

    Ok(OrchestratorImprovementCandidatesReport {
        status: "loaded".to_string(),
        schema_version: IMPROVEMENT_CANDIDATES_SCHEMA_VERSION.to_string(),
        generated_at: Utc::now(),
        total_workflows,
        candidate_count: candidates.len(),
        selection_policy: vec![
            "prefer live workflows with stale or missing runtime signals".to_string(),
            "prefer workflows with failed, blocked or retrying tasks before cosmetic improvement"
                .to_string(),
            "surface parallel-ready task sets when dependencies and context allow multiple handoffs"
                .to_string(),
            "penalize support-only completion when the user asked for final outcomes".to_string(),
        ],
        candidates,
    })
}

fn build_improvement_candidate(
    store: &ForgeStore,
    workflow: &Workflow,
    runs: &[RunRecord],
) -> Result<Option<OrchestratorImprovementCandidate>> {
    let events = store.load_workflow_events(&workflow.id)?;
    let outcome_status = assess_workflow_outcome_metadata(workflow);
    let parallel_plan = plan_parallel_execution(workflow);
    let parallelization = build_parallelization_report(workflow, &parallel_plan);
    let cost_efficiency = build_cost_efficiency_report(workflow, &events);
    let latest_events = latest_event_summaries(&events);
    let rework_event_count = events.iter().filter(|event| is_rework_event(event)).count();
    let heartbeat_event_count = events
        .iter()
        .filter(|event| event.kind == "async_request_heartbeat")
        .count();
    let final_delivery_package_count = events
        .iter()
        .filter(|event| event.kind == "final_delivery_package_created")
        .count();
    let task_counts = count_tasks(workflow);
    let run_evidence = runs
        .iter()
        .map(run_evidence)
        .collect::<Vec<ImprovementRunEvidence>>();
    let active_run_count = run_evidence.iter().filter(|run| run.active).count();
    let stale_run_count = run_evidence
        .iter()
        .filter(|run| run.heartbeat_status == "stale")
        .count();
    let missing_heartbeat_count = run_evidence
        .iter()
        .filter(|run| run.status == "running" && run.heartbeat_status == "missing")
        .count();
    let needs_attention_run_count = run_evidence
        .iter()
        .filter(|run| run.status == "needs_attention")
        .count();

    let evidence = ImprovementCandidateEvidence {
        event_count: events.len(),
        latest_event_at: events.last().map(|event| event.created_at.clone()),
        rework_event_count,
        heartbeat_event_count,
        final_delivery_package_count,
        pending_task_count: task_counts.pending,
        running_task_count: task_counts.running,
        blocked_task_count: task_counts.blocked,
        failed_task_count: task_counts.failed,
        completed_task_count: task_counts.completed,
        run_count: runs.len(),
        active_run_count,
        stale_run_count,
        needs_attention_run_count,
    };

    let mut score = 0;
    let mut reasons = Vec::new();
    if needs_attention_run_count > 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "run_needs_attention",
            "critical",
            95,
            format!(
                "{needs_attention_run_count} run(s) already need attention; the orchestrator should resume, cancel or inspect before continuing."
            ),
        );
    }
    if stale_run_count > 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "stale_running_run",
            "critical",
            90,
            format!(
                "{stale_run_count} running run(s) have stale heartbeats; runtime state is no longer trustworthy without recovery."
            ),
        );
    }
    if missing_heartbeat_count > 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "missing_runtime_heartbeat",
            "high",
            55,
            format!(
                "{missing_heartbeat_count} running run(s) have no heartbeat; the orchestrator lacks enough live logs."
            ),
        );
    }
    if task_counts.failed > 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "failed_tasks",
            "critical",
            85 + (task_counts.failed as i64 * 5),
            format!(
                "{} task(s) failed; improvement should target the failing branch before expanding scope.",
                task_counts.failed
            ),
        );
    }
    if task_counts.blocked > 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "blocked_tasks",
            "high",
            70 + (task_counts.blocked as i64 * 5),
            format!(
                "{} task(s) are blocked; the orchestrator should repair dependencies, context or human gates.",
                task_counts.blocked
            ),
        );
    }
    if rework_event_count > 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "rework_loop_signal",
            "high",
            45 + (rework_event_count.min(4) as i64 * 10),
            format!("{rework_event_count} retry/rework event(s) were found in workflow logs."),
        );
    }
    if parallelization.ready_parallel_task_count >= 2 {
        push_reason(
            &mut reasons,
            &mut score,
            "parallelization_opportunity",
            "high",
            45,
            format!(
                "{} pending task(s) are ready at the same time; start parallel handoffs within quota/resource limits.",
                parallelization.ready_parallel_task_count
            ),
        );
    } else if parallelization.parallel_opportunity {
        push_reason(
            &mut reasons,
            &mut score,
            "dag_parallelization_available",
            "medium",
            20,
            "The DAG has independent waves that can run concurrently when dependencies unlock."
                .to_string(),
        );
    }
    if cost_efficiency.repetitive_or_deterministic_ai_task_count > 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "avoidable_ai_cost",
            "medium",
            30 + (cost_efficiency
                .repetitive_or_deterministic_ai_task_count
                .min(5) as i64
                * 5),
            format!(
                "{} AI task(s) look repetitive or deterministic; average avoidable estimated AI cost per execution is ${:.6} and avoidable estimated cost is ${:.6}.",
                cost_efficiency.repetitive_or_deterministic_ai_task_count,
                cost_efficiency.avoidable_estimated_cost_average_usd,
                cost_efficiency.avoidable_estimated_cost_usd
            ),
        );
    }
    match outcome_status.status.as_str() {
        "needs_user_delivery_evidence" => push_reason(
            &mut reasons,
            &mut score,
            "missing_user_delivery_evidence",
            "high",
            50,
            outcome_status.reason.clone(),
        ),
        "needs_final_outcome_audit" | "needs_final_outcome_audit_evaluation" => push_reason(
            &mut reasons,
            &mut score,
            "missing_final_outcome_audit",
            "high",
            45,
            outcome_status.reason.clone(),
        ),
        "support_only" => push_reason(
            &mut reasons,
            &mut score,
            "support_only_output_risk",
            "medium",
            25,
            outcome_status.reason.clone(),
        ),
        _ => {}
    }
    if workflow.status == "completed" && final_delivery_package_count == 0 {
        push_reason(
            &mut reasons,
            &mut score,
            "completed_without_final_package",
            "medium",
            35,
            "Workflow is completed but has no final delivery package event for user handoff."
                .to_string(),
        );
    }
    if workflow.status == "running"
        && events.len() >= 25
        && task_counts.completed < workflow.tasks.len()
    {
        push_reason(
            &mut reasons,
            &mut score,
            "high_log_volume_without_completion",
            "medium",
            20,
            "Workflow has substantial runtime logs without reaching completion; inspect for loop or scope drift."
                .to_string(),
        );
    }

    if score <= 0 {
        return Ok(None);
    }

    let suggested_commands = suggested_commands(workflow, runs, &reasons, &parallelization);

    Ok(Some(OrchestratorImprovementCandidate {
        workflow_id: workflow.id.clone(),
        goal: workflow.goal.clone(),
        workflow_status: workflow.status.clone(),
        score,
        priority: priority_for_score(score),
        recommended_action: recommended_action(&reasons),
        reasons,
        evidence,
        parallelization,
        cost_efficiency,
        outcome_status,
        active_runs: run_evidence,
        latest_events,
        suggested_commands,
    }))
}

fn push_reason(
    reasons: &mut Vec<ImprovementCandidateReason>,
    score: &mut i64,
    code: &str,
    severity: &str,
    points: i64,
    summary: String,
) {
    *score += points;
    reasons.push(ImprovementCandidateReason {
        code: code.to_string(),
        severity: severity.to_string(),
        score: points,
        summary,
    });
}

fn run_evidence(run: &RunRecord) -> ImprovementRunEvidence {
    let activity: RunActivity = build_run_activity(run);
    ImprovementRunEvidence {
        run_id: run.run_id.clone(),
        status: run.status.clone(),
        heartbeat_status: activity.heartbeat_status,
        active: activity.active,
        executor: activity.executor,
        updated_at: run.updated_at,
        summary: run.progress_summary.clone(),
        recovery_action: activity.recovery.action,
    }
}

fn build_parallelization_report(
    workflow: &Workflow,
    plan: &ParallelSchedulePlan,
) -> ParallelizationOpportunityReport {
    let (ready_parallel_task_ids, ready_parallel_task_titles) = ready_parallel_tasks(workflow);
    let ready_parallel_task_count = ready_parallel_task_ids.len();
    let parallel_wave_count = plan.waves.iter().filter(|wave| wave.concurrent).count();
    let max_parallel_width = plan
        .waves
        .iter()
        .map(|wave| wave.task_count)
        .max()
        .unwrap_or(0);
    let recommended_max_parallelism = ready_parallel_task_count
        .max(max_parallel_width)
        .clamp(1, 8);
    let parallel_opportunity = ready_parallel_task_count >= 2 || plan.parallel_opportunity;
    let policy = if ready_parallel_task_count >= 2 {
        "start_all_ready_handoffs_with_quota_and_resource_guard".to_string()
    } else if plan.parallel_opportunity {
        "use_wave_plan_when_dependencies_unlock".to_string()
    } else {
        "sequential_execution_expected".to_string()
    };

    ParallelizationOpportunityReport {
        schema_version: "forge.improve.parallelization_opportunity.v1".to_string(),
        parallel_opportunity,
        ready_parallel_task_count,
        ready_parallel_task_ids,
        ready_parallel_task_titles,
        total_waves: plan.total_waves,
        parallel_wave_count,
        max_parallel_width,
        latency_reduction_bps: plan.latency_reduction_bps,
        recommended_max_parallelism,
        policy,
    }
}

fn build_cost_efficiency_report(
    workflow: &Workflow,
    events: &[crate::storage::StoreEvent],
) -> CostEfficiencyReport {
    let ai_tasks = workflow
        .tasks
        .iter()
        .filter(|task| task.executor == ExecutorKind::Ai)
        .collect::<Vec<_>>();
    let ai_task_count = ai_tasks.len();
    let estimated_ai_cost_total_usd = ai_tasks
        .iter()
        .map(|task| task.cost.estimated_cost_usd)
        .sum::<f64>();
    let estimated_ai_cost_average_usd = average_cost(estimated_ai_cost_total_usd, ai_task_count);
    let repetitive_ai_tasks = ai_tasks
        .iter()
        .copied()
        .filter(|task| looks_repetitive_or_deterministic_ai_task(task))
        .collect::<Vec<_>>();
    let avoidable_estimated_cost_usd = repetitive_ai_tasks
        .iter()
        .map(|task| task.cost.estimated_cost_usd)
        .sum::<f64>();
    let avoidable_estimated_cost_average_usd =
        average_cost(avoidable_estimated_cost_usd, repetitive_ai_tasks.len());
    let observed_costs = events
        .iter()
        .filter_map(observed_ai_cost_from_event)
        .collect::<Vec<_>>();
    let observed_ai_cost_total_usd =
        (!observed_costs.is_empty()).then(|| observed_costs.iter().sum());
    let observed_ai_cost_average_usd =
        observed_ai_cost_total_usd.map(|total| average_cost(total, observed_costs.len()));
    let repetitive_task_ids = repetitive_ai_tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    let avoidable_observed_costs = events
        .iter()
        .filter(|event| {
            event_task_id(event).is_some_and(|task_id| repetitive_task_ids.contains(task_id))
        })
        .filter_map(observed_ai_cost_from_event)
        .collect::<Vec<_>>();
    let avoidable_observed_cost_total_usd =
        (!avoidable_observed_costs.is_empty()).then(|| avoidable_observed_costs.iter().sum());
    let avoidable_observed_cost_average_usd = avoidable_observed_cost_total_usd
        .map(|total| average_cost(total, avoidable_observed_costs.len()));
    let recommendation = if repetitive_ai_tasks.is_empty() {
        "keep_current_executor_mix".to_string()
    } else {
        "convert_repetitive_ai_tasks_to_command_nodes_or_reusable_subflows".to_string()
    };

    CostEfficiencyReport {
        schema_version: "forge.improve.cost_efficiency.v1".to_string(),
        ai_task_count,
        repetitive_or_deterministic_ai_task_count: repetitive_ai_tasks.len(),
        repetitive_or_deterministic_ai_task_ids: repetitive_ai_tasks
            .iter()
            .map(|task| task.id.clone())
            .collect(),
        repetitive_or_deterministic_ai_task_titles: repetitive_ai_tasks
            .iter()
            .map(|task| task.title.clone())
            .collect(),
        estimated_ai_cost_total_usd,
        estimated_ai_cost_average_usd,
        observed_ai_cost_total_usd,
        observed_ai_cost_average_usd,
        avoidable_estimated_cost_usd,
        avoidable_estimated_cost_average_usd,
        avoidable_observed_cost_total_usd,
        avoidable_observed_cost_average_usd,
        recommendation,
    }
}

fn looks_repetitive_or_deterministic_ai_task(task: &crate::graph::AtomicTask) -> bool {
    let text = format!("{} {} {}", task.title, task.goal, task.expected_output).to_lowercase();
    let deterministic_keywords = [
        "repeated",
        "repetitive",
        "frequent",
        "daily",
        "cron",
        "scheduled",
        "calculate",
        "calcular",
        "calculation",
        "cost",
        "custo",
        "format",
        "parse",
        "extract",
        "transform",
        "copy",
        "seed",
        "sync",
        "inventory",
        "export",
        "import",
        "csv",
        "json",
    ];
    task.schedule.is_some()
        || task.execution_policy.deterministic
        || deterministic_keywords
            .iter()
            .any(|keyword| text.contains(keyword))
}

fn observed_ai_cost_from_event(event: &crate::storage::StoreEvent) -> Option<f64> {
    event
        .data
        .get("cost")
        .and_then(|cost| cost.get("estimated_usd"))
        .or_else(|| {
            event
                .data
                .get("executor_cost")
                .and_then(|cost| cost.get("estimated_usd"))
        })
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn event_task_id(event: &crate::storage::StoreEvent) -> Option<&str> {
    event.data.get("task_id").and_then(|value| value.as_str())
}

fn average_cost(total: f64, count: usize) -> f64 {
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

fn ready_parallel_tasks(workflow: &Workflow) -> (Vec<String>, Vec<String>) {
    let completed: BTreeSet<&str> = workflow
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .map(|task| task.id.as_str())
        .collect();
    let known: BTreeSet<&str> = workflow.tasks.iter().map(|task| task.id.as_str()).collect();
    let ready = workflow
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending)
        .filter(|task| {
            task.dependencies.iter().all(|dependency| {
                known.contains(dependency.as_str()) && completed.contains(dependency.as_str())
            })
        })
        .collect::<Vec<_>>();
    (
        ready.iter().map(|task| task.id.clone()).collect(),
        ready.iter().map(|task| task.title.clone()).collect(),
    )
}

fn latest_event_summaries(events: &[crate::storage::StoreEvent]) -> Vec<ImprovementEventSummary> {
    let mut latest = events
        .iter()
        .rev()
        .take(5)
        .map(|event| ImprovementEventSummary {
            kind: event.kind.clone(),
            created_at: event.created_at.clone(),
        })
        .collect::<Vec<_>>();
    latest.reverse();
    latest
}

fn is_rework_event(event: &crate::storage::StoreEvent) -> bool {
    event.kind.contains("rework")
        || event
            .data
            .get("response_status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| status == "needs_retry")
}

#[derive(Debug, Default)]
struct TaskCounts {
    pending: usize,
    running: usize,
    blocked: usize,
    failed: usize,
    completed: usize,
}

fn count_tasks(workflow: &Workflow) -> TaskCounts {
    let mut counts = TaskCounts::default();
    for task in &workflow.tasks {
        match task.status {
            TaskStatus::Pending => counts.pending += 1,
            TaskStatus::Running => counts.running += 1,
            TaskStatus::Blocked => counts.blocked += 1,
            TaskStatus::Failed => counts.failed += 1,
            TaskStatus::Completed => counts.completed += 1,
        }
    }
    counts
}

fn priority_for_score(score: i64) -> String {
    match score {
        100.. => "critical",
        70..=99 => "high",
        35..=69 => "medium",
        _ => "low",
    }
    .to_string()
}

fn recommended_action(reasons: &[ImprovementCandidateReason]) -> String {
    let has = |code: &str| reasons.iter().any(|reason| reason.code == code);
    if has("stale_running_run") || has("missing_runtime_heartbeat") {
        "recover_or_resume_run_before_mutation"
    } else if has("run_needs_attention") {
        "inspect_resume_or_cancel_run"
    } else if has("failed_tasks") || has("blocked_tasks") {
        "repair_failed_or_blocked_tasks"
    } else if has("rework_loop_signal") {
        "run_rework_loop"
    } else if has("parallelization_opportunity") {
        "start_parallel_handoffs"
    } else if has("missing_user_delivery_evidence") || has("missing_final_outcome_audit") {
        "produce_and_package_final_user_outcome"
    } else {
        "inspect_workflow_for_improvement"
    }
    .to_string()
}

fn suggested_commands(
    workflow: &Workflow,
    runs: &[RunRecord],
    reasons: &[ImprovementCandidateReason],
    parallelization: &ParallelizationOpportunityReport,
) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    for run in runs {
        let activity = build_run_activity(run);
        if run.status == "running" && activity.heartbeat_status == "stale" {
            commands.push(vec![
                "forge".to_string(),
                "request".to_string(),
                "recover-stale".to_string(),
                "--run".to_string(),
                run.run_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ]);
        } else if run.status == "needs_attention" {
            commands.push(vec![
                "forge".to_string(),
                "request".to_string(),
                "status".to_string(),
                "--run".to_string(),
                run.run_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ]);
        }
    }
    if has_reason(reasons, "completed_without_final_package") {
        if let Some(run) = latest_run_with_status(runs, "completed").or_else(|| latest_run(runs)) {
            commands.push(vec![
                "forge".to_string(),
                "request".to_string(),
                "final-package".to_string(),
                "--run".to_string(),
                run.run_id.clone(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]);
        }
    }
    if parallelization.ready_parallel_task_count > 0 || has_reason(reasons, "rework_loop_signal") {
        if let Some(run) = latest_driveable_run(runs) {
            commands.push(vec![
                "forge".to_string(),
                "request".to_string(),
                "drive".to_string(),
                "--run".to_string(),
                run.run_id.clone(),
                "--executor".to_string(),
                run.active_executor
                    .clone()
                    .unwrap_or_else(|| "codex".to_string()),
                "--ttl-seconds".to_string(),
                "300".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]);
        }
    }
    commands.push(vec![
        "forge".to_string(),
        "improve".to_string(),
        "--workflow".to_string(),
        workflow.id.clone(),
        "--output".to_string(),
        "json".to_string(),
    ]);
    commands
}

fn has_reason(reasons: &[ImprovementCandidateReason], code: &str) -> bool {
    reasons.iter().any(|reason| reason.code == code)
}

fn latest_run_with_status<'a>(runs: &'a [RunRecord], status: &str) -> Option<&'a RunRecord> {
    runs.iter()
        .filter(|run| run.status == status)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
}

fn latest_run(runs: &[RunRecord]) -> Option<&RunRecord> {
    runs.iter()
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
}

fn latest_driveable_run(runs: &[RunRecord]) -> Option<&RunRecord> {
    runs.iter()
        .filter(|run| {
            matches!(
                run.status.as_str(),
                "planned" | "accepted" | "resumed" | "running"
            )
        })
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
}

pub fn generate_improvement(
    store: &ForgeStore,
    workflow: &Workflow,
    target_version: Option<String>,
) -> Result<ImprovementProposal> {
    let validation = validate_workflow(workflow);
    let target_version = target_version.unwrap_or_else(|| "next".to_string());
    let relative_path = format!(
        "artifacts/{}/improvement-{}.json",
        workflow.id,
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    let changelog_path = format!("artifacts/{}/changelog-{}.md", workflow.id, target_version);
    let evolution_domains = vec![
        "task_structure".to_string(),
        "prompt_system".to_string(),
        "process_runtime".to_string(),
        "validation_governance".to_string(),
        "executor_policy".to_string(),
    ];
    let candidate_changes = vec![
        "evolve task structure with backlog state, subtasks, impediments, ownership and acceptance criteria".to_string(),
        "version prompt packets so executor instructions can be benchmarked and rolled back".to_string(),
        "add process-level workflow policies for Scrum/SAFe-style planning, blocked work and promotion readiness".to_string(),
        "generate a strong changelog for every version with validation evidence, risk notes and migration guidance".to_string(),
    ];
    let payload = json!({
        "workflow_id": workflow.id,
        "generated_at": Utc::now().to_rfc3339(),
        "status": "experiment_generated",
        "auto_promoted": false,
        "promotion_gate": "benchmark_and_validation_required",
        "target_version": target_version,
        "baseline_validation_status": validation.status,
        "evolution_domains": evolution_domains,
        "metrics_used": [
            "completion_rate",
            "recovery_rate",
            "context_efficiency",
            "validation_pass_rate",
            "execution_latency",
            "blocked_work_age",
            "impediment_resolution_rate",
            "prompt_regression_rate"
        ],
        "candidate_changes": candidate_changes,
        "safety": {
            "unrestricted_self_modification": false,
            "requires_validation_before_promotion": true
        }
    });
    let (_full_path, sha256) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
    let changelog_full_path = store.base_dir().join(&changelog_path);
    if let Some(parent) = changelog_full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &changelog_full_path,
        render_changelog(&target_version, workflow, &candidate_changes),
    )?;
    store.record_event(
        &workflow.id,
        "improvement_experiment_generated",
        &json!({
            "artifact_path": relative_path,
            "changelog_path": changelog_path,
            "sha256": sha256
        }),
    )?;

    Ok(ImprovementProposal {
        workflow_id: workflow.id.clone(),
        status: "experiment_generated".to_string(),
        auto_promoted: false,
        promotion_gate: "benchmark_and_validation_required".to_string(),
        target_version,
        artifact_path: relative_path,
        changelog_path,
        candidate_changes,
        evolution_domains,
        metrics_used: vec![
            "completion_rate".to_string(),
            "recovery_rate".to_string(),
            "context_efficiency".to_string(),
            "validation_pass_rate".to_string(),
            "execution_latency".to_string(),
            "blocked_work_age".to_string(),
            "impediment_resolution_rate".to_string(),
            "prompt_regression_rate".to_string(),
        ],
    })
}

fn render_changelog(
    target_version: &str,
    workflow: &Workflow,
    candidate_changes: &[String],
) -> String {
    format!(
        r#"# Forge Core {target_version} Changelog

## Summary

This candidate version evolves Forge structurally instead of only tuning prompts or changing executor choices.

## Task Structure

- Adds backlog state, subtasks, impediments, owner role and acceptance criteria to atomic tasks.
- Keeps work visible as operational backlog, not just a flat execution list.
- Supports blocked-work tracking needed for Scrum/SAFe-style governance.

## Prompt System

- Treats prompts as versioned execution packets that can be benchmarked.
- Keeps rollback possible when a prompt/process change reduces validation quality.

## Process Runtime

- Uses workflow `{}` as the baseline for the experiment.
- Keeps promotion blocked until benchmark and validation gates pass.

## Candidate Changes

{}

## Validation

- `auto_promoted=false`
- `promotion_gate=benchmark_and_validation_required`
- Requires fresh validation evidence before this candidate can become the active runtime behavior.
"#,
        workflow.id,
        candidate_changes
            .iter()
            .map(|change| format!("- {change}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
