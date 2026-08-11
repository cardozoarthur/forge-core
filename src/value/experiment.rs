use super::contract::{PolicyRef, ValueContract};
use super::outcome::OutcomeMetric;
use super::validation::{require_text, validate_policy_ref, validate_text_refs};
use crate::artifact::hex_sha256;
use crate::graph::Workflow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EXPERIMENT_ASSIGNMENT_SCHEMA_VERSION: &str = "foundry.experiment_assignment.v1";

fn experiment_assignment_schema_version() -> String {
    EXPERIMENT_ASSIGNMENT_SCHEMA_VERSION.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExperimentAssignmentInput {
    pub experiment_id: String,
    pub arm: String,
    pub cohort_id: String,
    pub policy: PolicyRef,
    pub assignment_method: ExperimentAssignmentMethod,
    #[serde(default)]
    pub assignment_evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default)]
    pub shadow_mode: bool,
    #[serde(default)]
    pub holdout: bool,
    pub primary_endpoint: OutcomeMetric,
    #[serde(default)]
    pub secondary_endpoints: Vec<OutcomeMetric>,
    #[serde(default)]
    pub kill_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperimentAssignment {
    #[serde(default = "experiment_assignment_schema_version")]
    pub schema_version: String,
    pub experiment_id: String,
    pub arm: String,
    pub cohort_id: String,
    pub policy: PolicyRef,
    pub assignment_method: ExperimentAssignmentMethod,
    #[serde(default)]
    pub assignment_evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub shadow_mode: bool,
    pub holdout: bool,
    pub primary_endpoint: OutcomeMetric,
    #[serde(default)]
    pub secondary_endpoints: Vec<OutcomeMetric>,
    #[serde(default)]
    pub kill_conditions: Vec<String>,
    pub registered_workflow_revision: u64,
    #[serde(default)]
    pub workflow_protocol_fingerprint: String,
    #[serde(default)]
    pub value_contract_sha256: String,
    #[serde(default)]
    pub task_definition_fingerprints: BTreeMap<String, String>,
    pub registered_at: DateTime<Utc>,
}

impl ExperimentAssignment {
    pub fn from_input(
        input: ExperimentAssignmentInput,
        registered_workflow_revision: u64,
        workflow_protocol_fingerprint: String,
        value_contract_sha256: String,
        task_definition_fingerprints: BTreeMap<String, String>,
        registered_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: experiment_assignment_schema_version(),
            experiment_id: input.experiment_id,
            arm: input.arm,
            cohort_id: input.cohort_id,
            policy: input.policy,
            assignment_method: input.assignment_method,
            assignment_evidence_refs: input.assignment_evidence_refs,
            seed: input.seed,
            shadow_mode: input.shadow_mode,
            holdout: input.holdout,
            primary_endpoint: input.primary_endpoint,
            secondary_endpoints: input.secondary_endpoints,
            kill_conditions: input.kill_conditions,
            registered_workflow_revision,
            workflow_protocol_fingerprint,
            value_contract_sha256,
            task_definition_fingerprints,
            registered_at,
        }
    }
}

pub fn validate_experiment_input(input: &ExperimentAssignmentInput) -> Vec<String> {
    let mut violations = Vec::new();
    require_text(&input.experiment_id, "experiment_id", &mut violations);
    require_text(&input.arm, "arm", &mut violations);
    require_text(&input.cohort_id, "cohort_id", &mut violations);
    validate_policy_ref(&input.policy, &mut violations);
    validate_text_refs(
        &input.assignment_evidence_refs,
        "assignment_evidence_refs",
        &mut violations,
    );
    let mut endpoints = std::collections::BTreeSet::new();
    endpoints.insert(input.primary_endpoint);
    for endpoint in &input.secondary_endpoints {
        if !endpoints.insert(*endpoint) {
            violations.push(format!(
                "experiment endpoint {} is duplicated",
                endpoint.as_str()
            ));
        }
    }
    validate_text_refs(&input.kill_conditions, "kill_conditions", &mut violations);
    if matches!(
        input.assignment_method,
        ExperimentAssignmentMethod::Randomized
            | ExperimentAssignmentMethod::Paired
            | ExperimentAssignmentMethod::Stratified
    ) {
        if input.seed.is_none() {
            violations.push(
                "seed is required for randomized, paired or stratified assignment".to_string(),
            );
        }
        if input.assignment_evidence_refs.is_empty() {
            violations.push(
                "assignment_evidence_refs are required for randomized, paired or stratified assignment"
                    .to_string(),
            );
        }
    }
    violations
}

pub fn validate_experiment_assignment(assignment: &ExperimentAssignment) -> Vec<String> {
    let mut violations = validate_experiment_input(&ExperimentAssignmentInput {
        experiment_id: assignment.experiment_id.clone(),
        arm: assignment.arm.clone(),
        cohort_id: assignment.cohort_id.clone(),
        policy: assignment.policy.clone(),
        assignment_method: assignment.assignment_method,
        assignment_evidence_refs: assignment.assignment_evidence_refs.clone(),
        seed: assignment.seed,
        shadow_mode: assignment.shadow_mode,
        holdout: assignment.holdout,
        primary_endpoint: assignment.primary_endpoint,
        secondary_endpoints: assignment.secondary_endpoints.clone(),
        kill_conditions: assignment.kill_conditions.clone(),
    });
    if assignment.schema_version != EXPERIMENT_ASSIGNMENT_SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {EXPERIMENT_ASSIGNMENT_SCHEMA_VERSION}"
        ));
    }
    if assignment.registered_workflow_revision == 0 {
        violations.push("registered_workflow_revision must be greater than zero".to_string());
    }
    if assignment.workflow_protocol_fingerprint.len() != 64
        || !assignment
            .workflow_protocol_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        violations.push("workflow_protocol_fingerprint must be a SHA-256 hash".to_string());
    }
    if assignment.value_contract_sha256.len() != 64
        || !assignment
            .value_contract_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        violations.push("value_contract_sha256 must be a SHA-256 hash".to_string());
    }
    if assignment.task_definition_fingerprints.is_empty() {
        violations.push(
            "task_definition_fingerprints cannot be empty for a registered experiment".to_string(),
        );
    }
    for (task_id, fingerprint) in &assignment.task_definition_fingerprints {
        require_text(
            task_id,
            "task_definition_fingerprints.task_id",
            &mut violations,
        );
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            violations.push(format!(
                "task_definition_fingerprints.{task_id} must be a SHA-256 hash"
            ));
        }
    }
    violations
}

pub fn task_protocol_fingerprints(
    workflow: &Workflow,
) -> serde_json::Result<BTreeMap<String, String>> {
    workflow
        .tasks
        .iter()
        .map(|task| {
            let subtasks = task
                .work_item
                .subtasks
                .iter()
                .map(|subtask| {
                    serde_json::json!({
                        "id": subtask.id,
                        "title": subtask.title,
                        "goal": subtask.goal,
                        "definition_of_done": subtask.definition_of_done,
                    })
                })
                .collect::<Vec<_>>();
            let work_item = serde_json::json!({
                "item_type": task.work_item.item_type,
                "priority": task.work_item.priority,
                "owner_role": task.work_item.owner_role,
                "parent_id": task.work_item.parent_id,
                "subtasks": subtasks,
                "acceptance_criteria": task.work_item.acceptance_criteria,
                "goal_validation": {
                    "goal": task.work_item.goal_validation.goal,
                    "evidence_required": task.work_item.goal_validation.evidence_required,
                    "rework_policy": task.work_item.goal_validation.rework_policy,
                }
            });
            let schedule = task.schedule.as_ref().map(|schedule| {
                serde_json::json!({
                    "schema_version": schedule.schema_version,
                    "kind": schedule.kind,
                    "cron": schedule.cron,
                    "timezone": schedule.timezone,
                    "missed_run_policy": schedule.missed_run_policy,
                    "scale_to_zero_when_idle": schedule.scale_to_zero_when_idle,
                })
            });
            let loop_control = task.loop_control.as_ref().map(|loop_control| {
                serde_json::json!({
                    "schema_version": loop_control.schema_version,
                    "kind": loop_control.kind,
                    "items": loop_control.items,
                    "max_iterations": loop_control.max_iterations,
                    "condition": loop_control.condition,
                    "backoff_policy": loop_control.backoff_policy,
                    "subflow_mode": loop_control.subflow_mode,
                    "stop_policy": loop_control.stop_policy,
                })
            });
            let child_subflows = task
                .child_subflows
                .iter()
                .map(|subflow| {
                    serde_json::json!({
                        "workflow_id": subflow.workflow_id,
                        "task_id": subflow.task_id,
                        "title": subflow.title,
                        "reuse_key": subflow.reuse_key,
                        "context_lineage_sha256": subflow.context_lineage_sha256,
                        "validation_gate": subflow.validation_gate,
                        "reason": subflow.reason,
                    })
                })
                .collect::<Vec<_>>();
            let human_interaction_protocol = task.human_interaction.as_ref().map(|interaction| {
                serde_json::json!({
                    "schema_version": interaction.schema_version,
                    "kind": interaction.kind,
                    "prompt": interaction.prompt,
                    "required": interaction.required,
                    "explanation": interaction.explanation,
                    "choices": interaction.choices,
                    "form": interaction.form,
                    "timeout_at": interaction.timeout_at,
                    "on_timeout": interaction.on_timeout,
                })
            });
            let protocol = serde_json::json!({
                "id": task.id,
                "title": task.title,
                "goal": task.goal,
                "dependencies": task.dependencies,
                "context_requirements": task.context_requirements,
                "validation_rules": task.validation_rules,
                "expected_output": task.expected_output,
                "executor": task.executor,
                "human_required": task.human_required,
                "schedule": schedule,
                "loop_control": loop_control,
                "native_subflow": task.native_subflow,
                "cost": task.cost,
                "notification": task.notification,
                "persona": task.persona,
                "work_item": work_item,
                "async_policy": task.async_policy,
                "execution_policy": task.execution_policy,
                "node_brain_routing": task.node_brain_routing,
                "child_subflows": child_subflows,
                "human_interaction_protocol": human_interaction_protocol,
            });
            serde_json::to_vec(&protocol).map(|bytes| (task.id.clone(), hex_sha256(&bytes)))
        })
        .collect()
}

pub fn workflow_protocol_fingerprint(workflow: &Workflow) -> serde_json::Result<String> {
    let protocol = serde_json::json!({
        "goal": workflow.goal,
        "intent": workflow.intent,
        "core_orchestration": workflow.core_orchestration,
        "runtime": workflow.runtime,
    });
    serde_json::to_vec(&protocol).map(|bytes| hex_sha256(&bytes))
}

pub fn value_contract_fingerprint(contract: &ValueContract) -> serde_json::Result<String> {
    serde_json::to_vec(contract).map(|bytes| hex_sha256(&bytes))
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentAssignmentMethod {
    Deterministic,
    Randomized,
    Paired,
    Stratified,
}
