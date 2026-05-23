use crate::artifact::hex_sha256;
use crate::graph::{AtomicTask, ExecutionPolicySpec, ExecutorKind, TaskStatus, Workflow};
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
    pub reusable_subflows: usize,
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
    pub reusable_subflows: Vec<ReusableSubflowRef>,
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

#[derive(Debug, Clone, Serialize)]
pub struct ReusableSubflowRef {
    pub workflow_id: String,
    pub task_id: String,
    pub title: String,
    pub executor: String,
    pub policy_mode: String,
    pub reuse_hint: String,
    pub reuse_key: String,
    pub context_lineage_sha256: String,
    pub language: Option<String>,
    pub entrypoint: Option<String>,
    pub validation_gate: String,
    pub lifecycle_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowReuseCandidate {
    pub requested_task_id: String,
    pub requested_title: String,
    pub candidate_workflow_id: String,
    pub candidate_task_id: String,
    pub candidate_title: String,
    pub reuse_key: String,
    pub context_lineage_sha256: String,
    pub policy_mode: String,
    pub candidate_lifecycle_state: String,
    pub attachable_as_child_subflow: bool,
    pub reason: String,
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
    let reusable_subflows = rows.iter().map(|row| row.reusable_subflows.len()).sum();
    let summary = WorkflowRegistrySummary {
        total: rows.len(),
        running,
        non_running: rows.len().saturating_sub(running),
        reusable_subflows,
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

pub fn find_reuse_candidates(
    store: &ForgeStore,
    requested_workflow: &Workflow,
) -> Result<Vec<WorkflowReuseCandidate>> {
    let requested_subflows = reusable_subflows(requested_workflow, "candidate");
    if requested_subflows.is_empty() {
        return Ok(Vec::new());
    }

    let registry = list_workflows(store)?;
    let mut candidates = Vec::new();
    for requested in &requested_subflows {
        for row in &registry.workflows {
            if row.workflow_id == requested.workflow_id {
                continue;
            }
            for existing in &row.reusable_subflows {
                if existing.reuse_key != requested.reuse_key
                    || existing.context_lineage_sha256 != requested.context_lineage_sha256
                {
                    continue;
                }
                let attachable = attachable_lifecycle(&existing.lifecycle_state);
                candidates.push(WorkflowReuseCandidate {
                    requested_task_id: requested.task_id.clone(),
                    requested_title: requested.title.clone(),
                    candidate_workflow_id: existing.workflow_id.clone(),
                    candidate_task_id: existing.task_id.clone(),
                    candidate_title: existing.title.clone(),
                    reuse_key: existing.reuse_key.clone(),
                    context_lineage_sha256: existing.context_lineage_sha256.clone(),
                    policy_mode: existing.policy_mode.clone(),
                    candidate_lifecycle_state: existing.lifecycle_state.clone(),
                    attachable_as_child_subflow: attachable,
                    reason: if attachable {
                        "compatible deterministic code node can be attached as a child subflow"
                            .to_string()
                    } else {
                        "compatible deterministic code node exists but lifecycle is not attachable"
                            .to_string()
                    },
                });
            }
        }
    }

    candidates.sort_by(|left, right| {
        right
            .attachable_as_child_subflow
            .cmp(&left.attachable_as_child_subflow)
            .then_with(|| left.candidate_workflow_id.cmp(&right.candidate_workflow_id))
            .then_with(|| left.candidate_task_id.cmp(&right.candidate_task_id))
    });
    Ok(candidates)
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
    let reusable_subflows = reusable_subflows(workflow, &lifecycle_state);

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
        reusable_subflows,
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

fn reusable_subflows(workflow: &Workflow, lifecycle_state: &str) -> Vec<ReusableSubflowRef> {
    workflow
        .tasks
        .iter()
        .filter_map(|task| reusable_subflow(workflow, lifecycle_state, task))
        .collect()
}

fn reusable_subflow(
    workflow: &Workflow,
    lifecycle_state: &str,
    task: &AtomicTask,
) -> Option<ReusableSubflowRef> {
    let policy = &task.execution_policy;
    if policy.reuse_hint != "reuse_compatible_code_node" {
        return None;
    }
    let reuse_key = execution_policy_reuse_key(policy)?;
    let runtime = policy.code_runtime.as_ref();
    Some(ReusableSubflowRef {
        workflow_id: workflow.id.clone(),
        task_id: task.id.clone(),
        title: task.title.clone(),
        executor: executor_kind(&task.executor).to_string(),
        policy_mode: policy.mode.clone(),
        reuse_hint: policy.reuse_hint.clone(),
        reuse_key,
        context_lineage_sha256: context_lineage_key(task),
        language: runtime.map(|runtime| runtime.language.clone()),
        entrypoint: runtime.map(|runtime| runtime.entrypoint.clone()),
        validation_gate: policy.validation_gate.clone(),
        lifecycle_state: lifecycle_state.to_string(),
    })
}

fn execution_policy_reuse_key(policy: &ExecutionPolicySpec) -> Option<String> {
    if policy.mode != "local_code_node" || policy.ai_allowed || !policy.deterministic {
        return None;
    }
    let runtime = policy.code_runtime.as_ref()?;
    Some(format!(
        "{}:{}:{}:{}",
        policy.mode, runtime.language, runtime.entrypoint, policy.validation_gate
    ))
}

fn context_lineage_key(task: &AtomicTask) -> String {
    let validation_rules = task
        .validation_rules
        .iter()
        .map(|rule| {
            format!(
                "{}:{}:{}",
                rule.kind,
                rule.command.as_deref().unwrap_or(""),
                rule.expected
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let seed = format!(
        "{}\n{}\n{}\n{}",
        task.title,
        task.expected_output,
        task.context_requirements.join("\n"),
        validation_rules
    );
    hex_sha256(seed.as_bytes())
}

fn attachable_lifecycle(lifecycle_state: &str) -> bool {
    matches!(lifecycle_state, "idle" | "completed" | "scaled_to_zero")
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

fn executor_kind(executor: &ExecutorKind) -> &'static str {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
}
