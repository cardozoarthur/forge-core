use crate::artifact::list_workflow_artifacts;
use crate::checkpoint::{load_workflow_checkpoints, TaskCheckpoint};
use crate::context::{
    build_context_handoff_summary, ContextHandoffSummary, DEFAULT_CONTEXT_BUDGET,
};
use crate::graph::{create_workflow, TaskStatus, Workflow};
use crate::intent::parse_intent;
use crate::registry::{
    attach_reuse_candidates_as_child_subflows, find_reuse_candidates, WorkflowReuseCandidate,
};
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

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
    pub activity: RunActivity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_fallbacks: Vec<String>,
    pub executor_switch_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_executor_switch: Option<ExecutorSwitchRecord>,
    pub handoff_summary: ContextHandoffSummary,
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
pub struct RequestExecutorSwitchReport {
    pub status: String,
    pub schema_version: String,
    pub run_id: String,
    pub workflow_id: String,
    pub previous_status: String,
    pub origin: String,
    pub previous_executor: Option<String>,
    pub new_executor: String,
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
    let mut workflow = create_workflow(parse_intent(goal));
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
    let mut run = load_run_record(store, run_id)?;
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

pub fn heartbeat_request(
    store: &ForgeStore,
    run_id: &str,
    executor: &str,
    summary: &str,
    ttl_seconds: u64,
    pid: Option<u32>,
    origin: &str,
) -> Result<RequestHeartbeatReport> {
    let mut run = load_run_record(store, run_id)?;
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

pub fn switch_request_executor(
    store: &ForgeStore,
    run_id: &str,
    input: RequestExecutorSwitchInput,
) -> Result<RequestExecutorSwitchReport> {
    let mut run = load_run_record(store, run_id)?;
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
    let seconds_until_stale = run
        .heartbeat_expires_at
        .map(|expires_at| (expires_at - now).num_seconds());
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

pub fn load_request_status(store: &ForgeStore, run_id: &str) -> Result<RequestStatusReport> {
    let run = load_run_record(store, run_id)?;
    let workflow = store.load_workflow(&run.workflow_id)?;
    let task_summary = summarize_tasks(&workflow);
    let latest_validation_evidence = load_latest_validation_evidence(store, &workflow.id)?;
    let checkpoints = load_workflow_checkpoints(store, &workflow.id)?;
    let latest_checkpoint = checkpoints.last().cloned();
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
        activity,
        executor_fallbacks: run.executor_fallbacks,
        executor_switch_count: run.executor_switches.len(),
        latest_executor_switch: run.executor_switches.last().cloned(),
        handoff_summary,
        latest_validation_evidence,
        created_at: run.created_at,
        updated_at: run.updated_at,
    })
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
    let mut run = load_run_record(store, run_id)?;
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
    let mut run = load_run_record(store, run_id)?;
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
    let mut run = load_run_record(store, run_id)?;
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
                "handoff_summary".to_string(),
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
