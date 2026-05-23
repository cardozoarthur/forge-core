use crate::graph::{AtomicTask, ExecutorKind, SubtaskSpec, TaskStatus, ValidationRule};
use crate::registry::{list_workflows, WorkflowRegistryRow};
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowInspectionReport {
    pub status: String,
    pub workflow_id: String,
    pub initial_request: String,
    pub current_goal: String,
    pub lifecycle_state: String,
    pub workflow_revision: u64,
    pub artifact_count: usize,
    pub task_count: usize,
    pub verbose: bool,
    pub subflow_count: usize,
    pub subflows: Vec<SubflowInspection>,
    pub nodes: Vec<TaskInspectionNode>,
    pub diagram: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskInspectionNode {
    pub id: String,
    pub title: String,
    pub status: String,
    pub dependencies: Vec<String>,
    pub executor: String,
    pub persona_mode: Option<String>,
    pub goal: String,
    pub expected_output: String,
    pub subtasks: Vec<SubtaskSpec>,
    pub validation_rules: Vec<ValidationRule>,
    pub subflow_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubflowInspection {
    pub id: String,
    pub title: String,
    pub lifecycle_state: String,
}

pub fn inspect_workflow(
    store: &ForgeStore,
    workflow_id: &str,
    verbose: bool,
) -> Result<WorkflowInspectionReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let registry = list_workflows(store)?;
    let registry_row = registry
        .workflows
        .into_iter()
        .find(|row| row.workflow_id == workflow.id)
        .with_context(|| format!("workflow not found in registry: {}", workflow.id))?;
    let nodes = workflow
        .tasks
        .iter()
        .map(|task| task_node(task, verbose))
        .collect::<Vec<_>>();
    let diagram = render_diagram(&registry_row, &nodes, verbose);

    Ok(WorkflowInspectionReport {
        status: "inspected".to_string(),
        workflow_id: workflow.id,
        initial_request: registry_row.initial_request,
        current_goal: workflow.goal,
        lifecycle_state: registry_row.lifecycle_state,
        workflow_revision: registry_row.workflow_revision,
        artifact_count: registry_row.artifact_count,
        task_count: nodes.len(),
        verbose,
        subflow_count: 0,
        subflows: Vec::new(),
        nodes,
        diagram,
    })
}

fn task_node(task: &AtomicTask, verbose: bool) -> TaskInspectionNode {
    TaskInspectionNode {
        id: task.id.clone(),
        title: task.title.clone(),
        status: task_status(&task.status).to_string(),
        dependencies: task.dependencies.clone(),
        executor: executor_kind(&task.executor).to_string(),
        persona_mode: task.persona.as_ref().map(|persona| persona.mode.clone()),
        goal: task.goal.clone(),
        expected_output: task.expected_output.clone(),
        subtasks: if verbose {
            task.work_item.subtasks.clone()
        } else {
            Vec::new()
        },
        validation_rules: if verbose {
            task.validation_rules.clone()
        } else {
            Vec::new()
        },
        subflow_refs: Vec::new(),
    }
}

fn render_diagram(
    row: &WorkflowRegistryRow,
    nodes: &[TaskInspectionNode],
    verbose: bool,
) -> String {
    let mut lines = vec![
        format!("Workflow {} [{}]", row.workflow_id, row.lifecycle_state),
        format!("initial_request: {}", row.initial_request),
        format!("current_goal: {}", row.current_goal),
        format!(
            "revision: {} artifacts: {} tasks: {} subflows: 0",
            row.workflow_revision,
            row.artifact_count,
            nodes.len()
        ),
    ];

    for node in nodes {
        let dependency = if node.dependencies.is_empty() {
            "root".to_string()
        } else {
            format!("depends_on {}", node.dependencies.join(","))
        };
        let persona = node
            .persona_mode
            .as_ref()
            .map(|mode| format!(" persona {mode}"))
            .unwrap_or_default();
        lines.push(format!(
            "{} {} [{}] {}{} executor {}",
            node.id, node.title, node.status, dependency, persona, node.executor
        ));

        if verbose {
            lines.push(format!("  goal: {}", node.goal));
            lines.push(format!("  expected_output: {}", node.expected_output));
            for rule in &node.validation_rules {
                lines.push(format!("  validates {} -> {}", rule.kind, rule.expected));
            }
            for subtask in &node.subtasks {
                lines.push(format!(
                    "  subtask {} {} [{}]",
                    subtask.id,
                    subtask.title,
                    task_status(&subtask.status)
                ));
            }
        }
    }

    lines.join("\n")
}

fn task_status(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
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
