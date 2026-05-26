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
    pub handoff_contract: AgentHandoffContract,
    pub reuse_candidates: Vec<WorkflowReuseCandidate>,
    pub attached_subflows: usize,
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
    pub executor: Option<String>,
    pub pid: Option<u32>,
    pub summary: Option<String>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub heartbeat_expires_at: Option<DateTime<Utc>>,
    pub heartbeat_ttl_seconds: Option<u64>,
    pub seconds_until_stale: Option<i64>,
    pub recovery: RunRecoveryRecommendation,
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

pub fn start_async_request(
    store: &ForgeStore,
    goal: &str,
    origin: &str,
) -> Result<RequestStartReport> {
    let mut workflow = create_workflow(parse_intent(goal));
    let reuse_candidates = find_reuse_candidates(store, &workflow)?;
    let attached_subflows =
        attach_reuse_candidates_as_child_subflows(&mut workflow, &reuse_candidates);
    let run = create_run_record(&workflow, origin, "accepted");
    store.save_workflow(&workflow)?;
    save_run_record(store, &run)?;
    store.record_event(
        &workflow.id,
        "async_request_started",
        &serde_json::to_value(&run)?,
    )?;
    let handoff_contract = build_agent_handoff_contract(&run);
    Ok(RequestStartReport {
        status: run.status,
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        goal: run.goal,
        origin: run.origin,
        async_run: run.async_run,
        handoff_contract,
        reuse_candidates,
        attached_subflows,
    })
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

pub fn build_run_activity(run: &RunRecord) -> RunActivity {
    build_run_activity_at(run, Utc::now())
}

fn build_run_activity_at(run: &RunRecord, now: DateTime<Utc>) -> RunActivity {
    let seconds_until_stale = run
        .heartbeat_expires_at
        .map(|expires_at| (expires_at - now).num_seconds());
    let heartbeat_status = if run.status == "needs_attention" {
        "needs_attention"
    } else if run.status == "running" {
        match run.heartbeat_expires_at {
            Some(expires_at) if expires_at > now => "fresh",
            Some(_) => "stale",
            None => "missing",
        }
    } else if run.last_heartbeat_at.is_some() {
        "inactive"
    } else {
        "not_running"
    };
    let active = run.status == "running" && heartbeat_status == "fresh";
    let recovery = recovery_recommendation(run, heartbeat_status);
    RunActivity {
        schema_version: "forge.run_activity.v1".to_string(),
        active,
        heartbeat_status: heartbeat_status.to_string(),
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

fn build_agent_handoff_contract(run: &RunRecord) -> AgentHandoffContract {
    AgentHandoffContract {
        schema_version: "forge.agent_handoff_contract.v1".to_string(),
        run_id: run.run_id.clone(),
        workflow_id: run.workflow_id.clone(),
        origin: run.origin.clone(),
        policy: AgentHandoffPolicy {
            execution_authority: "forge".to_string(),
            async_run: true,
            source_of_truth: "forge_sqlite_workflow_state".to_string(),
            executor_policy_required: true,
            validation_before_promotion: true,
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
            "executor-policy-must-allow-local-executor".to_string(),
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
