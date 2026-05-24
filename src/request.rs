use crate::artifact::list_workflow_artifacts;
use crate::checkpoint::{load_workflow_checkpoints, TaskCheckpoint};
use crate::graph::{create_workflow, TaskStatus, Workflow};
use crate::intent::parse_intent;
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
    pub latest_validation_evidence: Option<ValidationEvidenceSummary>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    let workflow = create_workflow(parse_intent(goal));
    let run = create_run_record(&workflow, origin, "accepted");
    store.save_workflow(&workflow)?;
    save_run_record(store, &run)?;
    store.record_event(
        &workflow.id,
        "async_request_started",
        &serde_json::to_value(&run)?,
    )?;
    Ok(RequestStartReport {
        status: run.status,
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        goal: run.goal,
        origin: run.origin,
        async_run: run.async_run,
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
        latest_validation_evidence,
        created_at: run.created_at,
        updated_at: run.updated_at,
    })
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
