use super::contract::{PolicyRef, ValueContract};
use super::validation::{
    require_text, validate_optional_bps, validate_optional_finite, validate_optional_hash,
    validate_optional_non_negative, validate_policy_ref, validate_text_refs,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const OUTCOME_CONTRACT_SCHEMA_VERSION: &str = "foundry.outcome_contract.v1";

fn outcome_contract_schema_version() -> String {
    OUTCOME_CONTRACT_SCHEMA_VERSION.to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeMeasurementStatus {
    Observed,
    Simulated,
    Estimated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Accepted,
    Rejected,
    Partial,
    Modeled,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeMetric {
    ProcessQualityBps,
    ArtifactQualityBps,
    QualityInUseBps,
    DirectCost,
    ProcessCost,
    AssuranceCost,
    InternalFailureCost,
    ExternalFailureCost,
    DelayCost,
    OpportunityCost,
    RealizedValue,
    ServiceTimeMs,
    QueueTimeMs,
    WaitTimeMs,
    HumanTimeMs,
    CapacityUnits,
    EscapedDefect,
    Accepted,
}

impl OutcomeMetric {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProcessQualityBps => "process_quality_bps",
            Self::ArtifactQualityBps => "artifact_quality_bps",
            Self::QualityInUseBps => "quality_in_use_bps",
            Self::DirectCost => "direct_cost",
            Self::ProcessCost => "process_cost",
            Self::AssuranceCost => "assurance_cost",
            Self::InternalFailureCost => "internal_failure_cost",
            Self::ExternalFailureCost => "external_failure_cost",
            Self::DelayCost => "delay_cost",
            Self::OpportunityCost => "opportunity_cost",
            Self::RealizedValue => "realized_value",
            Self::ServiceTimeMs => "service_time_ms",
            Self::QueueTimeMs => "queue_time_ms",
            Self::WaitTimeMs => "wait_time_ms",
            Self::HumanTimeMs => "human_time_ms",
            Self::CapacityUnits => "capacity_units",
            Self::EscapedDefect => "escaped_defect",
            Self::Accepted => "accepted",
        }
    }

    pub fn is_measured(self, input: &OutcomeContractInput) -> bool {
        match self {
            Self::ProcessQualityBps => input.process_quality_bps.is_some(),
            Self::ArtifactQualityBps => input.artifact_quality_bps.is_some(),
            Self::QualityInUseBps => input.quality_in_use_bps.is_some(),
            Self::DirectCost => input.direct_cost.is_some(),
            Self::ProcessCost => input.process_cost.is_some(),
            Self::AssuranceCost => input.assurance_cost.is_some(),
            Self::InternalFailureCost => input.internal_failure_cost.is_some(),
            Self::ExternalFailureCost => input.external_failure_cost.is_some(),
            Self::DelayCost => input.delay_cost.is_some(),
            Self::OpportunityCost => input.opportunity_cost.is_some(),
            Self::RealizedValue => input.realized_value.is_some(),
            Self::ServiceTimeMs => input.service_time_ms.is_some(),
            Self::QueueTimeMs => input.queue_time_ms.is_some(),
            Self::WaitTimeMs => input.wait_time_ms.is_some(),
            Self::HumanTimeMs => input.human_time_ms.is_some(),
            Self::CapacityUnits => input.capacity_units.is_some(),
            Self::EscapedDefect => input.escaped_defect.is_some(),
            Self::Accepted => input.accepted.is_some(),
        }
    }

    pub fn is_monetary(self) -> bool {
        matches!(
            self,
            Self::DirectCost
                | Self::ProcessCost
                | Self::AssuranceCost
                | Self::InternalFailureCost
                | Self::ExternalFailureCost
                | Self::DelayCost
                | Self::OpportunityCost
                | Self::RealizedValue
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutcomeContractInput {
    pub idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_receipt_sha256: Option<String>,
    pub measurement_status: OutcomeMeasurementStatus,
    pub status: OutcomeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_quality_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_quality_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_in_use_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assurance_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_failure_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_failure_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opportunity_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opportunity_cost_counterfactual: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opportunity_cost_method_version: Option<String>,
    #[serde(default)]
    pub opportunity_cost_evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_value_method_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_time_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_time_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_time_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_time_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_units: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escaped_defect: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<String>,
    #[serde(default)]
    pub gate_decision_ids: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub metric_provenance: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluated_policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_executed_policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_arm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutcomeContract {
    #[serde(default = "outcome_contract_schema_version")]
    pub schema_version: String,
    pub outcome_id: String,
    pub workflow_id: String,
    #[serde(flatten)]
    pub measurement: OutcomeContractInput,
    pub recorded_at: DateTime<Utc>,
}

pub fn validate_outcome_input(input: &OutcomeContractInput) -> Vec<String> {
    let mut violations = Vec::new();
    require_text(&input.idempotency_key, "idempotency_key", &mut violations);
    validate_optional_hash(input.input_hash.as_deref(), "input_hash", &mut violations);
    validate_optional_hash(input.output_hash.as_deref(), "output_hash", &mut violations);
    validate_optional_hash(
        input.execution_receipt_sha256.as_deref(),
        "execution_receipt_sha256",
        &mut violations,
    );
    for (value, field) in [
        (input.process_quality_bps, "process_quality_bps"),
        (input.artifact_quality_bps, "artifact_quality_bps"),
        (input.quality_in_use_bps, "quality_in_use_bps"),
    ] {
        validate_optional_bps(value, field, &mut violations);
    }
    for (value, field) in [
        (input.direct_cost, "direct_cost"),
        (input.process_cost, "process_cost"),
        (input.assurance_cost, "assurance_cost"),
        (input.internal_failure_cost, "internal_failure_cost"),
        (input.external_failure_cost, "external_failure_cost"),
        (input.delay_cost, "delay_cost"),
        (input.opportunity_cost, "opportunity_cost"),
        (input.capacity_units, "capacity_units"),
    ] {
        validate_optional_non_negative(value, field, &mut violations);
    }
    validate_optional_finite(input.realized_value, "realized_value", &mut violations);
    let has_monetary_measurement = input.realized_value.is_some()
        || input.direct_cost.is_some()
        || input.process_cost.is_some()
        || input.assurance_cost.is_some()
        || input.internal_failure_cost.is_some()
        || input.external_failure_cost.is_some()
        || input.delay_cost.is_some()
        || input.opportunity_cost.is_some();
    if has_monetary_measurement
        && input
            .currency
            .as_deref()
            .is_none_or(|currency| currency.trim().is_empty())
    {
        violations.push("currency is required for monetary outcome measurements".to_string());
    }
    if input.realized_value.is_some()
        && input
            .realized_value_method_version
            .as_deref()
            .is_none_or(|version| version.trim().is_empty())
    {
        violations.push(
            "realized_value_method_version is required when realized_value is present".to_string(),
        );
    }
    if input.opportunity_cost.is_some() {
        if input
            .opportunity_cost_counterfactual
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            violations.push(
                "opportunity_cost_counterfactual is required when opportunity_cost is present"
                    .to_string(),
            );
        }
        if input
            .opportunity_cost_method_version
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            violations.push(
                "opportunity_cost_method_version is required when opportunity_cost is present"
                    .to_string(),
            );
        }
        if !input
            .opportunity_cost_evidence_refs
            .iter()
            .any(|reference| !reference.trim().is_empty())
        {
            violations.push(
                "opportunity_cost_evidence_refs requires at least one evidence reference"
                    .to_string(),
            );
        }
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
    if input.experiment_id.is_some() {
        if input
            .cohort_id
            .as_deref()
            .is_none_or(|cohort| cohort.trim().is_empty())
        {
            violations.push("cohort_id is required for experiment-linked outcomes".to_string());
        }
        if input
            .run_id
            .as_deref()
            .is_none_or(|run_id| run_id.trim().is_empty())
        {
            violations.push("run_id is required for experiment-linked outcomes".to_string());
        }
        if input
            .lease_id
            .as_deref()
            .is_none_or(|lease_id| lease_id.trim().is_empty())
        {
            violations.push("lease_id is required for experiment-linked outcomes".to_string());
        }
        if input.input_hash.is_none() {
            violations.push("input_hash is required for experiment-linked outcomes".to_string());
        }
    }
    if let Some(policy) = &input.evaluated_policy {
        validate_policy_ref(policy, &mut violations);
    }
    if let Some(policy) = &input.reported_executed_policy {
        validate_policy_ref(policy, &mut violations);
    }
    if input.experiment_id.is_some() && input.evaluated_policy.is_none() {
        violations.push(
            "evaluated_policy is required when outcome telemetry is linked to an experiment"
                .to_string(),
        );
    }
    if input.measurement_status == OutcomeMeasurementStatus::Observed
        && input.reported_executed_policy.is_none()
    {
        violations.push("observed outcomes require reported_executed_policy".to_string());
    }
    if input.measurement_status == OutcomeMeasurementStatus::Observed {
        for (field, value) in [
            ("task_id", input.task_id.as_deref()),
            ("run_id", input.run_id.as_deref()),
            ("lease_id", input.lease_id.as_deref()),
        ] {
            if value.is_none_or(|value| value.trim().is_empty()) {
                violations.push(format!("{field} is required for observed outcomes"));
            }
        }
        if input.input_hash.is_none() {
            violations.push("input_hash is required for observed outcomes".to_string());
        }
        if input.output_hash.is_none() {
            violations.push("output_hash is required for observed outcomes".to_string());
        }
        if input.execution_receipt_sha256.is_none() {
            violations
                .push("execution_receipt_sha256 is required for observed outcomes".to_string());
        }
    } else {
        if input.execution_receipt_sha256.is_some() {
            violations
                .push("execution_receipt_sha256 is only valid for observed outcomes".to_string());
        }
        if input.reported_executed_policy.is_some() {
            violations
                .push("reported_executed_policy is only valid for observed outcomes".to_string());
        }
    }
    validate_text_refs(
        &input.gate_decision_ids,
        "gate_decision_ids",
        &mut violations,
    );
    let mut unique_gate_decision_ids = std::collections::BTreeSet::new();
    if input
        .gate_decision_ids
        .iter()
        .any(|decision_id| !unique_gate_decision_ids.insert(decision_id.as_str()))
    {
        violations.push("gate_decision_ids cannot contain duplicates".to_string());
    }
    validate_text_refs(&input.evidence_refs, "evidence_refs", &mut violations);
    validate_text_refs(&input.artifact_ids, "artifact_ids", &mut violations);
    validate_text_refs(
        &input.opportunity_cost_evidence_refs,
        "opportunity_cost_evidence_refs",
        &mut violations,
    );
    for (metric, provenance) in &input.metric_provenance {
        if metric.trim().is_empty() || provenance.trim().is_empty() {
            violations.push(
                "metric_provenance requires non-empty metric names and provenance".to_string(),
            );
            break;
        }
    }
    if input.measurement_status == OutcomeMeasurementStatus::Observed
        && !input
            .evidence_refs
            .iter()
            .any(|reference| !reference.trim().is_empty())
    {
        violations.push("observed outcomes require at least one evidence_ref".to_string());
    }
    match (input.status, input.accepted) {
        (OutcomeStatus::Accepted, Some(false)) => violations
            .push("outcome status indicates acceptance/success but accepted=false".to_string()),
        (OutcomeStatus::Rejected, Some(true)) => violations
            .push("outcome status indicates rejection/failure but accepted=true".to_string()),
        _ => {}
    }
    violations
}

pub fn validate_outcome_against_value_contract(
    input: &OutcomeContractInput,
    contract: &ValueContract,
) -> Vec<String> {
    let mut violations = Vec::new();
    if let (Some(outcome_currency), Some(contract_currency)) =
        (input.currency.as_deref(), contract.currency.as_deref())
    {
        if !outcome_currency.eq_ignore_ascii_case(contract_currency) {
            violations.push(format!(
                "outcome currency {outcome_currency} does not match value contract currency {contract_currency}"
            ));
        }
    }
    if contract.accounting.terminal_value_includes_delay && input.delay_cost.is_some() {
        violations.push(
            "delay_cost cannot be recorded separately when terminal value already includes delay"
                .to_string(),
        );
    }
    if contract.accounting.terminal_value_includes_opportunity && input.opportunity_cost.is_some() {
        violations.push(
            "opportunity_cost cannot be recorded separately when terminal value already includes opportunity loss"
                .to_string(),
        );
    }
    if contract.accounting.terminal_value_includes_failure
        && (input.internal_failure_cost.is_some() || input.external_failure_cost.is_some())
    {
        violations.push(
            "failure costs cannot be recorded separately when terminal value already includes failure loss"
                .to_string(),
        );
    }
    violations
}

pub fn validate_outcome_endpoints(
    input: &OutcomeContractInput,
    primary_endpoint: OutcomeMetric,
    secondary_endpoints: &[OutcomeMetric],
) -> Vec<String> {
    let mut violations = Vec::new();
    for endpoint in std::iter::once(primary_endpoint).chain(secondary_endpoints.iter().copied()) {
        if !endpoint.is_measured(input) {
            violations.push(format!(
                "registered endpoint {} is not measured by the outcome",
                endpoint.as_str()
            ));
        }
        if input
            .metric_provenance
            .get(endpoint.as_str())
            .is_none_or(|provenance| provenance.trim().is_empty())
        {
            violations.push(format!(
                "registered endpoint {} requires metric_provenance",
                endpoint.as_str()
            ));
        }
    }
    violations
}

pub fn validate_outcome_execution_policy(
    input: &OutcomeContractInput,
    assignment_policy: &PolicyRef,
    shadow_or_holdout: bool,
) -> Vec<String> {
    if input.measurement_status != OutcomeMeasurementStatus::Observed {
        return Vec::new();
    }
    match input.reported_executed_policy.as_ref() {
        Some(executed_policy) if shadow_or_holdout && executed_policy == assignment_policy => vec![
            "observed shadow/holdout outcome must distinguish reported_executed_policy from evaluated_policy"
                .to_string(),
        ],
        Some(executed_policy) if !shadow_or_holdout && executed_policy != assignment_policy => vec![
            "observed non-shadow outcome reported_executed_policy must match the experiment assignment policy"
                .to_string(),
        ],
        _ => Vec::new(),
    }
}

pub fn validate_outcome_contract(outcome: &OutcomeContract) -> Vec<String> {
    let mut violations = validate_outcome_input(&outcome.measurement);
    if outcome.schema_version != OUTCOME_CONTRACT_SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {OUTCOME_CONTRACT_SCHEMA_VERSION}"
        ));
    }
    require_text(&outcome.outcome_id, "outcome_id", &mut violations);
    require_text(&outcome.workflow_id, "workflow_id", &mut violations);
    violations
}
