use crate::checkpoint::load_latest_task_checkpoint;
use crate::context::{
    build_context_package_with_checkpoint, ContextHandoffBlocker, ContextPackage,
};
use crate::graph::{ExecutorKind, ValidationRule};
use crate::lease::{acquire_task_lease, TaskLease};
use crate::storage::ForgeStore;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

const EXECUTOR_HANDOFF_SCHEMA_VERSION: &str = "forge.executor_handoff.v3";

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
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub handoff_blockers: Vec<ContextHandoffBlocker>,
    pub expected_output: String,
    pub validation_gate: String,
    pub validation_rules: Vec<ValidationRule>,
    pub execution_policy_mode: String,
    pub persona_mode: Option<String>,
    pub resume_context_status: String,
    pub resume_plan: ExecutorHandoffResumePlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorHandoffResumePlan {
    pub status: String,
    pub action: String,
    pub partial_retry_recommended: bool,
    pub checkpoint_id: Option<String>,
    pub checkpoint_context_sha256: Option<String>,
    pub checkpoint_context_routing_cache_key: Option<String>,
    pub current_context_routing_cache_key: String,
    pub reason: String,
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
    execution_policy_mode: String,
    persona_mode: Option<String>,
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
            execution_policy_mode: task.execution_policy.mode.clone(),
            persona_mode: task.persona.as_ref().map(|persona| persona.mode.clone()),
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
        execution_policy_mode: task.execution_policy.mode.clone(),
        persona_mode: task.persona.as_ref().map(|persona| persona.mode.clone()),
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
            handoff_ready: parts.context.handoff_ready,
            handoff_status: parts.context.handoff_status.clone(),
            handoff_blockers: parts.context.handoff_blockers.clone(),
            expected_output: parts.expected_output,
            validation_gate: parts.validation_gate,
            validation_rules: parts.validation_rules,
            execution_policy_mode: parts.execution_policy_mode,
            persona_mode: parts.persona_mode,
            resume_context_status: parts.context.resume_context_status.clone(),
            resume_plan: build_resume_plan(parts.context),
        }
    }
}

fn build_resume_plan(context: &ContextPackage) -> ExecutorHandoffResumePlan {
    let current_route = context.routing_fingerprint.cache_key.clone();
    let Some(checkpoint) = &context.latest_checkpoint else {
        return ExecutorHandoffResumePlan {
            status: "no_checkpoint".to_string(),
            action: "start_fresh".to_string(),
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route,
            reason: "no checkpoint recorded for this workflow task".to_string(),
        };
    };

    let checkpoint_route = checkpoint.context_routing_cache_key.clone();
    let (status, action, partial_retry_recommended, reason) =
        if context.resume_context_status == "checkpoint_stale" {
            (
                "checkpoint_stale",
                "refresh_context_before_resume",
                false,
                context.resume_context_reason.as_str(),
            )
        } else if checkpoint_route.is_none() {
            (
                "checkpoint_route_unknown",
                "refresh_context_before_resume",
                false,
                "checkpoint does not carry a context routing cache key",
            )
        } else if checkpoint_route.as_deref() == Some(current_route.as_str()) {
            (
                "checkpoint_route_current",
                "resume_from_checkpoint",
                false,
                "checkpoint route matches current handoff route",
            )
        } else {
            (
                "checkpoint_route_changed",
                "partial_retry_with_fresh_context",
                true,
                "checkpoint route differs from current handoff route",
            )
        };

    ExecutorHandoffResumePlan {
        status: status.to_string(),
        action: action.to_string(),
        partial_retry_recommended,
        checkpoint_id: Some(checkpoint.checkpoint_id.clone()),
        checkpoint_context_sha256: Some(checkpoint.context_sha256.clone()),
        checkpoint_context_routing_cache_key: checkpoint_route,
        current_context_routing_cache_key: current_route,
        reason: reason.to_string(),
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
