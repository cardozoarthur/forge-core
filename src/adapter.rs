use crate::artifact::hex_sha256;
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const EXECUTOR_RESPONSE_SCHEMA_VERSION: &str = "forge.executor_response.v1";
const EXECUTOR_RESPONSE_VALIDATION_SCHEMA_VERSION: &str = "forge.executor_response_validation.v1";

#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorResponse {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub trace_ref: String,
    #[serde(default)]
    pub cost: ExecutorResponseCost,
    #[serde(default)]
    pub validation_evidence: Vec<ExecutorValidationEvidence>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExecutorResponseCost {
    #[serde(default)]
    pub estimated_usd: f64,
    #[serde(default)]
    pub tokens_in: i64,
    #[serde(default)]
    pub tokens_out: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorValidationEvidence {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorResponseValidationReport {
    pub schema_version: String,
    pub status: String,
    pub accepted: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub response_schema_version: String,
    pub response_status: String,
    pub response_sha256: String,
    pub validation_summary: ExecutorValidationSummary,
    pub violations: Vec<ExecutorResponseViolation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorValidationSummary {
    pub total: usize,
    pub passing: usize,
    pub failing: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorResponseViolation {
    pub code: String,
    pub field: String,
    pub message: String,
}

pub fn validate_executor_response_file(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    response_path: &Path,
) -> Result<ExecutorResponseValidationReport> {
    let workflow = store.load_workflow(workflow_id)?;
    workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .with_context(|| format!("task not found in workflow {workflow_id}: {task_id}"))?;

    let response_bytes = std::fs::read(response_path).with_context(|| {
        format!(
            "failed to read executor response {}",
            response_path.display()
        )
    })?;
    let response_sha256 = hex_sha256(&response_bytes);
    let response: ExecutorResponse = serde_json::from_slice(&response_bytes)
        .with_context(|| format!("invalid executor response JSON {}", response_path.display()))?;
    let report = validate_executor_response(workflow_id, task_id, &response, response_sha256);
    store.record_event(
        workflow_id,
        "executor_response_validated",
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

pub fn validate_executor_response(
    workflow_id: &str,
    task_id: &str,
    response: &ExecutorResponse,
    response_sha256: String,
) -> ExecutorResponseValidationReport {
    let mut violations = Vec::new();

    if response.schema_version != EXECUTOR_RESPONSE_SCHEMA_VERSION {
        violations.push(violation(
            "schema_version_unsupported",
            "schema_version",
            format!("executor response schema must be {EXECUTOR_RESPONSE_SCHEMA_VERSION}"),
        ));
    }

    if response.task_id != task_id {
        violations.push(violation(
            "task_id_mismatch",
            "task_id",
            format!("executor response task_id must match {task_id}"),
        ));
    }

    if !matches!(
        response.status.as_str(),
        "completed" | "failed" | "needs_retry"
    ) {
        violations.push(violation(
            "status_unsupported",
            "status",
            "status must be completed, failed or needs_retry",
        ));
    }

    if response.trace_ref.trim().is_empty() {
        violations.push(violation(
            "trace_ref_required",
            "trace_ref",
            "executor response must include a replayable trace reference",
        ));
    }

    if !response.cost.estimated_usd.is_finite() || response.cost.estimated_usd < 0.0 {
        violations.push(violation(
            "cost_estimated_usd_non_negative",
            "cost.estimated_usd",
            "estimated executor cost must be finite and non-negative",
        ));
    }

    if response.cost.tokens_in < 0 {
        violations.push(violation(
            "cost_tokens_in_non_negative",
            "cost.tokens_in",
            "input token count must be non-negative",
        ));
    }

    if response.cost.tokens_out < 0 {
        violations.push(violation(
            "cost_tokens_out_non_negative",
            "cost.tokens_out",
            "output token count must be non-negative",
        ));
    }

    for (index, evidence) in response.validation_evidence.iter().enumerate() {
        if evidence.command.trim().is_empty() {
            violations.push(violation(
                "validation_command_required",
                format!("validation_evidence[{index}].command"),
                "validation evidence must name the command or gate that ran",
            ));
        }
    }

    let validation_summary = summarize_validation_evidence(&response.validation_evidence);
    if response.status == "completed" && validation_summary.passing == 0 {
        violations.push(violation(
            "completed_requires_passing_validation_evidence",
            "validation_evidence",
            "completed executor responses require at least one passing validation evidence item",
        ));
    }

    let accepted = violations.is_empty();
    ExecutorResponseValidationReport {
        schema_version: EXECUTOR_RESPONSE_VALIDATION_SCHEMA_VERSION.to_string(),
        status: if accepted { "accepted" } else { "rejected" }.to_string(),
        accepted,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        response_schema_version: response.schema_version.clone(),
        response_status: response.status.clone(),
        response_sha256,
        validation_summary,
        violations,
    }
}

fn summarize_validation_evidence(
    evidence: &[ExecutorValidationEvidence],
) -> ExecutorValidationSummary {
    let passing = evidence.iter().filter(|item| item.exit_code == 0).count();
    ExecutorValidationSummary {
        total: evidence.len(),
        passing,
        failing: evidence.len().saturating_sub(passing),
    }
}

fn violation(
    code: impl Into<String>,
    field: impl Into<String>,
    message: impl Into<String>,
) -> ExecutorResponseViolation {
    ExecutorResponseViolation {
        code: code.into(),
        field: field.into(),
        message: message.into(),
    }
}
