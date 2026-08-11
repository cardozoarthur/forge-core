use super::contract::{PolicyRef, PolicySource, ValueContract};
use super::validation::{
    require_text, validate_optional_bps, validate_optional_finite, validate_optional_hash,
    validate_optional_non_negative, validate_policy_ref, validate_text_refs,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const GATE_DECISION_SCHEMA_VERSION: &str = "foundry.value_gate_decision.v1";

fn gate_decision_schema_version() -> String {
    GATE_DECISION_SCHEMA_VERSION.to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ValueGate {
    Gate0ValueAdmission,
    Gate1InferenceNeed,
    Gate2ResourceSelection,
    Gate3Assurance,
    Gate4Stopping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CandidatePrediction {
    pub candidate_id: String,
    pub resource_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_success_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_direct_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_risk_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_incremental_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty_bps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GateDecisionInput {
    pub idempotency_key: String,
    pub decision_point: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    pub gate: ValueGate,
    pub decision: String,
    #[serde(default)]
    pub candidates: Vec<CandidatePrediction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_candidate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_bps: Option<u32>,
    pub rationale: String,
    pub policy: PolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_arm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub hard_constraint_violations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateDecisionReceipt {
    #[serde(default = "gate_decision_schema_version")]
    pub schema_version: String,
    pub decision_id: String,
    pub workflow_id: String,
    pub idempotency_key: String,
    pub decision_point: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    pub gate: ValueGate,
    pub decision: String,
    #[serde(default)]
    pub candidates: Vec<CandidatePrediction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_candidate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_bps: Option<u32>,
    pub rationale: String,
    pub policy: PolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_arm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub applied: bool,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub hard_constraint_violations: Vec<String>,
    pub recorded_at: DateTime<Utc>,
}

impl GateDecisionReceipt {
    pub fn from_input(
        decision_id: String,
        workflow_id: String,
        input: GateDecisionInput,
        recorded_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: gate_decision_schema_version(),
            decision_id,
            workflow_id,
            idempotency_key: input.idempotency_key,
            decision_point: input.decision_point,
            task_id: input.task_id,
            run_id: input.run_id,
            lease_id: input.lease_id,
            input_hash: input.input_hash,
            gate: input.gate,
            decision: input.decision,
            candidates: input.candidates,
            selected_candidate_id: input.selected_candidate_id,
            confidence_bps: input.confidence_bps,
            rationale: input.rationale,
            policy: input.policy,
            cohort_id: input.cohort_id,
            experiment_id: input.experiment_id,
            experiment_arm: input.experiment_arm,
            seed: input.seed,
            applied: input.applied,
            evidence_refs: input.evidence_refs,
            hard_constraint_violations: input.hard_constraint_violations,
            recorded_at,
        }
    }

    pub fn matches_input(&self, input: &GateDecisionInput) -> bool {
        self.idempotency_key == input.idempotency_key
            && self.decision_point == input.decision_point
            && self.task_id == input.task_id
            && self.run_id == input.run_id
            && self.lease_id == input.lease_id
            && self.input_hash == input.input_hash
            && self.gate == input.gate
            && self.decision == input.decision
            && self.candidates == input.candidates
            && self.selected_candidate_id == input.selected_candidate_id
            && self.confidence_bps == input.confidence_bps
            && self.rationale == input.rationale
            && self.policy == input.policy
            && self.cohort_id == input.cohort_id
            && self.experiment_id == input.experiment_id
            && self.experiment_arm == input.experiment_arm
            && self.seed == input.seed
            && self.applied == input.applied
            && self.evidence_refs == input.evidence_refs
            && self.hard_constraint_violations == input.hard_constraint_violations
    }
}

pub fn validate_gate_decision_input(input: &GateDecisionInput) -> Vec<String> {
    let mut violations = Vec::new();
    require_text(&input.idempotency_key, "idempotency_key", &mut violations);
    require_text(&input.decision_point, "decision_point", &mut violations);
    require_text(&input.decision, "decision", &mut violations);
    require_text(&input.rationale, "rationale", &mut violations);
    validate_policy_ref(&input.policy, &mut violations);
    validate_optional_bps(input.confidence_bps, "confidence_bps", &mut violations);
    validate_optional_hash(input.input_hash.as_deref(), "input_hash", &mut violations);
    if input.applied {
        violations.push(
            "value-research v1 is observational; applied=true is not supported until a typed control-plane mutation consumes the decision"
                .to_string(),
        );
    }
    let allowed = allowed_gate_decisions(input.gate);
    if !allowed.contains(&input.decision.as_str()) && !input.decision.starts_with("custom:") {
        violations.push(format!(
            "decision `{}` is not valid for {:?}",
            input.decision, input.gate
        ));
    }
    if input.gate != ValueGate::Gate0ValueAdmission
        && input
            .task_id
            .as_deref()
            .is_none_or(|task| task.trim().is_empty())
    {
        violations.push("task_id is required for gates 1 through 4".to_string());
    }
    if input.gate == ValueGate::Gate0ValueAdmission
        && (input.task_id.is_some()
            || input.run_id.is_some()
            || input.lease_id.is_some()
            || input.input_hash.is_some())
    {
        violations.push(
            "gate 0 is workflow/cohort scoped and must omit task_id, run_id, lease_id and input_hash"
                .to_string(),
        );
    }
    if input.experiment_id.is_some() != input.experiment_arm.is_some() {
        violations.push(
            "experiment_id and experiment_arm must either both be set or both be omitted"
                .to_string(),
        );
    }
    if input.experiment_id.is_none() && input.seed.is_some() {
        violations.push("seed requires experiment_id and experiment_arm".to_string());
    }
    if input.experiment_id.is_some()
        && input
            .cohort_id
            .as_deref()
            .is_none_or(|cohort| cohort.trim().is_empty())
    {
        violations.push("cohort_id is required for experiment-linked decisions".to_string());
    }
    if input.experiment_id.is_some() && input.gate != ValueGate::Gate0ValueAdmission {
        if input
            .run_id
            .as_deref()
            .is_none_or(|run_id| run_id.trim().is_empty())
        {
            violations
                .push("run_id is required for experiment-linked gates 1 through 4".to_string());
        }
        if input
            .lease_id
            .as_deref()
            .is_none_or(|lease_id| lease_id.trim().is_empty())
        {
            violations
                .push("lease_id is required for experiment-linked gates 1 through 4".to_string());
        }
        if input.input_hash.is_none() {
            violations
                .push("input_hash is required for experiment-linked gates 1 through 4".to_string());
        }
    }
    validate_text_refs(&input.evidence_refs, "evidence_refs", &mut violations);
    validate_text_refs(
        &input.hard_constraint_violations,
        "hard_constraint_violations",
        &mut violations,
    );
    if input.decision.starts_with("custom:") {
        if input.decision["custom:".len()..].trim().is_empty() {
            violations.push("custom decision requires a non-empty name after custom:".to_string());
        }
        if !input
            .evidence_refs
            .iter()
            .any(|reference| !reference.trim().is_empty())
        {
            violations.push("custom decisions require at least one evidence_ref".to_string());
        }
        if input.policy.source != PolicySource::Addon {
            violations.push("custom decisions require policy.source=addon".to_string());
        }
    }
    let mut candidate_ids = BTreeSet::new();
    for candidate in &input.candidates {
        require_text(
            &candidate.candidate_id,
            "candidate.candidate_id",
            &mut violations,
        );
        require_text(
            &candidate.resource_type,
            "candidate.resource_type",
            &mut violations,
        );
        if !candidate_ids.insert(candidate.candidate_id.as_str()) {
            violations.push(format!(
                "candidate id {} is duplicated",
                candidate.candidate_id
            ));
        }
        validate_optional_bps(
            candidate.predicted_success_bps,
            "candidate.predicted_success_bps",
            &mut violations,
        );
        validate_optional_bps(
            candidate.predicted_risk_bps,
            "candidate.predicted_risk_bps",
            &mut violations,
        );
        validate_optional_bps(
            candidate.uncertainty_bps,
            "candidate.uncertainty_bps",
            &mut violations,
        );
        validate_optional_non_negative(
            candidate.predicted_direct_cost,
            "candidate.predicted_direct_cost",
            &mut violations,
        );
        validate_optional_finite(
            candidate.predicted_incremental_value,
            "candidate.predicted_incremental_value",
            &mut violations,
        );
        if (candidate.predicted_direct_cost.is_some()
            || candidate.predicted_incremental_value.is_some())
            && candidate
                .currency
                .as_deref()
                .is_none_or(|currency| currency.trim().is_empty())
        {
            violations.push("candidate.currency is required for monetary predictions".to_string());
        }
    }
    if let Some(selected) = &input.selected_candidate_id {
        if !candidate_ids.contains(selected.as_str()) {
            violations.push(format!(
                "selected_candidate_id {selected} is not present in candidates"
            ));
        }
    }
    if input.gate == ValueGate::Gate2ResourceSelection
        && input.decision == "select"
        && input.selected_candidate_id.is_none()
    {
        violations.push("gate 2 select decision requires selected_candidate_id".to_string());
    }
    violations
}

pub fn validate_gate_decision_receipt(receipt: &GateDecisionReceipt) -> Vec<String> {
    let mut violations = validate_gate_decision_input(&GateDecisionInput {
        idempotency_key: receipt.idempotency_key.clone(),
        decision_point: receipt.decision_point.clone(),
        task_id: receipt.task_id.clone(),
        run_id: receipt.run_id.clone(),
        lease_id: receipt.lease_id.clone(),
        input_hash: receipt.input_hash.clone(),
        gate: receipt.gate,
        decision: receipt.decision.clone(),
        candidates: receipt.candidates.clone(),
        selected_candidate_id: receipt.selected_candidate_id.clone(),
        confidence_bps: receipt.confidence_bps,
        rationale: receipt.rationale.clone(),
        policy: receipt.policy.clone(),
        cohort_id: receipt.cohort_id.clone(),
        experiment_id: receipt.experiment_id.clone(),
        experiment_arm: receipt.experiment_arm.clone(),
        seed: receipt.seed,
        applied: receipt.applied,
        evidence_refs: receipt.evidence_refs.clone(),
        hard_constraint_violations: receipt.hard_constraint_violations.clone(),
    });
    if receipt.schema_version != GATE_DECISION_SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {GATE_DECISION_SCHEMA_VERSION}"
        ));
    }
    require_text(&receipt.decision_id, "decision_id", &mut violations);
    require_text(&receipt.workflow_id, "workflow_id", &mut violations);
    violations
}

pub fn validate_gate_decision_against_value_contract(
    input: &GateDecisionInput,
    contract: &ValueContract,
) -> Vec<String> {
    let Some(contract_currency) = contract.currency.as_deref() else {
        return Vec::new();
    };
    input
        .candidates
        .iter()
        .filter_map(|candidate| {
            let currency = candidate.currency.as_deref()?;
            (!currency.eq_ignore_ascii_case(contract_currency)).then(|| {
                format!(
                    "candidate {} currency {currency} does not match value contract currency {contract_currency}",
                    candidate.candidate_id
                )
            })
        })
        .collect()
}

fn allowed_gate_decisions(gate: ValueGate) -> &'static [&'static str] {
    match gate {
        ValueGate::Gate0ValueAdmission => &[
            "admit",
            "defer",
            "negotiate",
            "reject",
            "abstained_missing_contract",
        ],
        ValueGate::Gate1InferenceNeed => &["deterministic", "generative", "mixed", "abstain"],
        ValueGate::Gate2ResourceSelection => &["select", "abstain", "fallback"],
        ValueGate::Gate3Assurance => &[
            "a0",
            "a1",
            "a2",
            "a3",
            "abstain",
            "abstained_missing_contract",
        ],
        ValueGate::Gate4Stopping => &["continue", "stop", "escalate", "abstained_missing_contract"],
    }
}
