use crate::artifact::write_json_artifact;
use crate::graph::{
    AtomicTask, ExecutionPolicySpec, ExecutorKind, TaskStatus, Workflow, WorkflowRevision,
};
use crate::outcome::{
    assess_workflow_outcome, assess_workflow_outcome_metadata, OutcomeStatusReport,
};
use crate::request::{
    build_run_activity, final_completion_audit_block_reason, RunActivity, RunRecord,
};
use crate::scheduler::{plan_parallel_execution, ParallelSchedulePlan};
use crate::storage::{ForgeStore, StoreEvent};
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
    pub matched_workflows: usize,
    pub filter: ImprovementCandidateFilter,
    pub candidate_count: usize,
    pub selection_policy: Vec<String>,
    pub candidates: Vec<OrchestratorImprovementCandidate>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImprovementCandidateFilter {
    pub workflow_ids: Vec<String>,
    pub goal_contains: Vec<String>,
}

impl ImprovementCandidateFilter {
    pub fn active(&self) -> bool {
        !self.workflow_ids.is_empty() || !self.goal_contains.is_empty()
    }
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
    pub repetitive_or_deterministic_ai_cost_item_count: usize,
    pub repetitive_or_deterministic_ai_cost_items: Vec<RepetitiveAiCostItemReport>,
    pub measured_repetitive_or_deterministic_ai_execution_count: usize,
    pub measured_repetitive_or_deterministic_ai_cost_total_usd: Option<f64>,
    pub measured_repetitive_or_deterministic_ai_cost_average_usd: Option<f64>,
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
pub struct RepetitiveAiCostItemReport {
    pub item_key: String,
    pub classification: String,
    pub estimated_execution_count: usize,
    pub observed_execution_count: usize,
    pub task_count: usize,
    pub task_ids: Vec<String>,
    pub task_titles: Vec<String>,
    pub estimated_cost_total_usd: f64,
    pub estimated_cost_average_per_execution_usd: f64,
    pub replacement_estimated_cost_total_usd: f64,
    pub replacement_estimated_cost_average_per_execution_usd: f64,
    pub avoidable_estimated_cost_total_usd: f64,
    pub avoidable_estimated_cost_average_per_execution_usd: f64,
    pub estimated_savings_after_replacement_total_usd: f64,
    pub estimated_savings_after_replacement_average_per_execution_usd: f64,
    pub observed_cost_total_usd: Option<f64>,
    pub observed_cost_average_per_execution_usd: Option<f64>,
    pub avoidable_observed_cost_total_usd: Option<f64>,
    pub avoidable_observed_cost_average_per_execution_usd: Option<f64>,
    pub recommended_replacement: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct AvoidableAiCostNormalizationReport {
    pub status: String,
    pub schema_version: String,
    pub workflow_id: String,
    pub origin: String,
    pub revision: u64,
    pub normalized_task_count: usize,
    pub normalized_tasks: Vec<NormalizedAvoidableAiTask>,
    pub propagated_version_task_count: usize,
    pub propagated_version_task_ids: Vec<String>,
    pub avoided_estimated_cost_usd: f64,
    pub validation_status: String,
    pub promotable: bool,
    pub event_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizedAvoidableAiTask {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub previous_executor: String,
    pub new_executor: String,
    pub previous_policy_mode: String,
    pub new_policy_mode: String,
    pub previous_estimated_cost_usd: f64,
    pub new_estimated_cost_usd: f64,
    pub avoidable_estimated_cost_usd: f64,
    pub previous_version: u64,
    pub new_version: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AvoidableAiCostBatchNormalizationReport {
    pub status: String,
    pub schema_version: String,
    pub origin: String,
    pub requested_limit: usize,
    pub ranked_candidate_count: usize,
    pub attempted_workflow_count: usize,
    pub normalized_workflow_count: usize,
    pub repaired_workflow_count: usize,
    pub no_change_workflow_count: usize,
    pub total_normalized_task_count: usize,
    pub total_propagated_version_task_count: usize,
    pub total_avoided_estimated_cost_usd: f64,
    pub reports: Vec<AvoidableAiCostNormalizationReport>,
}

pub fn rank_improvement_candidates(
    store: &ForgeStore,
    limit: usize,
) -> Result<OrchestratorImprovementCandidatesReport> {
    rank_improvement_candidates_with_filter(store, limit, ImprovementCandidateFilter::default())
}

pub fn rank_improvement_candidates_with_filter(
    store: &ForgeStore,
    limit: usize,
    filter: ImprovementCandidateFilter,
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
    let mut matched_workflows = 0;
    for workflow in workflows {
        if !improvement_candidate_filter_matches(&workflow, &filter) {
            continue;
        }
        matched_workflows += 1;
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
            "measure repetitive or deterministic AI work by item group and average cost per execution before normalizing"
                .to_string(),
            "penalize support-only completion when the user asked for final outcomes".to_string(),
            "deprioritize workflows whose final user outcome is already verified; treat remaining support tasks as cleanup instead of delivery blockers".to_string(),
            "allow focused candidate scans by workflow id or goal text so active work is not drowned by unrelated historical backlog".to_string(),
        ],
        matched_workflows,
        filter,
        candidates,
    })
}

fn improvement_candidate_filter_matches(
    workflow: &Workflow,
    filter: &ImprovementCandidateFilter,
) -> bool {
    if !filter.workflow_ids.is_empty()
        && !filter
            .workflow_ids
            .iter()
            .any(|workflow_id| workflow_id == &workflow.id)
    {
        return false;
    }
    if !filter.goal_contains.is_empty() {
        let mut haystack = workflow.goal.clone();
        if let Some(initial_goal) = &workflow.initial_goal {
            haystack.push('\n');
            haystack.push_str(initial_goal);
        }
        let haystack = haystack.to_lowercase();
        if !filter
            .goal_contains
            .iter()
            .map(|needle| needle.trim().to_lowercase())
            .filter(|needle| !needle.is_empty())
            .any(|needle| haystack.contains(&needle))
        {
            return false;
        }
    }
    true
}

fn build_improvement_candidate(
    store: &ForgeStore,
    workflow: &Workflow,
    runs: &[RunRecord],
) -> Result<Option<OrchestratorImprovementCandidate>> {
    let events = store.load_workflow_events(&workflow.id)?;
    let final_completion_audit_block_reason = final_completion_audit_block_reason(store, workflow)?;
    let outcome_status = assess_workflow_outcome(
        workflow,
        true,
        final_completion_audit_block_reason.as_deref(),
    );
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

    let final_outcome_verified = outcome_status.status == "final_outcome_verified";
    let has_active_runtime_issue = needs_attention_run_count > 0
        || stale_run_count > 0
        || missing_heartbeat_count > 0
        || task_counts.failed > 0
        || task_counts.blocked > 0;
    if final_outcome_verified && !has_active_runtime_issue {
        let mut score = 0;
        let mut reasons = Vec::new();
        if workflow.status != "completed" || task_counts.pending > 0 || task_counts.running > 0 {
            push_reason(
                &mut reasons,
                &mut score,
                "post_final_verification_cleanup",
                "low",
                10,
                "Final user outcome is already verified; remaining running/pending support state should be completed, archived or compacted without reopening the user deliverable."
                    .to_string(),
            );
        }
        if final_delivery_package_count == 0 {
            push_reason(
                &mut reasons,
                &mut score,
                "verified_without_final_package",
                "low",
                15,
                "Final outcome audit passed, but no final delivery package event was recorded; refresh the handoff package only if the user-facing summary is needed."
                    .to_string(),
            );
        }
        if score <= 0 {
            return Ok(None);
        }

        let suggested_commands =
            suggested_commands(workflow, runs, &reasons, &parallelization, &events);

        return Ok(Some(OrchestratorImprovementCandidate {
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
        }));
    }

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
        let highest_average_item = cost_efficiency
            .repetitive_or_deterministic_ai_cost_items
            .first()
            .map(|item| {
                format!(
                    "; highest item `{}` averages ${:.6} estimated AI cost per execution",
                    item.item_key, item.estimated_cost_average_per_execution_usd
                )
            })
            .unwrap_or_default();
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
                "{} AI task(s) across {} repeated/deterministic item group(s) look avoidable; average avoidable estimated AI cost per execution is ${:.6} and avoidable estimated cost is ${:.6}{}.",
                cost_efficiency.repetitive_or_deterministic_ai_task_count,
                cost_efficiency.repetitive_or_deterministic_ai_cost_item_count,
                cost_efficiency.avoidable_estimated_cost_average_usd,
                cost_efficiency.avoidable_estimated_cost_usd,
                highest_average_item
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

    let suggested_commands =
        suggested_commands(workflow, runs, &reasons, &parallelization, &events);

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
    let ai_task_ids = ai_tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    let ai_task_count = ai_tasks.len();
    let estimated_ai_cost_total_usd = normalize_zero_cost(
        ai_tasks
            .iter()
            .map(|task| task.cost.estimated_cost_usd)
            .sum::<f64>(),
    );
    let estimated_ai_cost_average_usd = average_cost(estimated_ai_cost_total_usd, ai_task_count);
    let repetitive_ai_tasks = ai_tasks
        .iter()
        .copied()
        .filter(|task| normalizable_avoidable_ai_task(task))
        .collect::<Vec<_>>();
    let observed_cost_task_ids = events
        .iter()
        .filter(|event| observed_ai_cost_from_event(event).is_some())
        .filter_map(event_task_id)
        .collect::<BTreeSet<_>>();
    let measured_repetitive_ai_tasks = ai_tasks
        .iter()
        .copied()
        .filter(|task| looks_repetitive_or_deterministic_ai_task(task))
        .filter(|task| {
            task.status != TaskStatus::Completed
                || observed_cost_task_ids.contains(task.id.as_str())
        })
        .collect::<Vec<_>>();
    let repetitive_or_deterministic_ai_cost_items =
        build_repetitive_ai_cost_item_reports(&measured_repetitive_ai_tasks, events);
    let measured_repetitive_or_deterministic_ai_execution_count =
        repetitive_or_deterministic_ai_cost_items
            .iter()
            .map(|item| item.observed_execution_count)
            .sum::<usize>();
    let measured_repetitive_or_deterministic_ai_cost_total_usd = {
        let total = repetitive_or_deterministic_ai_cost_items
            .iter()
            .filter_map(|item| item.observed_cost_total_usd)
            .sum::<f64>();
        (measured_repetitive_or_deterministic_ai_execution_count > 0)
            .then(|| normalize_zero_cost(total))
    };
    let measured_repetitive_or_deterministic_ai_cost_average_usd =
        measured_repetitive_or_deterministic_ai_cost_total_usd.map(|total| {
            average_cost(
                total,
                measured_repetitive_or_deterministic_ai_execution_count,
            )
        });
    let avoidable_estimated_cost_usd = normalize_zero_cost(
        repetitive_ai_tasks
            .iter()
            .map(|task| task.cost.estimated_cost_usd)
            .sum::<f64>(),
    );
    let avoidable_estimated_cost_average_usd =
        average_cost(avoidable_estimated_cost_usd, repetitive_ai_tasks.len());
    let observed_costs = events
        .iter()
        .filter(|event| event_task_id(event).map_or(true, |task_id| ai_task_ids.contains(task_id)))
        .filter_map(observed_ai_cost_from_event)
        .collect::<Vec<_>>();
    let observed_ai_cost_total_usd =
        (!observed_costs.is_empty()).then(|| normalize_zero_cost(observed_costs.iter().sum()));
    let observed_ai_cost_average_usd =
        observed_ai_cost_total_usd.map(|total| average_cost(total, observed_costs.len()));
    let repetitive_task_ids = measured_repetitive_ai_tasks
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
    let avoidable_observed_cost_total_usd = (!avoidable_observed_costs.is_empty())
        .then(|| normalize_zero_cost(avoidable_observed_costs.iter().sum()));
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
        repetitive_or_deterministic_ai_cost_item_count: repetitive_or_deterministic_ai_cost_items
            .len(),
        repetitive_or_deterministic_ai_cost_items,
        measured_repetitive_or_deterministic_ai_execution_count,
        measured_repetitive_or_deterministic_ai_cost_total_usd,
        measured_repetitive_or_deterministic_ai_cost_average_usd,
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

fn build_repetitive_ai_cost_item_reports(
    tasks: &[&AtomicTask],
    events: &[crate::storage::StoreEvent],
) -> Vec<RepetitiveAiCostItemReport> {
    let mut tasks_by_item: BTreeMap<String, Vec<&AtomicTask>> = BTreeMap::new();
    for task in tasks {
        tasks_by_item
            .entry(repetitive_cost_item_key(task))
            .or_default()
            .push(*task);
    }

    let mut observed_costs_by_task: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for event in events {
        let Some(task_id) = event_task_id(event) else {
            continue;
        };
        let Some(cost) = observed_ai_cost_from_event(event) else {
            continue;
        };
        observed_costs_by_task
            .entry(task_id)
            .or_default()
            .push(cost);
    }

    let mut reports = Vec::new();
    for (item_key, item_tasks) in tasks_by_item {
        let task_count = item_tasks.len();
        let estimated_cost_total_usd = normalize_zero_cost(
            item_tasks
                .iter()
                .map(|task| task.cost.estimated_cost_usd)
                .sum::<f64>(),
        );
        let replacement_estimated_cost_total_usd = normalize_zero_cost(
            item_tasks
                .iter()
                .map(|task| normalized_command_cost(task))
                .sum::<f64>(),
        );
        let estimated_savings_after_replacement_total_usd =
            (estimated_cost_total_usd - replacement_estimated_cost_total_usd).max(0.0);
        let observed_costs = item_tasks
            .iter()
            .flat_map(|task| {
                observed_costs_by_task
                    .get(task.id.as_str())
                    .into_iter()
                    .flatten()
                    .copied()
            })
            .collect::<Vec<_>>();
        let observed_execution_count = observed_costs.len();
        let observed_cost_total_usd = (!observed_costs.is_empty())
            .then(|| normalize_zero_cost(observed_costs.iter().sum::<f64>()));
        let observed_cost_average_per_execution_usd =
            observed_cost_total_usd.map(|total| average_cost(total, observed_execution_count));
        let classification = if task_count > 1
            || item_tasks
                .iter()
                .any(|task| task_has_repetition_signal(task))
        {
            "repetitive_ai_item"
        } else {
            "deterministic_ai_item"
        };

        reports.push(RepetitiveAiCostItemReport {
            item_key,
            classification: classification.to_string(),
            estimated_execution_count: task_count,
            observed_execution_count,
            task_count,
            task_ids: item_tasks.iter().map(|task| task.id.clone()).collect(),
            task_titles: item_tasks.iter().map(|task| task.title.clone()).collect(),
            estimated_cost_total_usd,
            estimated_cost_average_per_execution_usd: average_cost(
                estimated_cost_total_usd,
                task_count,
            ),
            replacement_estimated_cost_total_usd,
            replacement_estimated_cost_average_per_execution_usd: average_cost(
                replacement_estimated_cost_total_usd,
                task_count,
            ),
            avoidable_estimated_cost_total_usd: estimated_cost_total_usd,
            avoidable_estimated_cost_average_per_execution_usd: average_cost(
                estimated_cost_total_usd,
                task_count,
            ),
            estimated_savings_after_replacement_total_usd,
            estimated_savings_after_replacement_average_per_execution_usd: average_cost(
                estimated_savings_after_replacement_total_usd,
                task_count,
            ),
            observed_cost_total_usd,
            observed_cost_average_per_execution_usd,
            avoidable_observed_cost_total_usd: observed_cost_total_usd,
            avoidable_observed_cost_average_per_execution_usd:
                observed_cost_average_per_execution_usd,
            recommended_replacement: "command_node_or_cached_reusable_subflow".to_string(),
        });
    }

    reports.sort_by(|left, right| {
        right
            .avoidable_estimated_cost_total_usd
            .partial_cmp(&left.avoidable_estimated_cost_total_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.item_key.cmp(&right.item_key))
    });
    reports
}

fn looks_repetitive_or_deterministic_ai_task(task: &crate::graph::AtomicTask) -> bool {
    if looks_like_final_completion_audit_task(task) && !task.execution_policy.deterministic {
        return false;
    }

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
        "telegram",
        "playwright",
        "pdf",
        "markdown report",
        "delivery evidence",
        "delivery record",
        "verified evidence",
        "regulation inspection",
        "inspection evidence",
        "report from verified evidence",
    ];
    task_has_repetition_signal(task)
        || task.execution_policy.deterministic
        || deterministic_keywords
            .iter()
            .any(|keyword| text.contains(keyword))
}

fn task_has_repetition_signal(task: &crate::graph::AtomicTask) -> bool {
    let text = format!("{} {} {}", task.title, task.goal, task.expected_output).to_lowercase();
    let repetitive_keywords = [
        "repeated",
        "repetitive",
        "recurring",
        "frequent",
        "daily",
        "weekly",
        "monthly",
        "hourly",
        "nightly",
        "cron",
        "scheduled",
        "diário",
        "diária",
        "diario",
        "diaria",
        "semanal",
        "mensal",
        "recorrente",
        "agendado",
    ];
    task.schedule.is_some()
        || repetitive_keywords
            .iter()
            .any(|keyword| text.contains(keyword))
}

fn repetitive_cost_item_key(task: &crate::graph::AtomicTask) -> String {
    let title = normalize_repetitive_signature_text(&task.title);
    let expected_output = normalize_repetitive_signature_text(&task.expected_output);
    if expected_output.is_empty() {
        title
    } else {
        format!("{title} -> {expected_output}")
    }
}

fn normalize_repetitive_signature_text(value: &str) -> String {
    let normalized = value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    normalized
        .split_whitespace()
        .filter(|token| !repetitive_signature_stopword(token))
        .filter(|token| token.parse::<u64>().is_err())
        .collect::<Vec<_>>()
        .join(" ")
}

fn repetitive_signature_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "the"
            | "de"
            | "da"
            | "do"
            | "das"
            | "dos"
            | "daily"
            | "weekly"
            | "monthly"
            | "hourly"
            | "nightly"
            | "cron"
            | "scheduled"
            | "recurring"
            | "repeated"
            | "repetitive"
            | "frequent"
            | "diário"
            | "diária"
            | "diario"
            | "diaria"
            | "semanal"
            | "mensal"
            | "agendado"
            | "recorrente"
    )
}

fn normalizable_avoidable_ai_task(task: &crate::graph::AtomicTask) -> bool {
    task.executor == ExecutorKind::Ai
        && task.status != TaskStatus::Completed
        && looks_repetitive_or_deterministic_ai_task(task)
}

fn normalized_command_cost(task: &crate::graph::AtomicTask) -> f64 {
    let text = format!("{} {}", task.title, task.expected_output).to_lowercase();
    if text.contains("extract requirements") || text.contains("requirements") {
        0.0002
    } else {
        0.0005
    }
}

fn deterministic_execution_policy(selection_reason: &str) -> ExecutionPolicySpec {
    ExecutionPolicySpec {
        mode: "deterministic_executor".to_string(),
        ai_allowed: false,
        deterministic: true,
        code_runtime: None,
        reuse_hint: "task_local".to_string(),
        selection_reason: selection_reason.to_string(),
        validation_gate: "task_validation_rules".to_string(),
    }
}

fn propagate_dependency_version_boundary(tasks: &mut [AtomicTask]) -> Vec<String> {
    let mut propagated = BTreeSet::new();
    loop {
        let versions = tasks
            .iter()
            .map(|task| (task.id.clone(), task.version))
            .collect::<BTreeMap<_, _>>();
        let mut changed = false;
        for task in tasks.iter_mut() {
            let minimum_dependency_version = task
                .dependencies
                .iter()
                .filter_map(|dependency| versions.get(dependency))
                .copied()
                .max()
                .unwrap_or(task.version);
            if task.version < minimum_dependency_version {
                task.version = minimum_dependency_version;
                propagated.insert(task.id.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    propagated.into_iter().collect()
}

pub fn normalize_avoidable_ai_costs(
    store: &ForgeStore,
    workflow_id: &str,
    origin: &str,
) -> Result<AvoidableAiCostNormalizationReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    let mut normalized_tasks = Vec::new();
    let mut avoided_estimated_cost_usd = 0.0;

    for task in &mut workflow.tasks {
        if !normalizable_avoidable_ai_task(task) {
            continue;
        }

        let previous_estimated_cost_usd = task.cost.estimated_cost_usd;
        let new_estimated_cost_usd = normalized_command_cost(task);
        let previous_policy_mode = task.execution_policy.mode.clone();
        let previous_version = task.version;
        task.executor = ExecutorKind::Command;
        task.execution_policy = deterministic_execution_policy(
            "normalized avoidable AI cost: repetitive or deterministic work can run without a live model call",
        );
        task.cost.estimated_cost_usd = new_estimated_cost_usd;
        task.version += 1;
        let avoidable_estimated_cost_usd =
            (previous_estimated_cost_usd - new_estimated_cost_usd).max(0.0);
        avoided_estimated_cost_usd += avoidable_estimated_cost_usd;

        normalized_tasks.push(NormalizedAvoidableAiTask {
            task_id: task.id.clone(),
            title: task.title.clone(),
            status: serde_json::to_value(&task.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            previous_executor: "ai".to_string(),
            new_executor: "command".to_string(),
            previous_policy_mode,
            new_policy_mode: task.execution_policy.mode.clone(),
            previous_estimated_cost_usd,
            new_estimated_cost_usd,
            avoidable_estimated_cost_usd,
            previous_version,
            new_version: task.version,
        });
    }

    let propagated_version_task_ids = propagate_dependency_version_boundary(&mut workflow.tasks);
    if normalized_tasks.is_empty() && propagated_version_task_ids.is_empty() {
        let validation = validate_workflow(&workflow);
        return Ok(AvoidableAiCostNormalizationReport {
            status: "no_changes".to_string(),
            schema_version: "forge.improve.avoidable_ai_cost_normalization.v1".to_string(),
            workflow_id: workflow.id,
            origin: origin.to_string(),
            revision: workflow
                .revisions
                .last()
                .map(|item| item.revision)
                .unwrap_or(0),
            normalized_task_count: 0,
            normalized_tasks,
            propagated_version_task_count: 0,
            propagated_version_task_ids: Vec::new(),
            avoided_estimated_cost_usd,
            validation_status: validation.status,
            promotable: validation.promotable,
            event_kind: "avoidable_ai_cost_normalization_noop".to_string(),
        });
    }

    let validation = validate_workflow(&workflow);
    let revision = workflow
        .revisions
        .last()
        .map(|item| item.revision + 1)
        .unwrap_or(1);
    let (status, change_type, event_kind, summary) = if normalized_tasks.is_empty() {
        (
            "version_boundary_repaired",
            "avoidable_ai_cost_version_boundary_repaired",
            "avoidable_ai_cost_version_boundary_repaired",
            format!(
                "repaired dependency version boundary for {} downstream task(s)",
                propagated_version_task_ids.len()
            ),
        )
    } else {
        (
            "normalized",
            "avoidable_ai_cost_normalized",
            "avoidable_ai_cost_normalized",
            format!(
                "normalized {} repetitive or deterministic AI task(s) to command executor",
                normalized_tasks.len()
            ),
        )
    };
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: change_type.to_string(),
        summary,
        created_at: Utc::now(),
    });
    store.save_workflow(&workflow)?;
    store.record_event(
        &workflow.id,
        event_kind,
        &json!({
            "schema_version": "forge.improve.avoidable_ai_cost_normalization.v1",
            "revision": revision,
            "origin": origin,
            "normalized_task_count": normalized_tasks.len(),
            "normalized_tasks": normalized_tasks.clone(),
            "propagated_version_task_count": propagated_version_task_ids.len(),
            "propagated_version_task_ids": propagated_version_task_ids.clone(),
            "avoided_estimated_cost_usd": avoided_estimated_cost_usd,
            "validation_status": validation.status.clone(),
            "promotable": validation.promotable
        }),
    )?;

    Ok(AvoidableAiCostNormalizationReport {
        status: status.to_string(),
        schema_version: "forge.improve.avoidable_ai_cost_normalization.v1".to_string(),
        workflow_id: workflow.id,
        origin: origin.to_string(),
        revision,
        normalized_task_count: normalized_tasks.len(),
        normalized_tasks,
        propagated_version_task_count: propagated_version_task_ids.len(),
        propagated_version_task_ids,
        avoided_estimated_cost_usd,
        validation_status: validation.status,
        promotable: validation.promotable,
        event_kind: event_kind.to_string(),
    })
}

pub fn normalize_avoidable_ai_costs_for_candidates(
    store: &ForgeStore,
    limit: usize,
    origin: &str,
) -> Result<AvoidableAiCostBatchNormalizationReport> {
    let candidates = rank_improvement_candidates(store, limit)?;
    let mut reports = Vec::new();
    let mut attempted = BTreeSet::new();

    for candidate in &candidates.candidates {
        if candidate
            .cost_efficiency
            .repetitive_or_deterministic_ai_task_count
            == 0
        {
            continue;
        }
        if !attempted.insert(candidate.workflow_id.clone()) {
            continue;
        }
        reports.push(normalize_avoidable_ai_costs(
            store,
            &candidate.workflow_id,
            origin,
        )?);
    }

    let normalized_workflow_count = reports
        .iter()
        .filter(|report| report.normalized_task_count > 0)
        .count();
    let repaired_workflow_count = reports
        .iter()
        .filter(|report| {
            report.normalized_task_count == 0 && report.propagated_version_task_count > 0
        })
        .count();
    let no_change_workflow_count = reports
        .iter()
        .filter(|report| {
            report.normalized_task_count == 0 && report.propagated_version_task_count == 0
        })
        .count();
    let total_normalized_task_count = reports
        .iter()
        .map(|report| report.normalized_task_count)
        .sum();
    let total_propagated_version_task_count = reports
        .iter()
        .map(|report| report.propagated_version_task_count)
        .sum();
    let total_avoided_estimated_cost_usd = reports
        .iter()
        .map(|report| report.avoided_estimated_cost_usd)
        .sum();
    let status = if normalized_workflow_count > 0 || repaired_workflow_count > 0 {
        "normalized"
    } else {
        "no_changes"
    };

    Ok(AvoidableAiCostBatchNormalizationReport {
        status: status.to_string(),
        schema_version: "forge.improve.avoidable_ai_cost_batch_normalization.v1".to_string(),
        origin: origin.to_string(),
        requested_limit: limit,
        ranked_candidate_count: candidates.candidate_count,
        attempted_workflow_count: reports.len(),
        normalized_workflow_count,
        repaired_workflow_count,
        no_change_workflow_count,
        total_normalized_task_count,
        total_propagated_version_task_count,
        total_avoided_estimated_cost_usd,
        reports,
    })
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
        normalize_zero_cost(total / count as f64)
    }
}

fn normalize_zero_cost(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
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
    } else if has("verified_without_final_package") {
        "refresh_final_delivery_package_if_needed"
    } else if has("post_final_verification_cleanup") {
        "complete_or_archive_verified_support_state"
    } else if has("missing_final_outcome_audit") {
        "produce_and_package_final_user_outcome"
    } else if has("support_only_output_risk") {
        "update_goal_or_tasks_with_user_facing_deliverables"
    } else if has("rework_loop_signal") {
        "run_rework_loop"
    } else if has("parallelization_opportunity") {
        "start_parallel_handoffs"
    } else if has("missing_user_delivery_evidence") {
        "produce_and_package_final_user_outcome"
    } else if has("completed_without_final_package") {
        "refresh_final_delivery_package_if_needed"
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
    events: &[StoreEvent],
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
    if has_reason(reasons, "support_only_output_risk")
        && !has_reason(reasons, "parallelization_opportunity")
        && !has_reason(reasons, "rework_loop_signal")
    {
        commands.push(vec![
            "forge".to_string(),
            "status".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ]);
        commands.push(vec![
            "forge".to_string(),
            "improve".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ]);
        return commands;
    }
    if has_reason(reasons, "rework_loop_signal") {
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
        } else {
            for task_id in latest_generated_rework_task_ids(events, workflow)
                .into_iter()
                .take(3)
            {
                push_task_handoff_command(&mut commands, workflow, &task_id);
            }
        }
    }
    let final_package_is_ready_or_verified = has_reason(reasons, "verified_without_final_package")
        || (has_reason(reasons, "completed_without_final_package")
            && !has_reason(reasons, "missing_user_delivery_evidence")
            && !has_reason(reasons, "missing_final_outcome_audit"));
    if final_package_is_ready_or_verified {
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
    if has_reason(reasons, "post_final_verification_cleanup") {
        commands.push(vec![
            "forge".to_string(),
            "status".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ]);
    }
    if has_reason(reasons, "missing_final_outcome_audit")
        && workflow_ready_for_final_audit_command(workflow)
    {
        commands.push(vec![
            "forge".to_string(),
            "request".to_string(),
            "ensure-final-audit".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--executor".to_string(),
            "codex".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ]);
    }
    if parallelization.ready_parallel_task_count > 0 {
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
        } else if parallelization.ready_parallel_task_count > 0 {
            let mut ready_task_ids = parallelization
                .ready_parallel_task_ids
                .iter()
                .collect::<Vec<_>>();
            ready_task_ids.sort_by_key(|task_id| {
                if task_id_is_final_completion_audit(workflow, task_id) {
                    0
                } else {
                    1
                }
            });
            let task_ids_to_suggest = ready_task_ids
                .into_iter()
                .filter(|task_id| !task_handoff_already_suggested(&commands, task_id))
                .take(
                    parallelization
                        .recommended_max_parallelism
                        .max(1)
                        .min(parallelization.ready_parallel_task_count),
                )
                .cloned()
                .collect::<Vec<_>>();
            commands.extend(
                task_ids_to_suggest
                    .iter()
                    .map(|task_id| task_handoff_command(workflow, task_id)),
            );
        }
    }
    if has_reason(reasons, "avoidable_ai_cost")
        && workflow.tasks.iter().any(normalizable_avoidable_ai_task)
    {
        commands.push(vec![
            "forge".to_string(),
            "improve".to_string(),
            "normalize-cost".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ]);
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

fn push_task_handoff_command(commands: &mut Vec<Vec<String>>, workflow: &Workflow, task_id: &str) {
    if !task_handoff_already_suggested(commands, task_id) {
        commands.push(task_handoff_command(workflow, task_id));
    }
}

fn task_handoff_command(workflow: &Workflow, task_id: &str) -> Vec<String> {
    let executor = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .map(suggested_handoff_executor)
        .unwrap_or("codex");
    vec![
        "forge".to_string(),
        "task".to_string(),
        "handoff".to_string(),
        "--workflow".to_string(),
        workflow.id.clone(),
        "--task".to_string(),
        task_id.to_string(),
        "--executor".to_string(),
        executor.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

fn suggested_handoff_executor(task: &AtomicTask) -> &'static str {
    match task.executor {
        ExecutorKind::Ai | ExecutorKind::Mixed if task.execution_policy.ai_allowed => "codex",
        _ => "forge_cli",
    }
}

fn task_handoff_already_suggested(commands: &[Vec<String>], task_id: &str) -> bool {
    commands.iter().any(|command| {
        command.first().is_some_and(|part| part == "forge")
            && command.get(1).is_some_and(|part| part == "task")
            && command.get(2).is_some_and(|part| part == "handoff")
            && command
                .windows(2)
                .any(|window| window[0] == "--task" && window[1] == task_id)
    })
}

fn latest_generated_rework_task_ids(events: &[StoreEvent], workflow: &Workflow) -> Vec<String> {
    let pending_task_ids = workflow
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending)
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut task_ids = Vec::new();
    for event in events.iter().rev() {
        if event.kind != "executor_response_promoted" {
            continue;
        }
        if event
            .data
            .get("response_status")
            .and_then(|value| value.as_str())
            != Some("needs_retry")
        {
            continue;
        }
        let Some(generated) = event
            .data
            .get("generated_rework_task_ids")
            .and_then(|value| value.as_array())
        else {
            continue;
        };
        for task_id in generated.iter().filter_map(|value| value.as_str()) {
            if pending_task_ids.contains(task_id) && seen.insert(task_id.to_string()) {
                task_ids.push(task_id.to_string());
            }
        }
        if !task_ids.is_empty() {
            break;
        }
    }
    task_ids
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

fn workflow_ready_for_final_audit_command(workflow: &Workflow) -> bool {
    if workflow_has_evidenced_user_outcome(workflow) {
        return true;
    }

    if let Some(audit_task) = workflow
        .tasks
        .iter()
        .find(|task| looks_like_final_completion_audit_task(task))
    {
        return audit_task.dependencies.iter().all(|dependency| {
            workflow
                .tasks
                .iter()
                .any(|task| task.id == *dependency && task.status == TaskStatus::Completed)
        });
    }

    !workflow.tasks.is_empty()
        && workflow
            .tasks
            .iter()
            .all(|task| task.status == TaskStatus::Completed)
}

fn workflow_has_evidenced_user_outcome(workflow: &Workflow) -> bool {
    let outcome = assess_workflow_outcome_metadata(workflow);
    outcome.user_facing_deliverable_count > 0 && outcome.missing_user_facing_deliverable_count == 0
}

fn task_id_is_final_completion_audit(workflow: &Workflow, task_id: &str) -> bool {
    workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .is_some_and(looks_like_final_completion_audit_task)
}

fn looks_like_final_completion_audit_task(task: &crate::graph::AtomicTask) -> bool {
    let title = task.title.to_lowercase();
    let expected_output = task.expected_output.to_lowercase();
    title.contains("final completion")
        || expected_output.contains("final_completion_audit")
        || expected_output.contains("final completion audit")
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
