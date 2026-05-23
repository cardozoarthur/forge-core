use crate::artifact::copy_artifact;
use crate::graph::{ArtifactRecord, WorkflowRevision};
use crate::storage::ForgeStore;
use anyhow::Result;
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
