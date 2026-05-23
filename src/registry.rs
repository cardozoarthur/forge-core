use crate::graph::{TaskStatus, Workflow};
use crate::request::RunRecord;
use crate::storage::ForgeStore;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRegistryReport {
    pub status: String,
    pub summary: WorkflowRegistrySummary,
    pub workflows: Vec<WorkflowRegistryRow>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkflowRegistrySummary {
    pub total: usize,
    pub running: usize,
    pub non_running: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRegistryRow {
    pub workflow_id: String,
    pub run_ids: Vec<String>,
    pub run_statuses: Vec<String>,
    pub initial_request: String,
    pub current_goal: String,
    pub workflow_status: String,
    pub lifecycle_state: String,
    pub running: bool,
    pub workflow_revision: u64,
    pub artifact_count: usize,
    pub task_summary: RegistryTaskStatusSummary,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryTaskStatusSummary {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub blocked: usize,
    pub failed: usize,
}

pub fn list_workflows(store: &ForgeStore) -> Result<WorkflowRegistryReport> {
    let workflows = store.load_workflows()?;
    let runs_by_workflow = load_runs_by_workflow(store)?;
    let mut rows = Vec::new();

    for workflow in workflows {
        let runs = runs_by_workflow
            .get(&workflow.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        rows.push(registry_row(&workflow, runs));
    }

    let running = rows.iter().filter(|row| row.running).count();
    let summary = WorkflowRegistrySummary {
        total: rows.len(),
        running,
        non_running: rows.len().saturating_sub(running),
    };

    Ok(WorkflowRegistryReport {
        status: "loaded".to_string(),
        summary,
        workflows: rows,
    })
}

fn load_runs_by_workflow(store: &ForgeStore) -> Result<BTreeMap<String, Vec<RunRecord>>> {
    let mut runs_by_workflow: BTreeMap<String, Vec<RunRecord>> = BTreeMap::new();
    for value in store.load_runs()? {
        let run: RunRecord = serde_json::from_value(value)?;
        runs_by_workflow
            .entry(run.workflow_id.clone())
            .or_default()
            .push(run);
    }
    Ok(runs_by_workflow)
}

fn registry_row(workflow: &Workflow, runs: &[RunRecord]) -> WorkflowRegistryRow {
    let task_summary = summarize_tasks(workflow);
    let lifecycle_state = derive_lifecycle_state(workflow, &task_summary);
    let running = lifecycle_state == "running";
    let workflow_revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0);

    WorkflowRegistryRow {
        workflow_id: workflow.id.clone(),
        run_ids: runs.iter().map(|run| run.run_id.clone()).collect(),
        run_statuses: runs.iter().map(|run| run.status.clone()).collect(),
        initial_request: initial_request(workflow, runs),
        current_goal: workflow.goal.clone(),
        workflow_status: workflow.status.clone(),
        lifecycle_state,
        running,
        workflow_revision,
        artifact_count: workflow.artifacts.len(),
        task_summary,
        created_at: workflow.created_at,
    }
}

fn initial_request(workflow: &Workflow, runs: &[RunRecord]) -> String {
    workflow
        .initial_goal
        .as_ref()
        .filter(|goal| !goal.trim().is_empty())
        .cloned()
        .or_else(|| runs.first().map(|run| run.goal.clone()))
        .unwrap_or_else(|| workflow.goal.clone())
}

fn derive_lifecycle_state(workflow: &Workflow, task_summary: &RegistryTaskStatusSummary) -> String {
    if workflow.status == "failed" || task_summary.failed > 0 {
        return "failed".to_string();
    }
    if workflow.status == "blocked" || task_summary.blocked > 0 {
        return "blocked".to_string();
    }
    if task_summary.running > 0 {
        return "running".to_string();
    }
    if workflow.status == "completed" {
        if task_summary.total == task_summary.completed {
            return "scaled_to_zero".to_string();
        }
        return "completed".to_string();
    }
    "idle".to_string()
}

fn summarize_tasks(workflow: &Workflow) -> RegistryTaskStatusSummary {
    let mut summary = RegistryTaskStatusSummary {
        total: workflow.tasks.len(),
        ..RegistryTaskStatusSummary::default()
    };
    for task in &workflow.tasks {
        match &task.status {
            TaskStatus::Pending => summary.pending += 1,
            TaskStatus::Running => summary.running += 1,
            TaskStatus::Completed => summary.completed += 1,
            TaskStatus::Blocked => summary.blocked += 1,
            TaskStatus::Failed => summary.failed += 1,
        }
    }
    summary
}
