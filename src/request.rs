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
use chrono::{DateTime, Utc};
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
        handoff_summary,
        latest_validation_evidence,
        created_at: run.created_at,
        updated_at: run.updated_at,
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
