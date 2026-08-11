use super::contract::PolicyRef;

pub(super) fn validate_policy_ref(policy: &PolicyRef, violations: &mut Vec<String>) {
    require_text(&policy.id, "policy.id", violations);
    require_text(&policy.version, "policy.version", violations);
}

pub(super) fn require_text(value: &str, field: &str, violations: &mut Vec<String>) {
    if value.trim().is_empty() {
        violations.push(format!("{field} cannot be empty"));
    }
}

pub(super) fn validate_text_refs(values: &[String], field: &str, violations: &mut Vec<String>) {
    for (index, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            violations.push(format!("{field}[{index}] cannot be empty"));
        }
    }
}

pub(super) fn validate_optional_bps(value: Option<u32>, field: &str, violations: &mut Vec<String>) {
    if value.is_some_and(|value| value > 10_000) {
        violations.push(format!("{field} must be between 0 and 10000"));
    }
}

pub(super) fn validate_optional_non_negative(
    value: Option<f64>,
    field: &str,
    violations: &mut Vec<String>,
) {
    if let Some(value) = value {
        if !value.is_finite() || value < 0.0 {
            violations.push(format!("{field} must be finite and non-negative"));
        }
    }
}

pub(super) fn validate_optional_finite(
    value: Option<f64>,
    field: &str,
    violations: &mut Vec<String>,
) {
    if value.is_some_and(|value| !value.is_finite()) {
        violations.push(format!("{field} must be finite"));
    }
}

pub(super) fn validate_optional_hash(
    value: Option<&str>,
    field: &str,
    violations: &mut Vec<String>,
) {
    if let Some(value) = value {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            violations.push(format!("{field} must be a 64-character hexadecimal hash"));
        }
    }
}
