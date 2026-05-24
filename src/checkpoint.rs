use crate::storage::ForgeStore;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    pub checkpoint_id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub state: String,
    pub summary: String,
    pub context_sha256: String,
    #[serde(default)]
    pub context_routing_cache_key: Option<String>,
    pub workflow_revision: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskCheckpointReport {
    pub status: String,
    pub checkpoint: TaskCheckpoint,
}

pub struct TaskCheckpointRequest<'a> {
    pub workflow_id: &'a str,
    pub task_id: &'a str,
    pub executor: &'a str,
    pub state: &'a str,
    pub summary: &'a str,
    pub context_sha256: &'a str,
    pub context_routing_cache_key: Option<&'a str>,
    pub workflow_revision: u64,
}

pub fn record_task_checkpoint(
    store: &ForgeStore,
    request: TaskCheckpointRequest<'_>,
) -> Result<TaskCheckpointReport> {
    let workflow = store.load_workflow(request.workflow_id)?;
    if !workflow.tasks.iter().any(|task| task.id == request.task_id) {
        bail!(
            "task not found in workflow {}: {}",
            request.workflow_id,
            request.task_id
        );
    }
    if request.executor.trim().is_empty() {
        bail!("executor cannot be empty");
    }
    if request.state.trim().is_empty() {
        bail!("checkpoint state cannot be empty");
    }
    if request.summary.trim().is_empty() {
        bail!("checkpoint summary cannot be empty");
    }
    if !is_sha256(request.context_sha256) {
        bail!("context sha256 must be a 64 character hex string");
    }
    if let Some(cache_key) = request.context_routing_cache_key {
        if !is_sha256(cache_key) {
            bail!("context routing cache key must be a 64 character hex string");
        }
    }

    let checkpoint = TaskCheckpoint {
        checkpoint_id: format!("ckpt_{}", Uuid::new_v4().to_string().replace('-', "")),
        workflow_id: request.workflow_id.to_string(),
        task_id: request.task_id.to_string(),
        executor: request.executor.to_string(),
        state: request.state.to_string(),
        summary: request.summary.to_string(),
        context_sha256: request.context_sha256.to_string(),
        context_routing_cache_key: request.context_routing_cache_key.map(str::to_string),
        workflow_revision: request.workflow_revision,
        created_at: Utc::now(),
    };
    store.save_task_checkpoint(&checkpoint)?;
    let report = TaskCheckpointReport {
        status: "checkpoint_recorded".to_string(),
        checkpoint,
    };
    store.record_event(
        request.workflow_id,
        "task_checkpoint_recorded",
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

pub fn load_workflow_checkpoints(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<Vec<TaskCheckpoint>> {
    store
        .load_task_checkpoints(workflow_id, None)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub fn load_latest_task_checkpoint(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<Option<TaskCheckpoint>> {
    Ok(store
        .load_task_checkpoints(workflow_id, Some(task_id))?
        .into_iter()
        .last()
        .map(serde_json::from_value)
        .transpose()?)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
