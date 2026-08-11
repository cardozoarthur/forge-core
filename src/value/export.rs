use super::contract::{validate_value_contract, ValueContract};
use super::experiment::{
    task_protocol_fingerprints, validate_experiment_assignment, value_contract_fingerprint,
    workflow_protocol_fingerprint, ExperimentAssignment,
};
use super::gate::{validate_gate_decision_receipt, GateDecisionReceipt, ValueGate};
use super::outcome::{
    validate_outcome_contract, validate_outcome_endpoints, OutcomeContract,
    OutcomeMeasurementStatus,
};
use crate::graph::{ExecutorKind, ResearchRevision, TaskStatus, Workflow};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const RESEARCH_EXPORT_SCHEMA_VERSION: &str = "foundry.research_export.v1";

#[derive(Debug, Clone, Serialize)]
pub struct ResearchTaskSnapshot {
    pub task_id: String,
    pub status: TaskStatus,
    pub executor: ExecutorKind,
    pub estimated_direct_cost_usd: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResearchReadiness {
    pub status: String,
    pub declared_trace_complete: bool,
    pub runtime_evidence_verified: bool,
    pub prospective_gate_capture_ready: bool,
    pub executed_policy_verified: bool,
    pub causal_claim_ready: bool,
    pub missing: Vec<String>,
    pub recorded_gates: Vec<ValueGate>,
    pub duration_estimates_complete: bool,
    pub observed_evidence_verification_failures: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResearchExport {
    pub schema_version: String,
    pub workflow_id: String,
    pub workflow_revision: u64,
    pub research_revision: u64,
    pub exported_at: DateTime<Utc>,
    pub readiness: ResearchReadiness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_contract: Option<ValueContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment: Option<ExperimentAssignment>,
    pub gate_decisions: Vec<GateDecisionReceipt>,
    pub outcomes: Vec<OutcomeContract>,
    pub research_revisions: Vec<ResearchRevision>,
    pub tasks: Vec<ResearchTaskSnapshot>,
}

pub(crate) fn build_research_export(
    workflow: &Workflow,
    observed_evidence_verification_failures: BTreeMap<String, String>,
) -> ResearchExport {
    let current_task_definition_fingerprints = task_protocol_fingerprints(workflow).ok();
    let current_workflow_protocol_fingerprint = workflow_protocol_fingerprint(workflow).ok();
    let current_value_contract_sha256 = workflow
        .value_contract
        .as_ref()
        .and_then(|contract| value_contract_fingerprint(contract).ok());
    let frozen_task_definition = workflow.experiment.as_ref().is_some_and(|experiment| {
        current_task_definition_fingerprints.as_ref()
            == Some(&experiment.task_definition_fingerprints)
            && current_workflow_protocol_fingerprint.as_ref()
                == Some(&experiment.workflow_protocol_fingerprint)
            && current_value_contract_sha256.as_ref() == Some(&experiment.value_contract_sha256)
    });
    let linked_receipts = workflow
        .experiment
        .as_ref()
        .map(|experiment| {
            workflow
                .gate_decisions
                .iter()
                .filter(|receipt| {
                    receipt.experiment_id.as_deref() == Some(experiment.experiment_id.as_str())
                        && receipt.experiment_arm.as_deref() == Some(experiment.arm.as_str())
                        && receipt.cohort_id.as_deref() == Some(experiment.cohort_id.as_str())
                        && receipt.seed == experiment.seed
                        && receipt.policy == experiment.policy
                        && validate_gate_decision_receipt(receipt).is_empty()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let recorded_gates = linked_receipts
        .iter()
        .filter(|receipt| gate_decision_satisfies_declared_trace(receipt.gate, &receipt.decision))
        .map(|receipt| receipt.gate)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let linked_receipts_by_id = linked_receipts
        .iter()
        .map(|receipt| (receipt.decision_id.as_str(), *receipt))
        .collect::<BTreeMap<_, _>>();
    let linked_outcomes = workflow
        .experiment
        .as_ref()
        .map(|experiment| {
            workflow
                .outcomes
                .iter()
                .filter(|outcome| {
                    let Some(outcome_task_id) = outcome.measurement.task_id.as_deref() else {
                        return false;
                    };
                    let (Some(outcome_run_id), Some(outcome_lease_id), Some(outcome_input_hash)) = (
                        outcome.measurement.run_id.as_deref(),
                        outcome.measurement.lease_id.as_deref(),
                        outcome.measurement.input_hash.as_deref(),
                    ) else {
                        return false;
                    };
                    if outcome.measurement.experiment_id.as_deref()
                        != Some(experiment.experiment_id.as_str())
                        || outcome.measurement.experiment_arm.as_deref()
                            != Some(experiment.arm.as_str())
                        || outcome.measurement.cohort_id.as_deref()
                            != Some(experiment.cohort_id.as_str())
                        || outcome.measurement.seed != experiment.seed
                        || outcome.measurement.evaluated_policy.as_ref() != Some(&experiment.policy)
                        || !validate_outcome_contract(outcome).is_empty()
                        || !validate_outcome_endpoints(
                            &outcome.measurement,
                            experiment.primary_endpoint,
                            &experiment.secondary_endpoints,
                        )
                        .is_empty()
                    {
                        return false;
                    }
                    let referenced_gates = outcome
                        .measurement
                        .gate_decision_ids
                        .iter()
                        .filter_map(|decision_id| linked_receipts_by_id.get(decision_id.as_str()))
                        .filter(|receipt| {
                            gate_decision_satisfies_declared_trace(receipt.gate, &receipt.decision)
                                && (receipt.gate == ValueGate::Gate0ValueAdmission
                                    || (receipt.task_id.as_deref() == Some(outcome_task_id)
                                        && receipt.run_id.as_deref() == Some(outcome_run_id)
                                        && receipt.lease_id.as_deref() == Some(outcome_lease_id)
                                        && receipt.input_hash.as_deref()
                                            == Some(outcome_input_hash)))
                        })
                        .map(|receipt| receipt.gate)
                        .collect::<BTreeSet<_>>();
                    all_value_gates()
                        .iter()
                        .all(|gate| referenced_gates.contains(gate))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let has_linked_outcome = !linked_outcomes.is_empty();
    let has_linked_observed_outcome = linked_outcomes.iter().any(|outcome| {
        outcome.measurement.measurement_status == OutcomeMeasurementStatus::Observed
            && outcome.measurement.execution_receipt_sha256.is_some()
            && !observed_evidence_verification_failures.contains_key(&outcome.outcome_id)
    });
    let duration_estimates_complete = workflow
        .tasks
        .iter()
        .all(|task| task.cost.estimated_duration_ms.is_some());
    let mut missing = Vec::new();
    if workflow.value_contract.is_none() {
        missing.push("value_contract".to_string());
    } else if workflow
        .value_contract
        .as_ref()
        .is_some_and(|contract| !validate_value_contract(contract).is_empty())
    {
        missing.push("valid_value_contract".to_string());
    }
    if workflow.experiment.is_none() {
        missing.push("experiment_assignment".to_string());
    } else if workflow
        .experiment
        .as_ref()
        .is_some_and(|experiment| !validate_experiment_assignment(experiment).is_empty())
    {
        missing.push("valid_experiment_assignment".to_string());
    }
    if workflow.experiment.is_some() && !frozen_task_definition {
        missing.push("frozen_task_definition".to_string());
    }
    for gate in all_value_gates() {
        if !recorded_gates.contains(&gate) {
            missing.push(format!("gate_receipt:{gate:?}"));
        }
    }
    if workflow.outcomes.is_empty() {
        missing.push("outcome_contract".to_string());
    } else if !has_linked_outcome {
        missing.push("linked_outcome_contract".to_string());
    }
    if !duration_estimates_complete {
        missing.push("task_duration_estimates".to_string());
    }
    let declared_trace_complete = missing.is_empty();
    let runtime_evidence_verified = declared_trace_complete
        && has_linked_observed_outcome
        && observed_evidence_verification_failures.is_empty();
    let tasks = workflow
        .tasks
        .iter()
        .map(|task| ResearchTaskSnapshot {
            task_id: task.id.clone(),
            status: task.status.clone(),
            executor: task.executor.clone(),
            estimated_direct_cost_usd: task.cost.estimated_cost_usd,
            estimated_duration_ms: task.cost.estimated_duration_ms,
        })
        .collect();
    ResearchExport {
        schema_version: RESEARCH_EXPORT_SCHEMA_VERSION.to_string(),
        workflow_id: workflow.id.clone(),
        workflow_revision: workflow
            .revisions
            .last()
            .map(|revision| revision.revision)
            .unwrap_or(0),
        research_revision: workflow
            .research_revisions
            .last()
            .map(|revision| revision.revision)
            .unwrap_or(0),
        exported_at: Utc::now(),
        readiness: ResearchReadiness {
            status: if runtime_evidence_verified {
                "runtime_evidence_verified"
            } else if declared_trace_complete {
                "declared_trace_complete"
            } else {
                "trace_incomplete"
            }
            .to_string(),
            declared_trace_complete,
            runtime_evidence_verified,
            prospective_gate_capture_ready: false,
            executed_policy_verified: false,
            causal_claim_ready: false,
            missing,
            recorded_gates,
            duration_estimates_complete,
            observed_evidence_verification_failures,
        },
        value_contract: workflow.value_contract.clone(),
        experiment: workflow.experiment.clone(),
        gate_decisions: workflow.gate_decisions.clone(),
        outcomes: workflow.outcomes.clone(),
        research_revisions: workflow.research_revisions.clone(),
        tasks,
    }
}

fn all_value_gates() -> [ValueGate; 5] {
    [
        ValueGate::Gate0ValueAdmission,
        ValueGate::Gate1InferenceNeed,
        ValueGate::Gate2ResourceSelection,
        ValueGate::Gate3Assurance,
        ValueGate::Gate4Stopping,
    ]
}

pub(crate) fn gate_decision_satisfies_declared_trace(gate: ValueGate, decision: &str) -> bool {
    gate != ValueGate::Gate4Stopping
        || matches!(decision, "stop" | "escalate" | "abstained_missing_contract")
}
