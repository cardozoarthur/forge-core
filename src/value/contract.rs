use super::validation::{
    require_text, validate_optional_bps, validate_optional_non_negative, validate_policy_ref,
    validate_text_refs,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const VALUE_CONTRACT_SCHEMA_VERSION: &str = "foundry.value_contract.v1";

fn value_contract_schema_version() -> String {
    VALUE_CONTRACT_SCHEMA_VERSION.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueMeasurementMode {
    TerminalValue,
    GrossValueWithSeparateLosses,
    ConstrainedMulticriteria,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DelayFunctionKind {
    Linear,
    DeadlineLinear,
    Convex,
    Step,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicySource {
    Addon,
    Human,
    External,
    CoreBaseline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicyRef {
    pub id: String,
    pub version: String,
    pub source: PolicySource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DelayCostSpec {
    pub kind: DelayFunctionKind,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_per_second: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quadratic_rate_per_second_squared: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_model_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpportunityCostSpec {
    pub counterfactual: String,
    pub method_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_loss: Option<f64>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FailureCostSpec {
    pub method_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severe_probability_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_loss: Option<f64>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ValueConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_direct_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_human_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_success_probability_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_quality_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_severe_failure_probability_bps: Option<u32>,
    #[serde(default)]
    pub hard_constraints: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ValueAccountingBoundary {
    #[serde(default)]
    pub terminal_value_includes_delay: bool,
    #[serde(default)]
    pub terminal_value_includes_failure: bool,
    #[serde(default)]
    pub terminal_value_includes_opportunity: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ValueContract {
    #[serde(default = "value_contract_schema_version")]
    pub schema_version: String,
    pub value_class: String,
    pub measurement_mode: ValueMeasurementMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_cost: Option<DelayCostSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opportunity_cost: Option<OpportunityCostSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_cost: Option<FailureCostSpec>,
    pub severity: String,
    pub reversibility: String,
    #[serde(default)]
    pub constraints: ValueConstraints,
    #[serde(default)]
    pub accounting: ValueAccountingBoundary,
    pub policy: PolicyRef,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

pub fn validate_value_contract(contract: &ValueContract) -> Vec<String> {
    let mut violations = Vec::new();
    if contract.schema_version != VALUE_CONTRACT_SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {VALUE_CONTRACT_SCHEMA_VERSION}"
        ));
    }
    require_text(&contract.value_class, "value_class", &mut violations);
    validate_policy_ref(&contract.policy, &mut violations);
    validate_text_refs(&contract.evidence_refs, "evidence_refs", &mut violations);
    validate_text_refs(
        &contract.constraints.hard_constraints,
        "constraints.hard_constraints",
        &mut violations,
    );
    validate_optional_non_negative(contract.expected_value, "expected_value", &mut violations);
    if contract.measurement_mode != ValueMeasurementMode::ConstrainedMulticriteria
        && contract.expected_value.is_none()
    {
        violations.push("expected_value is required for a monetary measurement mode".to_string());
    }
    let has_monetary_amount = contract.expected_value.is_some()
        || contract.delay_cost.as_ref().is_some_and(|delay| {
            delay.rate_per_second.is_some()
                || delay.quadratic_rate_per_second_squared.is_some()
                || delay.fixed_penalty.is_some()
        })
        || contract
            .opportunity_cost
            .as_ref()
            .is_some_and(|opportunity| opportunity.estimated_loss.is_some())
        || contract
            .failure_cost
            .as_ref()
            .is_some_and(|failure| failure.estimated_loss.is_some())
        || contract.constraints.max_direct_cost.is_some()
        || contract.constraints.max_total_cost.is_some();
    if has_monetary_amount
        && contract
            .currency
            .as_deref()
            .is_none_or(|currency| currency.trim().is_empty())
    {
        violations.push(
            "currency is required whenever the value contract contains monetary amounts"
                .to_string(),
        );
    }
    if !matches!(
        contract.severity.as_str(),
        "low" | "medium" | "high" | "critical"
    ) {
        violations.push("severity must be low, medium, high or critical".to_string());
    }
    if !matches!(
        contract.reversibility.as_str(),
        "reversible" | "partially_reversible" | "irreversible"
    ) {
        violations.push(
            "reversibility must be reversible, partially_reversible or irreversible".to_string(),
        );
    }
    if contract.accounting.terminal_value_includes_delay && contract.delay_cost.is_some() {
        violations.push(
            "delay_cost must be omitted when terminal value already includes delay".to_string(),
        );
    }
    if contract.accounting.terminal_value_includes_failure && contract.failure_cost.is_some() {
        violations.push(
            "failure_cost must be omitted when terminal value already includes failure".to_string(),
        );
    }
    if contract.accounting.terminal_value_includes_opportunity
        && contract.opportunity_cost.is_some()
    {
        violations.push(
            "opportunity_cost must be omitted when terminal value already includes opportunity"
                .to_string(),
        );
    }
    if contract.measurement_mode == ValueMeasurementMode::GrossValueWithSeparateLosses
        && (contract.accounting.terminal_value_includes_delay
            || contract.accounting.terminal_value_includes_failure
            || contract.accounting.terminal_value_includes_opportunity)
    {
        violations.push(
            "gross_value_with_separate_losses cannot mark losses as embedded in terminal value"
                .to_string(),
        );
    }
    if let Some(delay) = &contract.delay_cost {
        validate_delay_cost(delay, contract.deadline, &mut violations);
    }
    if let Some(opportunity) = &contract.opportunity_cost {
        require_text(
            &opportunity.counterfactual,
            "opportunity_cost.counterfactual",
            &mut violations,
        );
        require_text(
            &opportunity.method_version,
            "opportunity_cost.method_version",
            &mut violations,
        );
        validate_optional_non_negative(
            opportunity.estimated_loss,
            "opportunity_cost.estimated_loss",
            &mut violations,
        );
        validate_text_refs(
            &opportunity.evidence_refs,
            "opportunity_cost.evidence_refs",
            &mut violations,
        );
        if opportunity.evidence_refs.is_empty() {
            violations.push(
                "opportunity_cost.evidence_refs requires at least one evidence reference"
                    .to_string(),
            );
        }
    }
    if let Some(failure) = &contract.failure_cost {
        require_text(
            &failure.method_version,
            "failure_cost.method_version",
            &mut violations,
        );
        validate_optional_bps(
            failure.probability_bps,
            "failure_cost.probability_bps",
            &mut violations,
        );
        validate_optional_bps(
            failure.severe_probability_bps,
            "failure_cost.severe_probability_bps",
            &mut violations,
        );
        validate_optional_non_negative(
            failure.estimated_loss,
            "failure_cost.estimated_loss",
            &mut violations,
        );
        validate_text_refs(
            &failure.evidence_refs,
            "failure_cost.evidence_refs",
            &mut violations,
        );
        if let (Some(severe), Some(any_failure)) =
            (failure.severe_probability_bps, failure.probability_bps)
        {
            if severe > any_failure {
                violations.push(
                    "failure_cost.severe_probability_bps cannot exceed failure_cost.probability_bps"
                        .to_string(),
                );
            }
        }
    }
    validate_optional_non_negative(
        contract.constraints.max_direct_cost,
        "constraints.max_direct_cost",
        &mut violations,
    );
    if let (Some(direct), Some(total)) = (
        contract.constraints.max_direct_cost,
        contract.constraints.max_total_cost,
    ) {
        if direct > total {
            violations.push(
                "constraints.max_direct_cost cannot exceed constraints.max_total_cost".to_string(),
            );
        }
    }
    validate_optional_non_negative(
        contract.constraints.max_total_cost,
        "constraints.max_total_cost",
        &mut violations,
    );
    validate_optional_bps(
        contract.constraints.min_success_probability_bps,
        "constraints.min_success_probability_bps",
        &mut violations,
    );
    validate_optional_bps(
        contract.constraints.min_quality_bps,
        "constraints.min_quality_bps",
        &mut violations,
    );
    validate_optional_bps(
        contract.constraints.max_severe_failure_probability_bps,
        "constraints.max_severe_failure_probability_bps",
        &mut violations,
    );
    violations
}

fn validate_delay_cost(
    delay: &DelayCostSpec,
    contract_deadline: Option<DateTime<Utc>>,
    violations: &mut Vec<String>,
) {
    require_text(&delay.version, "delay_cost.version", violations);
    validate_optional_non_negative(
        delay.rate_per_second,
        "delay_cost.rate_per_second",
        violations,
    );
    validate_optional_non_negative(
        delay.quadratic_rate_per_second_squared,
        "delay_cost.quadratic_rate_per_second_squared",
        violations,
    );
    validate_optional_non_negative(delay.fixed_penalty, "delay_cost.fixed_penalty", violations);
    match delay.kind {
        DelayFunctionKind::Linear => {
            if delay.rate_per_second.is_none() {
                violations.push("linear delay_cost requires rate_per_second".to_string());
            }
            reject_delay_fields(
                delay,
                &["threshold_at", "quadratic", "fixed", "external"],
                "linear",
                violations,
            );
        }
        DelayFunctionKind::DeadlineLinear => {
            if delay.rate_per_second.is_none() {
                violations.push("deadline_linear delay_cost requires rate_per_second".to_string());
            }
            if delay.threshold_at.is_none() && contract_deadline.is_none() {
                violations.push(
                    "deadline_linear delay_cost requires threshold_at or contract deadline"
                        .to_string(),
                );
            }
            reject_delay_fields(
                delay,
                &["quadratic", "fixed", "external"],
                "deadline_linear",
                violations,
            );
        }
        DelayFunctionKind::Convex => {
            if delay.quadratic_rate_per_second_squared.is_none() {
                violations.push(
                    "convex delay_cost requires quadratic_rate_per_second_squared".to_string(),
                );
            }
            if delay.threshold_at.is_none() && contract_deadline.is_none() {
                violations.push(
                    "convex delay_cost requires threshold_at or contract deadline".to_string(),
                );
            }
            reject_delay_fields(delay, &["fixed", "external"], "convex", violations);
        }
        DelayFunctionKind::Step => {
            if delay.fixed_penalty.is_none() {
                violations.push("step delay_cost requires fixed_penalty".to_string());
            }
            if delay.threshold_at.is_none() && contract_deadline.is_none() {
                violations
                    .push("step delay_cost requires threshold_at or contract deadline".to_string());
            }
            reject_delay_fields(
                delay,
                &["rate", "quadratic", "external"],
                "step",
                violations,
            );
        }
        DelayFunctionKind::External => {
            if delay
                .external_model_ref
                .as_deref()
                .is_none_or(|reference| reference.trim().is_empty())
            {
                violations.push("external delay_cost requires external_model_ref".to_string());
            }
            reject_delay_fields(
                delay,
                &["threshold_at", "rate", "quadratic", "fixed"],
                "external",
                violations,
            );
        }
    }
}

fn reject_delay_fields(
    delay: &DelayCostSpec,
    fields: &[&str],
    kind: &str,
    violations: &mut Vec<String>,
) {
    for field in fields {
        let present = match *field {
            "threshold_at" => delay.threshold_at.is_some(),
            "rate" => delay.rate_per_second.is_some(),
            "quadratic" => delay.quadratic_rate_per_second_squared.is_some(),
            "fixed" => delay.fixed_penalty.is_some(),
            "external" => delay.external_model_ref.is_some(),
            _ => false,
        };
        if present {
            violations.push(format!("{kind} delay_cost does not allow field {field}"));
        }
    }
}
