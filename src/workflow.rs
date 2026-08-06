use crate::addon::{default_addon_dirs, load_addon_catalog_from_store};
use crate::artifact::copy_artifact;
use crate::graph::{
    node_brain_routing_for_executor, task as build_task, ArtifactRecord, ExecutorKind,
    NodeBrainAgentSlotSpec, NodeBrainRoutingSpec, TaskImpediment, TaskStatus, Workflow,
    WorkflowRevision,
};
use crate::identity::ensure_workflow_policy;
use crate::intent::parse_intent_with_catalog_and_context;
use crate::ir::{
    preview_token_change_impact, resolve_token_collection, CollaborationAuditEvent,
    CollaborationComment, CollaborationConflictEvent, CollaborationPatchEvent,
    CollaborationPresence, CollaborationRollbackEvent, ConcreteChange, CreativeArtifact,
    CreativeCollaborationState, CreativeCollaborationSummary, PatchByIntent, PatchRecord,
    TokenCollection, TokenImpactPreview, TokenResolutionReport,
};
use crate::storage::FoundryStore;
use crate::validation::{validate_workflow, validate_workflow_structure, ValidationReport};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use uuid::Uuid;

const WORKFLOW_MUTATION_SCHEMA_VERSION: &str = "foundry.workflow_mutation.v1";

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowGoalUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub previous_goal: String,
    pub new_goal: String,
    pub revision: u64,
    pub previous_deliverable_count: usize,
    pub new_deliverable_count: usize,
    pub added_deliverables: Vec<String>,
    pub removed_deliverables: Vec<String>,
    pub previous_capabilities: Vec<String>,
    pub new_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStatusUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub previous_status: String,
    pub new_status: String,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<ValidationReport>,
}

#[derive(Debug, Clone)]
pub struct WorkflowTaskUpdateInput<'a> {
    pub task_id: &'a str,
    pub title: Option<&'a str>,
    pub goal: Option<&'a str>,
    pub expected_output: Option<&'a str>,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTaskUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub previous_title: String,
    pub new_title: String,
    pub previous_goal: String,
    pub new_goal: String,
    pub previous_expected_output: String,
    pub new_expected_output: String,
    pub previous_version: u64,
    pub new_version: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowTaskBatchUpdateItem {
    pub task_id: String,
    pub title: String,
    pub goal: String,
    pub expected_output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTaskBatchUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub updated_task_ids: Vec<String>,
    pub unchanged_task_ids: Vec<String>,
    pub revision: u64,
}

#[derive(Debug, Clone)]
pub struct WorkflowTaskAddInput {
    pub task_id: Option<String>,
    pub description: String,
    pub priority: String,
    pub origin: String,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WorkflowTaskPriorityInput {
    pub task_id: String,
    pub priority: String,
    pub origin: String,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WorkflowTaskDependencyInput {
    pub task_id: String,
    pub dependency_task_id: String,
    pub origin: String,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WorkflowTaskImpedimentInput {
    pub task_id: String,
    pub reason: String,
    pub kind: String,
    pub origin: String,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WorkflowTaskImpedimentClearInput {
    pub task_id: String,
    pub impediment_id: Option<String>,
    pub origin: String,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowMutationReport {
    pub schema_version: String,
    pub status: String,
    pub mutation: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub changed: bool,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_task_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_task_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impediment: Option<TaskImpediment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cleared_impediment_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_task_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowNodeBrainRoutingUpdateInput {
    pub task_id: String,
    pub default_brain: Option<String>,
    pub allowed_brains: Vec<String>,
    pub agent_slots: Vec<NodeBrainAgentSlotSpec>,
    pub max_parallel_agents: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowNodeBrainRoutingUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub orchestrator_brain: String,
    pub can_switch_without_stopping_workflow: bool,
    pub previous_version: u64,
    pub new_version: u64,
    pub previous_routing: NodeBrainRoutingSpec,
    pub new_routing: NodeBrainRoutingSpec,
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

#[derive(Debug, Clone)]
pub(crate) struct PreparedArtifactAttach {
    artifact: ArtifactRecord,
    bytes: u64,
    origin: String,
    source_description: String,
}

impl PreparedArtifactAttach {
    pub(crate) fn artifact_id(&self) -> &str {
        &self.artifact.id
    }
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
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductDecisionReport {
    pub status: String,
    pub workflow_id: String,
    pub decision_id: String,
    pub revision: u64,
    pub decision: crate::graph::ProductDecision,
}

#[derive(Debug, Clone)]
pub struct ProductDecisionInput {
    pub title: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
    pub trade_offs: Vec<String>,
    pub success_metrics: Vec<String>,
    pub backlog_mutation: String,
    pub author: String,
    pub affected_goals: Vec<String>,
    pub affected_tasks: Vec<String>,
    pub affected_artifacts: Vec<String>,
    pub origin: String,
}

pub fn record_product_decision(
    store: &FoundryStore,
    workflow_id: &str,
    input: ProductDecisionInput,
) -> Result<ProductDecisionReport> {
    ensure_workflow_policy(store, workflow_id, "record product decision")?;
    let mut workflow = store.load_workflow(workflow_id)?;
    let decision_id = format!("dec_{}", Uuid::new_v4().to_string().replace('-', ""));
    let revision = push_revision(
        &mut workflow.revisions,
        &input.origin,
        "product_decision_recorded",
        &format!("recorded product decision: {}", input.title),
    );
    let decision = crate::graph::ProductDecision {
        id: decision_id.clone(),
        title: input.title,
        rationale: input.rationale,
        alternatives: input.alternatives,
        trade_offs: input.trade_offs,
        success_metrics: input.success_metrics,
        backlog_mutation: input.backlog_mutation,
        author: input.author,
        status: "approved".to_string(), // default to approved for now as per human-guided requirement
        revision,
        created_at: Utc::now(),
        affected_goals: input.affected_goals,
        affected_tasks: input.affected_tasks,
        affected_artifacts: input.affected_artifacts,
    };
    workflow.product_decisions.push(decision.clone());
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "product_decision_recorded",
        &serde_json::to_value(&decision)?,
    )?;

    Ok(ProductDecisionReport {
        status: "product_decision_recorded".to_string(),
        workflow_id: workflow_id.to_string(),
        decision_id,
        revision,
        decision,
    })
}

pub fn update_workflow_goal(
    store: &FoundryStore,
    workflow_id: &str,
    goal: &str,
    origin: &str,
) -> Result<WorkflowGoalUpdateReport> {
    update_workflow_goal_with_expected_revision(store, workflow_id, goal, origin, None)
}

pub fn update_workflow_goal_with_expected_revision(
    store: &FoundryStore,
    workflow_id: &str,
    goal: &str,
    origin: &str,
    expected_revision: Option<u64>,
) -> Result<WorkflowGoalUpdateReport> {
    let goal = goal.trim();
    if goal.is_empty() {
        bail!("workflow goal cannot be empty");
    }
    store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "workflow goal update")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_expected_revision(&workflow, expected_revision)?;
        ensure_structural_mutation_allowed(&workflow, "update goal")?;
        ensure_core_orchestration_integrity(&workflow)?;
        let previous_goal = workflow.goal.clone();
        let previous_intent = workflow.intent.clone();
        let previous_deliverables = previous_intent.deliverables.clone();
        let previous_capabilities = previous_intent
            .required_capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect::<Vec<_>>();
        if previous_goal == goal {
            return Ok(WorkflowGoalUpdateReport {
                status: "workflow_goal_unchanged".to_string(),
                workflow_id: workflow_id.to_string(),
                origin: origin.to_string(),
                previous_goal,
                new_goal: goal.to_string(),
                revision: latest_revision(&workflow),
                previous_deliverable_count: previous_deliverables.len(),
                new_deliverable_count: previous_deliverables.len(),
                added_deliverables: Vec::new(),
                removed_deliverables: Vec::new(),
                previous_capabilities: previous_capabilities.clone(),
                new_capabilities: previous_capabilities,
            });
        }
        let addon_catalog = load_addon_catalog_from_store(store, &default_addon_dirs())?;
        let new_intent = parse_intent_with_catalog_and_context(
            goal,
            &addon_catalog,
            previous_intent.operating_context.clone(),
        );
        let new_deliverables = new_intent.deliverables.clone();
        let new_capabilities = new_intent
            .required_capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect::<Vec<_>>();
        let added_deliverables = diff_added(&previous_deliverables, &new_deliverables);
        let removed_deliverables = diff_removed(&previous_deliverables, &new_deliverables);
        workflow.goal = goal.to_string();
        workflow.intent = new_intent;
        let revision = push_revision(
            &mut workflow.revisions,
            origin,
            "goal_update",
            &format!("goal changed from `{previous_goal}` to `{goal}` and intent was reparsed"),
        );
        let report = WorkflowGoalUpdateReport {
            status: "workflow_goal_updated".to_string(),
            workflow_id: workflow_id.to_string(),
            origin: origin.to_string(),
            previous_goal,
            new_goal: goal.to_string(),
            revision,
            previous_deliverable_count: previous_deliverables.len(),
            new_deliverable_count: new_deliverables.len(),
            added_deliverables,
            removed_deliverables,
            previous_capabilities,
            new_capabilities,
        };
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "workflow_goal_updated",
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

pub fn pause_workflow(
    store: &FoundryStore,
    workflow_id: &str,
    origin: &str,
) -> Result<WorkflowStatusUpdateReport> {
    set_workflow_status(
        store,
        workflow_id,
        "paused",
        origin,
        "workflow_paused",
        None,
    )
}

pub fn resume_workflow(
    store: &FoundryStore,
    workflow_id: &str,
    origin: &str,
) -> Result<WorkflowStatusUpdateReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let status = resumed_workflow_status(&workflow);
    set_workflow_status(
        store,
        workflow_id,
        &status,
        origin,
        "workflow_resumed",
        None,
    )
}

pub fn complete_workflow(
    store: &FoundryStore,
    workflow_id: &str,
    origin: &str,
) -> Result<WorkflowStatusUpdateReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let validation = validate_workflow(&workflow);
    if !validation.promotable {
        bail!(
            "workflow {workflow_id} is not validation-ready for completion: {} failed rule(s)",
            validation.failed_rules.len()
        );
    }
    set_workflow_status(
        store,
        workflow_id,
        "completed",
        origin,
        "workflow_completed",
        Some(validation),
    )
}

fn set_workflow_status(
    store: &FoundryStore,
    workflow_id: &str,
    new_status: &str,
    origin: &str,
    event_kind: &str,
    validation: Option<ValidationReport>,
) -> Result<WorkflowStatusUpdateReport> {
    ensure_workflow_policy(store, workflow_id, event_kind)?;
    let mut workflow = store.load_workflow(workflow_id)?;
    let previous_status = workflow.status.clone();
    workflow.status = new_status.to_string();
    let revision = push_revision(
        &mut workflow.revisions,
        origin,
        event_kind,
        &format!("workflow status changed from `{previous_status}` to `{new_status}`"),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        event_kind,
        &serde_json::json!({
            "origin": origin,
            "previous_status": previous_status,
            "new_status": new_status,
            "revision": revision,
            "validation": validation,
        }),
    )?;
    Ok(WorkflowStatusUpdateReport {
        status: event_kind.to_string(),
        workflow_id: workflow_id.to_string(),
        origin: origin.to_string(),
        previous_status,
        new_status: new_status.to_string(),
        revision,
        validation,
    })
}

fn resumed_workflow_status(workflow: &crate::graph::Workflow) -> String {
    if workflow
        .tasks
        .iter()
        .any(|task| matches!(task.status, TaskStatus::Running))
    {
        "running".to_string()
    } else if workflow
        .tasks
        .iter()
        .all(|task| matches!(task.status, TaskStatus::Completed))
    {
        "completed".to_string()
    } else {
        "running".to_string()
    }
}

pub fn update_workflow_task(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskUpdateInput<'_>,
) -> Result<WorkflowTaskUpdateReport> {
    update_workflow_task_with_expected_revision(store, workflow_id, input, None)
}

pub fn update_workflow_task_with_expected_revision(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskUpdateInput<'_>,
    expected_revision: Option<u64>,
) -> Result<WorkflowTaskUpdateReport> {
    store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "workflow task update")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_expected_revision(&workflow, expected_revision)?;
        ensure_structural_mutation_allowed(&workflow, "update task")?;
        ensure_core_orchestration_integrity(&workflow)?;
        ensure_no_task_lease(store, workflow_id, input.task_id)?;
        let task_index = workflow_task_index(&workflow, input.task_id)?;
        ensure_task_definition_mutable(&workflow.tasks[task_index], "update")?;

        let task = &mut workflow.tasks[task_index];
        let previous_title = task.title.clone();
        let previous_goal = task.goal.clone();
        let previous_expected_output = task.expected_output.clone();
        let previous_version = task.version;

        if let Some(title) = input.title.filter(|value| !value.trim().is_empty()) {
            task.title = title.trim().to_string();
        }
        if let Some(goal) = input.goal.filter(|value| !value.trim().is_empty()) {
            task.goal = goal.trim().to_string();
            task.work_item.goal_validation.goal = task.goal.clone();
        }
        if let Some(expected_output) = input
            .expected_output
            .filter(|value| !value.trim().is_empty())
        {
            task.expected_output = expected_output.trim().to_string();
        }

        let changed = previous_title != task.title
            || previous_goal != task.goal
            || previous_expected_output != task.expected_output;
        if changed {
            task.version = task.version.saturating_add(1);
        }
        let new_title = task.title.clone();
        let new_goal = task.goal.clone();
        let new_expected_output = task.expected_output.clone();

        if !changed {
            return Ok(WorkflowTaskUpdateReport {
                status: "workflow_task_unchanged".to_string(),
                workflow_id: workflow_id.to_string(),
                task_id: input.task_id.to_string(),
                origin: input.origin.to_string(),
                previous_title,
                new_title,
                previous_goal,
                new_goal,
                previous_expected_output,
                new_expected_output,
                previous_version,
                new_version: previous_version,
                revision: latest_revision(&workflow),
            });
        }

        propagate_dependency_version_boundary(&mut workflow.tasks);
        ensure_core_orchestration_integrity(&workflow)?;
        let new_version = workflow.tasks[task_index].version;
        let revision = push_revision(
            &mut workflow.revisions,
            input.origin,
            "task_updated",
            &format!(
                "updated task {} from version {} to {}",
                input.task_id, previous_version, new_version
            ),
        );
        let report = WorkflowTaskUpdateReport {
            status: "workflow_task_updated".to_string(),
            workflow_id: workflow_id.to_string(),
            task_id: input.task_id.to_string(),
            origin: input.origin.to_string(),
            previous_title,
            new_title,
            previous_goal,
            new_goal,
            previous_expected_output,
            new_expected_output,
            previous_version,
            new_version,
            revision,
        };
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "workflow_task_updated",
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

pub fn update_workflow_tasks_batch(
    store: &FoundryStore,
    workflow_id: &str,
    updates: &[WorkflowTaskBatchUpdateItem],
    origin: &str,
) -> Result<WorkflowTaskBatchUpdateReport> {
    if updates.is_empty() || updates.len() > 64 {
        bail!("workflow task batch update requires between 1 and 64 tasks");
    }
    let mut unique_task_ids = BTreeSet::new();
    for update in updates {
        if update.task_id.trim().is_empty()
            || update.title.trim().is_empty()
            || update.goal.trim().is_empty()
            || update.expected_output.trim().is_empty()
        {
            bail!("workflow task batch update fields must be non-empty");
        }
        if !unique_task_ids.insert(update.task_id.as_str()) {
            bail!(
                "workflow task batch update contains duplicate task {}",
                update.task_id
            );
        }
    }

    store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "workflow task batch update")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_structural_mutation_allowed(&workflow, "batch update tasks")?;
        ensure_core_orchestration_integrity(&workflow)?;

        let mut updated_task_ids = Vec::new();
        let mut unchanged_task_ids = Vec::new();
        for update in updates {
            ensure_no_task_lease(store, workflow_id, &update.task_id)?;
            let task_index = workflow_task_index(&workflow, &update.task_id)?;
            ensure_task_definition_mutable(&workflow.tasks[task_index], "update")?;
            let task = &mut workflow.tasks[task_index];
            let title = update.title.trim();
            let goal = update.goal.trim();
            let expected_output = update.expected_output.trim();
            let changed =
                task.title != title || task.goal != goal || task.expected_output != expected_output;
            if changed {
                task.title = title.to_string();
                task.goal = goal.to_string();
                task.work_item.goal_validation.goal = task.goal.clone();
                task.expected_output = expected_output.to_string();
                task.version = task.version.saturating_add(1);
                updated_task_ids.push(update.task_id.clone());
            } else {
                unchanged_task_ids.push(update.task_id.clone());
            }
        }

        if updated_task_ids.is_empty() {
            return Ok(WorkflowTaskBatchUpdateReport {
                status: "workflow_tasks_unchanged".to_string(),
                workflow_id: workflow_id.to_string(),
                origin: origin.to_string(),
                updated_task_ids,
                unchanged_task_ids,
                revision: latest_revision(&workflow),
            });
        }

        propagate_dependency_version_boundary(&mut workflow.tasks);
        ensure_core_orchestration_integrity(&workflow)?;
        let revision = push_revision(
            &mut workflow.revisions,
            origin,
            "tasks_batch_updated",
            &format!("updated {} task definitions", updated_task_ids.len()),
        );
        let report = WorkflowTaskBatchUpdateReport {
            status: "workflow_tasks_batch_updated".to_string(),
            workflow_id: workflow_id.to_string(),
            origin: origin.to_string(),
            updated_task_ids,
            unchanged_task_ids,
            revision,
        };
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "workflow_tasks_batch_updated",
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

pub fn add_workflow_task(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskAddInput,
) -> Result<WorkflowMutationReport> {
    let description = required_text(&input.description, "task description")?;
    let priority = normalize_workflow_priority(&input.priority)?;
    let requested_task_id = input
        .task_id
        .as_deref()
        .map(|value| required_task_id(value, "task id"))
        .transpose()?;
    store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "workflow task add")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_expected_revision(&workflow, input.expected_revision)?;
        ensure_structural_mutation_allowed(&workflow, "add task")?;
        ensure_core_orchestration_integrity(&workflow)?;

        let task_id = requested_task_id
            .clone()
            .unwrap_or_else(|| next_dynamic_task_id(&workflow));
        let mut task = build_task(
            &task_id,
            &description,
            &[],
            &["workflow goal", "current task graph", "mutation origin"],
            Vec::new(),
            &format!("Validated outcome for {description}"),
            (ExecutorKind::Ai, 0.01),
        );
        task.goal = format!("{description}: reach a definitively ready state");
        task.work_item.goal_validation.goal = task.goal.clone();
        task.work_item.priority = priority.clone();

        if let Some(existing) = workflow.tasks.iter().find(|task| task.id == task_id) {
            if serde_json::to_value(existing)? == serde_json::to_value(&task)? {
                return Ok(workflow_mutation_report(
                    "workflow_task_unchanged",
                    "add_task",
                    workflow_id,
                    &task_id,
                    &input.origin,
                    false,
                    latest_revision(&workflow),
                    Some(existing.version),
                    Some(existing.version),
                    Some(priority.clone()),
                    None,
                    None,
                    Vec::new(),
                    vec![task_id.clone()],
                ));
            }
            bail!("task id {task_id} already exists with a different definition");
        }

        workflow.tasks.push(task);
        ensure_core_orchestration_integrity(&workflow)?;
        let revision = push_revision(
            &mut workflow.revisions,
            &input.origin,
            "task_added",
            &format!("added task {task_id} with {priority} priority"),
        );
        let report = workflow_mutation_report(
            "workflow_task_added",
            "add_task",
            workflow_id,
            &task_id,
            &input.origin,
            true,
            revision,
            None,
            Some(1),
            Some(priority),
            None,
            None,
            Vec::new(),
            vec![task_id.clone()],
        );
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "workflow_task_added",
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

pub fn set_workflow_task_priority(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskPriorityInput,
) -> Result<WorkflowMutationReport> {
    let priority = normalize_workflow_priority(&input.priority)?;
    store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "workflow task priority update")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_expected_revision(&workflow, input.expected_revision)?;
        ensure_structural_mutation_allowed(&workflow, "reprioritize task")?;
        ensure_core_orchestration_integrity(&workflow)?;
        let task_index = workflow_task_index(&workflow, &input.task_id)?;
        ensure_task_runtime_mutable(&workflow.tasks[task_index], "reprioritize")?;
        ensure_no_task_lease(store, workflow_id, &input.task_id)?;
        let previous_version = workflow.tasks[task_index].version;
        if workflow.tasks[task_index].work_item.priority == priority {
            return Ok(workflow_mutation_report(
                "workflow_task_priority_unchanged",
                "set_priority",
                workflow_id,
                &input.task_id,
                &input.origin,
                false,
                latest_revision(&workflow),
                Some(previous_version),
                Some(previous_version),
                Some(priority),
                None,
                None,
                Vec::new(),
                vec![input.task_id.clone()],
            ));
        }
        workflow.tasks[task_index].work_item.priority = priority.clone();
        workflow.tasks[task_index].version = previous_version.saturating_add(1);
        let mut affected_task_ids = propagate_dependency_version_boundary(&mut workflow.tasks);
        affected_task_ids.push(input.task_id.clone());
        normalize_ids(&mut affected_task_ids);
        ensure_core_orchestration_integrity(&workflow)?;
        let new_version = workflow.tasks[task_index].version;
        let revision = push_revision(
            &mut workflow.revisions,
            &input.origin,
            "task_priority_updated",
            &format!("set task {} priority to {priority}", input.task_id),
        );
        let report = workflow_mutation_report(
            "workflow_task_priority_updated",
            "set_priority",
            workflow_id,
            &input.task_id,
            &input.origin,
            true,
            revision,
            Some(previous_version),
            Some(new_version),
            Some(priority),
            None,
            None,
            Vec::new(),
            affected_task_ids,
        );
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "workflow_task_priority_updated",
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

pub fn add_workflow_task_dependency(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskDependencyInput,
) -> Result<WorkflowMutationReport> {
    mutate_workflow_task_dependency(store, workflow_id, input, true)
}

pub fn remove_workflow_task_dependency(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskDependencyInput,
) -> Result<WorkflowMutationReport> {
    mutate_workflow_task_dependency(store, workflow_id, input, false)
}

fn mutate_workflow_task_dependency(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskDependencyInput,
    add: bool,
) -> Result<WorkflowMutationReport> {
    store.with_transaction(|| {
        let action = if add {
            "workflow task dependency add"
        } else {
            "workflow task dependency remove"
        };
        ensure_workflow_policy(store, workflow_id, action)?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_expected_revision(&workflow, input.expected_revision)?;
        ensure_structural_mutation_allowed(
            &workflow,
            if add {
                "add task dependency"
            } else {
                "remove task dependency"
            },
        )?;
        ensure_core_orchestration_integrity(&workflow)?;
        ensure_no_task_lease(store, workflow_id, &input.task_id)?;
        let task_index = workflow_task_index(&workflow, &input.task_id)?;
        let _dependency_index = workflow_task_index(&workflow, &input.dependency_task_id)?;
        if input.task_id == input.dependency_task_id {
            bail!("task {} cannot depend on itself", input.task_id);
        }
        ensure_task_definition_mutable(
            &workflow.tasks[task_index],
            if add {
                "add a dependency to"
            } else {
                "remove a dependency from"
            },
        )?;
        let previous_version = workflow.tasks[task_index].version;
        let contains = workflow.tasks[task_index]
            .dependencies
            .contains(&input.dependency_task_id);
        if contains == add {
            return Ok(workflow_mutation_report(
                if add {
                    "workflow_task_dependency_unchanged"
                } else {
                    "workflow_task_dependency_absent"
                },
                if add {
                    "add_dependency"
                } else {
                    "remove_dependency"
                },
                workflow_id,
                &input.task_id,
                &input.origin,
                false,
                latest_revision(&workflow),
                Some(previous_version),
                Some(previous_version),
                None,
                Some(input.dependency_task_id.clone()),
                None,
                Vec::new(),
                vec![input.task_id.clone()],
            ));
        }
        if add {
            workflow.tasks[task_index]
                .dependencies
                .push(input.dependency_task_id.clone());
        } else {
            workflow.tasks[task_index]
                .dependencies
                .retain(|dependency| dependency != &input.dependency_task_id);
        }
        workflow.tasks[task_index].version = previous_version.saturating_add(1);
        let mut affected_task_ids = propagate_dependency_version_boundary(&mut workflow.tasks);
        affected_task_ids.push(input.task_id.clone());
        normalize_ids(&mut affected_task_ids);
        ensure_core_orchestration_integrity(&workflow)?;
        let new_version = workflow.tasks[task_index].version;
        let (status, mutation, change_type, event_kind, summary) = if add {
            (
                "workflow_task_dependency_added",
                "add_dependency",
                "task_dependency_added",
                "workflow_task_dependency_added",
                format!(
                    "task {} now depends on {}",
                    input.task_id, input.dependency_task_id
                ),
            )
        } else {
            (
                "workflow_task_dependency_removed",
                "remove_dependency",
                "task_dependency_removed",
                "workflow_task_dependency_removed",
                format!(
                    "removed dependency {} from task {}",
                    input.dependency_task_id, input.task_id
                ),
            )
        };
        let revision = push_revision(
            &mut workflow.revisions,
            &input.origin,
            change_type,
            &summary,
        );
        let report = workflow_mutation_report(
            status,
            mutation,
            workflow_id,
            &input.task_id,
            &input.origin,
            true,
            revision,
            Some(previous_version),
            Some(new_version),
            None,
            Some(input.dependency_task_id),
            None,
            Vec::new(),
            affected_task_ids,
        );
        store.save_workflow(&workflow)?;
        store.record_event(workflow_id, event_kind, &serde_json::to_value(&report)?)?;
        Ok(report)
    })
}

pub fn set_workflow_task_impediment(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskImpedimentInput,
) -> Result<WorkflowMutationReport> {
    let reason = required_text(&input.reason, "impediment reason")?;
    let kind = normalize_workflow_impediment_kind(&input.kind)?;
    store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "workflow task impediment set")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_expected_revision(&workflow, input.expected_revision)?;
        ensure_structural_mutation_allowed(&workflow, "set task impediment")?;
        ensure_core_orchestration_integrity(&workflow)?;
        ensure_no_task_lease(store, workflow_id, &input.task_id)?;
        let task_index = workflow_task_index(&workflow, &input.task_id)?;
        ensure_task_runtime_mutable(&workflow.tasks[task_index], "block")?;
        let previous_version = workflow.tasks[task_index].version;
        if let Some(existing) = workflow.tasks[task_index]
            .active_impediments
            .iter()
            .find(|impediment| {
                impediment.reason == reason
                    && impediment.kind == kind
                    && impediment.origin == input.origin
            })
            .cloned()
        {
            return Ok(workflow_mutation_report(
                "workflow_task_impediment_unchanged",
                "set_impediment",
                workflow_id,
                &input.task_id,
                &input.origin,
                false,
                latest_revision(&workflow),
                Some(previous_version),
                Some(previous_version),
                None,
                None,
                Some(existing),
                Vec::new(),
                vec![input.task_id.clone()],
            ));
        }
        if workflow.tasks[task_index].status == TaskStatus::Blocked
            && workflow.tasks[task_index].work_item.backlog_state != "blocked_by_active_impediment"
        {
            bail!(
                "task {} is blocked by another authority and cannot be overwritten",
                input.task_id
            );
        }
        let impediment = TaskImpediment {
            id: format!("imp_{}", Uuid::new_v4().to_string().replace('-', "")),
            kind,
            reason,
            origin: input.origin.clone(),
            created_at: Utc::now(),
        };
        workflow.tasks[task_index]
            .active_impediments
            .push(impediment.clone());
        workflow.tasks[task_index].status = TaskStatus::Blocked;
        workflow.tasks[task_index].work_item.backlog_state =
            "blocked_by_active_impediment".to_string();
        workflow.tasks[task_index].version = previous_version.saturating_add(1);
        let mut affected_task_ids = propagate_dependency_version_boundary(&mut workflow.tasks);
        affected_task_ids.push(input.task_id.clone());
        normalize_ids(&mut affected_task_ids);
        ensure_core_orchestration_integrity(&workflow)?;
        let new_version = workflow.tasks[task_index].version;
        let revision = push_revision(
            &mut workflow.revisions,
            &input.origin,
            "task_impediment_set",
            &format!("set impediment {} on task {}", impediment.id, input.task_id),
        );
        let report = workflow_mutation_report(
            "workflow_task_impediment_set",
            "set_impediment",
            workflow_id,
            &input.task_id,
            &input.origin,
            true,
            revision,
            Some(previous_version),
            Some(new_version),
            None,
            None,
            Some(impediment),
            Vec::new(),
            affected_task_ids,
        );
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "workflow_task_impediment_set",
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

pub fn clear_workflow_task_impediment(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowTaskImpedimentClearInput,
) -> Result<WorkflowMutationReport> {
    store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "workflow task impediment clear")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        ensure_not_mission_bound(store, &workflow)?;
        ensure_expected_revision(&workflow, input.expected_revision)?;
        ensure_structural_mutation_allowed(&workflow, "clear task impediment")?;
        ensure_core_orchestration_integrity(&workflow)?;
        ensure_no_task_lease(store, workflow_id, &input.task_id)?;
        let task_index = workflow_task_index(&workflow, &input.task_id)?;
        ensure_task_runtime_mutable(&workflow.tasks[task_index], "unblock")?;
        let previous_version = workflow.tasks[task_index].version;
        let dependency_ready =
            workflow.tasks[task_index]
                .dependencies
                .iter()
                .all(|dependency_id| {
                    workflow
                        .tasks
                        .iter()
                        .find(|task| task.id == *dependency_id)
                        .is_some_and(|task| task.status == TaskStatus::Completed)
                });
        let previous_ids = workflow.tasks[task_index]
            .active_impediments
            .iter()
            .map(|impediment| impediment.id.clone())
            .collect::<BTreeSet<_>>();
        if let Some(impediment_id) = input.impediment_id.as_deref() {
            workflow.tasks[task_index]
                .active_impediments
                .retain(|impediment| impediment.id != impediment_id);
        } else {
            workflow.tasks[task_index]
                .active_impediments
                .retain(|impediment| impediment.kind != "manual");
        }
        let remaining_ids = workflow.tasks[task_index]
            .active_impediments
            .iter()
            .map(|impediment| impediment.id.clone())
            .collect::<BTreeSet<_>>();
        let cleared_impediment_ids = previous_ids
            .difference(&remaining_ids)
            .cloned()
            .collect::<Vec<_>>();
        if cleared_impediment_ids.is_empty() {
            return Ok(workflow_mutation_report(
                "workflow_task_impediment_unchanged",
                "clear_impediment",
                workflow_id,
                &input.task_id,
                &input.origin,
                false,
                latest_revision(&workflow),
                Some(previous_version),
                Some(previous_version),
                None,
                None,
                None,
                Vec::new(),
                vec![input.task_id.clone()],
            ));
        }
        if workflow.tasks[task_index].active_impediments.is_empty()
            && workflow.tasks[task_index].status == TaskStatus::Blocked
            && workflow.tasks[task_index].work_item.backlog_state == "blocked_by_active_impediment"
        {
            workflow.tasks[task_index].status = TaskStatus::Pending;
            workflow.tasks[task_index].work_item.backlog_state = if dependency_ready {
                "ready".to_string()
            } else {
                "waiting_on_dependencies".to_string()
            };
        }
        workflow.tasks[task_index].version = previous_version.saturating_add(1);
        let mut affected_task_ids = propagate_dependency_version_boundary(&mut workflow.tasks);
        affected_task_ids.push(input.task_id.clone());
        normalize_ids(&mut affected_task_ids);
        ensure_core_orchestration_integrity(&workflow)?;
        let new_version = workflow.tasks[task_index].version;
        let revision = push_revision(
            &mut workflow.revisions,
            &input.origin,
            "task_impediment_cleared",
            &format!(
                "cleared {} impediment(s) from task {}",
                cleared_impediment_ids.len(),
                input.task_id
            ),
        );
        let report = workflow_mutation_report(
            "workflow_task_impediment_cleared",
            "clear_impediment",
            workflow_id,
            &input.task_id,
            &input.origin,
            true,
            revision,
            Some(previous_version),
            Some(new_version),
            None,
            None,
            None,
            cleared_impediment_ids,
            affected_task_ids,
        );
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "workflow_task_impediment_cleared",
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

fn ensure_core_orchestration_integrity(workflow: &Workflow) -> Result<()> {
    let violations = validate_workflow_structure(workflow);
    if violations.is_empty() {
        return Ok(());
    }
    bail!(
        "workflow {} failed Core orchestration integrity: {}",
        workflow.id,
        violations
            .iter()
            .map(|violation| format!(
                "{}:{}:{}",
                violation.kind, violation.task_id, violation.message
            ))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn ensure_structural_mutation_allowed(workflow: &Workflow, action: &str) -> Result<()> {
    match workflow.status.trim().to_ascii_lowercase().as_str() {
        "completed" | "complete" | "cancelled" | "failed" => bail!(
            "cannot {action} on terminal workflow {} while status is {}; create a new workflow before changing its task graph",
            workflow.id,
            workflow.status
        ),
        _ => Ok(()),
    }
}

fn ensure_not_mission_bound(store: &FoundryStore, workflow: &Workflow) -> Result<()> {
    if store.workflow_is_mission_bound(&workflow.id)? {
        bail!(
            "workflow {} is mission-bound; generic workflow mutations require a mission-aware adapter",
            workflow.id
        );
    }
    Ok(())
}

fn ensure_expected_revision(workflow: &Workflow, expected_revision: Option<u64>) -> Result<()> {
    let Some(expected_revision) = expected_revision else {
        return Ok(());
    };
    let current_revision = latest_revision(workflow);
    if current_revision != expected_revision {
        bail!(
            "stale workflow revision for {}: expected {}, current {}; reload workflow state and retry",
            workflow.id,
            expected_revision,
            current_revision
        );
    }
    Ok(())
}

fn ensure_no_task_lease(store: &FoundryStore, workflow_id: &str, task_id: &str) -> Result<()> {
    if store.load_task_lease(workflow_id, task_id)?.is_some() {
        bail!(
            "task {task_id} in workflow {workflow_id} has an active execution lease; release or cancel the handoff before mutation"
        );
    }
    Ok(())
}

fn ensure_task_definition_mutable(task: &crate::graph::AtomicTask, action: &str) -> Result<()> {
    match task.status {
        TaskStatus::Pending | TaskStatus::Blocked => Ok(()),
        _ => bail!(
            "cannot {action} task {} while status is {}",
            task.id,
            format!("{:?}", task.status).to_ascii_lowercase()
        ),
    }
}

fn ensure_task_runtime_mutable(task: &crate::graph::AtomicTask, action: &str) -> Result<()> {
    match task.status {
        TaskStatus::Pending | TaskStatus::Blocked => Ok(()),
        _ => bail!(
            "cannot {action} task {} while status is {}",
            task.id,
            format!("{:?}", task.status).to_ascii_lowercase()
        ),
    }
}

fn workflow_task_index(workflow: &Workflow, task_id: &str) -> Result<usize> {
    workflow
        .tasks
        .iter()
        .position(|task| task.id == task_id)
        .with_context(|| format!("task {task_id} not found in workflow {}", workflow.id))
}

fn required_text(value: &str, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{field} cannot be empty");
    }
    Ok(value.to_string())
}

fn required_task_id(value: &str, field: &str) -> Result<String> {
    let value = required_text(value, field)?;
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        bail!("{field} must contain only ASCII letters, digits, '-' or '_'");
    }
    Ok(value)
}

fn normalize_workflow_priority(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" | "p0" | "p1" => Ok("high".to_string()),
        "medium" | "p2" => Ok("medium".to_string()),
        "low" | "p3" => Ok("low".to_string()),
        other => {
            bail!("unsupported workflow task priority `{other}`; expected high, medium or low")
        }
    }
}

fn normalize_workflow_impediment_kind(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "manual" => Ok("manual".to_string()),
        "resource" => Ok("resource".to_string()),
        "authorization" => Ok("authorization".to_string()),
        "policy" => Ok("policy".to_string()),
        other => bail!(
            "unsupported workflow task impediment kind `{other}`; expected manual, resource, authorization or policy"
        ),
    }
}

fn next_dynamic_task_id(workflow: &Workflow) -> String {
    let existing = workflow
        .tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut sequence = workflow.tasks.len().saturating_add(1);
    loop {
        let candidate = format!("task-{sequence:03}");
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
        sequence = sequence.saturating_add(1);
    }
}

fn propagate_dependency_version_boundary(tasks: &mut [crate::graph::AtomicTask]) -> Vec<String> {
    let mut propagated = BTreeSet::new();
    loop {
        let versions = tasks
            .iter()
            .map(|task| (task.id.clone(), task.version))
            .collect::<BTreeMap<_, _>>();
        let mut changed = false;
        for task in tasks.iter_mut() {
            let minimum_dependency_version = task
                .dependencies
                .iter()
                .filter_map(|dependency| versions.get(dependency))
                .copied()
                .max()
                .unwrap_or(task.version);
            if task.version < minimum_dependency_version {
                task.version = minimum_dependency_version;
                propagated.insert(task.id.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    propagated.into_iter().collect()
}

fn normalize_ids(ids: &mut Vec<String>) {
    ids.sort();
    ids.dedup();
}

fn latest_revision(workflow: &Workflow) -> u64 {
    workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn workflow_mutation_report(
    status: &str,
    mutation: &str,
    workflow_id: &str,
    task_id: &str,
    origin: &str,
    changed: bool,
    revision: u64,
    previous_task_version: Option<u64>,
    new_task_version: Option<u64>,
    priority: Option<String>,
    dependency_task_id: Option<String>,
    impediment: Option<TaskImpediment>,
    cleared_impediment_ids: Vec<String>,
    affected_task_ids: Vec<String>,
) -> WorkflowMutationReport {
    WorkflowMutationReport {
        schema_version: WORKFLOW_MUTATION_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        mutation: mutation.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: origin.to_string(),
        changed,
        revision,
        previous_task_version,
        new_task_version,
        priority,
        dependency_task_id,
        impediment,
        cleared_impediment_ids,
        affected_task_ids,
    }
}

pub fn parse_node_brain_agent_slot(value: &str) -> Result<NodeBrainAgentSlotSpec> {
    let (slot_id, rest) = value.split_once('=').with_context(|| {
        format!("invalid agent slot `{value}`; expected slot_id=brain_id:role:parallel_group")
    })?;
    let parts = rest.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!("invalid agent slot `{value}`; expected slot_id=brain_id:role:parallel_group");
    }
    let slot_id = slot_id.trim();
    let role = parts[1].trim();
    let parallel_group = parts[2].trim();
    if slot_id.is_empty() || role.is_empty() || parallel_group.is_empty() {
        bail!("invalid agent slot `{value}`; slot id, role and parallel group are required");
    }

    Ok(NodeBrainAgentSlotSpec {
        slot_id: slot_id.to_string(),
        brain_id: empty_to_none(parts[0]),
        role: role.to_string(),
        parallel_group: parallel_group.to_string(),
        state_owner: "foundry".to_string(),
    })
}

pub fn update_workflow_node_brain_routing(
    store: &FoundryStore,
    workflow_id: &str,
    input: WorkflowNodeBrainRoutingUpdateInput,
) -> Result<WorkflowNodeBrainRoutingUpdateReport> {
    ensure_workflow_policy(store, workflow_id, "workflow node brain update")?;
    let mut workflow = store.load_workflow(workflow_id)?;
    let task_index = workflow
        .tasks
        .iter()
        .position(|task| task.id == input.task_id)
        .with_context(|| {
            format!(
                "task {} not found in workflow {}",
                input.task_id, workflow_id
            )
        })?;

    let (previous_version, new_version, previous_routing, new_routing, orchestrator_brain) = {
        let task = &mut workflow.tasks[task_index];
        if !matches!(task.executor, ExecutorKind::Ai | ExecutorKind::Mixed) {
            bail!(
                "task {} uses executor {:?}; node brain routing is only mutable for AI or mixed tasks",
                input.task_id,
                task.executor
            );
        }
        if input.default_brain.is_none()
            && input.allowed_brains.is_empty()
            && input.agent_slots.is_empty()
            && input.max_parallel_agents.is_none()
        {
            bail!("no node brain routing changes were provided");
        }

        let previous_version = task.version;
        let previous_routing = task.node_brain_routing.clone();
        let mut routing = if task.node_brain_routing.scope == "agentic_ai_node" {
            task.node_brain_routing.clone()
        } else {
            node_brain_routing_for_executor(&task.executor)
        };

        routing.scope = "agentic_ai_node".to_string();
        routing.orchestrator_brain = "foundry".to_string();
        routing.selection_owner = "foundry".to_string();
        routing.supports_parallel_agent_brains = true;
        routing.supports_multiple_agents_per_brain = true;
        routing.hot_swappable = true;
        routing.state_owner = "foundry_workflow_state".to_string();
        routing.memory_source = "foundry_memory_router".to_string();
        routing.skills_source = "foundry_skill_router".to_string();
        routing.mcp_source = "foundry_mcp_router".to_string();
        routing.switch_command = vec![
            "foundry".to_string(),
            "request".to_string(),
            "switch-executor".to_string(),
            "--run".to_string(),
            "<run-id>".to_string(),
            "--executor".to_string(),
            "<brain-id>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ];
        routing.workflow_mutation_command = vec![
            "foundry".to_string(),
            "workflow".to_string(),
            "update-node-brain".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--default-brain".to_string(),
            "<brain-id>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ];

        let allowed_brains = clean_unique(input.allowed_brains);
        if !allowed_brains.is_empty() {
            routing.allowed_brains = allowed_brains;
        }
        if routing.allowed_brains.is_empty() {
            routing.allowed_brains = node_brain_routing_for_executor(&task.executor).allowed_brains;
        }

        if let Some(default_brain) = clean_optional_owned(input.default_brain) {
            ensure_unique_value(&mut routing.allowed_brains, &default_brain);
            routing.default_brain = Some(default_brain);
        }

        if !input.agent_slots.is_empty() {
            let mut seen_slots = Vec::new();
            for slot in &input.agent_slots {
                if seen_slots.contains(&slot.slot_id) {
                    bail!("duplicate node brain agent slot id: {}", slot.slot_id);
                }
                seen_slots.push(slot.slot_id.clone());
                if let Some(brain_id) = slot.brain_id.as_deref().and_then(empty_to_none) {
                    ensure_unique_value(&mut routing.allowed_brains, &brain_id);
                }
            }
            routing.agent_slots = input.agent_slots;
        }

        let requested_max = input.max_parallel_agents.unwrap_or_else(|| {
            routing
                .max_parallel_agents
                .max(routing.agent_slots.len())
                .max(1)
        });
        if requested_max == 0 && !routing.agent_slots.is_empty() {
            bail!("max parallel agents must be greater than zero when agent slots are configured");
        }
        if requested_max < routing.agent_slots.len() {
            bail!(
                "max parallel agents {} is lower than configured agent slots {}",
                requested_max,
                routing.agent_slots.len()
            );
        }
        routing.max_parallel_agents = requested_max;

        task.node_brain_routing = routing;
        task.version = task.version.saturating_add(1);
        let new_version = task.version;
        let new_routing = task.node_brain_routing.clone();
        let orchestrator_brain = new_routing.orchestrator_brain.clone();
        (
            previous_version,
            new_version,
            previous_routing,
            new_routing,
            orchestrator_brain,
        )
    };

    let revision = push_revision(
        &mut workflow.revisions,
        &input.origin,
        "node_brain_routing_updated",
        &format!(
            "updated node brain routing for task {} from version {} to {}",
            input.task_id, previous_version, new_version
        ),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "workflow_node_brain_routing_updated",
        &serde_json::json!({
            "origin": input.origin,
            "task_id": input.task_id,
            "previous_version": previous_version,
            "new_version": new_version,
            "previous_routing": previous_routing,
            "new_routing": new_routing,
            "revision": revision,
            "can_switch_without_stopping_workflow": true
        }),
    )?;

    Ok(WorkflowNodeBrainRoutingUpdateReport {
        status: "workflow_node_brain_routing_updated".to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: input.task_id,
        origin: input.origin,
        orchestrator_brain,
        can_switch_without_stopping_workflow: true,
        previous_version,
        new_version,
        previous_routing,
        new_routing,
        revision,
    })
}

pub fn attach_workflow_artifact(
    store: &FoundryStore,
    workflow_id: &str,
    source_path: &Path,
    kind: &str,
    origin: &str,
) -> Result<ArtifactAttachReport> {
    attach_workflow_artifact_with_tags(store, workflow_id, source_path, kind, origin, &[])
}

pub fn attach_workflow_artifact_with_tags(
    store: &FoundryStore,
    workflow_id: &str,
    source_path: &Path,
    kind: &str,
    origin: &str,
    tags: &[String],
) -> Result<ArtifactAttachReport> {
    ensure_workflow_policy(store, workflow_id, "workflow artifact attach")?;
    let (relative_path, sha256, bytes) =
        copy_artifact(&store.base_dir(), workflow_id, source_path, kind)?;
    let prepared = prepare_workflow_artifact_attach(
        kind,
        &relative_path,
        &sha256,
        bytes,
        origin,
        tags,
        source_path.display().to_string(),
    );
    record_prepared_workflow_artifact(store, workflow_id, &prepared)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_workflow_artifact_attach(
    kind: &str,
    relative_path: &str,
    sha256: &str,
    bytes: u64,
    origin: &str,
    explicit_tags: &[String],
    source_description: String,
) -> PreparedArtifactAttach {
    let tags = normalize_artifact_tags(kind, relative_path, origin, explicit_tags);
    PreparedArtifactAttach {
        artifact: ArtifactRecord {
            id: format!("artifact_{}", Uuid::new_v4().to_string().replace('-', "")),
            kind: kind.to_string(),
            path: relative_path.to_string(),
            sha256: sha256.to_string(),
            tags,
            created_at: Utc::now(),
            lineage: None,
        },
        bytes,
        origin: origin.to_string(),
        source_description,
    }
}

pub(crate) fn record_prepared_workflow_artifact(
    store: &FoundryStore,
    workflow_id: &str,
    prepared: &PreparedArtifactAttach,
) -> Result<ArtifactAttachReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    let artifact = prepared.artifact.clone();
    workflow.artifacts.push(artifact.clone());
    let revision = push_revision(
        &mut workflow.revisions,
        &prepared.origin,
        "artifact_attached",
        &format!(
            "attached artifact {} as {}",
            prepared.source_description, artifact.kind
        ),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "artifact_attached",
        &serde_json::json!({
            "origin": &prepared.origin,
            "path": &artifact.path,
            "sha256": &artifact.sha256,
            "tags": &artifact.tags,
            "revision": revision
        }),
    )?;

    Ok(ArtifactAttachReport {
        status: "artifact_attached".to_string(),
        workflow_id: workflow_id.to_string(),
        origin: prepared.origin.clone(),
        revision,
        artifact: AttachedArtifact {
            id: artifact.id,
            kind: artifact.kind,
            path: artifact.path,
            sha256: artifact.sha256,
            bytes: prepared.bytes,
            tags: artifact.tags,
        },
    })
}

fn normalize_artifact_tags(
    kind: &str,
    path: &str,
    origin: &str,
    explicit_tags: &[String],
) -> Vec<String> {
    let mut tags = BTreeSet::new();
    tags.insert("artifact".to_string());
    collect_tag_parts(&mut tags, kind);
    collect_tag_parts(&mut tags, path);
    collect_tag_parts(&mut tags, origin);
    for tag in explicit_tags {
        collect_tag_parts(&mut tags, tag);
    }
    tags.into_iter().collect()
}

fn collect_tag_parts(tags: &mut BTreeSet<String>, value: &str) {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return;
    }
    tags.insert(normalized.clone());
    for part in normalized.split(|ch: char| !ch.is_ascii_alphanumeric()) {
        if part.len() >= 2 {
            tags.insert(part.to_string());
        }
    }
}

pub fn validate_child_subflow_binding(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    child_workflow_id: &str,
    child_task_id: &str,
    origin: &str,
) -> Result<SubflowValidationReport> {
    ensure_workflow_policy(store, workflow_id, "validate child subflow binding")?;
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

fn empty_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_optional_owned(value: Option<String>) -> Option<String> {
    value.and_then(|item| empty_to_none(&item))
}

fn clean_unique(values: Vec<String>) -> Vec<String> {
    let mut cleaned = Vec::new();
    for value in values {
        if let Some(value) = empty_to_none(&value) {
            ensure_unique_value(&mut cleaned, &value);
        }
    }
    cleaned
}

fn ensure_unique_value(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|item| item == value) {
        values.push(value.to_string());
    }
}

fn diff_added(previous: &[String], next: &[String]) -> Vec<String> {
    next.iter()
        .filter(|value| !contains_case_insensitive(previous, value))
        .cloned()
        .collect()
}

fn diff_removed(previous: &[String], next: &[String]) -> Vec<String> {
    previous
        .iter()
        .filter(|value| !contains_case_insensitive(next, value))
        .cloned()
        .collect()
}

fn contains_case_insensitive(values: &[String], needle: &str) -> bool {
    values
        .iter()
        .any(|value| value.eq_ignore_ascii_case(needle))
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
pub struct CreativeCollaborationEventReport {
    pub status: String,
    pub workflow_id: String,
    pub artifact_id: String,
    pub origin: String,
    pub revision: u64,
    pub event_id: String,
    pub event_kind: String,
    pub summary: CreativeCollaborationSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreativeCollaborationStatusReport {
    pub status: String,
    pub workflow_id: String,
    pub artifact_id: String,
    pub summary: CreativeCollaborationSummary,
    pub collaboration: CreativeCollaborationState,
}

#[derive(Debug, Clone)]
pub struct CreativeCollaborationEventRequest {
    pub workflow_id: String,
    pub artifact_id: String,
    pub event_kind: String,
    pub actor: String,
    pub summary: String,
    pub target: String,
    pub selections: Vec<String>,
    pub origin: String,
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
    store: &FoundryStore,
    workflow_id: &str,
    artifact: CreativeArtifact,
    origin: &str,
) -> Result<CreativeArtifactAttachReport> {
    ensure_workflow_policy(store, workflow_id, "creative artifact attach")?;
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
    store: &FoundryStore,
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
    store: &FoundryStore,
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

pub fn record_creative_collaboration_event(
    store: &FoundryStore,
    request: CreativeCollaborationEventRequest,
) -> Result<CreativeCollaborationEventReport> {
    let workflow_id = request.workflow_id;
    ensure_workflow_policy(store, &workflow_id, "creative collaboration event")?;
    let artifact_id = request.artifact_id;
    let event_kind = request.event_kind;
    let actor = request.actor;
    let summary = request.summary;
    let target = request.target;
    let selections = request.selections;
    let origin = request.origin;
    let mut workflow = store.load_workflow(&workflow_id)?;
    let now = Utc::now();
    let normalized_kind = event_kind.trim().to_ascii_lowercase();
    let event_id = format!("collab_{}", Uuid::new_v4().to_string().replace('-', ""));

    let collaboration_summary = {
        let artifact = workflow
            .creative_artifacts
            .iter_mut()
            .find(|artifact| artifact.id == artifact_id.as_str())
            .with_context(|| format!("creative artifact not found: {artifact_id}"))?;

        match normalized_kind.as_str() {
            "presence" => {
                artifact.collaboration.presences.push(CollaborationPresence {
                    event_id: event_id.clone(),
                    actor: actor.clone(),
                    cursor: empty_to_none(&target),
                    selections,
                    status: "active".to_string(),
                    origin: origin.clone(),
                    updated_at: now,
                });
            }
            "comment" => {
                artifact.collaboration.comments.push(CollaborationComment {
                    event_id: event_id.clone(),
                    actor: actor.clone(),
                    target: target.clone(),
                    body: summary.clone(),
                    status: "open".to_string(),
                    origin: origin.clone(),
                    created_at: now,
                });
            }
            "patch" => {
                artifact
                    .collaboration
                    .patch_stream
                    .push(CollaborationPatchEvent {
                        event_id: event_id.clone(),
                        actor: actor.clone(),
                        target: target.clone(),
                        instruction: summary.clone(),
                        status: "applied".to_string(),
                        origin: origin.clone(),
                        created_at: now,
                    });
                artifact.patches.push(PatchRecord {
                    patch_id: event_id.clone(),
                    instruction: summary.clone(),
                    applied_at: now,
                    change_count: 0,
                });
            }
            "conflict" => {
                artifact
                    .collaboration
                    .conflicts
                    .push(CollaborationConflictEvent {
                        event_id: event_id.clone(),
                        actor: actor.clone(),
                        target: target.clone(),
                        summary: summary.clone(),
                        resolution_status: "unresolved".to_string(),
                        origin: origin.clone(),
                        created_at: now,
                    });
            }
            "rollback" => {
                artifact
                    .collaboration
                    .rollbacks
                    .push(CollaborationRollbackEvent {
                        event_id: event_id.clone(),
                        actor: actor.clone(),
                        target_event_id: target.clone(),
                        reason: summary.clone(),
                        origin: origin.clone(),
                        created_at: now,
                    });
            }
            other => bail!(
                "unknown collaboration event kind: {other}; expected one of: presence, comment, patch, conflict, rollback"
            ),
        }

        artifact
            .collaboration
            .audit_history
            .push(CollaborationAuditEvent {
                event_id: event_id.clone(),
                kind: normalized_kind.clone(),
                actor: actor.clone(),
                summary: summary.clone(),
                origin: origin.clone(),
                occurred_at: now,
            });
        artifact.collaboration.summary()
    };

    let revision = push_revision(
        &mut workflow.revisions,
        &origin,
        "creative_collaboration_event",
        &format!("recorded {normalized_kind} collaboration event {event_id} on {artifact_id}"),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        &workflow_id,
        "creative_collaboration_event_recorded",
        &serde_json::json!({
            "origin": origin.clone(),
            "artifact_id": artifact_id.clone(),
            "event_id": event_id.clone(),
            "event_kind": normalized_kind.clone(),
            "revision": revision
        }),
    )?;

    Ok(CreativeCollaborationEventReport {
        status: "creative_collaboration_event_recorded".to_string(),
        workflow_id,
        artifact_id,
        origin,
        revision,
        event_id,
        event_kind: normalized_kind,
        summary: collaboration_summary,
    })
}

pub fn inspect_creative_collaboration(
    store: &FoundryStore,
    workflow_id: &str,
    artifact_id: &str,
) -> Result<CreativeCollaborationStatusReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let artifact = workflow
        .creative_artifacts
        .iter()
        .find(|artifact| artifact.id == artifact_id)
        .with_context(|| format!("creative artifact not found: {artifact_id}"))?;

    Ok(CreativeCollaborationStatusReport {
        status: "creative_collaboration_status_loaded".to_string(),
        workflow_id: workflow_id.to_string(),
        artifact_id: artifact_id.to_string(),
        summary: artifact.collaboration.summary(),
        collaboration: artifact.collaboration.clone(),
    })
}

pub fn set_workflow_token_collection(
    store: &FoundryStore,
    workflow_id: &str,
    token_collection: TokenCollection,
    origin: &str,
) -> Result<TokenCollectionReport> {
    ensure_workflow_policy(store, workflow_id, "workflow token collection set")?;
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
    store: &FoundryStore,
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
    store: &FoundryStore,
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
    store: &FoundryStore,
    workflow_id: &str,
    token_name: &str,
    value: &str,
    origin: &str,
) -> Result<TokenPatchReport> {
    ensure_workflow_policy(store, workflow_id, "workflow token patch")?;
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
