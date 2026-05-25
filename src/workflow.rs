use crate::artifact::copy_artifact;
use crate::graph::{ArtifactRecord, TaskStatus, Workflow, WorkflowRevision};
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowGoalUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub previous_goal: String,
    pub new_goal: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactAttachReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub revision: u64,
    pub artifact: AttachedArtifact,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubflowValidationReport {
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub child_workflow_id: String,
    pub child_task_id: String,
    pub origin: String,
    pub previous_binding_status: String,
    pub binding_status: String,
    pub lifecycle_state: String,
    pub validation_gate: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttachedArtifact {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

pub fn update_workflow_goal(
    store: &ForgeStore,
    workflow_id: &str,
    goal: &str,
    origin: &str,
) -> Result<WorkflowGoalUpdateReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    let previous_goal = workflow.goal.clone();
    workflow.goal = goal.to_string();
    workflow.intent.goal = goal.to_string();
    let revision = push_revision(
        &mut workflow.revisions,
        origin,
        "goal_update",
        &format!("goal changed from `{previous_goal}` to `{goal}`"),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "workflow_goal_updated",
        &serde_json::json!({
            "origin": origin,
            "previous_goal": previous_goal,
            "new_goal": goal,
            "revision": revision
        }),
    )?;

    Ok(WorkflowGoalUpdateReport {
        status: "workflow_goal_updated".to_string(),
        workflow_id: workflow_id.to_string(),
        origin: origin.to_string(),
        previous_goal,
        new_goal: goal.to_string(),
        revision,
    })
}

pub fn attach_workflow_artifact(
    store: &ForgeStore,
    workflow_id: &str,
    source_path: &Path,
    kind: &str,
    origin: &str,
) -> Result<ArtifactAttachReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    let (relative_path, sha256, bytes) =
        copy_artifact(&store.base_dir(), workflow_id, source_path, kind)?;
    let artifact = ArtifactRecord {
        id: format!("artifact_{}", Uuid::new_v4().to_string().replace('-', "")),
        kind: kind.to_string(),
        path: relative_path.clone(),
        sha256: sha256.clone(),
        created_at: Utc::now(),
        lineage: None,
    };
    workflow.artifacts.push(artifact.clone());
    let revision = push_revision(
        &mut workflow.revisions,
        origin,
        "artifact_attached",
        &format!("attached artifact {} as {kind}", source_path.display()),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "artifact_attached",
        &serde_json::json!({
            "origin": origin,
            "path": relative_path,
            "sha256": sha256,
            "revision": revision
        }),
    )?;

    Ok(ArtifactAttachReport {
        status: "artifact_attached".to_string(),
        workflow_id: workflow_id.to_string(),
        origin: origin.to_string(),
        revision,
        artifact: AttachedArtifact {
            id: artifact.id,
            kind: artifact.kind,
            path: artifact.path,
            sha256: artifact.sha256,
            bytes,
        },
    })
}

pub fn validate_child_subflow_binding(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    child_workflow_id: &str,
    child_task_id: &str,
    origin: &str,
) -> Result<SubflowValidationReport> {
    let child_workflow = store.load_workflow(child_workflow_id)?;
    let child_task = child_workflow
        .tasks
        .iter()
        .find(|task| task.id == child_task_id)
        .with_context(|| {
            format!("child task {child_task_id} not found in workflow {child_workflow_id}")
        })?;
    let lifecycle_state = derive_child_lifecycle_state(&child_workflow);
    if lifecycle_state != "scaled_to_zero" {
        bail!(
            "child subflow {child_workflow_id}/{child_task_id} is not validation-ready: lifecycle state {lifecycle_state}"
        );
    }
    let validation_gate = child_task.execution_policy.validation_gate.clone();
    if validation_gate.trim().is_empty() {
        bail!(
            "child subflow {child_workflow_id}/{child_task_id} is not validation-ready: validation gate is empty"
        );
    }

    let mut workflow = store.load_workflow(workflow_id)?;
    let previous_binding_status = {
        let task = workflow
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .with_context(|| format!("task not found: {task_id}"))?;
        let subflow = task
            .child_subflows
            .iter_mut()
            .find(|subflow| {
                subflow.workflow_id == child_workflow_id && subflow.task_id == child_task_id
            })
            .with_context(|| {
                format!(
                    "child subflow {child_workflow_id}/{child_task_id} not found on task {task_id}"
                )
            })?;
        let previous = subflow.binding_status.clone();
        subflow.binding_status = "validated".to_string();
        subflow.lifecycle_state = lifecycle_state.clone();
        subflow.validation_gate = validation_gate.clone();
        previous
    };

    let revision = push_revision(
        &mut workflow.revisions,
        origin,
        "child_subflow_validated",
        &format!("validated child subflow {child_workflow_id}/{child_task_id} for task {task_id}"),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "child_subflow_validated",
        &serde_json::json!({
            "origin": origin,
            "task_id": task_id,
            "child_workflow_id": child_workflow_id,
            "child_task_id": child_task_id,
            "previous_binding_status": previous_binding_status,
            "binding_status": "validated",
            "lifecycle_state": lifecycle_state,
            "validation_gate": validation_gate,
            "revision": revision
        }),
    )?;

    Ok(SubflowValidationReport {
        status: "child_subflow_validated".to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        child_workflow_id: child_workflow_id.to_string(),
        child_task_id: child_task_id.to_string(),
        origin: origin.to_string(),
        previous_binding_status,
        binding_status: "validated".to_string(),
        lifecycle_state,
        validation_gate,
        revision,
    })
}

fn derive_child_lifecycle_state(workflow: &Workflow) -> String {
    if workflow.status == "failed"
        || workflow
            .tasks
            .iter()
            .any(|task| task.status == TaskStatus::Failed)
    {
        return "failed".to_string();
    }
    if workflow.status == "blocked"
        || workflow
            .tasks
            .iter()
            .any(|task| task.status == TaskStatus::Blocked)
    {
        return "blocked".to_string();
    }
    if workflow
        .tasks
        .iter()
        .any(|task| task.status == TaskStatus::Running)
    {
        return "running".to_string();
    }
    if workflow.status == "completed" {
        let all_completed = workflow
            .tasks
            .iter()
            .all(|task| task.status == TaskStatus::Completed);
        if all_completed {
            return "scaled_to_zero".to_string();
        }
        return "completed".to_string();
    }
    "idle".to_string()
}

fn push_revision(
    revisions: &mut Vec<WorkflowRevision>,
    origin: &str,
    change_type: &str,
    summary: &str,
) -> u64 {
    let revision = revisions.last().map(|item| item.revision + 1).unwrap_or(1);
    revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: change_type.to_string(),
        summary: summary.to_string(),
        created_at: Utc::now(),
    });
    revision
}
