use crate::checkpoint::load_workflow_checkpoints;
use crate::context::{
    build_context_package_with_checkpoint, context_next_action, summarize_context_handoff_tasks,
    ContextBudgetPlan, ContextHandoffBlocker, ContextHandoffSummary, ContextHandoffTask,
    ContextNextAction, ContextPackage, ContextRoutingQuality, ContextRoutingRepair,
    ContextRoutingSummary, DEFAULT_CONTEXT_BUDGET,
};
use crate::graph::{
    AtomicTask, ChildSubflowRef, ExecutionPolicySpec, ExecutorKind, SubtaskSpec, TaskStatus,
    ValidationRule,
};
use crate::registry::{list_workflows, WorkflowRegistryRow};
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeSet;

const INSPECT_EXECUTION_POLICY_SCHEMA_VERSION: &str = "forge.inspect_execution_policy.v1";

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
    pub handoff_summary: ContextHandoffSummary,
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
    pub execution_policy: ExecutionPolicyInspection,
    pub persona_mode: Option<String>,
    pub context_route: ContextInspectionRoute,
    pub goal: String,
    pub expected_output: String,
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub handoff_blockers: Vec<ContextHandoffBlocker>,
    pub subtasks: Vec<SubtaskSpec>,
    pub validation_rules: Vec<ValidationRule>,
    pub subflow_refs: Vec<ChildSubflowRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextInspectionRoute {
    pub schema_version: String,
    pub routing_policy: String,
    pub routing_fingerprint_schema_version: String,
    pub routing_cache_key: String,
    pub routing_lineage_sha256: String,
    pub profile_id: String,
    pub reasoning_allowed: bool,
    pub deterministic: bool,
    pub requested_budget: usize,
    pub effective_budget: usize,
    pub context_bytes: usize,
    pub context_sha256: String,
    pub context_ready: bool,
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub resume_context_status: String,
    pub missing_required_sections: Vec<String>,
    pub included_sections: Vec<String>,
    pub omitted_sections: Vec<String>,
    pub routing_summary: ContextRoutingSummary,
    pub routing_repair: ContextRoutingRepair,
    pub budget_plan: ContextBudgetPlan,
    pub routing_quality: ContextRoutingQuality,
    pub next_action: ContextNextAction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPolicyInspection {
    pub schema_version: String,
    pub mode: String,
    pub ai_allowed: bool,
    pub deterministic: bool,
    pub reuse_hint: String,
    pub selection_reason: String,
    pub validation_gate: String,
    pub code_runtime_language: Option<String>,
    pub code_runtime_entrypoint: Option<String>,
    pub code_runtime_sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubflowInspection {
    pub id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub title: String,
    pub lifecycle_state: String,
    pub binding_status: String,
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
    let checkpoints = load_workflow_checkpoints(store, &workflow.id)?;
    let mut nodes = Vec::new();
    let mut handoff_tasks = Vec::new();
    for task in &workflow.tasks {
        let latest_checkpoint = checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.task_id == task.id)
            .cloned();
        let context_package = build_context_package_with_checkpoint(
            &workflow,
            &task.id,
            DEFAULT_CONTEXT_BUDGET,
            latest_checkpoint,
        )?;
        let handoff = handoff_task(task, &context_package);
        nodes.push(task_node(task, &handoff, &context_package, verbose));
        handoff_tasks.push(handoff);
    }
    let handoff_summary = summarize_handoff_tasks(handoff_tasks);
    let subflows = collect_subflows(&nodes);
    let diagram = render_diagram(&registry_row, &nodes, &subflows, verbose);

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
        subflow_count: subflows.len(),
        subflows,
        handoff_summary,
        nodes,
        diagram,
    })
}

fn handoff_task(task: &AtomicTask, package: &ContextPackage) -> ContextHandoffTask {
    let blocking_refs = package
        .handoff_blockers
        .iter()
        .flat_map(|blocker| blocker.refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    ContextHandoffTask {
        task_id: task.id.clone(),
        title: task.title.clone(),
        executor: executor_kind(&task.executor).to_string(),
        context_ready: package.context_ready,
        dependency_ready: package.dependency_summary.ready,
        handoff_ready: package.handoff_ready,
        handoff_status: package.handoff_status.clone(),
        handoff_blockers: package.handoff_blockers.clone(),
        blocking_refs,
        context_sha256: package.context_sha256.clone(),
        resume_context_status: package.resume_context_status.clone(),
        routing_quality: package.routing_quality.clone(),
    }
}

fn summarize_handoff_tasks(tasks: Vec<ContextHandoffTask>) -> ContextHandoffSummary {
    summarize_context_handoff_tasks(tasks)
}

fn task_node(
    task: &AtomicTask,
    handoff: &ContextHandoffTask,
    context_package: &ContextPackage,
    verbose: bool,
) -> TaskInspectionNode {
    TaskInspectionNode {
        id: task.id.clone(),
        title: task.title.clone(),
        status: task_status(&task.status).to_string(),
        dependencies: task.dependencies.clone(),
        executor: executor_kind(&task.executor).to_string(),
        execution_policy: execution_policy_inspection(&task.execution_policy),
        persona_mode: task.persona.as_ref().map(|persona| persona.mode.clone()),
        context_route: context_route(context_package),
        goal: task.goal.clone(),
        expected_output: task.expected_output.clone(),
        handoff_ready: handoff.handoff_ready,
        handoff_status: handoff.handoff_status.clone(),
        handoff_blockers: handoff.handoff_blockers.clone(),
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
        subflow_refs: task.child_subflows.clone(),
    }
}

fn execution_policy_inspection(policy: &ExecutionPolicySpec) -> ExecutionPolicyInspection {
    ExecutionPolicyInspection {
        schema_version: INSPECT_EXECUTION_POLICY_SCHEMA_VERSION.to_string(),
        mode: policy.mode.clone(),
        ai_allowed: policy.ai_allowed,
        deterministic: policy.deterministic,
        reuse_hint: policy.reuse_hint.clone(),
        selection_reason: policy.selection_reason.clone(),
        validation_gate: policy.validation_gate.clone(),
        code_runtime_language: policy
            .code_runtime
            .as_ref()
            .map(|runtime| runtime.language.clone()),
        code_runtime_entrypoint: policy
            .code_runtime
            .as_ref()
            .map(|runtime| runtime.entrypoint.clone()),
        code_runtime_sandbox: policy
            .code_runtime
            .as_ref()
            .map(|runtime| runtime.sandbox.clone()),
    }
}

fn context_route(package: &ContextPackage) -> ContextInspectionRoute {
    ContextInspectionRoute {
        schema_version: package.schema_version.clone(),
        routing_policy: package.routing_policy.clone(),
        routing_fingerprint_schema_version: package.routing_fingerprint.schema_version.clone(),
        routing_cache_key: package.routing_fingerprint.cache_key.clone(),
        routing_lineage_sha256: package.routing_fingerprint.lineage_sha256.clone(),
        profile_id: package.executor_profile.id.clone(),
        reasoning_allowed: package.executor_profile.reasoning_allowed,
        deterministic: package.executor_profile.deterministic,
        requested_budget: package.requested_budget,
        effective_budget: package.effective_budget,
        context_bytes: package.context_bytes,
        context_sha256: package.context_sha256.clone(),
        context_ready: package.context_ready,
        handoff_ready: package.handoff_ready,
        handoff_status: package.handoff_status.clone(),
        resume_context_status: package.resume_context_status.clone(),
        missing_required_sections: package.missing_required_sections.clone(),
        included_sections: package.included_sections.clone(),
        omitted_sections: package.omitted_sections.clone(),
        routing_summary: package.routing_summary.clone(),
        routing_repair: package.routing_repair.clone(),
        budget_plan: package.budget_plan.clone(),
        routing_quality: package.routing_quality.clone(),
        next_action: context_next_action(package),
    }
}

fn collect_subflows(nodes: &[TaskInspectionNode]) -> Vec<SubflowInspection> {
    nodes
        .iter()
        .flat_map(|node| {
            node.subflow_refs.iter().map(|subflow| SubflowInspection {
                id: format!("{}/{}", subflow.workflow_id, subflow.task_id),
                workflow_id: subflow.workflow_id.clone(),
                task_id: subflow.task_id.clone(),
                title: subflow.title.clone(),
                lifecycle_state: subflow.lifecycle_state.clone(),
                binding_status: subflow.binding_status.clone(),
            })
        })
        .collect()
}

fn render_diagram(
    row: &WorkflowRegistryRow,
    nodes: &[TaskInspectionNode],
    subflows: &[SubflowInspection],
    verbose: bool,
) -> String {
    let mut lines = vec![
        format!("Workflow {} [{}]", row.workflow_id, row.lifecycle_state),
        format!("initial_request: {}", row.initial_request),
        format!("current_goal: {}", row.current_goal),
        format!(
            "revision: {} artifacts: {} tasks: {} subflows: {}",
            row.workflow_revision,
            row.artifact_count,
            nodes.len(),
            subflows.len()
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
        let subflow_refs = if node.subflow_refs.is_empty() {
            String::new()
        } else {
            format!(
                " subflows {}",
                node.subflow_refs
                    .iter()
                    .map(|subflow| format!(
                        "{}/{}:{}",
                        subflow.workflow_id, subflow.task_id, subflow.binding_status
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let context_route = format!(
            " context {} {} {}/{} cache {} next {} budget_plan {}/{} {}",
            node.context_route.profile_id,
            node.context_route.handoff_status,
            node.context_route.context_bytes,
            node.context_route.effective_budget,
            short_hash(&node.context_route.routing_cache_key),
            node.context_route.next_action.action,
            node.context_route.budget_plan.minimum_correct_budget_bytes,
            node.context_route.budget_plan.recommended_budget_bytes,
            node.context_route.budget_plan.status
        );
        let execution_policy = format!(
            " policy {} {} {} {} {}",
            node.execution_policy.mode,
            if node.execution_policy.ai_allowed {
                "ai"
            } else {
                "no_ai"
            },
            if node.execution_policy.deterministic {
                "deterministic"
            } else {
                "reasoning"
            },
            node.execution_policy
                .code_runtime_language
                .as_deref()
                .unwrap_or("adapter"),
            node.execution_policy.reuse_hint
        );
        lines.push(format!(
            "{} {} [{}] {}{}{} handoff {} executor {}{}{}",
            node.id,
            node.title,
            node.status,
            dependency,
            persona,
            subflow_refs,
            node.handoff_status,
            node.executor,
            context_route,
            execution_policy
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

fn short_hash(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}
