use crate::adapter::{validate_executor_response_file, ExecutorResponseValidationReport};
use crate::addon::{default_addon_dirs, load_addon_catalog_from_store};
use crate::artifact::{list_workflow_artifacts, write_json_artifact};
use crate::checkpoint::{load_workflow_checkpoints, TaskCheckpoint};
use crate::context::{
    build_context_handoff_summary, ContextHandoffSummary, ContextHandoffTask,
    DEFAULT_CONTEXT_BUDGET,
};
use crate::graph::{
    create_workflow, task, AtomicTask, ExecutorKind, TaskStatus, ValidationRule, Workflow,
    WorkflowRevision,
};
use crate::identity::{
    ensure_operating_context_policy, ensure_workflow_policy, load_project_operating_context,
};
use crate::intent::{parse_intent, parse_intent_with_catalog_and_context};
use crate::outcome::{
    assess_workflow_outcome, assess_workflow_outcome_with_evidence,
    is_final_completion_audit_artifact, workflow_has_explicit_final_criteria,
    workflow_requires_final_outcome_audit, OutcomeEvidenceDeliverable, OutcomeStatusReport,
    FINAL_COMPLETION_AUDIT_KIND,
};
use crate::registry::{
    attach_reuse_candidates_as_child_subflows, find_reuse_candidates, WorkflowReuseCandidate,
};
use crate::storage::ForgeStore;
use crate::workflow::ArtifactAttachReport;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const COMPLETION_AUDIT_HANDOFF_CONTEXT_BUDGET: usize = 4096;
const REWORK_HANDOFF_CONTEXT_BUDGET: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub run_id: String,
    pub workflow_id: String,
    pub status: String,
    pub goal: String,
    pub origin: String,
    #[serde(rename = "async")]
    pub async_run: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_executor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_ttl_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_fallbacks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_switches: Vec<ExecutorSwitchRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestStartReport {
    pub status: String,
    pub run_id: String,
    pub workflow_id: String,
    pub goal: String,
    pub origin: String,
    #[serde(rename = "async")]
    pub async_run: bool,
    pub flow_resolution: FlowResolutionReport,
    pub handoff_contract: AgentHandoffContract,
    pub reuse_candidates: Vec<WorkflowReuseCandidate>,
    pub attached_subflows: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowResolutionReport {
    pub schema_version: String,
    pub searched_existing_flows: bool,
    pub decision: String,
    pub decision_summary: String,
    pub candidate_count: usize,
    pub attachable_candidate_count: usize,
    pub attached_subflow_count: usize,
    pub reused_workflow_ids: Vec<String>,
    pub selected_existing_workflow_id: Option<String>,
    pub created_workflow_id: String,
    pub self_evolution_is_default: bool,
    pub policy: FlowResolutionPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowResolutionPolicy {
    pub reuse_existing_subflows_by_default: bool,
    pub create_new_flow_only_when_needed: bool,
    pub self_run_evolution_is_ordinary_flow: bool,
    pub preserve_user_requested_flow_scope: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestStatusReport {
    pub status: String,
    pub run_id: String,
    pub workflow_id: String,
    pub goal: String,
    pub requested_goal: String,
    pub origin: String,
    #[serde(rename = "async")]
    pub async_run: bool,
    pub workflow_status: String,
    pub workflow_revision: u64,
    pub artifact_count: usize,
    pub checkpoint_count: usize,
    pub latest_checkpoint: Option<TaskCheckpoint>,
    pub task_summary: TaskStatusSummary,
    pub outcome_status: OutcomeStatusReport,
    pub activity: RunActivity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_fallbacks: Vec<String>,
    pub executor_switch_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_executor_switch: Option<ExecutorSwitchRecord>,
    pub handoff_summary: ContextHandoffSummary,
    pub latest_executor_policy: Option<RequestExecutorPolicySummary>,
    pub latest_validation_evidence: Option<ValidationEvidenceSummary>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestResumeReport {
    pub status: String,
    pub run_id: String,
    pub workflow_id: String,
    pub origin: String,
    pub resumed_at: DateTime<Utc>,
    pub request_status: RequestStatusReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestHeartbeatReport {
    pub status: String,
    pub run_id: String,
    pub workflow_id: String,
    pub previous_status: String,
    pub origin: String,
    pub activity: RunActivity,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestDriveReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub run_id: String,
    pub workflow_id: String,
    pub executor: String,
    pub origin: String,
    pub activity: RunActivity,
    pub task_summary: TaskStatusSummary,
    pub outcome_status: OutcomeStatusReport,
    pub checkpoint_count: usize,
    pub latest_checkpoint: Option<TaskCheckpoint>,
    pub rework: Option<RequestDriveRework>,
    pub handoff_task: Option<RequestDriveTask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parallel_handoff_tasks: Vec<RequestDriveTask>,
    pub blocked_tasks: Vec<RequestDriveBlockedTask>,
    pub next_command: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parallel_next_commands: Vec<Vec<String>>,
    pub final_delivery_package: Option<RequestFinalDeliveryPackageReport>,
    pub reason: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestDriveRework {
    pub task_id: String,
    pub response_status: String,
    pub response_sha256: Option<String>,
    pub revision: Option<u64>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestDriveTask {
    pub task_id: String,
    pub title: String,
    pub executor: String,
    pub handoff_status: String,
    pub context_sha256: String,
    pub context_routing_cache_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestDriveBlockedTask {
    pub task_id: String,
    pub title: String,
    pub handoff_status: String,
    pub blocking_refs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RequestStepReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub run_id: String,
    pub workflow_id: String,
    pub executor: String,
    pub origin: String,
    pub activity: RunActivity,
    pub stepped_task: Option<RequestDriveTask>,
    pub output_artifact: Option<ArtifactAttachReport>,
    pub response_artifact_path: Option<String>,
    pub validation: Option<ExecutorResponseValidationReport>,
    pub drive_before: RequestDriveReport,
    pub drive_after: Option<RequestDriveReport>,
    pub reason: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RequestTaskCompletionInput<'a> {
    pub task_id: &'a str,
    pub executor: &'a str,
    pub summary: &'a str,
    pub artifact_paths: &'a [PathBuf],
    pub evidence_command: Option<&'a str>,
    pub evidence_summary: Option<&'a str>,
    pub estimated_usd: f64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub ttl_seconds: u64,
    pub origin: &'a str,
}

#[derive(Debug, Serialize)]
pub struct RequestTaskCompletionReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub run_id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub origin: String,
    pub trace_artifact: Option<ArtifactAttachReport>,
    pub attached_artifacts: Vec<ArtifactAttachReport>,
    pub response_artifact_path: Option<String>,
    pub validation: Option<ExecutorResponseValidationReport>,
    pub drive_before: RequestDriveReport,
    pub drive_after: Option<RequestDriveReport>,
    pub reason: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestFinalDeliveryPackageReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub run_id: String,
    pub workflow_id: String,
    pub origin: String,
    pub readiness: String,
    pub outcome_status: OutcomeStatusReport,
    pub task_summary: TaskStatusSummary,
    pub markdown_artifact: ArtifactAttachReport,
    pub json_artifact: ArtifactAttachReport,
    pub latest_validation_evidence: Option<ValidationEvidenceSummary>,
    pub reason: String,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestFinalAuditReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub workflow_id: String,
    pub origin: String,
    pub task_summary: TaskStatusSummary,
    pub outcome_status: OutcomeStatusReport,
    pub audit_task_id: Option<String>,
    pub audit_task_created: bool,
    pub audit_task_repaired: bool,
    pub next_command: Vec<String>,
    pub reason: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestExecutorSwitchReport {
    pub status: String,
    pub schema_version: String,
    pub run_id: String,
    pub workflow_id: String,
    pub previous_status: String,
    pub origin: String,
    pub previous_executor: Option<String>,
    pub new_executor: String,
    pub brain_switch_policy: BrainSwitchPolicyReport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_executors: Vec<String>,
    pub activity: RunActivity,
    pub executor_switch: ExecutorSwitchRecord,
    pub checkpoint_count: usize,
    pub latest_checkpoint: Option<TaskCheckpoint>,
    pub handoff_summary: ContextHandoffSummary,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSwitchPolicyReport {
    pub schema_version: String,
    pub orchestrator_brain: String,
    pub switch_scope: String,
    pub can_switch_without_stopping_workflow: bool,
    pub preserves_run_id: bool,
    pub preserves_workflow_id: bool,
    pub preserves_checkpoints: bool,
    pub preserves_user_directives: bool,
    pub node_brain_routing_source: String,
    pub node_brain_routing_mutation_command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestStaleRecoveryReport {
    pub status: String,
    pub schema_version: String,
    pub run_id: String,
    pub workflow_id: String,
    pub previous_status: String,
    pub previous_workflow_status: String,
    pub origin: String,
    pub activity: RunActivity,
    pub recovery: RunRecoveryRecommendation,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunActivity {
    pub schema_version: String,
    pub active: bool,
    pub heartbeat_status: String,
    pub process_status: String,
    pub process_alive: Option<bool>,
    pub executor: Option<String>,
    pub pid: Option<u32>,
    pub summary: Option<String>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub heartbeat_expires_at: Option<DateTime<Utc>>,
    pub heartbeat_ttl_seconds: Option<u64>,
    pub seconds_until_stale: Option<i64>,
    pub recovery: RunRecoveryRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorSwitchRecord {
    pub schema_version: String,
    pub from_executor: Option<String>,
    pub to_executor: String,
    pub from_pid: Option<u32>,
    pub to_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_executors: Vec<String>,
    pub previous_heartbeat_at: Option<DateTime<Utc>>,
    pub switched_at: DateTime<Utc>,
    pub origin: String,
    pub reason: String,
    pub summary: String,
    pub continuity_policy: ExecutorSwitchContinuityPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorSwitchContinuityPolicy {
    pub preserve_run_id: bool,
    pub preserve_workflow_id: bool,
    pub preserve_checkpoints: bool,
    pub keep_workflow_running: bool,
    pub old_executor_shutdown_required: bool,
    pub user_directives_remain_authoritative: bool,
}

#[derive(Debug, Clone)]
pub struct RequestExecutorSwitchInput {
    pub executor: String,
    pub fallback_executors: Vec<String>,
    pub summary: String,
    pub ttl_seconds: u64,
    pub pid: Option<u32>,
    pub origin: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunRecoveryRecommendation {
    pub schema_version: String,
    pub action: String,
    pub target_status: String,
    pub reason: String,
    pub confidence: f32,
    pub requires_human_approval: bool,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentHandoffContract {
    pub schema_version: String,
    pub run_id: String,
    pub workflow_id: String,
    pub origin: String,
    pub flow_resolution: FlowResolutionReport,
    pub policy: AgentHandoffPolicy,
    pub allowed_context: AgentAllowedContext,
    pub validation_rules: Vec<String>,
    pub artifact_refs: Vec<String>,
    pub status_poll: AgentStatusPoll,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentHandoffPolicy {
    pub execution_authority: String,
    #[serde(rename = "async")]
    pub async_run: bool,
    pub source_of_truth: String,
    pub executor_policy_required: bool,
    pub validation_before_promotion: bool,
    pub user_directives_remain_authoritative: bool,
    pub executor_hot_swap_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentAllowedContext {
    pub tool: String,
    pub command: Vec<String>,
    pub default_budget: usize,
    pub strict_by_default: bool,
    pub allowed_scope: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatusPoll {
    pub tool: String,
    pub command: Vec<String>,
    pub returns: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaskStatusSummary {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub blocked: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationEvidenceSummary {
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub schema_version: String,
    pub prompt_packet_version: String,
    pub status: String,
    pub validation_passed: bool,
    pub cycle: u32,
    pub executor: String,
    pub command_summary: ValidationCommandSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestExecutorPolicySummary {
    pub schema_version: String,
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub cycle: u32,
    pub requested_executor: String,
    pub selected_executor: String,
    pub active_repair_status: String,
    pub quota_decision_summary: String,
    pub selected_candidate: Option<RequestExecutorPolicyCandidateSummary>,
    pub fallback_order: Vec<RequestExecutorPolicyCandidateSummary>,
    pub quota_preservation: Vec<String>,
    pub repair_goals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestExecutorPolicyCandidateSummary {
    pub executor: String,
    pub provider: String,
    pub model: Option<String>,
    pub local_vs_non_local: String,
    pub selection_tier: u32,
    pub selection_status: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ValidationCommandSummary {
    pub total: usize,
    pub planned: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Deserialize)]
struct ValidationEvidenceArtifact {
    schema_version: String,
    prompt_packet_version: String,
    status: String,
    validation_passed: bool,
    cycle: u32,
    executor: String,
    commands: Vec<ValidationCommandArtifact>,
}

#[derive(Debug, Deserialize)]
struct ValidationCommandArtifact {
    status: String,
}

#[derive(Debug, Deserialize)]
struct SelfEvolutionCycleArtifact {
    cycle: u32,
    requested_executor: String,
    executor: String,
    executor_policy: SelfEvolutionExecutorPolicyArtifact,
}

#[derive(Debug, Deserialize)]
struct SelfEvolutionExecutorPolicyArtifact {
    active_repair_status: String,
    quota_decision_summary: String,
    selected_candidate: Option<SelfEvolutionSelectedExecutorArtifact>,
    fallback_order: Vec<RequestExecutorPolicyCandidateSummary>,
    skipped_to_preserve_quota: Vec<String>,
    repair_goals: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SelfEvolutionSelectedExecutorArtifact {
    executor: String,
    provider: String,
    model: Option<String>,
    local_vs_non_local: String,
    selection_tier: u32,
    selection_status: String,
}

impl From<SelfEvolutionSelectedExecutorArtifact> for RequestExecutorPolicyCandidateSummary {
    fn from(candidate: SelfEvolutionSelectedExecutorArtifact) -> Self {
        Self {
            executor: candidate.executor,
            provider: candidate.provider,
            model: candidate.model,
            local_vs_non_local: candidate.local_vs_non_local,
            selection_tier: candidate.selection_tier,
            selection_status: candidate.selection_status,
        }
    }
}

pub fn start_pm_session(
    store: &ForgeStore,
    objective: &str,
    origin: &str,
) -> Result<RequestStartReport> {
    let mut intent = parse_intent(objective);
    intent.goal = format!("PM Session: {}", intent.goal);
    let mut workflow = create_workflow(intent);

    // Customize workflow for PM session
    workflow.tasks = vec![
        crate::graph::task(
            "pm-001",
            "Clarify challenge and users",
            &[],
            &["objective"],
            vec![],
            "User and Challenge matrix",
            (crate::graph::ExecutorKind::Ai, 0.01),
        ),
        crate::graph::task(
            "pm-002",
            "Identify constraints and risks",
            &["pm-001"],
            &["User and Challenge matrix"],
            vec![],
            "Constraint and Risk log",
            (crate::graph::ExecutorKind::Ai, 0.01),
        ),
        crate::graph::task(
            "pm-003",
            "Define success metrics and trade-offs",
            &["pm-002"],
            &["Constraint and Risk log"],
            vec![],
            "Success Metric dashboard",
            (crate::graph::ExecutorKind::Ai, 0.01),
        ),
        crate::graph::task(
            "pm-004",
            "Define MVP boundaries and validation strategy",
            &["pm-003"],
            &["Success Metric dashboard"],
            vec![],
            "MVP Roadmap",
            (crate::graph::ExecutorKind::Ai, 0.01),
        ),
        crate::graph::task(
            "pm-005",
            "Generate Product Decision artifacts",
            &["pm-004"],
            &["MVP Roadmap"],
            vec![],
            "Product Decision records",
            (crate::graph::ExecutorKind::Command, 0.001),
        ),
        crate::graph::task(
            "pm-006",
            "Convert decisions into executable backlog",
            &["pm-005"],
            &["Product Decision records"],
            vec![],
            "Execution Backlog",
            (crate::graph::ExecutorKind::Ai, 0.02),
        ),
    ];

    let reuse_candidates = Vec::new(); // PM sessions are unique
    let attached_subflows = 0;
    let flow_resolution =
        build_flow_resolution_report(&workflow, &reuse_candidates, attached_subflows);
    let run = create_run_record(&workflow, origin, "accepted");
    store.save_workflow(&workflow)?;
    save_run_record(store, &run)?;
    store.record_event(
        &workflow.id,
        "pm_session_started",
        &serde_json::json!({
            "run": run,
            "objective": objective,
        }),
    )?;
    let handoff_contract = build_agent_handoff_contract(&run, flow_resolution.clone());
    Ok(RequestStartReport {
        status: run.status,
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        goal: run.goal,
        origin: run.origin,
        async_run: run.async_run,
        flow_resolution,
        handoff_contract,
        reuse_candidates,
        attached_subflows,
    })
}

pub fn start_async_request(
    store: &ForgeStore,
    goal: &str,
    origin: &str,
) -> Result<RequestStartReport> {
    let project_root = std::env::current_dir()?;
    let addon_catalog = load_addon_catalog_from_store(store, &default_addon_dirs())?;
    let operating_context = load_project_operating_context(&project_root)?;
    ensure_operating_context_policy(store, &operating_context, "request start")?;
    let intent = parse_intent_with_catalog_and_context(goal, &addon_catalog, operating_context);
    let mut workflow = create_workflow(intent);
    let reuse_candidates = find_reuse_candidates(store, &workflow)?;
    let attached_subflows =
        attach_reuse_candidates_as_child_subflows(&mut workflow, &reuse_candidates);
    let flow_resolution =
        build_flow_resolution_report(&workflow, &reuse_candidates, attached_subflows);
    let run = create_run_record(&workflow, origin, "accepted");
    store.save_workflow(&workflow)?;
    save_run_record(store, &run)?;
    store.record_event(
        &workflow.id,
        "async_request_started",
        &serde_json::json!({
            "run": run,
            "flow_resolution": flow_resolution,
        }),
    )?;
    let handoff_contract = build_agent_handoff_contract(&run, flow_resolution.clone());
    Ok(RequestStartReport {
        status: run.status,
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        goal: run.goal,
        origin: run.origin,
        async_run: run.async_run,
        flow_resolution,
        handoff_contract,
        reuse_candidates,
        attached_subflows,
    })
}

fn build_flow_resolution_report(
    workflow: &Workflow,
    reuse_candidates: &[WorkflowReuseCandidate],
    attached_subflows: usize,
) -> FlowResolutionReport {
    let attachable_candidate_count = reuse_candidates
        .iter()
        .filter(|candidate| candidate.attachable_as_child_subflow)
        .count();
    let mut reused_workflow_ids = Vec::new();
    for candidate in reuse_candidates
        .iter()
        .filter(|candidate| candidate.attachable_as_child_subflow)
    {
        if !reused_workflow_ids.contains(&candidate.candidate_workflow_id) {
            reused_workflow_ids.push(candidate.candidate_workflow_id.clone());
        }
    }

    let decision = if attached_subflows > 0 {
        "create_new_flow_with_reused_child_subflows"
    } else if reuse_candidates.is_empty() {
        "create_new_flow"
    } else {
        "create_new_flow_without_attachable_reuse"
    };
    let decision_summary = match decision {
        "create_new_flow_with_reused_child_subflows" => format!(
            "Forge searched existing flows, created a request-specific workflow, and attached {attached_subflows} reusable child subflow(s)."
        ),
        "create_new_flow_without_attachable_reuse" => format!(
            "Forge searched existing flows and found {} candidate(s), but none were attachable under lifecycle and validation policy; a new workflow was created.",
            reuse_candidates.len()
        ),
        _ => "Forge searched existing flows and found no reusable match; a new workflow was created."
            .to_string(),
    };

    FlowResolutionReport {
        schema_version: "forge.flow_resolution.v1".to_string(),
        searched_existing_flows: true,
        decision: decision.to_string(),
        decision_summary,
        candidate_count: reuse_candidates.len(),
        attachable_candidate_count,
        attached_subflow_count: attached_subflows,
        reused_workflow_ids,
        selected_existing_workflow_id: None,
        created_workflow_id: workflow.id.clone(),
        self_evolution_is_default: false,
        policy: FlowResolutionPolicy {
            reuse_existing_subflows_by_default: true,
            create_new_flow_only_when_needed: true,
            self_run_evolution_is_ordinary_flow: true,
            preserve_user_requested_flow_scope: true,
        },
    }
}

pub fn create_run_record(workflow: &Workflow, origin: &str, status: &str) -> RunRecord {
    RunRecord {
        run_id: format!("run_{}", Uuid::new_v4().to_string().replace('-', "")),
        workflow_id: workflow.id.clone(),
        status: status.to_string(),
        goal: workflow.goal.clone(),
        origin: origin.to_string(),
        async_run: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        active_executor: None,
        executor_pid: None,
        progress_summary: None,
        last_heartbeat_at: None,
        heartbeat_expires_at: None,
        heartbeat_ttl_seconds: None,
        executor_fallbacks: Vec::new(),
        executor_switches: Vec::new(),
    }
}

pub fn save_run_record(store: &ForgeStore, run: &RunRecord) -> Result<()> {
    store.save_run(
        &run.run_id,
        &run.workflow_id,
        &run.status,
        &serde_json::to_value(run)?,
    )
}

pub fn update_run_status(
    store: &ForgeStore,
    run_id: &str,
    status: &str,
    origin: &str,
) -> Result<RunRecord> {
    let mut run = load_run_record_for_action(store, run_id, "run status update")?;
    let previous_status = run.status.clone();
    run.status = status.to_string();
    run.updated_at = Utc::now();
    save_run_record(store, &run)?;
    store.record_event(
        &run.workflow_id,
        &format!("run_status_{status}"),
        &serde_json::json!({
            "run_id": run.run_id,
            "origin": origin,
            "previous_status": previous_status,
            "new_status": status,
            "updated_at": run.updated_at,
        }),
    )?;
    Ok(run)
}

fn mark_run_needs_attention_for_terminal_outcome(
    store: &ForgeStore,
    run: &RunRecord,
    workflow: &Workflow,
    origin: &str,
    reason: &str,
) -> Result<RunRecord> {
    let attention_at = Utc::now();
    let previous_status = run.status.clone();
    let previous_workflow_status = workflow.status.clone();
    let mut attention_run = run.clone();
    attention_run.status = "needs_attention".to_string();
    attention_run.active_executor = None;
    attention_run.executor_pid = None;
    attention_run.progress_summary = Some(reason.to_string());
    attention_run.last_heartbeat_at = None;
    attention_run.heartbeat_expires_at = None;
    attention_run.heartbeat_ttl_seconds = None;
    attention_run.updated_at = attention_at;
    save_run_record(store, &attention_run)?;

    let mut attention_workflow = workflow.clone();
    attention_workflow.status = "needs_attention".to_string();
    store.save_workflow(&attention_workflow)?;

    store.record_event(
        &attention_workflow.id,
        "terminal_outcome_needs_attention",
        &serde_json::json!({
            "run_id": attention_run.run_id,
            "origin": origin,
            "previous_status": previous_status,
            "new_status": attention_run.status,
            "previous_workflow_status": previous_workflow_status,
            "new_workflow_status": attention_workflow.status,
            "reason": reason,
            "updated_at": attention_at,
        }),
    )?;

    Ok(attention_run)
}

pub fn heartbeat_request(
    store: &ForgeStore,
    run_id: &str,
    executor: &str,
    summary: &str,
    ttl_seconds: u64,
    pid: Option<u32>,
    origin: &str,
) -> Result<RequestHeartbeatReport> {
    let mut run = load_run_record_for_action(store, run_id, "request heartbeat")?;
    let previous_status = run.status.clone();
    let heartbeat_at = Utc::now();
    let ttl_seconds = ttl_seconds.max(1);
    let expires_at = heartbeat_at + Duration::seconds(ttl_seconds.min(i64::MAX as u64) as i64);
    run.status = "running".to_string();
    run.active_executor = Some(executor.to_string());
    run.executor_pid = pid;
    run.progress_summary = Some(summary.to_string());
    run.last_heartbeat_at = Some(heartbeat_at);
    run.heartbeat_expires_at = Some(expires_at);
    run.heartbeat_ttl_seconds = Some(ttl_seconds);
    run.updated_at = heartbeat_at;
    save_run_record(store, &run)?;
    if let Ok(mut workflow) = store.load_workflow(&run.workflow_id) {
        workflow.status = "running".to_string();
        store.save_workflow(&workflow)?;
    }
    let activity = build_run_activity_at(&run, heartbeat_at);
    store.record_event(
        &run.workflow_id,
        "async_request_heartbeat",
        &serde_json::json!({
            "run_id": run.run_id,
            "origin": origin,
            "previous_status": previous_status,
            "new_status": run.status,
            "executor": executor,
            "pid": pid,
            "summary": summary,
            "last_heartbeat_at": heartbeat_at,
            "heartbeat_expires_at": expires_at,
            "heartbeat_ttl_seconds": ttl_seconds,
        }),
    )?;
    Ok(RequestHeartbeatReport {
        status: run.status,
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        previous_status,
        origin: origin.to_string(),
        activity,
        updated_at: heartbeat_at,
    })
}

pub fn drive_request(
    store: &ForgeStore,
    run_id: &str,
    executor: &str,
    ttl_seconds: u64,
    origin: &str,
) -> Result<RequestDriveReport> {
    let run = load_run_record_for_action(store, run_id, "request drive")?;
    let workflow = store.load_workflow(&run.workflow_id)?;
    let checkpoints = load_workflow_checkpoints(store, &workflow.id)?;
    let latest_checkpoint = latest_actionable_checkpoint(&workflow, &checkpoints);
    let task_summary = summarize_tasks(&workflow);
    let handoff_summary = build_context_handoff_summary(
        &workflow,
        request_drive_context_budget(&workflow),
        &checkpoints,
    )?;
    let outcome_status = request_outcome_status(store, &workflow)?;

    if let Some(rework) = latest_open_rework(store, &workflow)? {
        let heartbeat = heartbeat_request(
            store,
            run_id,
            executor,
            "forge drive evaluating next runnable action",
            ttl_seconds,
            None,
            origin,
        )?;
        let next_command = vec![
            "forge".to_string(),
            "task".to_string(),
            "handoff".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--task".to_string(),
            rework.task_id.clone(),
            "--executor".to_string(),
            executor.to_string(),
            "--ttl-seconds".to_string(),
            ttl_seconds.max(1).to_string(),
            "--budget".to_string(),
            REWORK_HANDOFF_CONTEXT_BUDGET.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ];
        return Ok(RequestDriveReport {
            schema_version: "forge.request_drive.v1".to_string(),
            status: "rework_required".to_string(),
            action: "rework_task".to_string(),
            run_id: run.run_id,
            workflow_id: workflow.id,
            executor: executor.to_string(),
            origin: origin.to_string(),
            activity: heartbeat.activity,
            task_summary,
            outcome_status,
            checkpoint_count: checkpoints.len(),
            latest_checkpoint,
            rework: Some(rework),
            handoff_task: None,
            parallel_handoff_tasks: Vec::new(),
            blocked_tasks: drive_blocked_tasks(&handoff_summary.tasks),
            next_command,
            parallel_next_commands: Vec::new(),
            final_delivery_package: None,
            reason: "Latest accepted executor response requested retry; rework must be handled before blind forward progress.".to_string(),
            updated_at: heartbeat.updated_at,
        });
    }

    if task_summary.completed == task_summary.total
        && task_summary.total > 0
        && outcome_status.status == "support_only"
        && (outcome_status.final_completion_audit_required
            || outcome_status.final_completion_audit_present)
    {
        let attention_reason = "All workflow tasks are complete, but the outcome is still support-only; the workflow needs explicit user-facing deliverables before final completion.";
        let attention_run = mark_run_needs_attention_for_terminal_outcome(
            store,
            &run,
            &workflow,
            origin,
            attention_reason,
        )?;
        let activity = build_run_activity(&attention_run);
        let next_command = vec![
            "forge".to_string(),
            "workflow".to_string(),
            "update-goal".to_string(),
            "--workflow".to_string(),
            workflow.id.clone(),
            "--goal".to_string(),
            "<goal with explicit user-facing deliverables>".to_string(),
            "--origin".to_string(),
            origin.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ];
        return Ok(RequestDriveReport {
            schema_version: "forge.request_drive.v1".to_string(),
            status: "blocked".to_string(),
            action: outcome_status.action.clone(),
            run_id: run.run_id,
            workflow_id: workflow.id,
            executor: executor.to_string(),
            origin: origin.to_string(),
            activity,
            task_summary,
            outcome_status,
            checkpoint_count: checkpoints.len(),
            latest_checkpoint,
            rework: None,
            handoff_task: None,
            parallel_handoff_tasks: Vec::new(),
            blocked_tasks: drive_blocked_tasks(&handoff_summary.tasks),
            next_command,
            parallel_next_commands: Vec::new(),
            final_delivery_package: None,
            reason: attention_reason.to_string(),
            updated_at: attention_run.updated_at,
        });
    }

    let heartbeat = heartbeat_request(
        store,
        run_id,
        executor,
        "forge drive evaluating next runnable action",
        ttl_seconds,
        None,
        origin,
    )?;
    let run = load_run_record(store, run_id)?;
    let mut workflow = store.load_workflow(&run.workflow_id)?;
    let checkpoints = load_workflow_checkpoints(store, &workflow.id)?;
    let latest_checkpoint = latest_actionable_checkpoint(&workflow, &checkpoints);
    let mut task_summary = summarize_tasks(&workflow);
    let mut handoff_summary = build_context_handoff_summary(
        &workflow,
        request_drive_context_budget(&workflow),
        &checkpoints,
    )?;
    let mut outcome_status = request_outcome_status(store, &workflow)?;

    if task_summary.completed == task_summary.total && task_summary.total > 0 {
        if let Some(reason) = final_completion_audit_block_reason(store, &workflow)? {
            if let Some(updated_workflow) =
                ensure_final_completion_audit_task(store, &workflow, origin, &reason)?
            {
                workflow = updated_workflow;
                task_summary = summarize_tasks(&workflow);
                handoff_summary = build_context_handoff_summary(
                    &workflow,
                    request_drive_context_budget(&workflow),
                    &checkpoints,
                )?;
                outcome_status = request_outcome_status(store, &workflow)?;
            } else {
                let next_command = vec![
                    "forge".to_string(),
                    "workflow".to_string(),
                    "attach-artifact".to_string(),
                    "--workflow".to_string(),
                    workflow.id.clone(),
                    "--path".to_string(),
                    "<final-completion-audit.json>".to_string(),
                    "--kind".to_string(),
                    FINAL_COMPLETION_AUDIT_KIND.to_string(),
                    "--origin".to_string(),
                    origin.to_string(),
                    "--output".to_string(),
                    "json".to_string(),
                ];
                store.record_event(
                    &workflow.id,
                    "completion_audit_required",
                    &serde_json::json!({
                        "run_id": run.run_id.clone(),
                        "origin": origin,
                        "reason": reason.clone(),
                        "required_artifact_kind": FINAL_COMPLETION_AUDIT_KIND,
                        "updated_at": heartbeat.updated_at,
                    }),
                )?;
                return Ok(RequestDriveReport {
                    schema_version: "forge.request_drive.v1".to_string(),
                    status: "completion_audit_required".to_string(),
                    action: "attach_final_completion_audit".to_string(),
                    run_id: run.run_id,
                    workflow_id: workflow.id,
                    executor: executor.to_string(),
                    origin: origin.to_string(),
                    activity: heartbeat.activity,
                    task_summary,
                    outcome_status,
                    checkpoint_count: checkpoints.len(),
                    latest_checkpoint,
                    rework: None,
                    handoff_task: None,
                    parallel_handoff_tasks: Vec::new(),
                    blocked_tasks: Vec::new(),
                    next_command,
                    parallel_next_commands: Vec::new(),
                    final_delivery_package: None,
                    reason,
                    updated_at: heartbeat.updated_at,
                });
            }
        }
    }

    if task_summary.completed == task_summary.total && task_summary.total > 0 {
        let completed_at = Utc::now();
        let mut completed_run = run.clone();
        let previous_status = completed_run.status.clone();
        completed_run.status = "completed".to_string();
        completed_run.updated_at = completed_at;
        save_run_record(store, &completed_run)?;

        let mut completed_workflow = workflow.clone();
        let previous_workflow_status = completed_workflow.status.clone();
        completed_workflow.status = "completed".to_string();
        store.save_workflow(&completed_workflow)?;

        store.record_event(
            &completed_workflow.id,
            "async_request_completed",
            &serde_json::json!({
                "run_id": completed_run.run_id.clone(),
                "origin": origin,
                "previous_status": previous_status,
                "new_status": completed_run.status.clone(),
                "previous_workflow_status": previous_workflow_status,
                "new_workflow_status": completed_workflow.status.clone(),
                "completed_at": completed_at,
            }),
        )?;
        let activity = build_run_activity_at(&completed_run, completed_at);
        let completion_reason = if workflow_requires_final_completion_audit(&completed_workflow) {
            "All workflow tasks are completed and final completion audit passed.".to_string()
        } else {
            "All workflow tasks are completed.".to_string()
        };
        let final_delivery_package = Some(create_final_delivery_package(
            store,
            &completed_run.run_id,
            origin,
        )?);
        return Ok(RequestDriveReport {
            schema_version: "forge.request_drive.v1".to_string(),
            status: "complete".to_string(),
            action: "none".to_string(),
            run_id: completed_run.run_id,
            workflow_id: completed_workflow.id.clone(),
            executor: executor.to_string(),
            origin: origin.to_string(),
            activity,
            task_summary,
            outcome_status: request_outcome_status(store, &completed_workflow)?,
            checkpoint_count: checkpoints.len(),
            latest_checkpoint,
            rework: None,
            handoff_task: None,
            parallel_handoff_tasks: Vec::new(),
            blocked_tasks: Vec::new(),
            next_command: Vec::new(),
            parallel_next_commands: Vec::new(),
            final_delivery_package,
            reason: completion_reason,
            updated_at: completed_at,
        });
    }

    let parallel_handoff_tasks = ready_handoff_tasks(&workflow, &handoff_summary.tasks);
    if let Some(task) = parallel_handoff_tasks.first().cloned() {
        let handoff_budget = handoff_context_budget_for_task(&workflow, &task.task_id);
        let next_command = handoff_command(
            &workflow.id,
            &task.task_id,
            executor,
            ttl_seconds,
            handoff_budget,
        );
        let parallel_next_commands = parallel_handoff_tasks
            .iter()
            .map(|task| {
                handoff_command(
                    &workflow.id,
                    &task.task_id,
                    executor,
                    ttl_seconds,
                    handoff_context_budget_for_task(&workflow, &task.task_id),
                )
            })
            .collect::<Vec<_>>();
        let parallel_ready_count = parallel_handoff_tasks.len();
        let action = if parallel_ready_count > 1 {
            "start_parallel_handoffs"
        } else {
            "start_handoff"
        };
        let reason = if parallel_ready_count > 1 {
            format!(
                "{parallel_ready_count} pending tasks have ready context and dependencies; start parallel executor handoffs within quota and resource limits."
            )
        } else {
            "A pending task has ready context and dependencies; start executor handoff.".to_string()
        };
        return Ok(RequestDriveReport {
            schema_version: "forge.request_drive.v1".to_string(),
            status: "ready_for_handoff".to_string(),
            action: action.to_string(),
            run_id: run.run_id,
            workflow_id: workflow.id,
            executor: executor.to_string(),
            origin: origin.to_string(),
            activity: heartbeat.activity,
            task_summary,
            outcome_status,
            checkpoint_count: checkpoints.len(),
            latest_checkpoint,
            rework: None,
            handoff_task: Some(task),
            parallel_handoff_tasks,
            blocked_tasks: drive_blocked_tasks(&handoff_summary.tasks),
            next_command,
            parallel_next_commands,
            final_delivery_package: None,
            reason,
            updated_at: heartbeat.updated_at,
        });
    }

    Ok(RequestDriveReport {
        schema_version: "forge.request_drive.v1".to_string(),
        status: "blocked".to_string(),
        action: "wait_or_repair_dependencies".to_string(),
        run_id: run.run_id,
        workflow_id: workflow.id,
        executor: executor.to_string(),
        origin: origin.to_string(),
        activity: heartbeat.activity,
        task_summary,
        outcome_status,
        checkpoint_count: checkpoints.len(),
        latest_checkpoint,
        rework: None,
        handoff_task: None,
        parallel_handoff_tasks: Vec::new(),
        blocked_tasks: drive_blocked_tasks(&handoff_summary.tasks),
        next_command: vec![
            "forge".to_string(),
            "request".to_string(),
            "status".to_string(),
            "--run".to_string(),
            run_id.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        parallel_next_commands: Vec::new(),
        final_delivery_package: None,
        reason: "No pending task is currently ready for handoff.".to_string(),
        updated_at: heartbeat.updated_at,
    })
}

pub fn step_request(
    store: &ForgeStore,
    run_id: &str,
    executor: &str,
    ttl_seconds: u64,
    origin: &str,
) -> Result<RequestStepReport> {
    let drive_before = drive_request(store, run_id, executor, ttl_seconds, origin)?;
    let run = load_run_record(store, run_id)?;
    let workflow = store.load_workflow(&run.workflow_id)?;
    let activity = drive_before.activity.clone();
    let updated_at = drive_before.updated_at;
    let Some(stepped_task) = drive_before.handoff_task.clone() else {
        return Ok(RequestStepReport {
            schema_version: "forge.request_step.v1".to_string(),
            status: "skipped".to_string(),
            action: "none".to_string(),
            run_id: run.run_id,
            workflow_id: workflow.id,
            executor: executor.to_string(),
            origin: origin.to_string(),
            activity,
            stepped_task: None,
            output_artifact: None,
            response_artifact_path: None,
            validation: None,
            drive_before,
            drive_after: None,
            reason: "request drive did not return a ready handoff task".to_string(),
            updated_at,
        });
    };

    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == stepped_task.task_id)
        .with_context(|| {
            format!(
                "request drive selected task {} but it is missing from workflow {}",
                stepped_task.task_id, workflow.id
            )
        })?;

    if !is_auto_steppable_task(task) {
        return Ok(RequestStepReport {
            schema_version: "forge.request_step.v1".to_string(),
            status: "handoff_required".to_string(),
            action: "start_handoff".to_string(),
            run_id: run.run_id,
            workflow_id: workflow.id,
            executor: executor.to_string(),
            origin: origin.to_string(),
            activity,
            stepped_task: Some(stepped_task),
            output_artifact: None,
            response_artifact_path: None,
            validation: None,
            drive_before,
            drive_after: None,
            reason: "ready task requires an external executor or explicit validation command; Forge will not fake execution".to_string(),
            updated_at,
        });
    }

    let generated_at = Utc::now();
    let timestamp = generated_at.format("%Y%m%dT%H%M%SZ");
    let output_payload =
        build_auto_step_output_payload(&workflow, task, executor, origin, generated_at);
    let output_relative_path = format!(
        "artifacts/{}/auto-step-output-{}-{}.json",
        workflow.id, task.id, timestamp
    );
    let (output_path, _) =
        write_json_artifact(&store.base_dir(), &output_relative_path, &output_payload)?;
    let output_artifact = crate::workflow::attach_workflow_artifact(
        store,
        &workflow.id,
        &output_path,
        "auto_step_output",
        origin,
    )?;

    let response_payload = serde_json::json!({
        "schema_version": "forge.executor_response.v1",
        "task_id": task.id,
        "status": "completed",
        "artifacts": [output_artifact.artifact.path.clone()],
        "trace_ref": format!("{run_id}/{}", task.id),
        "cost": {
            "estimated_usd": 0.0,
            "tokens_in": 0,
            "tokens_out": 0
        },
        "validation_evidence": [
            {
                "command": format!("forge request step --run {run_id} --executor {executor} --ttl-seconds {}", ttl_seconds.max(1)),
                "exit_code": 0,
                "summary": format!("Forge auto-stepped deterministic task {} and attached replayable output artifact {}.", task.id, output_artifact.artifact.path)
            }
        ]
    });
    let response_relative_path = format!(
        "artifacts/{}/auto-step-response-{}-{}.json",
        workflow.id, task.id, timestamp
    );
    let (response_path, _) = write_json_artifact(
        &store.base_dir(),
        &response_relative_path,
        &response_payload,
    )?;
    let validation =
        validate_executor_response_file(store, &workflow.id, &task.id, response_path.as_path())?;
    let drive_after = drive_request(store, run_id, executor, ttl_seconds, origin)?;

    Ok(RequestStepReport {
        schema_version: "forge.request_step.v1".to_string(),
        status: if validation.accepted {
            "stepped".to_string()
        } else {
            "validation_failed".to_string()
        },
        action: if validation.accepted {
            "auto_promoted_task".to_string()
        } else {
            "inspect_validation".to_string()
        },
        run_id: run.run_id,
        workflow_id: workflow.id,
        executor: executor.to_string(),
        origin: origin.to_string(),
        activity: drive_after.activity.clone(),
        stepped_task: Some(stepped_task),
        output_artifact: Some(output_artifact),
        response_artifact_path: Some(response_relative_path),
        validation: Some(validation),
        drive_before,
        drive_after: Some(drive_after),
        reason: "Forge executed a deterministic ready task through the normal executor-response validation path.".to_string(),
        updated_at: Utc::now(),
    })
}

pub fn complete_ready_task(
    store: &ForgeStore,
    run_id: &str,
    input: RequestTaskCompletionInput<'_>,
) -> Result<RequestTaskCompletionReport> {
    if input.summary.trim().is_empty() {
        anyhow::bail!("request task completion summary is required");
    }

    let drive_before = drive_request(
        store,
        run_id,
        input.executor,
        input.ttl_seconds,
        input.origin,
    )?;
    let run = load_run_record(store, run_id)?;
    let workflow = store.load_workflow(&run.workflow_id)?;
    let updated_at = drive_before.updated_at;
    let Some(handoff_task) = drive_before
        .parallel_handoff_tasks
        .iter()
        .find(|task| task.task_id == input.task_id)
        .cloned()
        .or_else(|| drive_before.handoff_task.clone())
    else {
        return Ok(RequestTaskCompletionReport {
            schema_version: "forge.request_task_completion.v1".to_string(),
            status: "not_ready".to_string(),
            action: "drive_request".to_string(),
            run_id: run.run_id,
            workflow_id: workflow.id,
            task_id: input.task_id.to_string(),
            executor: input.executor.to_string(),
            origin: input.origin.to_string(),
            trace_artifact: None,
            attached_artifacts: Vec::new(),
            response_artifact_path: None,
            validation: None,
            drive_before,
            drive_after: None,
            reason: "request drive did not return a ready handoff task".to_string(),
            updated_at,
        });
    };

    if handoff_task.task_id != input.task_id {
        let ready_task_ids = drive_before
            .parallel_handoff_tasks
            .iter()
            .map(|task| task.task_id.clone())
            .collect::<Vec<_>>();
        return Ok(RequestTaskCompletionReport {
            schema_version: "forge.request_task_completion.v1".to_string(),
            status: "not_ready".to_string(),
            action: "drive_request".to_string(),
            run_id: run.run_id,
            workflow_id: workflow.id,
            task_id: input.task_id.to_string(),
            executor: input.executor.to_string(),
            origin: input.origin.to_string(),
            trace_artifact: None,
            attached_artifacts: Vec::new(),
            response_artifact_path: None,
            validation: None,
            drive_before,
            drive_after: None,
            reason: format!(
                "ready handoff task is {}, not {}; parallel_ready_tasks={}",
                handoff_task.task_id,
                input.task_id,
                ready_task_ids.join(",")
            ),
            updated_at,
        });
    }

    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == input.task_id)
        .with_context(|| {
            format!(
                "request drive selected task {} but it is missing from workflow {}",
                input.task_id, workflow.id
            )
        })?;

    let mut attached_artifacts = Vec::new();
    for artifact_path in input.artifact_paths {
        attached_artifacts.push(crate::workflow::attach_workflow_artifact(
            store,
            &workflow.id,
            artifact_path,
            "executor_output",
            input.origin,
        )?);
    }

    let generated_at = Utc::now();
    let timestamp = generated_at.format("%Y%m%dT%H%M%SZ");
    let trace_payload = build_execution_trace_payload(ExecutionTracePayloadInput {
        workflow: &workflow,
        task,
        handoff_task: &handoff_task,
        run_id,
        completion: &input,
        attached_artifacts: &attached_artifacts,
        drive_before: &drive_before,
        generated_at,
    });
    let trace_relative_path = format!(
        "artifacts/{}/execution-trace-{}-{}.json",
        workflow.id, task.id, timestamp
    );
    let (trace_path, _) =
        write_json_artifact(&store.base_dir(), &trace_relative_path, &trace_payload)?;
    let trace_artifact = crate::workflow::attach_workflow_artifact(
        store,
        &workflow.id,
        &trace_path,
        "execution_trace",
        input.origin,
    )?;

    let mut response_artifacts = Vec::with_capacity(attached_artifacts.len() + 1);
    response_artifacts.push(trace_artifact.artifact.path.clone());
    response_artifacts.extend(
        attached_artifacts
            .iter()
            .map(|artifact| artifact.artifact.path.clone()),
    );

    let evidence_command = input.evidence_command.map(str::to_string).unwrap_or_else(|| {
        format!(
            "forge request complete-task --run {run_id} --task {} --executor {} --summary <executor-summary> --output json",
            input.task_id, input.executor
        )
    });
    let evidence_summary = input
        .evidence_summary
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or(input.summary);
    let response_payload = serde_json::json!({
        "schema_version": "forge.executor_response.v1",
        "task_id": task.id,
        "status": "completed",
        "artifacts": response_artifacts,
        "trace_ref": trace_artifact.artifact.path,
        "cost": {
            "estimated_usd": input.estimated_usd,
            "tokens_in": input.tokens_in,
            "tokens_out": input.tokens_out
        },
        "validation_evidence": [
            {
                "command": evidence_command,
                "exit_code": 0,
                "summary": evidence_summary
            }
        ]
    });
    let response_relative_path = format!(
        "artifacts/{}/executor-response-{}-{}.json",
        workflow.id, task.id, timestamp
    );
    let (response_path, _) = write_json_artifact(
        &store.base_dir(),
        &response_relative_path,
        &response_payload,
    )?;
    let validation =
        validate_executor_response_file(store, &workflow.id, &task.id, response_path.as_path())?;
    let drive_after = if validation.accepted {
        Some(drive_request(
            store,
            run_id,
            input.executor,
            input.ttl_seconds,
            input.origin,
        )?)
    } else {
        None
    };

    Ok(RequestTaskCompletionReport {
        schema_version: "forge.request_task_completion.v1".to_string(),
        status: if validation.accepted {
            "completed".to_string()
        } else {
            "validation_failed".to_string()
        },
        action: if validation.accepted {
            "promoted_task_and_drove_next_action".to_string()
        } else {
            "inspect_validation".to_string()
        },
        run_id: run.run_id,
        workflow_id: workflow.id,
        task_id: input.task_id.to_string(),
        executor: input.executor.to_string(),
        origin: input.origin.to_string(),
        trace_artifact: Some(trace_artifact),
        attached_artifacts,
        response_artifact_path: Some(response_relative_path),
        validation: Some(validation),
        drive_before,
        drive_after,
        reason: "Forge recorded executor evidence, generated a replayable execution trace, validated the response, and drove the run forward.".to_string(),
        updated_at: Utc::now(),
    })
}

pub fn create_final_delivery_package(
    store: &ForgeStore,
    run_id: &str,
    origin: &str,
) -> Result<RequestFinalDeliveryPackageReport> {
    let run = load_run_record_for_action(store, run_id, "final delivery package")?;
    let workflow = store.load_workflow(&run.workflow_id)?;
    let generated_at = Utc::now();
    let timestamp = generated_at.format("%Y%m%dT%H%M%SZ");
    let outcome_status = request_outcome_status(store, &workflow)?;
    let task_summary = summarize_tasks(&workflow);
    let latest_validation_evidence = load_latest_validation_evidence(store, &workflow.id)?;
    let listed_artifacts = list_workflow_artifacts(&store.base_dir(), &workflow.id)?;
    let (readiness, action, reason) =
        final_delivery_readiness(&outcome_status, &task_summary, &workflow.status);

    let package_context = FinalDeliveryPackageContext {
        run: &run,
        workflow: &workflow,
        outcome_status: &outcome_status,
        task_summary: &task_summary,
        latest_validation_evidence: latest_validation_evidence.as_ref(),
        listed_artifacts: &listed_artifacts,
        readiness: &readiness,
        reason: &reason,
        generated_at,
    };

    let package_payload = build_final_delivery_payload(&package_context);
    let json_relative_path = format!(
        "tmp/{}/final-delivery-package-{}.json",
        workflow.id, timestamp
    );
    let (json_path, _) =
        write_json_artifact(&store.base_dir(), &json_relative_path, &package_payload)?;
    let json_artifact = crate::workflow::attach_workflow_artifact(
        store,
        &workflow.id,
        &json_path,
        "final_delivery_package_json",
        origin,
    )?;

    let markdown = render_final_delivery_markdown(&package_context);
    let markdown_relative_path = format!(
        "tmp/{}/final-delivery-package-{}.md",
        workflow.id, timestamp
    );
    let markdown_path = write_text_artifact(
        &store.base_dir(),
        &markdown_relative_path,
        markdown.as_str(),
    )?;
    let markdown_artifact = crate::workflow::attach_workflow_artifact(
        store,
        &workflow.id,
        &markdown_path,
        "final_delivery_package",
        origin,
    )?;

    store.record_event(
        &workflow.id,
        "final_delivery_package_created",
        &serde_json::json!({
            "run_id": &run.run_id,
            "origin": origin,
            "readiness": &readiness,
            "markdown_artifact": &markdown_artifact.artifact.path,
            "json_artifact": &json_artifact.artifact.path,
            "generated_at": generated_at,
        }),
    )?;

    Ok(RequestFinalDeliveryPackageReport {
        schema_version: "forge.request_final_delivery_package.v1".to_string(),
        status: "final_delivery_package_created".to_string(),
        action,
        run_id: run.run_id,
        workflow_id: workflow.id,
        origin: origin.to_string(),
        readiness,
        outcome_status,
        task_summary,
        markdown_artifact,
        json_artifact,
        latest_validation_evidence,
        reason,
        generated_at,
    })
}

pub fn ensure_final_audit(
    store: &ForgeStore,
    workflow_id: &str,
    executor: &str,
    origin: &str,
) -> Result<RequestFinalAuditReport> {
    ensure_workflow_policy(store, workflow_id, "ensure final audit")?;
    let workflow = store.load_workflow(workflow_id)?;
    let updated_at = Utc::now();
    let Some(block_reason) = final_completion_audit_block_reason(store, &workflow)? else {
        return Ok(RequestFinalAuditReport {
            schema_version: "forge.request_final_audit.v1".to_string(),
            status: "final_audit_satisfied".to_string(),
            action: "none".to_string(),
            workflow_id: workflow.id.clone(),
            origin: origin.to_string(),
            task_summary: summarize_tasks(&workflow),
            outcome_status: request_outcome_status(store, &workflow)?,
            audit_task_id: final_completion_audit_task_id(&workflow),
            audit_task_created: false,
            audit_task_repaired: false,
            next_command: Vec::new(),
            reason: "Final completion audit is already satisfied or not required.".to_string(),
            updated_at,
        });
    };

    let existing_audit_task_id = final_completion_audit_task_id(&workflow);
    let audit_dependency_ids = final_completion_audit_dependency_ids(&workflow);
    if existing_audit_task_id.is_none()
        && !final_completion_audit_dependency_ids_completed(&workflow, &audit_dependency_ids)
    {
        return Ok(RequestFinalAuditReport {
            schema_version: "forge.request_final_audit.v1".to_string(),
            status: "final_audit_waiting_for_workflow_completion".to_string(),
            action: "continue_workflow".to_string(),
            workflow_id: workflow.id.clone(),
            origin: origin.to_string(),
            task_summary: summarize_tasks(&workflow),
            outcome_status: request_outcome_status(store, &workflow)?,
            audit_task_id: None,
            audit_task_created: false,
            audit_task_repaired: false,
            next_command: Vec::new(),
            reason: format!(
                "Final completion audit waits until required outcome evidence dependencies are complete. {block_reason}"
            ),
            updated_at,
        });
    }

    let maybe_updated =
        ensure_final_completion_audit_task(store, &workflow, origin, &block_reason)?;
    let active_workflow = maybe_updated.as_ref().unwrap_or(&workflow);
    let audit_task_created = maybe_updated.is_some() && existing_audit_task_id.is_none();
    let audit_task_repaired = maybe_updated.is_some() && existing_audit_task_id.is_some();
    let audit_task_id = final_completion_audit_task_id(active_workflow);
    let next_command = if let Some(task_id) = audit_task_id.as_deref() {
        let audit_task_is_completed = active_workflow
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .map(|task| task.status == TaskStatus::Completed)
            .unwrap_or(false);
        if audit_task_is_completed {
            final_completion_audit_attach_command(&active_workflow.id, origin)
        } else if final_completion_audit_dependencies_completed(active_workflow, task_id) {
            final_completion_audit_handoff_command(
                &active_workflow.id,
                task_id,
                executor,
                COMPLETION_AUDIT_HANDOFF_CONTEXT_BUDGET,
            )
        } else {
            Vec::new()
        }
    } else {
        final_completion_audit_attach_command(&active_workflow.id, origin)
    };
    let audit_waits_for_dependencies = audit_task_id.as_deref().is_some_and(|task_id| {
        !final_completion_audit_dependencies_completed(active_workflow, task_id)
    });
    let status = if audit_waits_for_dependencies {
        "final_audit_waiting_for_dependencies"
    } else if audit_task_created {
        "final_audit_task_created"
    } else if audit_task_repaired {
        "final_audit_task_dependencies_repaired"
    } else if audit_task_id.is_some() {
        "final_audit_task_ready"
    } else {
        "final_audit_artifact_required"
    };
    let action = if audit_waits_for_dependencies {
        "complete_prerequisites"
    } else if audit_task_id.is_some() {
        "handoff_final_completion_audit"
    } else {
        "attach_final_completion_audit"
    };

    Ok(RequestFinalAuditReport {
        schema_version: "forge.request_final_audit.v1".to_string(),
        status: status.to_string(),
        action: action.to_string(),
        workflow_id: active_workflow.id.clone(),
        origin: origin.to_string(),
        task_summary: summarize_tasks(active_workflow),
        outcome_status: request_outcome_status(store, active_workflow)?,
        audit_task_id,
        audit_task_created,
        audit_task_repaired,
        next_command,
        reason: block_reason,
        updated_at,
    })
}

fn final_delivery_readiness(
    outcome_status: &OutcomeStatusReport,
    task_summary: &TaskStatusSummary,
    workflow_status: &str,
) -> (String, String, String) {
    if matches!(
        outcome_status.status.as_str(),
        "final_outcome_verified" | "user_outcome_evidenced"
    ) {
        return (
            "ready_for_user".to_string(),
            "deliver_to_user".to_string(),
            "The package includes evidenced user-facing deliverables and the outcome gate is satisfied.".to_string(),
        );
    }

    if outcome_status.status == "support_only" {
        return (
            "not_ready_for_user".to_string(),
            "define_user_facing_deliverables".to_string(),
            outcome_status.reason.clone(),
        );
    }

    if task_summary.completed < task_summary.total || workflow_status != "completed" {
        return (
            "in_progress".to_string(),
            "continue_workflow".to_string(),
            "The workflow still has incomplete work or final evidence before the user-facing package can be treated as complete.".to_string(),
        );
    }

    (
        "not_ready_for_user".to_string(),
        outcome_status.action.clone(),
        outcome_status.reason.clone(),
    )
}

struct FinalDeliveryPackageContext<'a> {
    run: &'a RunRecord,
    workflow: &'a Workflow,
    outcome_status: &'a OutcomeStatusReport,
    task_summary: &'a TaskStatusSummary,
    latest_validation_evidence: Option<&'a ValidationEvidenceSummary>,
    listed_artifacts: &'a [crate::artifact::ListedArtifact],
    readiness: &'a str,
    reason: &'a str,
    generated_at: DateTime<Utc>,
}

fn build_final_delivery_payload(context: &FinalDeliveryPackageContext<'_>) -> serde_json::Value {
    serde_json::json!({
        "schema_version": "forge.final_delivery_package.v1",
        "status": context.readiness,
        "reason": context.reason,
        "generated_at": context.generated_at,
        "run": {
            "run_id": &context.run.run_id,
            "status": &context.run.status,
            "requested_goal": &context.run.goal,
            "origin": &context.run.origin,
        },
        "workflow": {
            "workflow_id": &context.workflow.id,
            "status": &context.workflow.status,
            "goal": &context.workflow.goal,
            "current_revision": context.workflow.revisions.last().map(|revision| revision.revision).unwrap_or(0),
        },
        "task_summary": context.task_summary,
        "outcome_status": context.outcome_status,
        "deliverables": &context.outcome_status.deliverables,
        "completed_tasks": context.workflow.tasks.iter()
            .filter(|task| task.status == TaskStatus::Completed)
            .map(|task| serde_json::json!({
                "task_id": &task.id,
                "title": &task.title,
                "goal": &task.goal,
                "expected_output": &task.expected_output,
            }))
            .collect::<Vec<_>>(),
        "open_tasks": context.workflow.tasks.iter()
            .filter(|task| task.status != TaskStatus::Completed)
            .map(|task| serde_json::json!({
                "task_id": &task.id,
                "title": &task.title,
                "status": format!("{:?}", task.status).to_lowercase(),
                "goal": &task.goal,
                "expected_output": &task.expected_output,
            }))
            .collect::<Vec<_>>(),
        "attached_artifacts": context.workflow.artifacts.iter()
            .map(|artifact| serde_json::json!({
                "id": &artifact.id,
                "kind": &artifact.kind,
                "path": &artifact.path,
                "sha256": &artifact.sha256,
                "created_at": artifact.created_at,
            }))
            .collect::<Vec<_>>(),
        "artifact_inventory": context.listed_artifacts,
        "latest_validation_evidence": context.latest_validation_evidence,
    })
}

fn render_final_delivery_markdown(context: &FinalDeliveryPackageContext<'_>) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Final Delivery Package\n\n");
    markdown.push_str(&format!("- Generated at: `{}`\n", context.generated_at));
    markdown.push_str(&format!("- Readiness: `{}`\n", context.readiness));
    markdown.push_str(&format!("- Reason: {}\n", context.reason));
    markdown.push_str(&format!(
        "- Run: `{}` ({})\n",
        context.run.run_id, context.run.status
    ));
    markdown.push_str(&format!(
        "- Workflow: `{}` ({})\n\n",
        context.workflow.id, context.workflow.status
    ));
    markdown.push_str("## Goal\n\n");
    markdown.push_str(&context.workflow.goal);
    markdown.push_str("\n\n");

    markdown.push_str("## Outcome\n\n");
    markdown.push_str(&format!(
        "- Status: `{}`\n- Action: `{}`\n- User-facing deliverables: {}/{}\n- Final audit passed: `{}`\n\n",
        context.outcome_status.status,
        context.outcome_status.action,
        context.outcome_status.evidenced_user_facing_deliverable_count,
        context.outcome_status.user_facing_deliverable_count,
        context.outcome_status.final_completion_audit_passed,
    ));

    markdown.push_str("## Deliverables\n\n");
    if context.outcome_status.deliverables.is_empty() {
        markdown.push_str("- No deliverables were declared.\n\n");
    } else {
        for deliverable in &context.outcome_status.deliverables {
            markdown.push_str(&format!(
                "- `{}`: {} ({})\n",
                deliverable.status, deliverable.name, deliverable.kind
            ));
            for artifact_ref in &deliverable.artifact_refs {
                markdown.push_str(&format!("  - Artifact: `{artifact_ref}`\n"));
            }
            for task_ref in &deliverable.completed_task_refs {
                markdown.push_str(&format!("  - Completed task: `{task_ref}`\n"));
            }
        }
        markdown.push('\n');
    }

    markdown.push_str("## Task Summary\n\n");
    markdown.push_str(&format!(
        "- Total: {}\n- Completed: {}\n- Pending: {}\n- Running: {}\n- Blocked: {}\n- Failed: {}\n\n",
        context.task_summary.total,
        context.task_summary.completed,
        context.task_summary.pending,
        context.task_summary.running,
        context.task_summary.blocked,
        context.task_summary.failed,
    ));

    markdown.push_str("## Completed Tasks\n\n");
    let completed_tasks = context
        .workflow
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .collect::<Vec<_>>();
    if completed_tasks.is_empty() {
        markdown.push_str("- No completed tasks yet.\n\n");
    } else {
        for task in completed_tasks {
            markdown.push_str(&format!(
                "- `{}`: {} -> {}\n",
                task.id, task.title, task.expected_output
            ));
        }
        markdown.push('\n');
    }

    markdown.push_str("## Attached Artifacts\n\n");
    if context.workflow.artifacts.is_empty() {
        markdown.push_str("- No attached workflow artifacts yet.\n\n");
    } else {
        for artifact in &context.workflow.artifacts {
            markdown.push_str(&format!(
                "- `{}` `{}` `{}`\n",
                artifact.kind, artifact.path, artifact.sha256
            ));
        }
        markdown.push('\n');
    }

    markdown.push_str("## Validation Evidence\n\n");
    if let Some(evidence) = context.latest_validation_evidence {
        markdown.push_str(&format!(
            "- Latest validation: `{}` from `{}`\n- Passed: `{}`\n- Artifact: `{}`\n\n",
            evidence.status, evidence.executor, evidence.validation_passed, evidence.artifact_path,
        ));
    } else {
        markdown
            .push_str("- No self-evolution validation artifact was found for this workflow.\n\n");
    }

    markdown.push_str("## Artifact Inventory\n\n");
    if context.listed_artifacts.is_empty() {
        markdown.push_str("- No artifact files were found on disk before package creation.\n");
    } else {
        for artifact in context.listed_artifacts {
            markdown.push_str(&format!(
                "- `{}` ({} bytes, `{}`)\n",
                artifact.path, artifact.bytes, artifact.sha256
            ));
        }
    }

    markdown
}

fn write_text_artifact(base_dir: &Path, relative_path: &str, content: &str) -> Result<PathBuf> {
    let full_path = base_dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create artifact directory {}", parent.display()))?;
    }
    fs::write(&full_path, content)
        .with_context(|| format!("failed to write artifact {}", full_path.display()))?;
    Ok(full_path)
}

fn is_auto_steppable_task(task: &AtomicTask) -> bool {
    matches!(
        task.executor,
        ExecutorKind::Command | ExecutorKind::Wait | ExecutorKind::Notification
    ) && task
        .validation_rules
        .iter()
        .all(|rule| rule.command.as_deref().unwrap_or("").trim().is_empty())
}

fn build_auto_step_output_payload(
    workflow: &Workflow,
    task: &AtomicTask,
    executor: &str,
    origin: &str,
    generated_at: DateTime<Utc>,
) -> serde_json::Value {
    let known_task_ids = workflow
        .tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<Vec<_>>();
    let missing_dependencies = task
        .dependencies
        .iter()
        .filter(|dependency| !known_task_ids.iter().any(|id| id == dependency))
        .cloned()
        .collect::<Vec<_>>();
    let completed_dependencies = task
        .dependencies
        .iter()
        .filter(|dependency| {
            workflow.tasks.iter().any(|candidate| {
                &candidate.id == *dependency && candidate.status == TaskStatus::Completed
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let graph_nodes = workflow
        .tasks
        .iter()
        .map(|node| {
            serde_json::json!({
                "id": node.id,
                "title": node.title,
                "dependencies": node.dependencies,
                "executor": node.executor,
                "status": node.status,
                "expected_output": node.expected_output
            })
        })
        .collect::<Vec<_>>();
    let graph_edges = workflow
        .tasks
        .iter()
        .flat_map(|node| {
            node.dependencies.iter().map(move |dependency| {
                serde_json::json!({
                    "from": dependency,
                    "to": node.id
                })
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema_version": "forge.auto_step_output.v1",
        "workflow_id": workflow.id,
        "workflow_revision": workflow.revisions.last().map(|revision| revision.revision).unwrap_or(0),
        "task_id": task.id,
        "title": task.title,
        "goal": task.goal,
        "expected_output": task.expected_output,
        "executor": executor,
        "origin": origin,
        "generated_at": generated_at,
        "auto_step_policy": {
            "only_deterministic_tasks": true,
            "external_commands_allowed": false,
            "promotion_path": "executor_response_validation"
        },
        "dependency_evidence": {
            "dependencies": task.dependencies,
            "completed_dependencies": completed_dependencies,
            "missing_dependencies": missing_dependencies
        },
        "validation_rules": task.validation_rules,
        "artifact_refs": workflow.artifacts.iter().map(|artifact| {
            serde_json::json!({
                "kind": artifact.kind,
                "path": artifact.path,
                "sha256": artifact.sha256
            })
        }).collect::<Vec<_>>(),
        "atomic_task_graph": {
            "node_count": graph_nodes.len(),
            "edge_count": graph_edges.len(),
            "nodes": graph_nodes,
            "edges": graph_edges
        }
    })
}

struct ExecutionTracePayloadInput<'a> {
    workflow: &'a Workflow,
    task: &'a AtomicTask,
    handoff_task: &'a RequestDriveTask,
    run_id: &'a str,
    completion: &'a RequestTaskCompletionInput<'a>,
    attached_artifacts: &'a [ArtifactAttachReport],
    drive_before: &'a RequestDriveReport,
    generated_at: DateTime<Utc>,
}

fn build_execution_trace_payload(input: ExecutionTracePayloadInput<'_>) -> serde_json::Value {
    let workflow = input.workflow;
    let task = input.task;
    let handoff_task = input.handoff_task;
    let completion = input.completion;
    let drive_before = input.drive_before;
    serde_json::json!({
        "schema_version": "forge.execution_trace.v1",
        "run_id": input.run_id,
        "workflow_id": workflow.id,
        "workflow_revision": workflow.revisions.last().map(|revision| revision.revision).unwrap_or(0),
        "task_id": task.id,
        "task_title": task.title,
        "task_executor": task.executor,
        "selected_executor": completion.executor,
        "origin": completion.origin,
        "generated_at": input.generated_at,
        "summary": completion.summary,
        "expected_output": task.expected_output,
        "goal": task.goal,
        "handoff": {
            "status": handoff_task.handoff_status,
            "context_sha256": handoff_task.context_sha256,
            "context_routing_cache_key": handoff_task.context_routing_cache_key
        },
        "drive_before": {
            "status": drive_before.status,
            "action": drive_before.action,
            "reason": drive_before.reason,
            "task_summary": drive_before.task_summary
        },
        "dependencies": task.dependencies,
        "validation_rules": task.validation_rules,
        "executor_cost": {
            "estimated_usd": completion.estimated_usd,
            "tokens_in": completion.tokens_in,
            "tokens_out": completion.tokens_out
        },
        "attached_artifacts": input.attached_artifacts.iter().map(|artifact| {
            serde_json::json!({
                "kind": artifact.artifact.kind,
                "path": artifact.artifact.path,
                "sha256": artifact.artifact.sha256,
                "bytes": artifact.artifact.bytes
            })
        }).collect::<Vec<_>>(),
        "replay": {
            "status_command": ["forge", "request", "status", "--run", input.run_id, "--output", "json"],
            "drive_command": ["forge", "request", "drive", "--run", input.run_id, "--executor", completion.executor, "--output", "json"],
            "response_path_kind": "executor_response"
        },
        "completion_policy": {
            "uses_executor_response_validation": true,
            "trace_is_replayable": true,
            "forge_promotes_only_after_validation": true
        }
    })
}

fn latest_open_rework(
    store: &ForgeStore,
    workflow: &Workflow,
) -> Result<Option<RequestDriveRework>> {
    let events = store.load_workflow_events(&workflow.id)?;
    for event in events.into_iter().rev() {
        if event.kind != "executor_response_promoted" {
            continue;
        }
        let response_status = event
            .data
            .get("response_status")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if response_status != "needs_retry" {
            return Ok(None);
        }
        let task_id = event
            .data
            .get("task_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if task_id.is_empty() {
            continue;
        }
        let task_is_completed = workflow
            .tasks
            .iter()
            .any(|task| task.id == task_id && task.status == TaskStatus::Completed);
        if task_is_completed {
            return Ok(None);
        }
        return Ok(Some(RequestDriveRework {
            task_id: task_id.to_string(),
            response_status: response_status.to_string(),
            response_sha256: event
                .data
                .get("response_sha256")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            revision: event.data.get("revision").and_then(|value| value.as_u64()),
            summary: format!(
                "Task {task_id} has an accepted needs_retry executor response recorded at {}.",
                event.created_at
            ),
        }));
    }
    Ok(None)
}

fn ready_handoff_tasks(
    workflow: &Workflow,
    handoff_tasks: &[ContextHandoffTask],
) -> Vec<RequestDriveTask> {
    handoff_tasks
        .iter()
        .filter_map(|handoff| {
            let task = workflow
                .tasks
                .iter()
                .find(|task| task.id == handoff.task_id)?;
            if task.status != TaskStatus::Pending || !handoff.handoff_ready {
                return None;
            }
            Some(RequestDriveTask {
                task_id: handoff.task_id.clone(),
                title: handoff.title.clone(),
                executor: handoff.executor.clone(),
                handoff_status: handoff.handoff_status.clone(),
                context_sha256: handoff.context_sha256.clone(),
                context_routing_cache_key: None,
            })
        })
        .collect()
}

fn handoff_command(
    workflow_id: &str,
    task_id: &str,
    executor: &str,
    ttl_seconds: u64,
    budget: usize,
) -> Vec<String> {
    vec![
        "forge".to_string(),
        "task".to_string(),
        "handoff".to_string(),
        "--workflow".to_string(),
        workflow_id.to_string(),
        "--task".to_string(),
        task_id.to_string(),
        "--executor".to_string(),
        executor.to_string(),
        "--ttl-seconds".to_string(),
        ttl_seconds.max(1).to_string(),
        "--budget".to_string(),
        budget.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

fn drive_blocked_tasks(handoff_tasks: &[ContextHandoffTask]) -> Vec<RequestDriveBlockedTask> {
    handoff_tasks
        .iter()
        .filter(|task| !task.handoff_ready)
        .map(|task| RequestDriveBlockedTask {
            task_id: task.task_id.clone(),
            title: task.title.clone(),
            handoff_status: task.handoff_status.clone(),
            blocking_refs: task.blocking_refs.clone(),
        })
        .collect()
}

pub fn switch_request_executor(
    store: &ForgeStore,
    run_id: &str,
    input: RequestExecutorSwitchInput,
) -> Result<RequestExecutorSwitchReport> {
    let mut run = load_run_record_for_action(store, run_id, "request switch executor")?;
    let previous_status = run.status.clone();
    let previous_executor = run.active_executor.clone();
    let previous_pid = run.executor_pid;
    let previous_heartbeat_at = run.last_heartbeat_at;
    let switched_at = Utc::now();
    let ttl_seconds = input.ttl_seconds.max(1);
    let expires_at = switched_at + Duration::seconds(ttl_seconds.min(i64::MAX as u64) as i64);
    let fallback_executors =
        normalize_executor_fallbacks(&input.executor, &input.fallback_executors);
    let continuity_policy = ExecutorSwitchContinuityPolicy {
        preserve_run_id: true,
        preserve_workflow_id: true,
        preserve_checkpoints: true,
        keep_workflow_running: true,
        old_executor_shutdown_required: false,
        user_directives_remain_authoritative: true,
    };
    let executor_switch = ExecutorSwitchRecord {
        schema_version: "forge.executor_switch.v1".to_string(),
        from_executor: previous_executor.clone(),
        to_executor: input.executor.clone(),
        from_pid: previous_pid,
        to_pid: input.pid,
        fallback_executors: fallback_executors.clone(),
        previous_heartbeat_at,
        switched_at,
        origin: input.origin.clone(),
        reason: input.reason.clone(),
        summary: input.summary.clone(),
        continuity_policy,
    };

    run.status = "running".to_string();
    run.active_executor = Some(input.executor.clone());
    run.executor_pid = input.pid;
    run.progress_summary = Some(input.summary.clone());
    run.last_heartbeat_at = Some(switched_at);
    run.heartbeat_expires_at = Some(expires_at);
    run.heartbeat_ttl_seconds = Some(ttl_seconds);
    run.executor_fallbacks = fallback_executors.clone();
    run.updated_at = switched_at;
    run.executor_switches.push(executor_switch.clone());
    save_run_record(store, &run)?;

    let mut workflow = store.load_workflow(&run.workflow_id)?;
    workflow.status = "running".to_string();
    store.save_workflow(&workflow)?;

    let checkpoints = load_workflow_checkpoints(store, &run.workflow_id)?;
    let latest_checkpoint = checkpoints.last().cloned();
    let handoff_summary =
        build_context_handoff_summary(&workflow, DEFAULT_CONTEXT_BUDGET, &checkpoints)?;
    let activity = build_run_activity_at(&run, switched_at);
    store.record_event(
        &run.workflow_id,
        "async_request_executor_switched",
        &serde_json::json!({
            "schema_version": "forge.request_executor_switch.v1",
            "run_id": run.run_id,
            "workflow_id": run.workflow_id,
            "origin": input.origin.clone(),
            "previous_status": previous_status,
            "new_status": run.status,
            "previous_executor": previous_executor,
            "new_executor": input.executor.clone(),
            "fallback_executors": fallback_executors,
            "previous_pid": previous_pid,
            "new_pid": input.pid,
            "summary": input.summary.clone(),
            "reason": input.reason.clone(),
            "switched_at": switched_at,
            "heartbeat_expires_at": expires_at,
            "heartbeat_ttl_seconds": ttl_seconds,
            "continuity_policy": executor_switch.continuity_policy,
        }),
    )?;

    Ok(RequestExecutorSwitchReport {
        status: run.status,
        schema_version: "forge.request_executor_switch.v1".to_string(),
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        previous_status,
        origin: input.origin,
        previous_executor,
        new_executor: input.executor,
        brain_switch_policy: BrainSwitchPolicyReport {
            schema_version: "forge.brain_switch_policy.v1".to_string(),
            orchestrator_brain: "forge".to_string(),
            switch_scope: "workflow_run_execution_brain".to_string(),
            can_switch_without_stopping_workflow: true,
            preserves_run_id: executor_switch.continuity_policy.preserve_run_id,
            preserves_workflow_id: executor_switch.continuity_policy.preserve_workflow_id,
            preserves_checkpoints: executor_switch.continuity_policy.preserve_checkpoints,
            preserves_user_directives: executor_switch
                .continuity_policy
                .user_directives_remain_authoritative,
            node_brain_routing_source: "workflow.tasks[].node_brain_routing".to_string(),
            node_brain_routing_mutation_command: vec![
                "forge".to_string(),
                "workflow".to_string(),
                "update-node-brain".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--task".to_string(),
                "<task-id>".to_string(),
                "--default-brain".to_string(),
                "<brain-id>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
        fallback_executors,
        activity,
        executor_switch,
        checkpoint_count: checkpoints.len(),
        latest_checkpoint,
        handoff_summary,
        updated_at: switched_at,
    })
}

fn normalize_executor_fallbacks(
    primary_executor: &str,
    fallback_executors: &[String],
) -> Vec<String> {
    let primary_executor = primary_executor.trim();
    let mut normalized = Vec::new();
    for fallback in fallback_executors {
        let fallback = fallback.trim();
        if fallback.is_empty()
            || fallback == primary_executor
            || normalized.iter().any(|existing| existing == fallback)
        {
            continue;
        }
        normalized.push(fallback.to_string());
    }
    normalized
}

pub fn build_run_activity(run: &RunRecord) -> RunActivity {
    build_run_activity_at(run, Utc::now())
}

fn build_run_activity_at(run: &RunRecord, now: DateTime<Utc>) -> RunActivity {
    let process_alive = run.executor_pid.and_then(process_alive);
    let process_status = match (run.executor_pid, process_alive) {
        (None, _) => "not_recorded",
        (Some(_), Some(true)) => "alive",
        (Some(_), Some(false)) => "not_found",
        (Some(_), None) => "unknown",
    };
    let heartbeat_status = if run.status == "needs_attention" {
        "needs_attention"
    } else if run.status == "running" {
        match run.heartbeat_expires_at {
            Some(expires_at) if expires_at > now => "fresh",
            Some(_) if process_alive == Some(true) => "process_alive",
            Some(_) => "stale",
            None if process_alive == Some(true) => "process_alive",
            None => "missing",
        }
    } else if run.last_heartbeat_at.is_some() {
        "inactive"
    } else {
        "not_running"
    };
    let active = run.status == "running" && matches!(heartbeat_status, "fresh" | "process_alive");
    let seconds_until_stale = if run.status == "running" {
        run.heartbeat_expires_at
            .map(|expires_at| (expires_at - now).num_seconds())
    } else {
        None
    };
    let recovery = recovery_recommendation(run, heartbeat_status);
    RunActivity {
        schema_version: "forge.run_activity.v1".to_string(),
        active,
        heartbeat_status: heartbeat_status.to_string(),
        process_status: process_status.to_string(),
        process_alive,
        executor: run.active_executor.clone(),
        pid: run.executor_pid,
        summary: run.progress_summary.clone(),
        last_heartbeat_at: run.last_heartbeat_at,
        heartbeat_expires_at: run.heartbeat_expires_at,
        heartbeat_ttl_seconds: run.heartbeat_ttl_seconds,
        seconds_until_stale,
        recovery,
    }
}

fn process_alive(pid: u32) -> Option<bool> {
    if pid == 0 {
        return Some(false);
    }

    #[cfg(target_os = "linux")]
    {
        Some(Path::new("/proc").join(pid.to_string()).exists())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

fn recovery_recommendation(run: &RunRecord, heartbeat_status: &str) -> RunRecoveryRecommendation {
    match heartbeat_status {
        "stale" => RunRecoveryRecommendation {
            schema_version: "forge.run_recovery_recommendation.v1".to_string(),
            action: "mark_needs_attention".to_string(),
            target_status: "needs_attention".to_string(),
            reason: "Heartbeat is stale; Forge should stop presenting this run as active and require resume, cancel or inspect before more executor work.".to_string(),
            confidence: 0.91,
            requires_human_approval: false,
            command: vec![
                "forge".to_string(),
                "request".to_string(),
                "recover-stale".to_string(),
                "--run".to_string(),
                run.run_id.clone(),
            ],
        },
        "needs_attention" => RunRecoveryRecommendation {
            schema_version: "forge.run_recovery_recommendation.v1".to_string(),
            action: "resume_cancel_or_inspect".to_string(),
            target_status: "needs_attention".to_string(),
            reason: "Run already needs attention; preserve lineage while a human or executor chooses resume, cancel or inspect.".to_string(),
            confidence: 0.88,
            requires_human_approval: false,
            command: vec![
                "forge".to_string(),
                "request".to_string(),
                "status".to_string(),
                "--run".to_string(),
                run.run_id.clone(),
            ],
        },
        _ => RunRecoveryRecommendation {
            schema_version: "forge.run_recovery_recommendation.v1".to_string(),
            action: "none".to_string(),
            target_status: run.status.clone(),
            reason: "No stale heartbeat recovery is required for the current run state.".to_string(),
            confidence: 1.0,
            requires_human_approval: false,
            command: Vec::new(),
        },
    }
}

pub fn load_run_record(store: &ForgeStore, run_id: &str) -> Result<RunRecord> {
    Ok(serde_json::from_value(store.load_run(run_id)?)?)
}

fn load_run_record_for_action(store: &ForgeStore, run_id: &str, action: &str) -> Result<RunRecord> {
    let run = load_run_record(store, run_id)?;
    ensure_workflow_policy(store, &run.workflow_id, action)?;
    Ok(run)
}

pub fn load_request_status(store: &ForgeStore, run_id: &str) -> Result<RequestStatusReport> {
    let run = load_run_record_for_action(store, run_id, "request status")?;
    let workflow = store.load_workflow(&run.workflow_id)?;
    let task_summary = summarize_tasks(&workflow);
    let outcome_status = request_outcome_status(store, &workflow)?;
    let latest_validation_evidence = load_latest_validation_evidence(store, &workflow.id)?;
    let latest_executor_policy = load_latest_executor_policy_summary(store, &workflow.id)?;
    let checkpoints = load_workflow_checkpoints(store, &workflow.id)?;
    let latest_checkpoint = latest_actionable_checkpoint(&workflow, &checkpoints);
    let handoff_summary =
        build_context_handoff_summary(&workflow, DEFAULT_CONTEXT_BUDGET, &checkpoints)?;
    let workflow_revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0);
    let activity = build_run_activity(&run);
    Ok(RequestStatusReport {
        status: run.status,
        run_id: run.run_id,
        workflow_id: workflow.id,
        goal: workflow.goal,
        requested_goal: run.goal,
        origin: run.origin,
        async_run: run.async_run,
        workflow_status: workflow.status,
        workflow_revision,
        artifact_count: workflow.artifacts.len(),
        checkpoint_count: checkpoints.len(),
        latest_checkpoint,
        task_summary,
        outcome_status,
        activity,
        executor_fallbacks: run.executor_fallbacks,
        executor_switch_count: run.executor_switches.len(),
        latest_executor_switch: run.executor_switches.last().cloned(),
        handoff_summary,
        latest_executor_policy,
        latest_validation_evidence,
        created_at: run.created_at,
        updated_at: run.updated_at,
    })
}

fn request_outcome_status(store: &ForgeStore, workflow: &Workflow) -> Result<OutcomeStatusReport> {
    let final_completion_audit_block_reason = final_completion_audit_block_reason(store, workflow)?;
    let evidence_deliverables = load_addon_user_outcome_evidence(store, workflow);
    Ok(assess_workflow_outcome_with_evidence(
        workflow,
        true,
        final_completion_audit_block_reason.as_deref(),
        &evidence_deliverables,
    ))
}

fn load_addon_user_outcome_evidence(
    store: &ForgeStore,
    workflow: &Workflow,
) -> Vec<OutcomeEvidenceDeliverable> {
    let mut evidence = Vec::new();
    for artifact in &workflow.artifacts {
        let artifact_text = format!("{} {}", artifact.kind, artifact.path)
            .to_lowercase()
            .replace('-', "_");
        if !artifact_text.contains("outcome_manifest")
            && !artifact_text.contains("user_outcome")
            && !artifact_text.contains("readiness_report")
        {
            continue;
        }
        let Ok(bytes) = fs::read(store.base_dir().join(&artifact.path)) else {
            continue;
        };
        let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        collect_user_outcome_evidence(&payload, &artifact.path, &mut evidence);
    }
    evidence.sort_by(|left, right| left.name.cmp(&right.name));
    evidence.dedup_by(|left, right| {
        left.name.eq_ignore_ascii_case(&right.name) && left.artifact_ref == right.artifact_ref
    });
    evidence
}

fn collect_user_outcome_evidence(
    payload: &serde_json::Value,
    artifact_ref: &str,
    evidence: &mut Vec<OutcomeEvidenceDeliverable>,
) {
    for key in ["outcomes", "deliverables", "user_facing_deliverables"] {
        if let Some(items) = payload.get(key).and_then(|value| value.as_array()) {
            collect_user_outcome_array(items, artifact_ref, evidence);
        }
    }
    if let Some(data) = payload.get("data") {
        collect_user_outcome_evidence(data, artifact_ref, evidence);
    }
}

fn collect_user_outcome_array(
    items: &[serde_json::Value],
    artifact_ref: &str,
    evidence: &mut Vec<OutcomeEvidenceDeliverable>,
) {
    for item in items {
        if !outcome_item_is_ready(item) {
            continue;
        }
        let name = item
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                ["deliverable", "name", "title", "outcome"]
                    .iter()
                    .find_map(|key| item.get(key).and_then(|value| value.as_str()))
                    .map(str::to_string)
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        evidence.push(OutcomeEvidenceDeliverable {
            name,
            artifact_ref: artifact_ref.to_string(),
        });
    }
}

fn outcome_item_is_ready(item: &serde_json::Value) -> bool {
    let status = item
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("ready")
        .to_lowercase();
    !matches!(
        status.as_str(),
        "blocked" | "failed" | "missing" | "rework_required" | "not_ready"
    )
}

fn latest_actionable_checkpoint(
    workflow: &Workflow,
    checkpoints: &[TaskCheckpoint],
) -> Option<TaskCheckpoint> {
    let task_summary = summarize_tasks(workflow);
    if task_summary.total > 0 && task_summary.completed == task_summary.total {
        return None;
    }

    checkpoints
        .iter()
        .rev()
        .find(|checkpoint| {
            workflow
                .tasks
                .iter()
                .find(|task| task.id == checkpoint.task_id)
                .map(|task| task.status != TaskStatus::Completed)
                .unwrap_or(true)
        })
        .cloned()
}

fn request_drive_context_budget(workflow: &Workflow) -> usize {
    if workflow
        .tasks
        .iter()
        .any(|task| task.status == TaskStatus::Pending && is_final_completion_audit_task(task))
    {
        COMPLETION_AUDIT_HANDOFF_CONTEXT_BUDGET
    } else {
        DEFAULT_CONTEXT_BUDGET
    }
}

fn handoff_context_budget_for_task(workflow: &Workflow, task_id: &str) -> usize {
    workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .filter(|task| is_final_completion_audit_task(task))
        .map(|_| COMPLETION_AUDIT_HANDOFF_CONTEXT_BUDGET)
        .unwrap_or(DEFAULT_CONTEXT_BUDGET)
}

fn ensure_final_completion_audit_task(
    store: &ForgeStore,
    workflow: &Workflow,
    origin: &str,
    block_reason: &str,
) -> Result<Option<Workflow>> {
    let expected_dependency_ids = final_completion_audit_dependency_ids(workflow);
    if let Some(existing_audit_task_index) = workflow
        .tasks
        .iter()
        .position(is_final_completion_audit_task)
    {
        let existing_dependencies = &workflow.tasks[existing_audit_task_index].dependencies;
        if existing_dependencies == &expected_dependency_ids {
            return Ok(None);
        }

        let mut updated = workflow.clone();
        let task_id = updated.tasks[existing_audit_task_index].id.clone();
        let previous_dependency_count = updated.tasks[existing_audit_task_index].dependencies.len();
        updated.tasks[existing_audit_task_index].dependencies = expected_dependency_ids.clone();
        let revision = updated
            .revisions
            .last()
            .map(|revision| revision.revision + 1)
            .unwrap_or(1);
        updated.revisions.push(WorkflowRevision {
            revision,
            origin: origin.to_string(),
            change_type: "completion_audit_dependencies_repaired".to_string(),
            summary: format!(
                "repaired {task_id} dependencies from {previous_dependency_count} to {} outcome evidence prerequisite(s)",
                expected_dependency_ids.len()
            ),
            created_at: Utc::now(),
        });
        store.save_workflow(&updated)?;
        store.record_event(
            &updated.id,
            "completion_audit_dependencies_repaired",
            &serde_json::json!({
                "origin": origin,
                "task_id": task_id,
                "previous_dependency_count": previous_dependency_count,
                "dependency_count": expected_dependency_ids.len(),
                "dependencies": expected_dependency_ids,
                "reason": block_reason,
                "revision": revision,
            }),
        )?;
        return Ok(Some(updated));
    }

    let mut updated = workflow.clone();
    let task_id = format!("task-{:03}", updated.tasks.len() + 1);
    let dependency_ids = expected_dependency_ids;
    let dependency_refs: Vec<&str> = dependency_ids.iter().map(String::as_str).collect();
    let mut audit_task = task(
        &task_id,
        "Audit final completion criteria",
        &dependency_refs,
        &[
            "workflow goal and final criteria",
            "all attached artifacts and validation evidence",
            "repository, CI, cloud, Telegram and interface evidence",
            "open gaps that should become rework instead of false completion",
        ],
        vec![ValidationRule {
            kind: "artifact".to_string(),
            command: Some(format!(
                "forge workflow attach-artifact --workflow {} --path <final-completion-audit.json> --kind {FINAL_COMPLETION_AUDIT_KIND} --output json",
                updated.id
            )),
            expected: "Attach a JSON final completion audit with status passed, goal_fully_satisfied true, non-empty evidence and no open_items or missing_criteria."
                .to_string(),
        }],
        "a Forge-attached final_completion_audit JSON artifact or a needs_retry response listing the exact missing final criteria",
        (ExecutorKind::Ai, 0.35),
    );
    audit_task.goal = format!(
        "Audit the explicit final criteria before completion. {block_reason} Inspect Forge artifacts and the target repositories. If any final criterion lacks evidence, return needs_retry with exact missing work; only attach `{FINAL_COMPLETION_AUDIT_KIND}` when every criterion is proven."
    );
    updated.tasks.push(audit_task);
    updated.status = "running".to_string();
    let revision = updated
        .revisions
        .last()
        .map(|revision| revision.revision + 1)
        .unwrap_or(1);
    updated.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: "completion_audit_task_added".to_string(),
        summary: format!(
            "added {task_id} to audit explicit final completion criteria before closing the run"
        ),
        created_at: Utc::now(),
    });
    store.save_workflow(&updated)?;
    store.record_event(
        &updated.id,
        "completion_audit_task_added",
        &serde_json::json!({
            "origin": origin,
            "task_id": task_id,
            "reason": block_reason,
            "dependency_count": dependency_ids.len(),
            "dependencies": dependency_ids,
            "required_artifact_kind": FINAL_COMPLETION_AUDIT_KIND,
            "revision": revision,
        }),
    )?;
    Ok(Some(updated))
}

fn final_completion_audit_task_id(workflow: &Workflow) -> Option<String> {
    workflow
        .tasks
        .iter()
        .find(|task| is_final_completion_audit_task(task))
        .map(|task| task.id.clone())
}

fn final_completion_audit_dependency_ids(workflow: &Workflow) -> Vec<String> {
    let outcome_status = assess_workflow_outcome(workflow, false, None);
    if outcome_status.user_facing_deliverable_count > 0
        && outcome_status.missing_user_facing_deliverable_count == 0
    {
        let known_non_audit_task_ids = workflow
            .tasks
            .iter()
            .filter(|task| !is_final_completion_audit_task(task))
            .map(|task| task.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut dependency_ids = BTreeSet::new();
        for deliverable in outcome_status
            .deliverables
            .iter()
            .filter(|deliverable| deliverable.kind == "user_facing")
        {
            for task_id in &deliverable.completed_task_refs {
                if known_non_audit_task_ids.contains(task_id.as_str()) {
                    dependency_ids.insert(task_id.clone());
                }
            }
        }
        return dependency_ids.into_iter().collect();
    }

    workflow
        .tasks
        .iter()
        .filter(|task| !is_final_completion_audit_task(task))
        .map(|task| task.id.clone())
        .collect()
}

fn final_completion_audit_dependency_ids_completed(
    workflow: &Workflow,
    dependency_ids: &[String],
) -> bool {
    dependency_ids.iter().all(|dependency| {
        workflow
            .tasks
            .iter()
            .any(|task| task.id == *dependency && task.status == TaskStatus::Completed)
    })
}

fn final_completion_audit_dependencies_completed(workflow: &Workflow, task_id: &str) -> bool {
    let Some(audit_task) = workflow.tasks.iter().find(|task| task.id == task_id) else {
        return false;
    };
    audit_task.dependencies.iter().all(|dependency| {
        workflow
            .tasks
            .iter()
            .any(|task| task.id == *dependency && task.status == TaskStatus::Completed)
    })
}

fn is_final_completion_audit_task(task: &AtomicTask) -> bool {
    let title = task.title.to_lowercase();
    let expected_output = task.expected_output.to_lowercase();
    title.contains("final completion")
        || expected_output.contains(FINAL_COMPLETION_AUDIT_KIND)
        || expected_output.contains("final_completion_audit")
}

fn final_completion_audit_handoff_command(
    workflow_id: &str,
    task_id: &str,
    executor: &str,
    budget: usize,
) -> Vec<String> {
    vec![
        "forge".to_string(),
        "task".to_string(),
        "handoff".to_string(),
        "--workflow".to_string(),
        workflow_id.to_string(),
        "--task".to_string(),
        task_id.to_string(),
        "--executor".to_string(),
        executor.to_string(),
        "--budget".to_string(),
        budget.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

fn final_completion_audit_attach_command(workflow_id: &str, origin: &str) -> Vec<String> {
    vec![
        "forge".to_string(),
        "workflow".to_string(),
        "attach-artifact".to_string(),
        "--workflow".to_string(),
        workflow_id.to_string(),
        "--path".to_string(),
        "<final-completion-audit.json>".to_string(),
        "--kind".to_string(),
        FINAL_COMPLETION_AUDIT_KIND.to_string(),
        "--origin".to_string(),
        origin.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

pub(crate) fn final_completion_audit_block_reason(
    store: &ForgeStore,
    workflow: &Workflow,
) -> Result<Option<String>> {
    if !workflow_requires_final_completion_audit(workflow) {
        return Ok(None);
    }

    let Some(artifact) = workflow
        .artifacts
        .iter()
        .rev()
        .find(|artifact| is_final_completion_audit_artifact(artifact))
    else {
        let reason = if workflow_has_explicit_final_criteria(workflow) {
            "Workflow goal declares explicit final criteria"
        } else {
            "Workflow intent declares user-facing deliverables"
        };
        return Ok(Some(format!(
            "{reason}; attach a final completion audit artifact with kind `{FINAL_COMPLETION_AUDIT_KIND}` before marking the run complete."
        )));
    };

    let artifact_path = store.base_dir().join(&artifact.path);
    let bytes = match fs::read(&artifact_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(Some(format!(
                "Final completion audit artifact {} could not be read: {error}",
                artifact.path
            )));
        }
    };
    let payload: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(Some(format!(
                "Final completion audit artifact {} is not valid JSON: {error}",
                artifact.path
            )));
        }
    };

    let status = payload
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if !matches!(status, "passed" | "complete" | "completed") {
        return Ok(Some(format!(
            "Final completion audit artifact {} must have status `passed`, `complete` or `completed`.",
            artifact.path
        )));
    }

    if payload
        .get("goal_fully_satisfied")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        return Ok(Some(format!(
            "Final completion audit artifact {} must set `goal_fully_satisfied` to true.",
            artifact.path
        )));
    }

    let evidence_count = payload
        .get("evidence")
        .and_then(|value| value.as_array())
        .map(|evidence| evidence.len())
        .unwrap_or(0);
    if evidence_count == 0 {
        return Ok(Some(format!(
            "Final completion audit artifact {} must include non-empty `evidence`.",
            artifact.path
        )));
    }

    let open_items = json_array_len(&payload, "open_items");
    let missing_criteria = json_array_len(&payload, "missing_criteria");
    if open_items > 0 || missing_criteria > 0 {
        return Ok(Some(format!(
            "Final completion audit artifact {} still lists open items or missing criteria.",
            artifact.path
        )));
    }

    Ok(None)
}

fn workflow_requires_final_completion_audit(workflow: &Workflow) -> bool {
    workflow_requires_final_outcome_audit(workflow)
}

fn json_array_len(payload: &serde_json::Value, key: &str) -> usize {
    payload
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestListReport {
    pub status: String,
    pub schema_version: String,
    pub total: usize,
    pub runs: Vec<RequestListRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestListRow {
    pub run_id: String,
    pub workflow_id: String,
    pub status: String,
    pub goal: String,
    pub origin: String,
    pub activity: RunActivity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_fallbacks: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestCancelReport {
    pub status: String,
    pub run_id: String,
    pub workflow_id: String,
    pub previous_status: String,
    pub origin: String,
    pub cancelled_at: DateTime<Utc>,
}

pub fn list_requests(store: &ForgeStore, status_filter: Option<&str>) -> Result<RequestListReport> {
    let records = store.load_runs()?;
    let mut runs: Vec<RequestListRow> = records
        .iter()
        .filter_map(|value| serde_json::from_value::<RunRecord>(value.clone()).ok())
        .filter(|run| {
            if let Some(filter) = status_filter {
                let normalized = filter.trim().to_ascii_lowercase();
                if normalized == "stale" {
                    return build_run_activity(run).heartbeat_status == "stale";
                }
                matches!(
                    normalized.as_str(),
                    "accepted"
                        | "resumed"
                        | "running"
                        | "needs_attention"
                        | "completed"
                        | "failed"
                        | "cancelled"
                        | "planned"
                )
                .then_some(run.status == normalized)
                .unwrap_or(true)
            } else {
                true
            }
        })
        .map(|run| RequestListRow {
            activity: build_run_activity(&run),
            run_id: run.run_id,
            workflow_id: run.workflow_id,
            status: run.status,
            goal: run.goal,
            origin: run.origin,
            created_at: run.created_at,
            executor_fallbacks: run.executor_fallbacks,
            updated_at: run.updated_at,
        })
        .collect();
    let total = runs.len();
    if status_filter.is_some_and(|f| {
        matches!(
            f.trim().to_ascii_lowercase().as_str(),
            "accepted" | "running" | "resumed"
        )
    }) {
        runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    }
    Ok(RequestListReport {
        status: "loaded".to_string(),
        schema_version: "forge.request_list.v1".to_string(),
        total,
        runs,
    })
}

pub fn cancel_request(
    store: &ForgeStore,
    run_id: &str,
    origin: &str,
) -> Result<RequestCancelReport> {
    let mut run = load_run_record_for_action(store, run_id, "request cancel")?;
    let previous_status = run.status.clone();
    run.status = "cancelled".to_string();
    let cancelled_at = Utc::now();
    run.updated_at = cancelled_at;
    save_run_record(store, &run)?;
    store.record_event(
        &run.workflow_id,
        "async_request_cancelled",
        &serde_json::json!({
            "run_id": run.run_id,
            "origin": origin,
            "previous_status": previous_status,
            "cancelled_at": cancelled_at
        }),
    )?;
    Ok(RequestCancelReport {
        status: "cancelled".to_string(),
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        previous_status,
        origin: origin.to_string(),
        cancelled_at,
    })
}

pub fn resume_async_request(
    store: &ForgeStore,
    run_id: &str,
    origin: &str,
) -> Result<RequestResumeReport> {
    let mut run = load_run_record_for_action(store, run_id, "request resume")?;
    let resumed_at = Utc::now();
    run.status = "resumed".to_string();
    run.updated_at = resumed_at;
    save_run_record(store, &run)?;
    store.record_event(
        &run.workflow_id,
        "async_request_resumed",
        &serde_json::json!({
            "run_id": run.run_id,
            "origin": origin,
            "resumed_at": resumed_at
        }),
    )?;
    let request_status = load_request_status(store, run_id)?;
    Ok(RequestResumeReport {
        status: "resumed".to_string(),
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        origin: origin.to_string(),
        resumed_at,
        request_status,
    })
}

pub fn recover_stale_request(
    store: &ForgeStore,
    run_id: &str,
    origin: &str,
) -> Result<RequestStaleRecoveryReport> {
    let mut run = load_run_record_for_action(store, run_id, "request recover stale")?;
    let before_activity = build_run_activity(&run);
    if run.status != "running" || before_activity.heartbeat_status != "stale" {
        anyhow::bail!(
            "run {run_id} is not a stale running request; heartbeat_status={} status={}",
            before_activity.heartbeat_status,
            run.status
        );
    }

    let previous_status = run.status.clone();
    let updated_at = Utc::now();
    run.status = "needs_attention".to_string();
    run.updated_at = updated_at;
    save_run_record(store, &run)?;

    let mut workflow = store.load_workflow(&run.workflow_id)?;
    let previous_workflow_status = workflow.status.clone();
    workflow.status = "needs_attention".to_string();
    store.save_workflow(&workflow)?;

    let activity = build_run_activity_at(&run, updated_at);
    let recovery = RunRecoveryRecommendation {
        schema_version: "forge.run_recovery_recommendation.v1".to_string(),
        action: "resume_cancel_or_inspect".to_string(),
        target_status: "needs_attention".to_string(),
        reason: "Heartbeat is stale; Forge moved the run to needs_attention so a human or executor can resume, cancel or inspect without losing lineage.".to_string(),
        confidence: 0.93,
        requires_human_approval: false,
        command: vec![
            "forge".to_string(),
            "request".to_string(),
            "status".to_string(),
            "--run".to_string(),
            run.run_id.clone(),
        ],
    };
    store.record_event(
        &run.workflow_id,
        "async_request_needs_attention",
        &serde_json::json!({
            "run_id": run.run_id,
            "origin": origin,
            "previous_status": previous_status,
            "new_status": run.status,
            "previous_workflow_status": previous_workflow_status,
            "new_workflow_status": workflow.status,
            "heartbeat_status": before_activity.heartbeat_status,
            "updated_at": updated_at,
        }),
    )?;

    Ok(RequestStaleRecoveryReport {
        status: run.status,
        schema_version: "forge.request_stale_recovery.v1".to_string(),
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        previous_status,
        previous_workflow_status,
        origin: origin.to_string(),
        activity,
        recovery,
        updated_at,
    })
}

fn build_agent_handoff_contract(
    run: &RunRecord,
    flow_resolution: FlowResolutionReport,
) -> AgentHandoffContract {
    AgentHandoffContract {
        schema_version: "forge.agent_handoff_contract.v1".to_string(),
        run_id: run.run_id.clone(),
        workflow_id: run.workflow_id.clone(),
        origin: run.origin.clone(),
        flow_resolution,
        policy: AgentHandoffPolicy {
            execution_authority: "forge".to_string(),
            async_run: true,
            source_of_truth: "forge_sqlite_workflow_state".to_string(),
            executor_policy_required: true,
            validation_before_promotion: true,
            user_directives_remain_authoritative: true,
            executor_hot_swap_supported: true,
        },
        allowed_context: AgentAllowedContext {
            tool: "forge.context.request".to_string(),
            command: vec![
                "forge".to_string(),
                "context".to_string(),
                "--workflow".to_string(),
                run.workflow_id.clone(),
                "--task".to_string(),
                "<task-id>".to_string(),
                "--budget".to_string(),
                DEFAULT_CONTEXT_BUDGET.to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            default_budget: DEFAULT_CONTEXT_BUDGET,
            strict_by_default: false,
            allowed_scope: "task_local_bounded_context".to_string(),
        },
        validation_rules: vec![
            "validate-before-promotion".to_string(),
            "mutations-must-be-revisioned".to_string(),
            "artifacts-must-be-content-addressed".to_string(),
            "existing-flows-and-subflows-must-be-checked-before-new-workflow-execution".to_string(),
            "self-run-evolution-is-a-normal-flow-not-the-default-flow-resolution".to_string(),
            "executor-policy-must-allow-local-executor".to_string(),
            "explicit-user-directives-outrank-autonomous-executor-preferences".to_string(),
            "executor-switch-must-preserve-run-workflow-checkpoints-and-artifacts".to_string(),
        ],
        artifact_refs: Vec::new(),
        status_poll: AgentStatusPoll {
            tool: "forge.run.status".to_string(),
            command: vec![
                "forge".to_string(),
                "request".to_string(),
                "status".to_string(),
                "--run".to_string(),
                run.run_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            returns: vec![
                "workflow_status".to_string(),
                "workflow_revision".to_string(),
                "task_summary".to_string(),
                "outcome_status".to_string(),
                "handoff_summary".to_string(),
                "latest_executor_policy".to_string(),
                "latest_validation_evidence".to_string(),
            ],
        },
    }
}

fn summarize_tasks(workflow: &Workflow) -> TaskStatusSummary {
    let mut summary = TaskStatusSummary {
        total: workflow.tasks.len(),
        ..TaskStatusSummary::default()
    };
    for task in &workflow.tasks {
        match task.status {
            TaskStatus::Pending => summary.pending += 1,
            TaskStatus::Running => summary.running += 1,
            TaskStatus::Completed => summary.completed += 1,
            TaskStatus::Blocked => summary.blocked += 1,
            TaskStatus::Failed => summary.failed += 1,
        }
    }
    summary
}

fn load_latest_validation_evidence(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<Option<ValidationEvidenceSummary>> {
    let artifacts = list_workflow_artifacts(&store.base_dir(), workflow_id)?;
    let Some(artifact) = artifacts.into_iter().rev().find(|artifact| {
        artifact.path.contains("/self-evolution-cycle-")
            && artifact.path.ends_with("-validation.json")
    }) else {
        return Ok(None);
    };

    let bytes = fs::read(store.base_dir().join(&artifact.path))
        .with_context(|| format!("failed to read validation artifact {}", artifact.path))?;
    let payload: ValidationEvidenceArtifact = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse validation artifact {}", artifact.path))?;

    Ok(Some(ValidationEvidenceSummary {
        artifact_path: artifact.path,
        artifact_sha256: artifact.sha256,
        schema_version: payload.schema_version,
        prompt_packet_version: payload.prompt_packet_version,
        status: payload.status,
        validation_passed: payload.validation_passed,
        cycle: payload.cycle,
        executor: payload.executor,
        command_summary: summarize_validation_commands(&payload.commands),
    }))
}

fn load_latest_executor_policy_summary(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<Option<RequestExecutorPolicySummary>> {
    let artifacts = list_workflow_artifacts(&store.base_dir(), workflow_id)?;
    let Some(artifact) = artifacts.into_iter().rev().find(|artifact| {
        artifact.path.contains("/self-evolution-cycle-") && artifact.path.ends_with("-report.json")
    }) else {
        return Ok(None);
    };

    let bytes = fs::read(store.base_dir().join(&artifact.path))
        .with_context(|| format!("failed to read self-evolution report {}", artifact.path))?;
    let payload: SelfEvolutionCycleArtifact = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse self-evolution report {}", artifact.path))?;
    let policy = payload.executor_policy;

    Ok(Some(RequestExecutorPolicySummary {
        schema_version: "forge.request_executor_policy_summary.v1".to_string(),
        artifact_path: artifact.path,
        artifact_sha256: artifact.sha256,
        cycle: payload.cycle,
        requested_executor: payload.requested_executor,
        selected_executor: payload.executor,
        active_repair_status: policy.active_repair_status,
        quota_decision_summary: policy.quota_decision_summary,
        selected_candidate: policy.selected_candidate.map(Into::into),
        fallback_order: policy.fallback_order,
        quota_preservation: policy.skipped_to_preserve_quota,
        repair_goals: policy.repair_goals,
    }))
}

fn summarize_validation_commands(
    commands: &[ValidationCommandArtifact],
) -> ValidationCommandSummary {
    let mut summary = ValidationCommandSummary {
        total: commands.len(),
        ..ValidationCommandSummary::default()
    };
    for command in commands {
        match command.status.as_str() {
            "planned" => summary.planned += 1,
            "passed" => summary.passed += 1,
            "failed" => summary.failed += 1,
            "skipped" => summary.skipped += 1,
            _ => {}
        }
    }
    summary
}
