use crate::artifact::write_json_artifact;
use crate::event::{build_event_improvement_policy, EventImprovementRecommendation};
use crate::graph::{
    AsyncPolicy, AtomicTask, ExecutionPolicySpec, ExecutorKind, TaskStatus, ValidationRule,
    Workflow, WorkflowRevision,
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
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
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
    pub event_improvement_policy: ImprovementProposalEventPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementProposalEventPolicy {
    pub schema_version: String,
    pub status: String,
    pub index_source: String,
    pub recommendation_count: usize,
    pub recommendations: Vec<EventImprovementRecommendation>,
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

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyApplicationReport {
    pub status: String,
    pub schema_version: String,
    pub workflow_id: String,
    pub origin: String,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    pub recommendation: EventImprovementRecommendation,
    pub proposed_change_count: usize,
    pub proposed_changes: Vec<EventPolicyProposedChange>,
    pub rollback_plan: EventPolicyRollbackPlan,
    pub equivalence_gate: EventPolicyEquivalenceGate,
    pub validation_status: String,
    pub promotable: bool,
    pub promotion_gate: String,
    pub revision: u64,
    pub event_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyProposedChange {
    pub task_id: String,
    pub title: String,
    pub policy: String,
    pub changed_fields: Vec<String>,
    pub previous_executor: String,
    pub new_executor: String,
    pub previous_execution_policy_mode: String,
    pub new_execution_policy_mode: String,
    pub previous_execution_policy_reuse_hint: String,
    pub new_execution_policy_reuse_hint: String,
    pub previous_async_policy_mode: String,
    pub new_async_policy_mode: String,
    pub previous_async_resume_strategy: String,
    pub new_async_resume_strategy: String,
    pub previous_context_requirement_count: usize,
    pub new_context_requirement_count: usize,
    pub previous_validation_rule_count: usize,
    pub new_validation_rule_count: usize,
    pub previous_estimated_cost_usd: f64,
    pub new_estimated_cost_usd: f64,
    pub previous_version: u64,
    pub new_version: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyRollbackPlan {
    pub status: String,
    pub requires_human_approval: bool,
    pub rollback_change_count: usize,
    pub changes: Vec<EventPolicyRollbackChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyRollbackChange {
    pub task_id: String,
    pub restore_executor: String,
    pub restore_execution_policy_mode: String,
    pub restore_execution_policy: ExecutionPolicySpec,
    pub restore_async_policy: AsyncPolicy,
    pub restore_context_requirements: Vec<String>,
    pub restore_validation_rules: Vec<ValidationRule>,
    pub restore_estimated_cost_usd: f64,
    pub restore_version: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyEquivalenceGate {
    pub status: String,
    pub required: bool,
    pub benchmark_required: bool,
    pub validation_required: bool,
    pub promotion_allowed: bool,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyBenchmarkReport {
    pub status: String,
    pub schema_version: String,
    pub workflow_id: String,
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation_id: Option<String>,
    pub recommended_policy: String,
    pub application_event_id: i64,
    pub application_revision: u64,
    pub benchmark: EventPolicyBenchmarkEvidence,
    pub equivalence: EventPolicyBenchmarkEquivalence,
    pub promotion_decision: EventPolicyPromotionDecision,
    pub validation_status: String,
    pub promotable: bool,
    pub event_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyBenchmarkEvidence {
    pub validation_passed: bool,
    pub rollback_ready: bool,
    pub proposed_change_count: usize,
    pub checked_task_count: usize,
    pub failed_check_count: usize,
    pub checks: Vec<String>,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyBenchmarkEquivalence {
    pub status: String,
    pub promotion_allowed: bool,
    pub task_shape_preserved: bool,
    pub executor_contract_applied: bool,
    pub rollback_ready: bool,
    pub validation_passed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyPromotionDecision {
    pub status: String,
    pub auto_promoted: bool,
    pub requires_human_approval: bool,
    pub promotion_gate: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyPromotionReport {
    pub status: String,
    pub schema_version: String,
    pub workflow_id: String,
    pub origin: String,
    pub approved_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation_id: Option<String>,
    pub recommended_policy: String,
    pub application_event_id: i64,
    pub application_revision: u64,
    pub benchmark_event_id: i64,
    pub benchmark: EventPolicyPromotionBenchmarkSummary,
    pub validation_status: String,
    pub promoted: bool,
    pub auto_promoted: bool,
    pub revision: u64,
    pub promotion_gate: String,
    pub reason: String,
    pub event_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPolicyPromotionBenchmarkSummary {
    pub status: String,
    pub promotion_allowed: bool,
    pub validation_passed: bool,
    pub rollback_ready: bool,
    pub failed_check_count: u64,
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

pub fn apply_event_improvement_policy(
    store: &ForgeStore,
    workflow_id: &str,
    recommendation_id: Option<&str>,
    recommended_policy: Option<&str>,
    apply: bool,
    approved_by: Option<&str>,
    origin: &str,
) -> Result<EventPolicyApplicationReport> {
    let approved_by = approved_by.map(str::trim).filter(|value| !value.is_empty());
    let mut workflow = store.load_workflow(workflow_id)?;
    let policy_report = build_event_improvement_policy(
        store,
        Some(workflow_id),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(20),
        None,
    )?;
    let Some(recommendation) = select_event_policy_recommendation(
        &policy_report.recommendations,
        recommendation_id,
        recommended_policy,
    ) else {
        bail!("no matching event improvement policy recommendation found for workflow `{workflow_id}`");
    };
    let mut proposed_changes = Vec::new();
    let mut rollback_changes = Vec::new();

    if let Some(node_ref) = recommendation.node_ref.as_deref() {
        if let Some(task) = workflow.tasks.iter_mut().find(|task| task.id == node_ref) {
            let previous_task = task.clone();
            let mut proposed_task = task.clone();
            let changed_fields =
                apply_event_policy_recommendation_to_task(&mut proposed_task, &recommendation);
            if changed_fields.is_empty()
                && recommendation.recommended_policy == "prefer_deterministic_node"
            {
                return Ok(event_policy_noop_report(
                    workflow,
                    origin,
                    apply,
                    approved_by,
                    recommendation,
                    "event_policy_application_already_applied",
                    "event_policy_application_noop",
                ));
            }
            if !changed_fields.is_empty() {
                proposed_task.version = previous_task.version + 1;
                proposed_changes.push(event_policy_proposed_change(
                    &previous_task,
                    &proposed_task,
                    &recommendation,
                    changed_fields,
                ));
                rollback_changes.push(event_policy_rollback_change(&previous_task));
                if apply && approved_by.is_some() {
                    *task = proposed_task;
                }
            }
        }
    }

    if proposed_changes.is_empty() {
        let validation = validate_workflow(&workflow);
        let revision = workflow
            .revisions
            .last()
            .map(|item| item.revision)
            .unwrap_or(0);
        return Ok(EventPolicyApplicationReport {
            status: "event_policy_application_plan_only".to_string(),
            schema_version: "forge.improve.event_policy_application.v1".to_string(),
            workflow_id: workflow.id,
            origin: origin.to_string(),
            dry_run: !apply,
            apply_requested: apply,
            applied: false,
            approved_by: approved_by.map(str::to_string),
            recommendation,
            proposed_change_count: 0,
            proposed_changes,
            rollback_plan: event_policy_rollback_plan(rollback_changes),
            equivalence_gate: event_policy_equivalence_gate(),
            validation_status: validation.status,
            promotable: validation.promotable,
            promotion_gate: "benchmark_and_validation_required".to_string(),
            revision,
            event_kind: "event_policy_application_plan_only".to_string(),
        });
    }

    let propagated_version_task_ids = if apply && approved_by.is_some() {
        propagate_dependency_version_boundary(&mut workflow.tasks)
    } else {
        Vec::new()
    };
    let validation = validate_workflow(&workflow);
    if apply && approved_by.is_none() {
        let revision = workflow
            .revisions
            .last()
            .map(|item| item.revision)
            .unwrap_or(0);
        return Ok(EventPolicyApplicationReport {
            status: "event_policy_application_blocked_missing_approval".to_string(),
            schema_version: "forge.improve.event_policy_application.v1".to_string(),
            workflow_id: workflow.id,
            origin: origin.to_string(),
            dry_run: false,
            apply_requested: true,
            applied: false,
            approved_by: None,
            recommendation,
            proposed_change_count: proposed_changes.len(),
            proposed_changes,
            rollback_plan: event_policy_rollback_plan(rollback_changes),
            equivalence_gate: event_policy_equivalence_gate(),
            validation_status: validation.status,
            promotable: validation.promotable,
            promotion_gate: "approval_benchmark_and_validation_required".to_string(),
            revision,
            event_kind: "event_policy_application_blocked".to_string(),
        });
    }

    if !apply {
        let revision = workflow
            .revisions
            .last()
            .map(|item| item.revision)
            .unwrap_or(0);
        return Ok(EventPolicyApplicationReport {
            status: "event_policy_application_planned".to_string(),
            schema_version: "forge.improve.event_policy_application.v1".to_string(),
            workflow_id: workflow.id,
            origin: origin.to_string(),
            dry_run: true,
            apply_requested: false,
            applied: false,
            approved_by: approved_by.map(str::to_string),
            recommendation,
            proposed_change_count: proposed_changes.len(),
            proposed_changes,
            rollback_plan: event_policy_rollback_plan(rollback_changes),
            equivalence_gate: event_policy_equivalence_gate(),
            validation_status: validation.status,
            promotable: validation.promotable,
            promotion_gate: "benchmark_and_validation_required".to_string(),
            revision,
            event_kind: "event_policy_application_planned".to_string(),
        });
    }

    let revision = workflow
        .revisions
        .last()
        .map(|item| item.revision + 1)
        .unwrap_or(1);
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: "event_improvement_policy_applied".to_string(),
        summary: format!(
            "applied event improvement policy `{}` to {} task(s)",
            recommendation.recommended_policy,
            proposed_changes.len()
        ),
        created_at: Utc::now(),
    });
    store.save_workflow(&workflow)?;
    store.record_event(
        &workflow.id,
        "event_improvement_policy_applied",
        &json!({
            "schema_version": "forge.improve.event_policy_application.v1",
            "revision": revision,
            "origin": origin,
            "approved_by": approved_by,
            "recommendation_id": recommendation.id,
            "recommended_policy": recommendation.recommended_policy,
            "proposed_change_count": proposed_changes.len(),
            "proposed_changes": proposed_changes.clone(),
            "rollback_plan": event_policy_rollback_plan(rollback_changes.clone()),
            "equivalence_gate": event_policy_equivalence_gate(),
            "validation_status": validation.status.clone(),
            "promotable": validation.promotable,
            "propagated_version_task_ids": propagated_version_task_ids,
        }),
    )?;

    Ok(EventPolicyApplicationReport {
        status: "event_policy_application_applied".to_string(),
        schema_version: "forge.improve.event_policy_application.v1".to_string(),
        workflow_id: workflow.id,
        origin: origin.to_string(),
        dry_run: false,
        apply_requested: true,
        applied: true,
        approved_by: approved_by.map(str::to_string),
        recommendation,
        proposed_change_count: proposed_changes.len(),
        proposed_changes,
        rollback_plan: event_policy_rollback_plan(rollback_changes),
        equivalence_gate: event_policy_equivalence_gate(),
        validation_status: validation.status,
        promotable: validation.promotable,
        promotion_gate: "benchmark_and_validation_required".to_string(),
        revision,
        event_kind: "event_improvement_policy_applied".to_string(),
    })
}

pub fn benchmark_event_improvement_policy(
    store: &ForgeStore,
    workflow_id: &str,
    recommendation_id: Option<&str>,
    recommended_policy: Option<&str>,
    origin: &str,
) -> Result<EventPolicyBenchmarkReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let events = store.load_workflow_events(workflow_id)?;
    let Some(application_event) =
        select_event_policy_application_event(&events, recommendation_id, recommended_policy)
    else {
        bail!("no applied event improvement policy found for workflow `{workflow_id}`");
    };
    let recommended_policy = json_string(&application_event.data, "recommended_policy")
        .or_else(|| recommended_policy.map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let recommendation_id = json_string(&application_event.data, "recommendation_id")
        .or_else(|| recommendation_id.map(str::to_string));
    let application_revision = json_u64(&application_event.data, "revision").unwrap_or(0);
    let proposed_changes = application_event
        .data
        .get("proposed_changes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rollback_ready = application_event
        .data
        .get("rollback_plan")
        .is_some_and(event_policy_application_rollback_ready);

    let mut checks = Vec::new();
    let mut failures = Vec::new();
    for change in &proposed_changes {
        let Some(task_id) = json_string(change, "task_id") else {
            failures.push("proposed change without task_id".to_string());
            continue;
        };
        let Some(task) = workflow.tasks.iter().find(|task| task.id == task_id) else {
            failures.push(format!("task {task_id} no longer exists"));
            continue;
        };
        checks.push(format!("task {task_id} exists"));
        event_policy_check_task_change(task, change, &mut checks, &mut failures);
    }

    if proposed_changes.is_empty() {
        failures.push("application event did not record proposed changes".to_string());
    }
    if !rollback_ready {
        failures.push("rollback plan is missing or not ready".to_string());
    } else {
        checks.push("rollback plan is ready".to_string());
    }

    let validation = validate_workflow(&workflow);
    if validation.promotable {
        checks.push("workflow validation passed".to_string());
    } else {
        failures.push(format!(
            "workflow validation is not promotable: {}",
            validation.status
        ));
    }

    let task_shape_preserved = !proposed_changes.is_empty()
        && proposed_changes.iter().all(|change| {
            json_string(change, "task_id")
                .as_deref()
                .is_some_and(|task_id| workflow.tasks.iter().any(|task| task.id == task_id))
        });
    let executor_contract_applied = failures.iter().all(|failure| {
        !failure.contains("executor")
            && !failure.contains("execution policy")
            && !failure.contains("async policy")
            && !failure.contains("context requirement count")
            && !failure.contains("validation rule count")
            && !failure.contains("version")
    });
    let promotion_allowed = validation.promotable
        && rollback_ready
        && task_shape_preserved
        && executor_contract_applied
        && failures.is_empty();
    let equivalence_status = if promotion_allowed {
        "equivalent"
    } else {
        "review_required"
    };
    let status = if promotion_allowed {
        "event_policy_benchmark_validated"
    } else {
        "event_policy_benchmark_review_required"
    };
    let promotion_status = if promotion_allowed {
        "promotion_ready"
    } else {
        "promotion_blocked"
    };
    let report = EventPolicyBenchmarkReport {
        status: status.to_string(),
        schema_version: "forge.improve.event_policy_benchmark.v1".to_string(),
        workflow_id: workflow.id.clone(),
        origin: origin.to_string(),
        recommendation_id,
        recommended_policy,
        application_event_id: application_event.id,
        application_revision,
        benchmark: EventPolicyBenchmarkEvidence {
            validation_passed: validation.promotable,
            rollback_ready,
            proposed_change_count: proposed_changes.len(),
            checked_task_count: proposed_changes
                .iter()
                .filter_map(|change| json_string(change, "task_id"))
                .filter(|task_id| workflow.tasks.iter().any(|task| task.id == *task_id))
                .count(),
            failed_check_count: failures.len(),
            checks,
            failures: failures.clone(),
        },
        equivalence: EventPolicyBenchmarkEquivalence {
            status: equivalence_status.to_string(),
            promotion_allowed,
            task_shape_preserved,
            executor_contract_applied,
            rollback_ready,
            validation_passed: validation.promotable,
        },
        promotion_decision: EventPolicyPromotionDecision {
            status: promotion_status.to_string(),
            auto_promoted: false,
            requires_human_approval: true,
            promotion_gate: "human_approval_after_benchmark".to_string(),
            reason: if promotion_allowed {
                "benchmark and validation evidence allow a later governed promotion; this command does not auto-promote".to_string()
            } else {
                "benchmark evidence is incomplete or validation failed; keep promotion blocked"
                    .to_string()
            },
        },
        validation_status: validation.status,
        promotable: validation.promotable,
        event_kind: "event_improvement_policy_benchmarked".to_string(),
    };
    store.record_event(
        &workflow.id,
        "event_improvement_policy_benchmarked",
        &json!({
            "schema_version": report.schema_version.clone(),
            "origin": report.origin.clone(),
            "recommendation_id": report.recommendation_id.clone(),
            "recommended_policy": report.recommended_policy.clone(),
            "application_event_id": report.application_event_id,
            "application_revision": report.application_revision,
            "benchmark": report.benchmark.clone(),
            "equivalence": report.equivalence.clone(),
            "promotion_decision": report.promotion_decision.clone(),
            "validation_status": report.validation_status.clone(),
            "promotable": report.promotable,
        }),
    )?;
    Ok(report)
}

pub fn promote_event_improvement_policy(
    store: &ForgeStore,
    workflow_id: &str,
    recommendation_id: Option<&str>,
    recommended_policy: Option<&str>,
    approved_by: Option<&str>,
    origin: &str,
) -> Result<EventPolicyPromotionReport> {
    let approved_by = approved_by
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("forge improve promote-event-policy requires --approved-by")
        })?;
    let mut workflow = store.load_workflow(workflow_id)?;
    let events = store.load_workflow_events(workflow_id)?;
    let Some(benchmark_event) =
        select_event_policy_benchmark_event(&events, recommendation_id, recommended_policy)
    else {
        bail!("no benchmarked event improvement policy found for workflow `{workflow_id}`");
    };
    let recommended_policy = json_string(&benchmark_event.data, "recommended_policy")
        .or_else(|| recommended_policy.map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let recommendation_id = json_string(&benchmark_event.data, "recommendation_id")
        .or_else(|| recommendation_id.map(str::to_string));
    let application_event_id =
        json_i64(&benchmark_event.data, "application_event_id").unwrap_or_default();
    let application_revision = json_u64(&benchmark_event.data, "application_revision").unwrap_or(0);
    let benchmark = event_policy_promotion_benchmark_summary(&benchmark_event.data);
    let validation = validate_workflow(&workflow);

    if let Some(existing_promotion) =
        select_event_policy_promotion_event(&events, benchmark_event.id)
    {
        let revision = json_u64(&existing_promotion.data, "revision")
            .or_else(|| workflow.revisions.last().map(|item| item.revision))
            .unwrap_or(0);
        return Ok(EventPolicyPromotionReport {
            status: "event_policy_promotion_already_accepted".to_string(),
            schema_version: "forge.improve.event_policy_promotion.v1".to_string(),
            workflow_id: workflow.id,
            origin: origin.to_string(),
            approved_by: approved_by.to_string(),
            recommendation_id,
            recommended_policy,
            application_event_id,
            application_revision,
            benchmark_event_id: benchmark_event.id,
            benchmark,
            validation_status: validation.status,
            promoted: false,
            auto_promoted: false,
            revision,
            promotion_gate: "already_promoted".to_string(),
            reason: "event policy benchmark was already accepted through a governed promotion"
                .to_string(),
            event_kind: "event_improvement_policy_promoted".to_string(),
        });
    }

    let promotion_allowed = benchmark.promotion_allowed
        && benchmark.validation_passed
        && benchmark.rollback_ready
        && benchmark.failed_check_count == 0
        && validation.promotable;
    if !promotion_allowed {
        return Ok(EventPolicyPromotionReport {
            status: "event_policy_promotion_blocked".to_string(),
            schema_version: "forge.improve.event_policy_promotion.v1".to_string(),
            workflow_id: workflow.id,
            origin: origin.to_string(),
            approved_by: approved_by.to_string(),
            recommendation_id,
            recommended_policy,
            application_event_id,
            application_revision,
            benchmark_event_id: benchmark_event.id,
            benchmark,
            validation_status: validation.status,
            promoted: false,
            auto_promoted: false,
            revision: workflow
                .revisions
                .last()
                .map(|item| item.revision)
                .unwrap_or(0),
            promotion_gate: "benchmark_validation_and_human_approval_required".to_string(),
            reason: "benchmark evidence or current validation does not allow promotion".to_string(),
            event_kind: "event_improvement_policy_promotion_blocked".to_string(),
        });
    }

    let revision = workflow
        .revisions
        .last()
        .map(|item| item.revision + 1)
        .unwrap_or(1);
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: "event_improvement_policy_promoted".to_string(),
        summary: format!(
            "accepted benchmarked event improvement policy `{recommended_policy}` after approval by `{approved_by}`"
        ),
        created_at: Utc::now(),
    });
    store.save_workflow(&workflow)?;
    store.record_event(
        &workflow.id,
        "event_improvement_policy_promoted",
        &json!({
            "schema_version": "forge.improve.event_policy_promotion.v1",
            "revision": revision,
            "origin": origin,
            "approved_by": approved_by,
            "recommendation_id": recommendation_id,
            "recommended_policy": recommended_policy,
            "application_event_id": application_event_id,
            "application_revision": application_revision,
            "benchmark_event_id": benchmark_event.id,
            "benchmark": benchmark.clone(),
            "validation_status": validation.status.clone(),
            "promoted": true,
            "auto_promoted": false,
            "promotion_gate": "governed_human_approval_after_benchmark",
        }),
    )?;

    Ok(EventPolicyPromotionReport {
        status: "event_policy_promotion_accepted".to_string(),
        schema_version: "forge.improve.event_policy_promotion.v1".to_string(),
        workflow_id: workflow.id,
        origin: origin.to_string(),
        approved_by: approved_by.to_string(),
        recommendation_id,
        recommended_policy,
        application_event_id,
        application_revision,
        benchmark_event_id: benchmark_event.id,
        benchmark,
        validation_status: validation.status,
        promoted: true,
        auto_promoted: false,
        revision,
        promotion_gate: "governed_human_approval_after_benchmark".to_string(),
        reason: "benchmark evidence was accepted by a human operator and recorded as a workflow revision"
            .to_string(),
        event_kind: "event_improvement_policy_promoted".to_string(),
    })
}

fn select_event_policy_recommendation(
    recommendations: &[EventImprovementRecommendation],
    recommendation_id: Option<&str>,
    recommended_policy: Option<&str>,
) -> Option<EventImprovementRecommendation> {
    let recommendation_id = recommendation_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let recommended_policy = recommended_policy
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if recommendation_id.is_none() && recommended_policy.is_none() {
        return recommendations.first().cloned();
    }
    recommendations
        .iter()
        .find(|recommendation| {
            recommendation_id.is_none_or(|id| recommendation.id == id)
                && recommended_policy
                    .is_none_or(|policy| recommendation.recommended_policy == policy)
        })
        .cloned()
}

fn select_event_policy_application_event<'a>(
    events: &'a [StoreEvent],
    recommendation_id: Option<&str>,
    recommended_policy: Option<&str>,
) -> Option<&'a StoreEvent> {
    let recommendation_id = recommendation_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let recommended_policy = recommended_policy
        .map(str::trim)
        .filter(|value| !value.is_empty());
    events.iter().rev().find(|event| {
        event.kind == "event_improvement_policy_applied"
            && recommendation_id.is_none_or(|id| {
                json_string(&event.data, "recommendation_id").as_deref() == Some(id)
            })
            && recommended_policy.is_none_or(|policy| {
                json_string(&event.data, "recommended_policy").as_deref() == Some(policy)
            })
    })
}

fn select_event_policy_benchmark_event<'a>(
    events: &'a [StoreEvent],
    recommendation_id: Option<&str>,
    recommended_policy: Option<&str>,
) -> Option<&'a StoreEvent> {
    let recommendation_id = recommendation_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let recommended_policy = recommended_policy
        .map(str::trim)
        .filter(|value| !value.is_empty());
    events.iter().rev().find(|event| {
        event.kind == "event_improvement_policy_benchmarked"
            && recommendation_id.is_none_or(|id| {
                json_string(&event.data, "recommendation_id").as_deref() == Some(id)
            })
            && recommended_policy.is_none_or(|policy| {
                json_string(&event.data, "recommended_policy").as_deref() == Some(policy)
            })
    })
}

fn select_event_policy_promotion_event(
    events: &[StoreEvent],
    benchmark_event_id: i64,
) -> Option<&StoreEvent> {
    events.iter().rev().find(|event| {
        event.kind == "event_improvement_policy_promoted"
            && json_i64(&event.data, "benchmark_event_id") == Some(benchmark_event_id)
    })
}

fn event_policy_promotion_benchmark_summary(
    benchmark_data: &Value,
) -> EventPolicyPromotionBenchmarkSummary {
    let benchmark = benchmark_data.get("benchmark").unwrap_or(&Value::Null);
    let equivalence = benchmark_data.get("equivalence").unwrap_or(&Value::Null);
    EventPolicyPromotionBenchmarkSummary {
        status: json_string(equivalence, "status").unwrap_or_else(|| "unknown".to_string()),
        promotion_allowed: json_bool(equivalence, "promotion_allowed").unwrap_or(false),
        validation_passed: json_bool(benchmark, "validation_passed").unwrap_or(false),
        rollback_ready: json_bool(benchmark, "rollback_ready").unwrap_or(false),
        failed_check_count: json_u64(benchmark, "failed_check_count").unwrap_or(u64::MAX),
    }
}

fn event_policy_application_rollback_ready(rollback_plan: &Value) -> bool {
    let status_ready = json_string(rollback_plan, "status")
        .is_some_and(|status| status == "rollback_ready_for_human_approval");
    let change_count = json_u64(rollback_plan, "rollback_change_count").unwrap_or(0);
    let changes_present = rollback_plan
        .get("changes")
        .and_then(Value::as_array)
        .is_some_and(|changes| !changes.is_empty());
    status_ready && change_count > 0 && changes_present
}

fn event_policy_check_task_change(
    task: &AtomicTask,
    change: &Value,
    checks: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    let task_id = task.id.as_str();
    if json_u64(change, "new_version").is_some_and(|version| version == task.version) {
        checks.push(format!("task {task_id} version matches proposed change"));
    } else if let Some(version) = json_u64(change, "new_version") {
        failures.push(format!(
            "task {task_id} version {} does not match proposed version {version}",
            task.version
        ));
    }
    if json_string(change, "new_executor")
        .as_deref()
        .is_some_and(|executor| executor == executor_name(&task.executor))
    {
        checks.push(format!("task {task_id} executor contract applied"));
    } else if let Some(executor) = json_string(change, "new_executor") {
        failures.push(format!(
            "task {task_id} executor {} does not match proposed executor {executor}",
            executor_name(&task.executor)
        ));
    }
    if json_string(change, "new_execution_policy_mode")
        .as_deref()
        .is_some_and(|mode| mode == task.execution_policy.mode)
    {
        checks.push(format!("task {task_id} execution policy mode applied"));
    } else if let Some(mode) = json_string(change, "new_execution_policy_mode") {
        failures.push(format!(
            "task {task_id} execution policy mode {} does not match proposed mode {mode}",
            task.execution_policy.mode
        ));
    }
    if let Some(reuse_hint) = json_string(change, "new_execution_policy_reuse_hint") {
        if reuse_hint == task.execution_policy.reuse_hint {
            checks.push(format!(
                "task {task_id} execution policy reuse hint applied"
            ));
        } else {
            failures.push(format!(
                "task {task_id} execution policy reuse hint {} does not match proposed hint {reuse_hint}",
                task.execution_policy.reuse_hint
            ));
        }
    }
    if let Some(async_mode) = json_string(change, "new_async_policy_mode") {
        if async_mode == task.async_policy.mode {
            checks.push(format!("task {task_id} async policy mode applied"));
        } else {
            failures.push(format!(
                "task {task_id} async policy mode {} does not match proposed mode {async_mode}",
                task.async_policy.mode
            ));
        }
    }
    if let Some(context_count) = json_u64(change, "new_context_requirement_count") {
        if task.context_requirements.len() as u64 >= context_count {
            checks.push(format!(
                "task {task_id} context requirement count is covered"
            ));
        } else {
            failures.push(format!(
                "task {task_id} context requirement count {} is below proposed count {context_count}",
                task.context_requirements.len()
            ));
        }
    }
    if let Some(rule_count) = json_u64(change, "new_validation_rule_count") {
        if task.validation_rules.len() as u64 >= rule_count {
            checks.push(format!("task {task_id} validation rule count is covered"));
        } else {
            failures.push(format!(
                "task {task_id} validation rule count {} is below proposed count {rule_count}",
                task.validation_rules.len()
            ));
        }
    }
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn json_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn apply_event_policy_recommendation_to_task(
    task: &mut AtomicTask,
    recommendation: &EventImprovementRecommendation,
) -> Vec<String> {
    let mut changed_fields = Vec::new();
    match recommendation.recommended_policy.as_str() {
        "prefer_deterministic_node" => {
            if task.executor == ExecutorKind::Command
                && task.execution_policy.deterministic
                && !task.execution_policy.ai_allowed
            {
                return changed_fields;
            }
            task.executor = ExecutorKind::Command;
            task.execution_policy = deterministic_execution_policy(
                "applied event improvement policy: repeated observed execution can be benchmarked as deterministic work",
            );
            task.cost.estimated_cost_usd = normalized_command_cost(task);
            mark_event_policy_changed_field(&mut changed_fields, "executor");
            mark_event_policy_changed_field(&mut changed_fields, "execution_policy");
            mark_event_policy_changed_field(&mut changed_fields, "cost");
        }
        "add_validation_or_rework_gate" => {
            let marker = format!("event_policy_rework_gate:{}", recommendation.id);
            if !task.validation_rules.iter().any(|rule| {
                rule.kind == "event_policy_rework_gate" && rule.expected.contains(&marker)
            }) {
                task.validation_rules.push(ValidationRule {
                    kind: "event_policy_rework_gate".to_string(),
                    command: None,
                    expected: format!(
                        "{marker}: inspect retry evidence before repeating execution; return needs_retry with exact missing contract when validation fails"
                    ),
                });
                mark_event_policy_changed_field(&mut changed_fields, "validation_rules");
            }
            if push_event_policy_context_requirement(
                &mut task.context_requirements,
                format!(
                    "event_policy_retry_evidence:{}: inspect retry count {} and last event sequence {} before executor handoff",
                    recommendation.id,
                    recommendation.total_retry_count,
                    recommendation.last_event_sequence
                ),
            ) {
                mark_event_policy_changed_field(&mut changed_fields, "context_requirements");
            }
        }
        "tighten_context_routing" => {
            if push_event_policy_context_requirement(
                &mut task.context_requirements,
                format!(
                    "event_policy_context_routing:{}: use node-scoped shards, compression, cache or governed memory search before executor handoff",
                    recommendation.id
                ),
            ) {
                mark_event_policy_changed_field(&mut changed_fields, "context_requirements");
            }
            if task.execution_policy.reuse_hint != "event_policy_context_cache" {
                task.execution_policy.reuse_hint = "event_policy_context_cache".to_string();
                task.execution_policy.selection_reason = format!(
                    "event improvement policy requested tighter context routing after max pressure {} bps",
                    recommendation.max_context_pressure_bps.unwrap_or_default()
                );
                mark_event_policy_changed_field(&mut changed_fields, "execution_policy");
            }
        }
        "supervise_wait_or_external_dependency" => {
            if push_event_policy_context_requirement(
                &mut task.context_requirements,
                format!(
                    "event_policy_wait_supervision:{}: route recurring wait evidence to event runtime reconciliation before manual retry",
                    recommendation.id
                ),
            ) {
                mark_event_policy_changed_field(&mut changed_fields, "context_requirements");
            }
            if task.async_policy.mode != "event_supervised_wait" {
                task.async_policy.mode = "event_supervised_wait".to_string();
                mark_event_policy_changed_field(&mut changed_fields, "async_policy");
            }
            if task.async_policy.resume_strategy != "event_runtime_reconcile_or_manual_resume" {
                task.async_policy.resume_strategy =
                    "event_runtime_reconcile_or_manual_resume".to_string();
                mark_event_policy_changed_field(&mut changed_fields, "async_policy");
            }
            if !task
                .async_policy
                .run_substrates
                .iter()
                .any(|substrate| substrate == "forge_event_runtime_daemon")
            {
                task.async_policy
                    .run_substrates
                    .push("forge_event_runtime_daemon".to_string());
                mark_event_policy_changed_field(&mut changed_fields, "async_policy");
            }
        }
        _ => {}
    }
    changed_fields
}

fn mark_event_policy_changed_field(changed_fields: &mut Vec<String>, field: &str) {
    if !changed_fields.iter().any(|existing| existing == field) {
        changed_fields.push(field.to_string());
    }
}

fn push_event_policy_context_requirement(
    context_requirements: &mut Vec<String>,
    requirement: String,
) -> bool {
    if context_requirements
        .iter()
        .any(|existing| existing == &requirement)
    {
        false
    } else {
        context_requirements.push(requirement);
        true
    }
}

fn event_policy_proposed_change(
    previous_task: &AtomicTask,
    proposed_task: &AtomicTask,
    recommendation: &EventImprovementRecommendation,
    changed_fields: Vec<String>,
) -> EventPolicyProposedChange {
    EventPolicyProposedChange {
        task_id: previous_task.id.clone(),
        title: previous_task.title.clone(),
        policy: recommendation.recommended_policy.clone(),
        changed_fields,
        previous_executor: executor_name(&previous_task.executor),
        new_executor: executor_name(&proposed_task.executor),
        previous_execution_policy_mode: previous_task.execution_policy.mode.clone(),
        new_execution_policy_mode: proposed_task.execution_policy.mode.clone(),
        previous_execution_policy_reuse_hint: previous_task.execution_policy.reuse_hint.clone(),
        new_execution_policy_reuse_hint: proposed_task.execution_policy.reuse_hint.clone(),
        previous_async_policy_mode: previous_task.async_policy.mode.clone(),
        new_async_policy_mode: proposed_task.async_policy.mode.clone(),
        previous_async_resume_strategy: previous_task.async_policy.resume_strategy.clone(),
        new_async_resume_strategy: proposed_task.async_policy.resume_strategy.clone(),
        previous_context_requirement_count: previous_task.context_requirements.len(),
        new_context_requirement_count: proposed_task.context_requirements.len(),
        previous_validation_rule_count: previous_task.validation_rules.len(),
        new_validation_rule_count: proposed_task.validation_rules.len(),
        previous_estimated_cost_usd: previous_task.cost.estimated_cost_usd,
        new_estimated_cost_usd: proposed_task.cost.estimated_cost_usd,
        previous_version: previous_task.version,
        new_version: proposed_task.version,
        reason: recommendation.reason.clone(),
    }
}

fn event_policy_rollback_change(previous_task: &AtomicTask) -> EventPolicyRollbackChange {
    EventPolicyRollbackChange {
        task_id: previous_task.id.clone(),
        restore_executor: executor_name(&previous_task.executor),
        restore_execution_policy_mode: previous_task.execution_policy.mode.clone(),
        restore_execution_policy: previous_task.execution_policy.clone(),
        restore_async_policy: previous_task.async_policy.clone(),
        restore_context_requirements: previous_task.context_requirements.clone(),
        restore_validation_rules: previous_task.validation_rules.clone(),
        restore_estimated_cost_usd: previous_task.cost.estimated_cost_usd,
        restore_version: previous_task.version,
    }
}

fn event_policy_rollback_plan(changes: Vec<EventPolicyRollbackChange>) -> EventPolicyRollbackPlan {
    EventPolicyRollbackPlan {
        status: if changes.is_empty() {
            "no_rollback_changes".to_string()
        } else {
            "rollback_ready_for_human_approval".to_string()
        },
        requires_human_approval: true,
        rollback_change_count: changes.len(),
        changes,
    }
}

fn event_policy_equivalence_gate() -> EventPolicyEquivalenceGate {
    EventPolicyEquivalenceGate {
        status: "pending_benchmark".to_string(),
        required: true,
        benchmark_required: true,
        validation_required: true,
        promotion_allowed: false,
        checks: vec![
            "run the previous and proposed node behavior against the same input set".to_string(),
            "compare output hashes, validation artifacts and user-facing acceptance criteria"
                .to_string(),
            "promote only after validation passes and benchmark evidence is attached".to_string(),
            "keep rollback ready until the next validated workflow version is accepted".to_string(),
        ],
    }
}

fn event_policy_noop_report(
    workflow: Workflow,
    origin: &str,
    apply: bool,
    approved_by: Option<&str>,
    recommendation: EventImprovementRecommendation,
    status: &str,
    event_kind: &str,
) -> EventPolicyApplicationReport {
    let validation = validate_workflow(&workflow);
    let revision = workflow
        .revisions
        .last()
        .map(|item| item.revision)
        .unwrap_or(0);
    EventPolicyApplicationReport {
        status: status.to_string(),
        schema_version: "forge.improve.event_policy_application.v1".to_string(),
        workflow_id: workflow.id,
        origin: origin.to_string(),
        dry_run: !apply,
        apply_requested: apply,
        applied: false,
        approved_by: approved_by.map(str::to_string),
        recommendation,
        proposed_change_count: 0,
        proposed_changes: Vec::new(),
        rollback_plan: event_policy_rollback_plan(Vec::new()),
        equivalence_gate: event_policy_equivalence_gate(),
        validation_status: validation.status,
        promotable: validation.promotable,
        promotion_gate: "benchmark_and_validation_required".to_string(),
        revision,
        event_kind: event_kind.to_string(),
    }
}

fn executor_name(executor: &ExecutorKind) -> String {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
    .to_string()
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
    if has("support_only_output_risk") {
        "update_goal_or_tasks_with_user_facing_deliverables"
    } else if has("stale_running_run") || has("missing_runtime_heartbeat") {
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
    if has_reason(reasons, "support_only_output_risk") {
        commands.push(vec![
            "forge".to_string(),
            "workflow".to_string(),
            "update-goal".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--goal".to_string(),
            "<goal with explicit user-facing deliverables>".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ]);
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
    let event_policy_report = build_event_improvement_policy(
        store,
        Some(&workflow.id),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(5),
        None,
    )?;
    let event_improvement_policy = ImprovementProposalEventPolicy {
        schema_version: event_policy_report.schema_version.clone(),
        status: event_policy_report.status.clone(),
        index_source: event_policy_report.index_source.clone(),
        recommendation_count: event_policy_report.recommendation_count,
        recommendations: event_policy_report.recommendations.clone(),
    };
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
    let mut candidate_changes = vec![
        "evolve task structure with backlog state, subtasks, impediments, ownership and acceptance criteria".to_string(),
        "version prompt packets so executor instructions can be benchmarked and rolled back".to_string(),
        "add process-level workflow policies for Scrum/SAFe-style planning, blocked work and promotion readiness".to_string(),
        "generate a strong changelog for every version with validation evidence, risk notes and migration guidance".to_string(),
    ];
    if event_improvement_policy.recommendation_count > 0 {
        for recommendation in &event_improvement_policy.recommendations {
            let target = recommendation
                .node_ref
                .as_deref()
                .or(recommendation.addon_id.as_deref())
                .unwrap_or("_workflow");
            candidate_changes.push(format!(
                "evaluate event policy `{}` for {} `{}` with controlled benchmark evidence before changing runtime behavior",
                recommendation.recommended_policy, recommendation.scope, target
            ));
        }
    }
    let mut metrics_used = vec![
        "completion_rate".to_string(),
        "recovery_rate".to_string(),
        "context_efficiency".to_string(),
        "validation_pass_rate".to_string(),
        "execution_latency".to_string(),
        "blocked_work_age".to_string(),
        "impediment_resolution_rate".to_string(),
        "prompt_regression_rate".to_string(),
    ];
    if event_improvement_policy.recommendation_count > 0 {
        metrics_used.extend([
            "event_observability_policy".to_string(),
            "event_policy_recommendation_count".to_string(),
            "event_policy_priority".to_string(),
        ]);
    }
    let payload = json!({
        "workflow_id": workflow.id,
        "generated_at": Utc::now().to_rfc3339(),
        "status": "experiment_generated",
        "auto_promoted": false,
        "promotion_gate": "benchmark_and_validation_required",
        "target_version": target_version,
        "baseline_validation_status": validation.status,
        "evolution_domains": evolution_domains,
        "metrics_used": metrics_used,
        "candidate_changes": candidate_changes,
        "event_improvement_policy": &event_improvement_policy,
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
        render_changelog(
            &target_version,
            workflow,
            &candidate_changes,
            &event_improvement_policy,
        ),
    )?;
    store.record_event(
        &workflow.id,
        "improvement_experiment_generated",
        &json!({
            "artifact_path": relative_path,
            "changelog_path": changelog_path,
            "sha256": sha256,
            "event_policy_recommendation_count": event_improvement_policy.recommendation_count
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
        metrics_used,
        event_improvement_policy,
    })
}

fn render_changelog(
    target_version: &str,
    workflow: &Workflow,
    candidate_changes: &[String],
    event_improvement_policy: &ImprovementProposalEventPolicy,
) -> String {
    let event_policy_section = render_event_policy_changelog_section(event_improvement_policy);
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
            .join("\n"),
        event_policy_section
    )
}

fn render_event_policy_changelog_section(
    event_improvement_policy: &ImprovementProposalEventPolicy,
) -> String {
    if event_improvement_policy.recommendation_count == 0 {
        return "## Event Improvement Policy\n\n- No event policy recommendations were selected for this experiment.".to_string();
    }
    let recommendations = event_improvement_policy
        .recommendations
        .iter()
        .map(|recommendation| {
            let target = recommendation
                .node_ref
                .as_deref()
                .or(recommendation.addon_id.as_deref())
                .unwrap_or("_workflow");
            format!(
                "- `{}` for {} `{}`: {}",
                recommendation.recommended_policy,
                recommendation.scope,
                target,
                recommendation.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "## Event Improvement Policy\n\n- Source: `{}`\n- Recommendations: `{}`\n{}",
        event_improvement_policy.index_source,
        event_improvement_policy.recommendation_count,
        recommendations
    )
}
