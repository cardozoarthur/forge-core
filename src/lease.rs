use crate::storage::{ForgeStore, TaskLeaseWrite};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLease {
    pub lease_id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskLeaseAcquireReport {
    pub status: String,
    pub allowed: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub lease: Option<TaskLease>,
    pub current_lease: Option<TaskLease>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskLeaseReleaseReport {
    pub status: String,
    pub released: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub lease_id: String,
    pub current_lease: Option<TaskLease>,
}

pub fn acquire_task_lease(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    executor: &str,
    ttl_seconds: u64,
) -> Result<TaskLeaseAcquireReport> {
    ensure_task_exists(store, workflow_id, task_id)?;
    if executor.trim().is_empty() {
        bail!("executor cannot be empty");
    }
    if ttl_seconds == 0 {
        bail!("ttl seconds must be greater than zero");
    }

    let acquired_at = Utc::now();
    let ttl_seconds = i64::try_from(ttl_seconds).context("ttl seconds exceeds supported range")?;
    let expires_at = acquired_at + Duration::seconds(ttl_seconds);
    let lease = TaskLease {
        lease_id: format!("lease_{}", Uuid::new_v4().to_string().replace('-', "")),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        executor: executor.to_string(),
        acquired_at,
        expires_at,
    };
    let lease_value = serde_json::to_value(&lease)?;
    let acquired_at_rfc3339 = acquired_at.to_rfc3339();
    let expires_at_rfc3339 = expires_at.to_rfc3339();
    let saved = store.try_save_task_lease(TaskLeaseWrite {
        workflow_id,
        task_id,
        lease_id: &lease.lease_id,
        executor,
        acquired_at: &acquired_at_rfc3339,
        expires_at: &expires_at_rfc3339,
        data: &lease_value,
    })?;

    if saved {
        let report = TaskLeaseAcquireReport {
            status: "lease_acquired".to_string(),
            allowed: true,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            executor: executor.to_string(),
            lease: Some(lease),
            current_lease: None,
            reason: None,
        };
        store.record_event(
            workflow_id,
            "task_lease_acquired",
            &serde_json::to_value(&report)?,
        )?;
        return Ok(report);
    }

    let current_lease = load_current_lease(store, workflow_id, task_id)?;
    let report = TaskLeaseAcquireReport {
        status: "lease_conflict".to_string(),
        allowed: false,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        executor: executor.to_string(),
        lease: None,
        current_lease,
        reason: Some("task already has an unexpired lease".to_string()),
    };
    store.record_event(
        workflow_id,
        "task_lease_conflict",
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

pub fn release_task_lease(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    lease_id: &str,
    executor: &str,
) -> Result<TaskLeaseReleaseReport> {
    ensure_task_exists(store, workflow_id, task_id)?;
    let released = store.delete_task_lease(workflow_id, task_id, lease_id)?;
    let current_lease = load_current_lease(store, workflow_id, task_id)?;
    let report = TaskLeaseReleaseReport {
        status: if released {
            "lease_released".to_string()
        } else {
            "lease_not_found".to_string()
        },
        released,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        executor: executor.to_string(),
        lease_id: lease_id.to_string(),
        current_lease,
    };
    store.record_event(
        workflow_id,
        if released {
            "task_lease_released"
        } else {
            "task_lease_release_failed"
        },
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

fn ensure_task_exists(store: &ForgeStore, workflow_id: &str, task_id: &str) -> Result<()> {
    let workflow = store.load_workflow(workflow_id)?;
    if workflow.tasks.iter().any(|task| task.id == task_id) {
        return Ok(());
    }
    bail!("task not found in workflow {workflow_id}: {task_id}");
}

fn load_current_lease(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<Option<TaskLease>> {
    store
        .load_task_lease(workflow_id, task_id)?
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}
