use crate::artifact::copy_artifact;
use crate::graph::{ArtifactRecord, TaskStatus, Workflow, WorkflowRevision};
use crate::ir::{
    preview_token_change_impact, resolve_token_collection, ConcreteChange, CreativeArtifact,
    PatchByIntent, TokenCollection, TokenImpactPreview, TokenResolutionReport,
};
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
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

// -- Creative artifact management --

#[derive(Debug, Clone, Serialize)]
pub struct CreativeArtifactAttachReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub revision: u64,
    pub artifact: CreativeArtifactSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreativeArtifactSummary {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub created_at: DateTime<Utc>,
    pub tag_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreativeArtifactListReport {
    pub status: String,
    pub workflow_id: String,
    pub artifacts: Vec<CreativeArtifactSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreativeArtifactInspectReport {
    pub status: String,
    pub workflow_id: String,
    pub artifact: CreativeArtifact,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenCollectionReport {
    pub status: String,
    pub workflow_id: String,
    pub token_collection: Option<TokenCollection>,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenResolutionWorkflowReport {
    pub status: String,
    pub workflow_id: String,
    pub revision: u64,
    pub resolution: TokenResolutionReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenPatchReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub revision: u64,
    pub token_name: String,
    pub old_value: String,
    pub new_value: String,
    pub patch: PatchByIntent,
    pub impact_preview: TokenImpactPreview,
    pub creative_artifacts_rewritten: bool,
}

pub fn attach_creative_artifact(
    store: &ForgeStore,
    workflow_id: &str,
    artifact: CreativeArtifact,
    origin: &str,
) -> Result<CreativeArtifactAttachReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    workflow.creative_artifacts.push(artifact);
    let summary = workflow.creative_artifacts.last().unwrap();
    let revision = push_revision(
        &mut workflow.revisions,
        origin,
        "creative_artifact_attached",
        &format!(
            "attached creative artifact {} as {:?}",
            summary.id, summary.kind
        ),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "creative_artifact_attached",
        &serde_json::json!({
            "origin": origin,
            "artifact_id": summary.id,
            "kind": format!("{:?}", summary.kind),
            "revision": revision
        }),
    )?;

    Ok(CreativeArtifactAttachReport {
        status: "creative_artifact_attached".to_string(),
        workflow_id: workflow_id.to_string(),
        origin: origin.to_string(),
        revision,
        artifact: CreativeArtifactSummary {
            id: summary.id.clone(),
            title: summary.title.clone(),
            kind: format!("{:?}", summary.kind),
            created_at: summary.created_at,
            tag_count: summary.tags.len(),
        },
    })
}

pub fn list_creative_artifacts(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<CreativeArtifactListReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let artifacts = workflow
        .creative_artifacts
        .iter()
        .map(|a| CreativeArtifactSummary {
            id: a.id.clone(),
            title: a.title.clone(),
            kind: format!("{:?}", a.kind),
            created_at: a.created_at,
            tag_count: a.tags.len(),
        })
        .collect();

    Ok(CreativeArtifactListReport {
        status: "creative_artifacts_listed".to_string(),
        workflow_id: workflow_id.to_string(),
        artifacts,
    })
}

pub fn inspect_creative_artifact(
    store: &ForgeStore,
    workflow_id: &str,
    artifact_id: &str,
) -> Result<CreativeArtifactInspectReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let artifact = workflow
        .creative_artifacts
        .iter()
        .find(|a| a.id == artifact_id)
        .with_context(|| format!("creative artifact not found: {artifact_id}"))?;

    Ok(CreativeArtifactInspectReport {
        status: "creative_artifact_inspected".to_string(),
        workflow_id: workflow_id.to_string(),
        artifact: artifact.clone(),
    })
}

pub fn set_workflow_token_collection(
    store: &ForgeStore,
    workflow_id: &str,
    token_collection: TokenCollection,
    origin: &str,
) -> Result<TokenCollectionReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    workflow.token_collection = Some(token_collection);
    let revision = push_revision(
        &mut workflow.revisions,
        origin,
        "token_collection_set",
        "design token collection updated",
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "token_collection_set",
        &serde_json::json!({
            "origin": origin,
            "revision": revision
        }),
    )?;

    Ok(TokenCollectionReport {
        status: "token_collection_set".to_string(),
        workflow_id: workflow_id.to_string(),
        token_collection: workflow.token_collection.clone(),
        revision,
    })
}

pub fn get_workflow_token_collection(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<TokenCollectionReport> {
    let workflow = store.load_workflow(workflow_id)?;
    Ok(TokenCollectionReport {
        status: "token_collection_loaded".to_string(),
        workflow_id: workflow_id.to_string(),
        token_collection: workflow.token_collection.clone(),
        revision: workflow.revisions.last().map(|r| r.revision).unwrap_or(0),
    })
}

pub fn resolve_workflow_tokens(
    store: &ForgeStore,
    workflow_id: &str,
    mode: Option<&str>,
) -> Result<TokenResolutionWorkflowReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let token_collection = workflow
        .token_collection
        .as_ref()
        .with_context(|| format!("token collection not set for workflow {workflow_id}"))?;
    let resolution = resolve_token_collection(token_collection, mode, &workflow.creative_artifacts);

    Ok(TokenResolutionWorkflowReport {
        status: "token_resolution_ready".to_string(),
        workflow_id: workflow_id.to_string(),
        revision: workflow.revisions.last().map(|r| r.revision).unwrap_or(0),
        resolution,
    })
}

pub fn patch_workflow_token(
    store: &ForgeStore,
    workflow_id: &str,
    token_name: &str,
    value: &str,
    origin: &str,
) -> Result<TokenPatchReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    let creative_artifacts = workflow.creative_artifacts.clone();
    let (collection_name, old_value, impact_preview) = {
        let token_collection = workflow
            .token_collection
            .as_mut()
            .with_context(|| format!("token collection not set for workflow {workflow_id}"))?;
        let token = token_collection
            .tokens
            .iter_mut()
            .find(|token| token.name == token_name)
            .with_context(|| format!("token not found in workflow {workflow_id}: {token_name}"))?;
        let old_value = token.value.clone();
        token.value = value.to_string();
        let impact_preview =
            preview_token_change_impact(token_collection, &creative_artifacts, token_name);
        (token_collection.name.clone(), old_value, impact_preview)
    };
    let patch = PatchByIntent {
        id: format!("patch_{}", Uuid::new_v4().to_string().replace('-', "")),
        instruction: format!("Set token {token_name} to {value}"),
        target_artifact_id: format!("token_collection:{collection_name}"),
        scope: "design_tokens".to_string(),
        applied_at: Utc::now(),
        applied_by: origin.to_string(),
        changes: vec![ConcreteChange {
            path: format!("token_collection.tokens[{token_name}].value"),
            old_value: Some(old_value.clone()),
            new_value: value.to_string(),
            description:
                "Targeted token patch; creative artifacts keep their own content and token references."
                    .to_string(),
        }],
    };
    let revision = push_revision(
        &mut workflow.revisions,
        origin,
        "token_patched",
        &format!("patched design token {token_name} without rewriting creative artifacts"),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "token_patched",
        &serde_json::json!({
            "origin": origin,
            "token_name": token_name,
            "old_value": old_value,
            "new_value": value,
            "revision": revision,
            "creative_artifacts_rewritten": false
        }),
    )?;

    Ok(TokenPatchReport {
        status: "token_patched".to_string(),
        workflow_id: workflow_id.to_string(),
        origin: origin.to_string(),
        revision,
        token_name: token_name.to_string(),
        old_value,
        new_value: value.to_string(),
        patch,
        impact_preview,
        creative_artifacts_rewritten: false,
    })
}
