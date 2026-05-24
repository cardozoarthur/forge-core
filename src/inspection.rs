use crate::checkpoint::load_workflow_checkpoints;
use crate::context::{
    build_context_package_with_checkpoint, context_next_action, summarize_context_handoff_tasks,
    ContextBudgetPlan, ContextDelta, ContextHandoffBlocker, ContextHandoffSummary,
    ContextHandoffTask, ContextNextAction, ContextPackage, ContextRoutingEconomy,
    ContextRoutingQuality, ContextRoutingRepair, ContextRoutingSummary, DEFAULT_CONTEXT_BUDGET,
};
use crate::graph::{
    AtomicTask, ChildSubflowRef, ExecutionPolicySpec, ExecutorKind, SubtaskSpec, TaskStatus,
    ValidationRule, Workflow,
};
use crate::registry::{list_workflows, WorkflowRegistryRow};
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
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
    pub workflow_task_count: usize,
    pub task_count: usize,
    pub verbose: bool,
    pub focus: Option<InspectionFocus>,
    pub subflow_count: usize,
    pub subflows: Vec<SubflowInspection>,
    pub handoff_summary: ContextHandoffSummary,
    pub nodes: Vec<TaskInspectionNode>,
    pub diagram: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectionFocus {
    pub task_id: String,
    pub title: String,
    pub node_count: usize,
    pub workflow_task_count: usize,
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
    pub persona_profile_id: Option<String>,
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
    pub prompt_packet_version: String,
    pub prompt_packet_sha256: String,
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
    pub routing_economy: ContextRoutingEconomy,
    pub routing_quality: ContextRoutingQuality,
    pub next_action: ContextNextAction,
    pub context_delta: ContextDelta,
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
    pub depth: usize,
    pub parent_workflow_id: String,
    pub parent_task_id: String,
    pub path: Vec<String>,
    pub reachable: bool,
    pub terminal: bool,
    pub child_workflow_status: Option<String>,
    pub child_lifecycle_state: Option<String>,
    pub child_task_count: usize,
    pub child_subflow_count: usize,
}

pub fn inspect_workflow(
    store: &ForgeStore,
    workflow_id: &str,
    verbose: bool,
) -> Result<WorkflowInspectionReport> {
    inspect_workflow_with_focus(store, workflow_id, verbose, None)
}

pub fn inspect_workflow_with_focus(
    store: &ForgeStore,
    workflow_id: &str,
    verbose: bool,
    focus_task_id: Option<&str>,
) -> Result<WorkflowInspectionReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let registry = list_workflows(store)?;
    let registry_row = registry
        .workflows
        .into_iter()
        .find(|row| row.workflow_id == workflow.id)
        .with_context(|| format!("workflow not found in registry: {}", workflow.id))?;
    let checkpoints = load_workflow_checkpoints(store, &workflow.id)?;
    let selected_tasks = selected_tasks(&workflow, focus_task_id)?;
    let mut nodes = Vec::new();
    let mut handoff_tasks = Vec::new();
    for task in selected_tasks {
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
    let focus = focus_task_id.map(|task_id| InspectionFocus {
        task_id: task_id.to_string(),
        title: nodes
            .first()
            .map(|node| node.title.clone())
            .unwrap_or_default(),
        node_count: nodes.len(),
        workflow_task_count: workflow.tasks.len(),
    });
    let handoff_summary = summarize_handoff_tasks(handoff_tasks);
    let subflows = collect_subflows(store, workflow_id, &nodes);
    let diagram = render_diagram(
        &registry_row,
        &nodes,
        &subflows,
        verbose,
        focus.as_ref(),
        workflow.tasks.len(),
    );

    Ok(WorkflowInspectionReport {
        status: "inspected".to_string(),
        workflow_id: workflow.id,
        initial_request: registry_row.initial_request,
        current_goal: workflow.goal,
        lifecycle_state: registry_row.lifecycle_state,
        workflow_revision: registry_row.workflow_revision,
        artifact_count: registry_row.artifact_count,
        workflow_task_count: workflow.tasks.len(),
        task_count: nodes.len(),
        verbose,
        focus,
        subflow_count: subflows.len(),
        subflows,
        handoff_summary,
        nodes,
        diagram,
    })
}

fn selected_tasks<'a>(
    workflow: &'a Workflow,
    focus_task_id: Option<&str>,
) -> Result<Vec<&'a AtomicTask>> {
    let Some(task_id) = focus_task_id else {
        return Ok(workflow.tasks.iter().collect());
    };

    let tasks = workflow
        .tasks
        .iter()
        .filter(|task| task.id == task_id)
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        bail!("task not found in workflow {}: {task_id}", workflow.id);
    }
    Ok(tasks)
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
        persona_profile_id: context_package
            .persona_profile
            .as_ref()
            .map(|profile| profile.profile_id.clone()),
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
        prompt_packet_version: package.prompt_packet.packet_version.clone(),
        prompt_packet_sha256: package.prompt_packet.packet_sha256.clone(),
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
        routing_economy: package.routing_economy.clone(),
        routing_quality: package.routing_quality.clone(),
        next_action: context_next_action(package),
        context_delta: package.context_delta.clone(),
    }
}

fn collect_subflows(
    store: &ForgeStore,
    root_workflow_id: &str,
    nodes: &[TaskInspectionNode],
) -> Vec<SubflowInspection> {
    let mut collector = SubflowCollector {
        store,
        visited_edges: BTreeSet::new(),
        subflows: Vec::new(),
    };

    for node in nodes {
        let root_path = vec![format!("{root_workflow_id}/{}", node.id)];
        collector.collect_refs(
            SubflowParentFrame {
                workflow_id: root_workflow_id,
                task_id: &node.id,
                path: &root_path,
                depth: 1,
            },
            &node.subflow_refs,
        );
    }

    collector.subflows
}

struct SubflowCollector<'a> {
    store: &'a ForgeStore,
    visited_edges: BTreeSet<String>,
    subflows: Vec<SubflowInspection>,
}

struct SubflowParentFrame<'a> {
    workflow_id: &'a str,
    task_id: &'a str,
    path: &'a [String],
    depth: usize,
}

impl SubflowCollector<'_> {
    fn collect_refs(&mut self, parent: SubflowParentFrame<'_>, refs: &[ChildSubflowRef]) {
        for subflow in refs {
            let current_ref = format!("{}/{}", subflow.workflow_id, subflow.task_id);
            let edge_key = format!("{}/{}->{current_ref}", parent.workflow_id, parent.task_id);
            if !self.visited_edges.insert(edge_key) {
                continue;
            }

            let mut path = parent.path.to_vec();
            path.push(current_ref.clone());
            let recursive_cycle = parent.path.iter().any(|ancestor| ancestor == &current_ref);

            match self.store.load_workflow(&subflow.workflow_id) {
                Ok(child_workflow) => {
                    self.collect_loaded_ref(
                        &parent,
                        subflow,
                        current_ref,
                        path,
                        recursive_cycle,
                        child_workflow,
                    );
                }
                Err(_) => {
                    self.subflows.push(SubflowInspection {
                        id: current_ref,
                        workflow_id: subflow.workflow_id.clone(),
                        task_id: subflow.task_id.clone(),
                        title: subflow.title.clone(),
                        lifecycle_state: subflow.lifecycle_state.clone(),
                        binding_status: subflow.binding_status.clone(),
                        depth: parent.depth,
                        parent_workflow_id: parent.workflow_id.to_string(),
                        parent_task_id: parent.task_id.to_string(),
                        path,
                        reachable: false,
                        terminal: true,
                        child_workflow_status: None,
                        child_lifecycle_state: None,
                        child_task_count: 0,
                        child_subflow_count: 0,
                    });
                }
            }
        }
    }

    fn collect_loaded_ref(
        &mut self,
        parent: &SubflowParentFrame<'_>,
        subflow: &ChildSubflowRef,
        current_ref: String,
        path: Vec<String>,
        recursive_cycle: bool,
        child_workflow: Workflow,
    ) {
        let child_task = child_workflow
            .tasks
            .iter()
            .find(|task| task.id == subflow.task_id);
        let child_refs = child_task
            .map(|task| task.child_subflows.clone())
            .unwrap_or_default();
        let child_subflow_count = child_refs.len();
        let child_task_count = child_workflow.tasks.len();
        let child_lifecycle_state = derive_child_lifecycle_state(&child_workflow);
        let terminal = child_refs.is_empty() || recursive_cycle;

        self.subflows.push(SubflowInspection {
            id: current_ref,
            workflow_id: subflow.workflow_id.clone(),
            task_id: subflow.task_id.clone(),
            title: subflow.title.clone(),
            lifecycle_state: subflow.lifecycle_state.clone(),
            binding_status: subflow.binding_status.clone(),
            depth: parent.depth,
            parent_workflow_id: parent.workflow_id.to_string(),
            parent_task_id: parent.task_id.to_string(),
            path: path.clone(),
            reachable: child_task.is_some(),
            terminal,
            child_workflow_status: Some(child_workflow.status.clone()),
            child_lifecycle_state: Some(child_lifecycle_state),
            child_task_count,
            child_subflow_count,
        });

        if !terminal {
            self.collect_refs(
                SubflowParentFrame {
                    workflow_id: &child_workflow.id,
                    task_id: &subflow.task_id,
                    path: &path,
                    depth: parent.depth + 1,
                },
                &child_refs,
            );
        }
    }
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

fn render_diagram(
    row: &WorkflowRegistryRow,
    nodes: &[TaskInspectionNode],
    subflows: &[SubflowInspection],
    verbose: bool,
    focus: Option<&InspectionFocus>,
    workflow_task_count: usize,
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
    if let Some(focus) = focus {
        lines.push(format!(
            "focus task: {} {} nodes: {}/{}",
            focus.task_id, focus.title, focus.node_count, workflow_task_count
        ));
    }

    for node in nodes {
        let dependency = if node.dependencies.is_empty() {
            "root".to_string()
        } else {
            format!("depends_on {}", node.dependencies.join(","))
        };
        let persona = node
            .persona_mode
            .as_ref()
            .map(|mode| {
                let profile = node
                    .persona_profile_id
                    .as_deref()
                    .map(|profile_id| format!(" profile {profile_id}"))
                    .unwrap_or_default();
                format!(" persona {mode}{profile}")
            })
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
            " context {} {} {}/{} cache {} packet {} next {} delta {} budget_plan {}/{} {}",
            node.context_route.profile_id,
            node.context_route.handoff_status,
            node.context_route.context_bytes,
            node.context_route.effective_budget,
            short_hash(&node.context_route.routing_cache_key),
            short_hash(&node.context_route.prompt_packet_sha256),
            node.context_route.next_action.action,
            node.context_route.context_delta.status,
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
        let economy = format!(
            " economy {} avoided {} reduction_bps {}",
            node.context_route.routing_economy.cost_decision,
            node.context_route.routing_economy.total_avoided_bytes,
            node.context_route.routing_economy.reduction_bps
        );
        lines.push(format!(
            "{} {} [{}] {}{}{} handoff {} executor {}{}{}{}",
            node.id,
            node.title,
            node.status,
            dependency,
            persona,
            subflow_refs,
            node.handoff_status,
            node.executor,
            context_route,
            execution_policy,
            economy
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

    if !subflows.is_empty() {
        lines.push("subflows:".to_string());
        for subflow in subflows {
            lines.push(format!(
                "  subflow depth {} {} [{}] binding {} reachable {} terminal {}",
                subflow.depth,
                subflow.path.join(" -> "),
                subflow
                    .child_lifecycle_state
                    .as_deref()
                    .unwrap_or(subflow.lifecycle_state.as_str()),
                subflow.binding_status,
                subflow.reachable,
                subflow.terminal
            ));
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
