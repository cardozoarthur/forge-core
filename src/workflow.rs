use crate::addon::{default_addon_dirs, load_addon_catalog_from_store};
use crate::artifact::copy_artifact;
use crate::graph::{
    node_brain_routing_for_executor, ArtifactRecord, ExecutorKind, NodeBrainAgentSlotSpec,
    NodeBrainRoutingSpec, TaskStatus, Workflow, WorkflowRevision,
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
use crate::storage::ForgeStore;
use crate::validation::{validate_workflow, ValidationReport};
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
    store: &ForgeStore,
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
    store: &ForgeStore,
    workflow_id: &str,
    goal: &str,
    origin: &str,
) -> Result<WorkflowGoalUpdateReport> {
    ensure_workflow_policy(store, workflow_id, "workflow goal update")?;
    let mut workflow = store.load_workflow(workflow_id)?;
    let previous_goal = workflow.goal.clone();
    let previous_intent = workflow.intent.clone();
    let previous_deliverables = previous_intent.deliverables.clone();
    let previous_capabilities = previous_intent
        .required_capabilities
        .iter()
        .map(|capability| capability.id.clone())
        .collect::<Vec<_>>();
    let addon_catalog = load_addon_catalog_from_store(store, &default_addon_dirs())?;
    let new_intent = parse_intent_with_catalog_and_context(
        goal,
        &addon_catalog,
        previous_intent.operating_context,
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
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "workflow_goal_updated",
        &serde_json::json!({
            "origin": origin,
            "previous_goal": previous_goal,
            "new_goal": goal,
            "revision": revision,
            "previous_deliverable_count": previous_deliverables.len(),
            "new_deliverable_count": new_deliverables.len(),
            "added_deliverables": added_deliverables,
            "removed_deliverables": removed_deliverables,
            "previous_capabilities": previous_capabilities,
            "new_capabilities": new_capabilities
        }),
    )?;

    Ok(WorkflowGoalUpdateReport {
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
    })
}

pub fn pause_workflow(
    store: &ForgeStore,
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
    store: &ForgeStore,
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
    store: &ForgeStore,
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
    store: &ForgeStore,
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
    store: &ForgeStore,
    workflow_id: &str,
    input: WorkflowTaskUpdateInput<'_>,
) -> Result<WorkflowTaskUpdateReport> {
    ensure_workflow_policy(store, workflow_id, "workflow task update")?;
    let mut workflow = store.load_workflow(workflow_id)?;
    let Some(task) = workflow
        .tasks
        .iter_mut()
        .find(|task| task.id == input.task_id)
    else {
        bail!(
            "task {} not found in workflow {}",
            input.task_id,
            workflow_id
        );
    };

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
    task.version = task.version.saturating_add(1);

    let new_title = task.title.clone();
    let new_goal = task.goal.clone();
    let new_expected_output = task.expected_output.clone();
    let new_version = task.version;

    let revision = push_revision(
        &mut workflow.revisions,
        input.origin,
        "task_updated",
        &format!(
            "updated task {} from version {} to {}",
            input.task_id, previous_version, new_version
        ),
    );
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "workflow_task_updated",
        &serde_json::json!({
            "origin": input.origin,
            "task_id": input.task_id,
            "previous_title": previous_title,
            "new_title": new_title,
            "previous_goal": previous_goal,
            "new_goal": new_goal,
            "previous_expected_output": previous_expected_output,
            "new_expected_output": new_expected_output,
            "previous_version": previous_version,
            "new_version": new_version,
            "revision": revision
        }),
    )?;

    Ok(WorkflowTaskUpdateReport {
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
    })
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
        state_owner: "forge".to_string(),
    })
}

pub fn update_workflow_node_brain_routing(
    store: &ForgeStore,
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
        routing.orchestrator_brain = "forge".to_string();
        routing.selection_owner = "forge".to_string();
        routing.supports_parallel_agent_brains = true;
        routing.supports_multiple_agents_per_brain = true;
        routing.hot_swappable = true;
        routing.state_owner = "forge_workflow_state".to_string();
        routing.memory_source = "forge_memory_router".to_string();
        routing.skills_source = "forge_skill_router".to_string();
        routing.mcp_source = "forge_mcp_router".to_string();
        routing.switch_command = vec![
            "forge".to_string(),
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
            "forge".to_string(),
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
    store: &ForgeStore,
    workflow_id: &str,
    source_path: &Path,
    kind: &str,
    origin: &str,
) -> Result<ArtifactAttachReport> {
    ensure_workflow_policy(store, workflow_id, "workflow artifact attach")?;
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
    store: &ForgeStore,
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

pub fn record_creative_collaboration_event(
    store: &ForgeStore,
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
    store: &ForgeStore,
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
    store: &ForgeStore,
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
