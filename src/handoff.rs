use crate::checkpoint::load_latest_task_checkpoint;
use crate::context::{
    build_compact_context_view_with_predecessor_plans,
    build_context_package_with_checkpoint_and_project,
    build_context_package_with_checkpoint_project_and_worktree, compact_text,
    compact_validation_rule, sanitize_compact_human_text, unresolved_predecessor_frontier,
    ContextCompactView, ContextContinuationPlan, ContextDeferredDiscoveryPlan, ContextDelta,
    ContextHandoffBlocker, ContextMemoryPolicy, ContextPackage, ContextPersonaSourceModelSummary,
    ContextPredecessorHandoffPlan, ContextRouterPlan, ContextRoutingQuality,
    COMPACT_EXPECTED_OUTPUT_BYTE_LIMIT, COMPACT_PREDECESSOR_TASK_LIMIT,
    COMPACT_PREDECESSOR_VALIDATION_RULE_LIMIT, COMPACT_VALIDATION_COMMAND_BYTE_LIMIT,
};
use crate::executor::{
    canonical_executor_id, decide_executor_model_for_task, load_executors,
    ExecutorModelDecisionOptions, ExecutorModelDecisionReport, ExecutorQuotaObservation,
    ExecutorState,
};
use crate::graph::{
    AtomicTask, ExecutionPolicySpec, ExecutorKind, NodeBrainRoutingSpec, PersonaRoutingSpec,
    TaskStatus, ValidationRule,
};
use crate::identity::ensure_workflow_policy;
use crate::lease::{acquire_task_lease, TaskLease};
use crate::storage::FoundryStore;
use crate::worktree::{
    bound_worktree_context, resolve_effective_project_root, WorktreeContextReport,
};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

pub const EXECUTOR_HANDOFF_SCHEMA_VERSION: &str = "foundry.executor_handoff.v9";
pub const EXECUTOR_HANDOFF_COMPACT_SCHEMA_VERSION: &str = "foundry.executor_handoff.compact.v1";
const PERSONA_HANDOFF_SCHEMA_VERSION: &str = "foundry.persona_handoff.v2";
const COMPACT_HANDOFF_ID_BYTE_LIMIT: usize = 128;
const COMPACT_HANDOFF_TEXT_BYTE_LIMIT: usize = 256;
const COMPACT_HANDOFF_FALLBACK_EXECUTOR_LIMIT: usize = 4;

#[derive(Debug, Clone, Serialize)]
pub struct TaskHandoffReport {
    pub status: String,
    pub allowed: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub selected_executor: String,
    pub selected_brain: String,
    pub orchestrator_brain: String,
    pub task_executor: String,
    pub lease: Option<TaskLease>,
    pub current_lease: Option<TaskLease>,
    pub context: ContextPackage,
    pub packet: ExecutorHandoffPacket,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_decision: Option<ExecutorModelDecisionReport>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskHandoffView {
    Compact,
    Full,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum TaskHandoffResponse {
    Compact(Box<TaskHandoffCompactReport>),
    Full(Box<TaskHandoffReport>),
}

impl TaskHandoffResponse {
    pub fn allowed(&self) -> bool {
        match self {
            Self::Compact(report) => report.allowed,
            Self::Full(report) => report.allowed,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskHandoffCompactReport {
    pub schema_version: String,
    pub status: String,
    pub allowed: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub selected_executor: String,
    pub selected_brain: String,
    pub orchestrator_brain: String,
    pub task_executor: String,
    pub lease: TaskHandoffCompactLease,
    pub context: ContextCompactView,
    pub secret_redaction_count: usize,
    pub execution: TaskHandoffCompactExecution,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskHandoffCompactLease {
    pub required: bool,
    pub status: String,
    pub lease_id: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub current_lease_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskHandoffCompactExecution {
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub selected_model: Option<TaskHandoffCompactModel>,
    pub capacity: TaskHandoffCompactCapacity,
    pub expected_output: String,
    pub validation_gate: String,
    pub validation_rules: Vec<ValidationRule>,
    pub validation_rules_omitted: usize,
    pub execution_policy_mode: String,
    pub resume_context_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskHandoffCompactModel {
    pub executor: String,
    pub provider: String,
    pub model: String,
    pub locality: String,
    pub selection_status: String,
    pub estimated_cost_usd: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskHandoffCompactCapacity {
    pub decision: String,
    pub source: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub remaining_quota: String,
    pub rate_limit_risk: String,
    pub fallback_executors: Vec<String>,
    pub fallback_executors_omitted: usize,
    pub stop_execution: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorHandoffPacket {
    pub schema_version: String,
    pub workflow_id: String,
    pub task_id: String,
    pub selected_executor: String,
    pub selected_brain: String,
    pub orchestrator_brain: String,
    pub task_executor: String,
    pub lease_required: bool,
    pub lease_status: String,
    pub lease_id: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub current_lease_id: Option<String>,
    pub context_schema_version: String,
    pub context_routing_policy: String,
    pub context_sha256: String,
    pub context_routing_fingerprint_schema_version: String,
    pub context_routing_cache_key: String,
    pub context_routing_lineage_sha256: String,
    pub context_bytes: usize,
    pub context_routing_quality: ContextRoutingQuality,
    pub context_delta: ContextDelta,
    pub memory_policy: ContextMemoryPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeContextReport>,
    pub deferred_discovery: ContextDeferredDiscoveryPlan,
    pub context_router: ContextRouterPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_decision: Option<ExecutorModelDecisionReport>,
    pub executor_capacity_decision: ExecutorCapacityDecision,
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub handoff_blockers: Vec<ContextHandoffBlocker>,
    pub expected_output: String,
    pub validation_gate: String,
    pub validation_rules: Vec<ValidationRule>,
    pub execution_policy_mode: String,
    pub execution_policy: ExecutionPolicySpec,
    pub node_brain_routing: NodeBrainRoutingSpec,
    pub persona_mode: Option<String>,
    pub persona_profile_id: Option<String>,
    pub persona_profile_sha256: Option<String>,
    pub persona_contract: Option<ExecutorHandoffPersonaContract>,
    pub resume_context_status: String,
    pub resume_plan: ContextContinuationPlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorHandoffPersonaContract {
    pub schema_version: String,
    pub profile_id: String,
    pub mode: String,
    pub scope: String,
    pub instruction_source: String,
    pub voice: String,
    pub tone: String,
    pub validation_gate: String,
    pub routing_rationale: String,
    pub source_models: Vec<String>,
    pub source_model_summaries: Vec<ContextPersonaSourceModelSummary>,
    pub auditable: bool,
    pub profile_sha256: String,
    pub lineage_sha256: String,
    pub persona_mode_sha256: String,
}

struct PacketParts<'a> {
    context: &'a ContextPackage,
    selected_executor: &'a str,
    task_executor: &'a str,
    node_brain_routing: NodeBrainRoutingSpec,
    lease_status: &'a str,
    lease: Option<&'a TaskLease>,
    current_lease: Option<&'a TaskLease>,
    expected_output: String,
    validation_gate: String,
    validation_rules: Vec<ValidationRule>,
    execution_policy: ExecutionPolicySpec,
    persona: Option<PersonaRoutingSpec>,
    model_decision: Option<ExecutorModelDecisionReport>,
    executor_capacity_decision: ExecutorCapacityDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorCapacityDecision {
    pub schema_version: String,
    pub selected_executor: String,
    pub task_executor: String,
    pub decision: String,
    pub capacity_source: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub remaining_quota: String,
    pub rate_limit_risk: String,
    pub fallback_executors: Vec<String>,
    pub stop_execution: bool,
    pub reason: String,
}

pub fn build_task_handoff(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    selected_executor: &str,
    budget: usize,
    ttl_seconds: u64,
) -> Result<TaskHandoffReport> {
    build_task_handoff_with_project(
        store,
        workflow_id,
        task_id,
        selected_executor,
        budget,
        ttl_seconds,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_task_handoff_response_with_project(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    selected_executor: &str,
    budget: usize,
    ttl_seconds: u64,
    project_root: Option<&Path>,
    view: TaskHandoffView,
) -> Result<TaskHandoffResponse> {
    if view == TaskHandoffView::Full {
        return build_task_handoff_with_project(
            store,
            workflow_id,
            task_id,
            selected_executor,
            budget,
            ttl_seconds,
            project_root,
        )
        .map(|report| TaskHandoffResponse::Full(Box::new(report)));
    }

    ensure_workflow_policy(store, workflow_id, "task handoff")?;
    let workflow = store.load_workflow(workflow_id)?;
    let effective_project_root =
        resolve_effective_project_root(store, workflow_id, Some(task_id), project_root)?;
    let predecessor_plans =
        build_predecessor_handoff_plans(store, &workflow, task_id, budget, project_root)?;
    let report = build_task_handoff_with_project(
        store,
        workflow_id,
        task_id,
        selected_executor,
        budget,
        ttl_seconds,
        project_root,
    )?;
    let context = build_compact_context_view_with_predecessor_plans(
        &report.context,
        &workflow,
        store.path(),
        effective_project_root.as_deref(),
        &predecessor_plans,
    );
    Ok(TaskHandoffResponse::Compact(Box::new(
        TaskHandoffCompactReport::from_report(&report, context),
    )))
}

pub fn build_predecessor_handoff_plans(
    store: &FoundryStore,
    workflow: &crate::graph::Workflow,
    task_id: &str,
    budget: usize,
    explicit_project_root: Option<&Path>,
) -> Result<BTreeMap<String, ContextPredecessorHandoffPlan>> {
    let mut plans = BTreeMap::new();
    for predecessor in unresolved_predecessor_frontier(workflow, task_id)
        .into_iter()
        .take(COMPACT_PREDECESSOR_TASK_LIMIT)
        .filter(|task| task.status == TaskStatus::Pending)
    {
        let latest_checkpoint = load_latest_task_checkpoint(store, &workflow.id, &predecessor.id)?;
        let bound_worktree = bound_worktree_context(store, &workflow.id, Some(&predecessor.id))?;
        let project_root = resolve_effective_project_root(
            store,
            &workflow.id,
            Some(&predecessor.id),
            if bound_worktree.is_some() {
                None
            } else {
                explicit_project_root
            },
        )?;
        let package = if bound_worktree.is_some() {
            build_context_package_with_checkpoint_project_and_worktree(
                workflow,
                &predecessor.id,
                budget,
                latest_checkpoint,
                project_root.as_deref(),
                bound_worktree,
            )?
        } else {
            build_context_package_with_checkpoint_and_project(
                workflow,
                &predecessor.id,
                budget,
                latest_checkpoint,
                project_root.as_deref(),
            )?
        };
        plans.insert(
            predecessor.id.clone(),
            ContextPredecessorHandoffPlan {
                recommended_budget_bytes: package.routing_repair.recommended_budget_bytes,
                project_root,
            },
        );
    }
    Ok(plans)
}

fn sanitize_handoff_validation_rule_human_text(
    rule: &ValidationRule,
    secret_redaction_count: &mut usize,
) -> ValidationRule {
    ValidationRule {
        kind: rule.kind.clone(),
        command: rule
            .command
            .as_deref()
            .map(|command| sanitize_compact_human_text(command, secret_redaction_count)),
        expected: sanitize_compact_human_text(&rule.expected, secret_redaction_count),
    }
}

impl TaskHandoffCompactReport {
    fn from_report(report: &TaskHandoffReport, context: ContextCompactView) -> Self {
        let packet = &report.packet;
        let mut secret_redaction_count = 0usize;
        let selected_model = report
            .model_decision
            .as_ref()
            .and_then(|decision| decision.selected.as_ref())
            .map(|selected| {
                let rationale =
                    sanitize_compact_human_text(&selected.rationale, &mut secret_redaction_count);
                TaskHandoffCompactModel {
                    executor: compact_text(&selected.executor, COMPACT_HANDOFF_ID_BYTE_LIMIT),
                    provider: compact_text(&selected.provider, COMPACT_HANDOFF_ID_BYTE_LIMIT),
                    model: compact_text(&selected.model, COMPACT_HANDOFF_ID_BYTE_LIMIT),
                    locality: compact_text(
                        &selected.local_vs_non_local,
                        COMPACT_HANDOFF_ID_BYTE_LIMIT,
                    ),
                    selection_status: compact_text(
                        &selected.selection_status,
                        COMPACT_HANDOFF_ID_BYTE_LIMIT,
                    ),
                    estimated_cost_usd: selected.estimated_cost_usd,
                    rationale: compact_text(&rationale, COMPACT_HANDOFF_TEXT_BYTE_LIMIT),
                }
            });
        let capacity = &packet.executor_capacity_decision;
        let fallback_executors = capacity
            .fallback_executors
            .iter()
            .take(COMPACT_HANDOFF_FALLBACK_EXECUTOR_LIMIT)
            .map(|executor| compact_text(executor, COMPACT_HANDOFF_ID_BYTE_LIMIT))
            .collect::<Vec<_>>();
        let sanitized_validation_rules = packet
            .validation_rules
            .iter()
            .map(|rule| {
                sanitize_handoff_validation_rule_human_text(rule, &mut secret_redaction_count)
            })
            .collect::<Vec<_>>();
        let validation_rules = sanitized_validation_rules
            .iter()
            .take(COMPACT_PREDECESSOR_VALIDATION_RULE_LIMIT)
            .map(compact_validation_rule)
            .collect::<Vec<_>>();
        let capacity_reason =
            sanitize_compact_human_text(&capacity.reason, &mut secret_redaction_count);
        let expected_output =
            sanitize_compact_human_text(&packet.expected_output, &mut secret_redaction_count);
        let report_reason = report
            .reason
            .as_deref()
            .map(|reason| sanitize_compact_human_text(reason, &mut secret_redaction_count));
        let secret_redaction_count = context
            .secret_redaction_count
            .saturating_add(secret_redaction_count);

        Self {
            schema_version: EXECUTOR_HANDOFF_COMPACT_SCHEMA_VERSION.to_string(),
            status: report.status.clone(),
            allowed: report.allowed,
            workflow_id: report.workflow_id.clone(),
            task_id: report.task_id.clone(),
            selected_executor: compact_text(
                &report.selected_executor,
                COMPACT_HANDOFF_ID_BYTE_LIMIT,
            ),
            selected_brain: compact_text(&report.selected_brain, COMPACT_HANDOFF_ID_BYTE_LIMIT),
            orchestrator_brain: compact_text(
                &report.orchestrator_brain,
                COMPACT_HANDOFF_ID_BYTE_LIMIT,
            ),
            task_executor: compact_text(&report.task_executor, COMPACT_HANDOFF_ID_BYTE_LIMIT),
            lease: TaskHandoffCompactLease {
                required: packet.lease_required,
                status: packet.lease_status.clone(),
                lease_id: packet.lease_id.clone(),
                lease_expires_at: packet.lease_expires_at,
                current_lease_id: packet.current_lease_id.clone(),
            },
            context,
            secret_redaction_count,
            execution: TaskHandoffCompactExecution {
                handoff_ready: packet.handoff_ready,
                handoff_status: packet.handoff_status.clone(),
                selected_model,
                capacity: TaskHandoffCompactCapacity {
                    decision: compact_text(&capacity.decision, COMPACT_HANDOFF_ID_BYTE_LIMIT),
                    source: compact_text(&capacity.capacity_source, COMPACT_HANDOFF_ID_BYTE_LIMIT),
                    provider: capacity
                        .provider
                        .as_deref()
                        .map(|value| compact_text(value, COMPACT_HANDOFF_ID_BYTE_LIMIT)),
                    model: capacity
                        .model
                        .as_deref()
                        .map(|value| compact_text(value, COMPACT_HANDOFF_ID_BYTE_LIMIT)),
                    remaining_quota: compact_text(
                        &capacity.remaining_quota,
                        COMPACT_HANDOFF_ID_BYTE_LIMIT,
                    ),
                    rate_limit_risk: compact_text(
                        &capacity.rate_limit_risk,
                        COMPACT_HANDOFF_ID_BYTE_LIMIT,
                    ),
                    fallback_executors,
                    fallback_executors_omitted: capacity
                        .fallback_executors
                        .len()
                        .saturating_sub(COMPACT_HANDOFF_FALLBACK_EXECUTOR_LIMIT),
                    stop_execution: capacity.stop_execution,
                    reason: compact_text(&capacity_reason, COMPACT_HANDOFF_TEXT_BYTE_LIMIT),
                },
                expected_output: compact_text(&expected_output, COMPACT_EXPECTED_OUTPUT_BYTE_LIMIT),
                validation_gate: compact_text(
                    &packet.validation_gate,
                    COMPACT_VALIDATION_COMMAND_BYTE_LIMIT,
                ),
                validation_rules,
                validation_rules_omitted: packet
                    .validation_rules
                    .len()
                    .saturating_sub(COMPACT_PREDECESSOR_VALIDATION_RULE_LIMIT),
                execution_policy_mode: compact_text(
                    &packet.execution_policy_mode,
                    COMPACT_HANDOFF_ID_BYTE_LIMIT,
                ),
                resume_context_status: compact_text(
                    &packet.resume_context_status,
                    COMPACT_HANDOFF_ID_BYTE_LIMIT,
                ),
            },
            reason: report_reason
                .as_deref()
                .map(|reason| compact_text(reason, COMPACT_HANDOFF_TEXT_BYTE_LIMIT)),
        }
    }
}

pub fn build_task_handoff_with_project(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    selected_executor: &str,
    budget: usize,
    ttl_seconds: u64,
    project_root: Option<&Path>,
) -> Result<TaskHandoffReport> {
    if selected_executor.trim().is_empty() {
        bail!("executor cannot be empty");
    }
    let selected_executor = canonical_executor_id(selected_executor);

    ensure_workflow_policy(store, workflow_id, "task handoff")?;
    let workflow = store.load_workflow(workflow_id)?;
    let task = workflow
        .tasks
        .iter()
        .find(|candidate| candidate.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found in workflow {workflow_id}: {task_id}"))?;
    let latest_checkpoint = load_latest_task_checkpoint(store, workflow_id, task_id)?;
    let effective_project_root =
        resolve_effective_project_root(store, workflow_id, Some(task_id), project_root)?;
    let bound_worktree = bound_worktree_context(store, workflow_id, Some(task_id))?;
    let context = if bound_worktree.is_some() {
        build_context_package_with_checkpoint_project_and_worktree(
            &workflow,
            task_id,
            budget,
            latest_checkpoint,
            effective_project_root.as_deref(),
            bound_worktree,
        )?
    } else {
        build_context_package_with_checkpoint_and_project(
            &workflow,
            task_id,
            budget,
            latest_checkpoint,
            effective_project_root.as_deref(),
        )?
    };
    let task_executor = executor_kind(&task.executor).to_string();
    if task.status != TaskStatus::Pending {
        let executor_capacity_decision =
            build_executor_capacity_decision(store, &selected_executor, &task_executor)?;
        let packet = ExecutorHandoffPacket::from_parts(PacketParts {
            context: &context,
            selected_executor: &selected_executor,
            task_executor: &task_executor,
            node_brain_routing: task.node_brain_routing.clone(),
            lease_status: "not_requested",
            lease: None,
            current_lease: None,
            expected_output: task.expected_output.clone(),
            validation_gate: task.execution_policy.validation_gate.clone(),
            validation_rules: task.validation_rules.clone(),
            execution_policy: task.execution_policy.clone(),
            persona: task.persona.clone(),
            model_decision: None,
            executor_capacity_decision,
        });
        return Ok(TaskHandoffReport {
            status: "handoff_blocked_task_status".to_string(),
            allowed: false,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            selected_executor: selected_executor.clone(),
            selected_brain: selected_executor.clone(),
            orchestrator_brain: packet.orchestrator_brain.clone(),
            task_executor,
            lease: None,
            current_lease: None,
            context,
            packet,
            model_decision: None,
            reason: Some(format!(
                "task status is {}; executor handoff requires pending status",
                task_status_name(&task.status)
            )),
        });
    }

    let model_decision = if selected_executor == "auto" {
        Some(resolve_auto_executor_model_decision(store, task, budget)?)
    } else {
        None
    };
    let effective_selected_executor = canonical_executor_id(
        model_decision
            .as_ref()
            .and_then(|decision| decision.selected.as_ref())
            .map(|candidate| candidate.executor.as_str())
            .unwrap_or(&selected_executor),
    );
    let executor_capacity_decision =
        build_executor_capacity_decision(store, &effective_selected_executor, &task_executor)?;

    if !context.handoff_ready {
        let packet = ExecutorHandoffPacket::from_parts(PacketParts {
            context: &context,
            selected_executor: &effective_selected_executor,
            task_executor: &task_executor,
            node_brain_routing: task.node_brain_routing.clone(),
            lease_status: "not_requested",
            lease: None,
            current_lease: None,
            expected_output: task.expected_output.clone(),
            validation_gate: task.execution_policy.validation_gate.clone(),
            validation_rules: task.validation_rules.clone(),
            execution_policy: task.execution_policy.clone(),
            persona: task.persona.clone(),
            model_decision: model_decision.clone(),
            executor_capacity_decision,
        });
        return Ok(TaskHandoffReport {
            status: "handoff_blocked".to_string(),
            allowed: false,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            selected_executor: effective_selected_executor.clone(),
            selected_brain: effective_selected_executor.clone(),
            orchestrator_brain: packet.orchestrator_brain.clone(),
            task_executor,
            lease: None,
            current_lease: None,
            context,
            packet,
            model_decision,
            reason: Some("context handoff is not ready".to_string()),
        });
    }

    if executor_capacity_decision.decision != "use" {
        let packet = ExecutorHandoffPacket::from_parts(PacketParts {
            context: &context,
            selected_executor: &effective_selected_executor,
            task_executor: &task_executor,
            node_brain_routing: task.node_brain_routing.clone(),
            lease_status: "not_requested",
            lease: None,
            current_lease: None,
            expected_output: task.expected_output.clone(),
            validation_gate: task.execution_policy.validation_gate.clone(),
            validation_rules: task.validation_rules.clone(),
            execution_policy: task.execution_policy.clone(),
            persona: task.persona.clone(),
            model_decision: model_decision.clone(),
            executor_capacity_decision,
        });
        return Ok(TaskHandoffReport {
            status: if packet.executor_capacity_decision.capacity_source == "executor_policy" {
                "handoff_blocked_executor_policy"
            } else {
                "handoff_blocked_executor_capacity"
            }
            .to_string(),
            allowed: false,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            selected_executor: effective_selected_executor.clone(),
            selected_brain: effective_selected_executor.clone(),
            orchestrator_brain: packet.orchestrator_brain.clone(),
            task_executor,
            lease: None,
            current_lease: None,
            context,
            reason: Some(packet.executor_capacity_decision.reason.clone()),
            model_decision,
            packet,
        });
    }

    let lease_report = acquire_task_lease(
        store,
        workflow_id,
        task_id,
        &effective_selected_executor,
        ttl_seconds,
    )?;
    let packet = ExecutorHandoffPacket::from_parts(PacketParts {
        context: &context,
        selected_executor: &effective_selected_executor,
        task_executor: &task_executor,
        node_brain_routing: task.node_brain_routing.clone(),
        lease_status: &lease_report.status,
        lease: lease_report.lease.as_ref(),
        current_lease: lease_report.current_lease.as_ref(),
        expected_output: task.expected_output.clone(),
        validation_gate: task.execution_policy.validation_gate.clone(),
        validation_rules: task.validation_rules.clone(),
        execution_policy: task.execution_policy.clone(),
        persona: task.persona.clone(),
        model_decision: model_decision.clone(),
        executor_capacity_decision,
    });
    let allowed = lease_report.allowed;
    Ok(TaskHandoffReport {
        status: if allowed {
            "handoff_ready".to_string()
        } else {
            lease_report.status
        },
        allowed,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        selected_executor: effective_selected_executor.clone(),
        selected_brain: effective_selected_executor,
        orchestrator_brain: packet.orchestrator_brain.clone(),
        task_executor,
        lease: lease_report.lease,
        current_lease: lease_report.current_lease,
        context,
        packet,
        model_decision,
        reason: lease_report.reason,
    })
}

impl ExecutorHandoffPacket {
    fn from_parts(parts: PacketParts<'_>) -> Self {
        let persona_mode = parts.persona.as_ref().map(|persona| persona.mode.clone());
        let persona_profile_id = parts
            .context
            .persona_profile
            .as_ref()
            .map(|profile| profile.profile_id.clone());
        let persona_profile_sha256 = parts
            .context
            .persona_profile
            .as_ref()
            .map(|profile| profile.profile_sha256.clone());
        let persona_contract = parts
            .persona
            .as_ref()
            .map(|persona| build_persona_contract(persona, parts.context));

        Self {
            schema_version: EXECUTOR_HANDOFF_SCHEMA_VERSION.to_string(),
            workflow_id: parts.context.workflow_id.clone(),
            task_id: parts.context.task_id.clone(),
            selected_executor: parts.selected_executor.to_string(),
            selected_brain: parts.selected_executor.to_string(),
            orchestrator_brain: parts.node_brain_routing.orchestrator_brain.clone(),
            task_executor: parts.task_executor.to_string(),
            lease_required: true,
            lease_status: parts.lease_status.to_string(),
            lease_id: parts.lease.map(|lease| lease.lease_id.clone()),
            lease_expires_at: parts.lease.map(|lease| lease.expires_at),
            current_lease_id: parts.current_lease.map(|lease| lease.lease_id.clone()),
            context_schema_version: parts.context.schema_version.clone(),
            context_routing_policy: parts.context.routing_policy.clone(),
            context_sha256: parts.context.context_sha256.clone(),
            context_routing_fingerprint_schema_version: parts
                .context
                .routing_fingerprint
                .schema_version
                .clone(),
            context_routing_cache_key: parts.context.routing_fingerprint.cache_key.clone(),
            context_routing_lineage_sha256: parts
                .context
                .routing_fingerprint
                .lineage_sha256
                .clone(),
            context_bytes: parts.context.context_bytes,
            context_routing_quality: parts.context.routing_quality.clone(),
            context_delta: parts.context.context_delta.clone(),
            memory_policy: parts.context.memory_policy.clone(),
            worktree: parts.context.worktree.clone(),
            deferred_discovery: parts.context.deferred_discovery.clone(),
            context_router: parts.context.context_router.clone(),
            model_decision: parts.model_decision,
            executor_capacity_decision: parts.executor_capacity_decision,
            handoff_ready: parts.context.handoff_ready,
            handoff_status: parts.context.handoff_status.clone(),
            handoff_blockers: parts.context.handoff_blockers.clone(),
            expected_output: parts.expected_output,
            validation_gate: parts.validation_gate,
            validation_rules: parts.validation_rules,
            execution_policy_mode: parts.execution_policy.mode.clone(),
            execution_policy: parts.execution_policy,
            node_brain_routing: parts.node_brain_routing,
            persona_mode,
            persona_profile_id,
            persona_profile_sha256,
            persona_contract,
            resume_context_status: parts.context.resume_context_status.clone(),
            resume_plan: parts.context.continuation_plan.clone(),
        }
    }
}

fn resolve_auto_executor_model_decision(
    store: &FoundryStore,
    task: &AtomicTask,
    budget: usize,
) -> Result<ExecutorModelDecisionReport> {
    let decision = decide_executor_model_for_task(
        store,
        ExecutorModelDecisionOptions {
            task: format!("{}: {}", task.title, task.goal),
            task_class: task_model_decision_class(task).to_string(),
            difficulty: task_model_decision_difficulty(task).to_string(),
            expected_input_tokens: budget as u64,
            expected_output_tokens: (budget as u64 / 4).max(256),
            configured_decider: None,
        },
    )?;
    if decision.selected.is_none() {
        bail!("auto executor selection found no eligible model candidate");
    }
    Ok(decision)
}

fn task_model_decision_class(task: &AtomicTask) -> &'static str {
    match task.executor {
        ExecutorKind::Command | ExecutorKind::Wait | ExecutorKind::Notification => {
            "deterministic_validation_file_inspection_reporting"
        }
        ExecutorKind::Ai | ExecutorKind::Mixed => {
            if task.execution_policy.mode.contains("business")
                || task.execution_policy.selection_reason.contains("product")
            {
                "high_value_pm_business_creative_reasoning"
            } else {
                "general_reasoning"
            }
        }
    }
}

fn task_model_decision_difficulty(task: &AtomicTask) -> &'static str {
    if task.cost.estimated_cost_usd >= 0.01
        || matches!(task.executor, ExecutorKind::Ai | ExecutorKind::Mixed)
    {
        "high"
    } else if task.cost.estimated_cost_usd <= 0.0005
        || matches!(
            task.executor,
            ExecutorKind::Command | ExecutorKind::Wait | ExecutorKind::Notification
        )
    {
        "low"
    } else {
        "medium"
    }
}

fn build_executor_capacity_decision(
    store: &FoundryStore,
    selected_executor: &str,
    task_executor: &str,
) -> Result<ExecutorCapacityDecision> {
    let selected_executor = canonical_executor_id(selected_executor);
    let observations = store
        .load_executor_quotas()?
        .into_iter()
        .filter_map(|value| serde_json::from_value::<ExecutorQuotaObservation>(value).ok())
        .collect::<Vec<_>>();
    let executor_states = load_executors(store)?.executors;
    let matching_observation = observations
        .iter()
        .find(|observation| canonical_executor_id(&observation.executor) == selected_executor);
    let fallback_executors =
        eligible_fallback_executors(&executor_states, &selected_executor, &observations);

    let selected_state = executor_states
        .iter()
        .find(|state| state.id == selected_executor);
    let policy_failures =
        if selected_state.is_some() || executor_is_managed_cognitive_adapter(&selected_executor) {
            executor_policy_failures(selected_state)
        } else {
            Vec::new()
        };
    if !policy_failures.is_empty() {
        return Ok(ExecutorCapacityDecision {
            schema_version: "foundry.executor_capacity_decision.v1".to_string(),
            selected_executor,
            task_executor: task_executor.to_string(),
            decision: "stop".to_string(),
            capacity_source: "executor_policy".to_string(),
            provider: None,
            model: None,
            remaining_quota: "unknown".to_string(),
            rate_limit_risk: "unknown".to_string(),
            fallback_executors,
            stop_execution: true,
            reason: format!(
                "executor policy blocks handoff: {}; run `foundry sync executors` and explicitly authorize the canonical executor before acquiring a lease",
                policy_failures.join(", ")
            ),
        });
    }

    if let Some(observation) = matching_observation.filter(|observation| {
        quota_blocks_handoff(&observation.remaining_quota, &observation.rate_limit_risk)
    }) {
        let has_fallback = !fallback_executors.is_empty();
        return Ok(ExecutorCapacityDecision {
            schema_version: "foundry.executor_capacity_decision.v1".to_string(),
            selected_executor: selected_executor.clone(),
            task_executor: task_executor.to_string(),
            decision: if has_fallback { "fallback" } else { "stop" }.to_string(),
            capacity_source: observation.source.clone(),
            provider: Some(observation.provider.clone()),
            model: observation.model.clone(),
            remaining_quota: observation.remaining_quota.clone(),
            rate_limit_risk: observation.rate_limit_risk.clone(),
            fallback_executors,
            stop_execution: !has_fallback,
            reason: if has_fallback {
                format!(
                    "{} reports {} quota/rate-limit state {}; use a fallback executor before acquiring a lease",
                    observation.source, selected_executor, observation.remaining_quota
                )
            } else {
                format!(
                    "{} reports {} quota/rate-limit state {}; stop execution before spending unavailable capacity",
                    observation.source, selected_executor, observation.remaining_quota
                )
            },
        });
    }

    Ok(ExecutorCapacityDecision {
        schema_version: "foundry.executor_capacity_decision.v1".to_string(),
        selected_executor,
        task_executor: task_executor.to_string(),
        decision: "use".to_string(),
        capacity_source: matching_observation
            .map(|observation| observation.source.clone())
            .unwrap_or_else(|| "no_capacity_observation".to_string()),
        provider: matching_observation.map(|observation| observation.provider.clone()),
        model: matching_observation.and_then(|observation| observation.model.clone()),
        remaining_quota: matching_observation
            .map(|observation| observation.remaining_quota.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        rate_limit_risk: matching_observation
            .map(|observation| observation.rate_limit_risk.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        fallback_executors,
        stop_execution: false,
        reason: "selected executor has no blocking capacity observation".to_string(),
    })
}

fn executor_is_managed_cognitive_adapter(executor: &str) -> bool {
    matches!(
        canonical_executor_id(executor).as_str(),
        "agy" | "claude" | "codex" | "gemini" | "ollama" | "opencode"
    )
}

fn executor_policy_failures(state: Option<&ExecutorState>) -> Vec<String> {
    let Some(state) = state else {
        return vec!["executor state is missing".to_string()];
    };
    let mut failures = Vec::new();
    if !state.installed {
        failures.push("installed=false".to_string());
    }
    if !state.configured {
        failures.push("configured=false".to_string());
    }
    if !state.allowed {
        failures.push("allowed=false".to_string());
    }
    if !state.non_interactive_ready {
        failures.push("non_interactive_ready=false".to_string());
    }
    failures
}

fn eligible_fallback_executors(
    states: &[ExecutorState],
    selected_executor: &str,
    observations: &[ExecutorQuotaObservation],
) -> Vec<String> {
    let selected_executor = canonical_executor_id(selected_executor);
    let mut executors = states
        .iter()
        .filter(|state| {
            canonical_executor_id(&state.id) != selected_executor
                && state.allowed
                && state.installed
                && state.configured
                && state.non_interactive_ready
                && !executor_has_blocking_observation(&state.id, observations)
        })
        .map(|state| canonical_executor_id(&state.id))
        .collect::<Vec<_>>();
    executors.sort();
    executors.dedup();
    executors
}

fn executor_has_blocking_observation(
    executor: &str,
    observations: &[ExecutorQuotaObservation],
) -> bool {
    let executor = canonical_executor_id(executor);
    observations.iter().any(|observation| {
        canonical_executor_id(&observation.executor) == executor
            && quota_blocks_handoff(&observation.remaining_quota, &observation.rate_limit_risk)
    })
}

fn quota_blocks_handoff(remaining_quota: &str, rate_limit_risk: &str) -> bool {
    let remaining = remaining_quota.to_lowercase();
    let risk = rate_limit_risk.to_lowercase();
    [
        "exhausted",
        "depleted",
        "no_remaining",
        "zero_remaining",
        "unavailable",
    ]
    .iter()
    .any(|needle| remaining.contains(needle))
        || ["blocked", "rate_limited", "quota_exhausted"]
            .iter()
            .any(|needle| risk.contains(needle))
}

fn build_persona_contract(
    persona: &PersonaRoutingSpec,
    context: &ContextPackage,
) -> ExecutorHandoffPersonaContract {
    let profile = context
        .persona_profile
        .as_ref()
        .expect("persona profile should be derived when persona routing exists");
    ExecutorHandoffPersonaContract {
        schema_version: PERSONA_HANDOFF_SCHEMA_VERSION.to_string(),
        profile_id: profile.profile_id.clone(),
        mode: persona.mode.clone(),
        scope: persona.scope.clone(),
        instruction_source: persona.instruction_source.clone(),
        voice: persona.voice.clone(),
        tone: persona.tone.clone(),
        validation_gate: persona.validation_gate.clone(),
        routing_rationale: profile.routing_rationale.clone(),
        source_models: persona.source_models.clone(),
        source_model_summaries: profile.source_model_summaries.clone(),
        auditable: persona.auditable,
        profile_sha256: profile.profile_sha256.clone(),
        lineage_sha256: context.lineage.lineage_sha256.clone(),
        persona_mode_sha256: context.lineage.persona_mode_sha256.clone(),
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

fn task_status_name(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::build_predecessor_handoff_plans;
    use crate::graph::{self, ExecutorKind, TaskStatus, ValidationRule};
    use crate::intent::parse_intent;
    use crate::storage::FoundryStore;
    use tempfile::tempdir;

    #[test]
    fn predecessor_handoff_plans_only_prepare_bounded_pending_frontier() {
        let temp = tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        let mut workflow = graph::create_workflow(parse_intent(
            "Prepare only the bounded actionable predecessor frontier",
        ));
        let predecessor_ids = (0..8)
            .map(|index| format!("task-predecessor-{index:02}"))
            .collect::<Vec<_>>();
        workflow.tasks = predecessor_ids
            .iter()
            .enumerate()
            .map(|(index, task_id)| {
                let mut task = graph::task(
                    task_id,
                    "Prepare predecessor evidence",
                    &[],
                    &["bounded predecessor input"],
                    vec![ValidationRule {
                        kind: "schema".to_string(),
                        command: None,
                        expected: "predecessor evidence is valid".to_string(),
                    }],
                    "PredecessorEvidence",
                    (ExecutorKind::Ai, 0.0),
                );
                if matches!(index, 0 | 2) {
                    task.status = TaskStatus::Blocked;
                }
                task
            })
            .collect();
        let dependency_refs = predecessor_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        workflow.tasks.push(graph::task(
            "task-target",
            "Execute after actionable predecessors",
            &dependency_refs,
            &["predecessor evidence"],
            vec![ValidationRule {
                kind: "review".to_string(),
                command: None,
                expected: "all predecessor evidence is reviewed".to_string(),
            }],
            "TargetEvidence",
            (ExecutorKind::Ai, 0.0),
        ));
        store.save_workflow(&workflow).unwrap();

        let plans =
            build_predecessor_handoff_plans(&store, &workflow, "task-target", 1200, None).unwrap();

        assert_eq!(plans.len(), 4);
        assert_eq!(
            plans.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![
                "task-predecessor-01",
                "task-predecessor-03",
                "task-predecessor-04",
                "task-predecessor-05",
            ]
        );
    }
}
