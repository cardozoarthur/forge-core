use crate::checkpoint::load_latest_task_checkpoint;
use crate::context::{
    build_context_package_with_checkpoint, ContextContinuationPlan, ContextDelta,
    ContextHandoffBlocker, ContextPackage, ContextPersonaSourceModelSummary, ContextRoutingQuality,
};
use crate::graph::{ExecutionPolicySpec, ExecutorKind, PersonaRoutingSpec, ValidationRule};
use crate::lease::{acquire_task_lease, TaskLease};
use crate::storage::ForgeStore;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

const EXECUTOR_HANDOFF_SCHEMA_VERSION: &str = "forge.executor_handoff.v8";
const PERSONA_HANDOFF_SCHEMA_VERSION: &str = "forge.persona_handoff.v2";

#[derive(Debug, Clone, Serialize)]
pub struct TaskHandoffReport {
    pub status: String,
    pub allowed: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub selected_executor: String,
    pub task_executor: String,
    pub lease: Option<TaskLease>,
    pub current_lease: Option<TaskLease>,
    pub context: ContextPackage,
    pub packet: ExecutorHandoffPacket,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorHandoffPacket {
    pub schema_version: String,
    pub workflow_id: String,
    pub task_id: String,
    pub selected_executor: String,
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
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub handoff_blockers: Vec<ContextHandoffBlocker>,
    pub expected_output: String,
    pub validation_gate: String,
    pub validation_rules: Vec<ValidationRule>,
    pub execution_policy_mode: String,
    pub execution_policy: ExecutionPolicySpec,
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
    lease_status: &'a str,
    lease: Option<&'a TaskLease>,
    current_lease: Option<&'a TaskLease>,
    expected_output: String,
    validation_gate: String,
    validation_rules: Vec<ValidationRule>,
    execution_policy: ExecutionPolicySpec,
    persona: Option<PersonaRoutingSpec>,
}

pub fn build_task_handoff(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    selected_executor: &str,
    budget: usize,
    ttl_seconds: u64,
) -> Result<TaskHandoffReport> {
    if selected_executor.trim().is_empty() {
        bail!("executor cannot be empty");
    }

    let workflow = store.load_workflow(workflow_id)?;
    let task = workflow
        .tasks
        .iter()
        .find(|candidate| candidate.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found in workflow {workflow_id}: {task_id}"))?;
    let latest_checkpoint = load_latest_task_checkpoint(store, workflow_id, task_id)?;
    let context =
        build_context_package_with_checkpoint(&workflow, task_id, budget, latest_checkpoint)?;
    let task_executor = executor_kind(&task.executor).to_string();

    if !context.handoff_ready {
        let packet = ExecutorHandoffPacket::from_parts(PacketParts {
            context: &context,
            selected_executor,
            task_executor: &task_executor,
            lease_status: "not_requested",
            lease: None,
            current_lease: None,
            expected_output: task.expected_output.clone(),
            validation_gate: task.execution_policy.validation_gate.clone(),
            validation_rules: task.validation_rules.clone(),
            execution_policy: task.execution_policy.clone(),
            persona: task.persona.clone(),
        });
        return Ok(TaskHandoffReport {
            status: "handoff_blocked".to_string(),
            allowed: false,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            selected_executor: selected_executor.to_string(),
            task_executor,
            lease: None,
            current_lease: None,
            context,
            packet,
            reason: Some("context handoff is not ready".to_string()),
        });
    }

    let lease_report =
        acquire_task_lease(store, workflow_id, task_id, selected_executor, ttl_seconds)?;
    let packet = ExecutorHandoffPacket::from_parts(PacketParts {
        context: &context,
        selected_executor,
        task_executor: &task_executor,
        lease_status: &lease_report.status,
        lease: lease_report.lease.as_ref(),
        current_lease: lease_report.current_lease.as_ref(),
        expected_output: task.expected_output.clone(),
        validation_gate: task.execution_policy.validation_gate.clone(),
        validation_rules: task.validation_rules.clone(),
        execution_policy: task.execution_policy.clone(),
        persona: task.persona.clone(),
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
        selected_executor: selected_executor.to_string(),
        task_executor,
        lease: lease_report.lease,
        current_lease: lease_report.current_lease,
        context,
        packet,
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
            handoff_ready: parts.context.handoff_ready,
            handoff_status: parts.context.handoff_status.clone(),
            handoff_blockers: parts.context.handoff_blockers.clone(),
            expected_output: parts.expected_output,
            validation_gate: parts.validation_gate,
            validation_rules: parts.validation_rules,
            execution_policy_mode: parts.execution_policy.mode.clone(),
            execution_policy: parts.execution_policy,
            persona_mode,
            persona_profile_id,
            persona_profile_sha256,
            persona_contract,
            resume_context_status: parts.context.resume_context_status.clone(),
            resume_plan: parts.context.continuation_plan.clone(),
        }
    }
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
