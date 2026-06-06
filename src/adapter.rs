use crate::artifact::hex_sha256;
use crate::graph::{task, ExecutorKind, TaskStatus, ValidationRule, Workflow, WorkflowRevision};
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use chrono::Utc;
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
    #[serde(default)]
    pub rework_items: Vec<ExecutorReworkItem>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorReworkItem {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub context_requirements: Vec<String>,
    #[serde(default)]
    pub expected_output: String,
    #[serde(default)]
    pub validation_rules: Vec<ValidationRule>,
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
    let mut workflow = store.load_workflow(workflow_id)?;
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
    let report =
        validate_executor_response(workflow_id, task_id, &response, response_sha256.clone());
    store.record_event(
        workflow_id,
        "executor_response_validated",
        &serde_json::to_value(&report)?,
    )?;
    if report.accepted {
        let promotion = promote_validated_task(&mut workflow, task_id, &response);
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "executor_response_promoted",
            &serde_json::json!({
                "task_id": task_id,
                "response_status": response.status,
                "response_sha256": response_sha256,
                "revision": promotion.revision,
                "generated_rework_task_ids": promotion.generated_rework_task_ids
            }),
        )?;
    }
    Ok(report)
}

struct PromotionResult {
    revision: u64,
    generated_rework_task_ids: Vec<String>,
}

fn promote_validated_task(
    workflow: &mut Workflow,
    task_id: &str,
    response: &ExecutorResponse,
) -> PromotionResult {
    let previous_workflow_status = workflow.status.clone();
    let Some(task_index) = workflow.tasks.iter().position(|task| task.id == task_id) else {
        return PromotionResult {
            revision: latest_revision(workflow),
            generated_rework_task_ids: Vec::new(),
        };
    };
    let previous_task_status = task_status_slug(&workflow.tasks[task_index].status);
    let base_dependencies = workflow.tasks[task_index].dependencies.clone();
    let expands_needs_retry = response.status == "needs_retry" && !response.rework_items.is_empty();

    {
        let task = &mut workflow.tasks[task_index];
        match response.status.as_str() {
            "completed" => {
                task.status = TaskStatus::Completed;
                task.work_item.backlog_state = "done".to_string();
                task.work_item.goal_validation.definitively_ready = true;
                for subtask in &mut task.work_item.subtasks {
                    subtask.status = TaskStatus::Completed;
                }
            }
            "failed" => {
                task.status = TaskStatus::Failed;
                task.work_item.backlog_state = "blocked".to_string();
                task.work_item.goal_validation.definitively_ready = false;
            }
            "needs_retry" => {
                if expands_needs_retry {
                    task.status = TaskStatus::Completed;
                    task.work_item.backlog_state = "done".to_string();
                    task.work_item.goal_validation.definitively_ready = true;
                    for subtask in &mut task.work_item.subtasks {
                        subtask.status = TaskStatus::Completed;
                    }
                } else {
                    task.status = TaskStatus::Pending;
                    task.work_item.backlog_state = "ready".to_string();
                    task.work_item.goal_validation.definitively_ready = false;
                }
            }
            _ => {}
        }
    }

    let mut generated_rework_task_ids = Vec::new();
    if expands_needs_retry {
        generated_rework_task_ids =
            append_rework_items_as_tasks(workflow, &base_dependencies, &response.rework_items);
    }

    let new_task_status = task_status_slug(&workflow.tasks[task_index].status);

    if workflow
        .tasks
        .iter()
        .all(|task| task.status == TaskStatus::Completed)
    {
        workflow.status = "completed".to_string();
    } else if workflow
        .tasks
        .iter()
        .any(|task| task.status == TaskStatus::Failed)
    {
        workflow.status = "failed".to_string();
    } else if workflow
        .tasks
        .iter()
        .any(|task| task.status == TaskStatus::Completed)
    {
        workflow.status = "running".to_string();
    }

    let mut summary = format!(
        "validated executor response promoted task {task_id} from {previous_task_status} to {new_task_status}; workflow status changed from {previous_workflow_status} to {}",
        workflow.status
    );
    if !generated_rework_task_ids.is_empty() {
        summary.push_str(&format!(
            "; generated rework tasks {}",
            generated_rework_task_ids.join(", ")
        ));
    }

    let revision = push_revision(
        &mut workflow.revisions,
        "executor_response",
        "executor_response_promoted",
        &summary,
    );
    PromotionResult {
        revision,
        generated_rework_task_ids,
    }
}

fn append_rework_items_as_tasks(
    workflow: &mut Workflow,
    base_dependencies: &[String],
    rework_items: &[ExecutorReworkItem],
) -> Vec<String> {
    let dependency_refs = base_dependencies
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut generated_task_ids = Vec::new();
    for item in rework_items {
        let task_id = format!("task-{:03}", workflow.tasks.len() + 1);
        let context_requirements = if item.context_requirements.is_empty() {
            vec!["executor needs_retry evidence".to_string()]
        } else {
            item.context_requirements.clone()
        };
        let context_refs = context_requirements
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let validation_rules = if item.validation_rules.is_empty() {
            vec![ValidationRule {
                kind: "evidence".to_string(),
                command: None,
                expected: "Attach executor response with validation evidence for this rework item."
                    .to_string(),
            }]
        } else {
            item.validation_rules.clone()
        };
        let expected_output = if item.expected_output.trim().is_empty() {
            item.goal.as_str()
        } else {
            item.expected_output.as_str()
        };
        let mut generated_task = task(
            &task_id,
            item.title.trim(),
            &dependency_refs,
            &context_refs,
            validation_rules,
            expected_output,
            (ExecutorKind::Ai, 0.25),
        );
        generated_task.goal = item.goal.trim().to_string();
        workflow.tasks.push(generated_task);
        generated_task_ids.push(task_id);
    }
    generated_task_ids
}

fn latest_revision(workflow: &Workflow) -> u64 {
    workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0)
}

fn push_revision(
    revisions: &mut Vec<WorkflowRevision>,
    origin: &str,
    change_type: &str,
    summary: &str,
) -> u64 {
    let revision = revisions.last().map(|item| item.revision + 1).unwrap_or(1);
    revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: change_type.to_string(),
        summary: summary.to_string(),
        created_at: Utc::now(),
    });
    revision
}

fn task_status_slug(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
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

    for (index, item) in response.rework_items.iter().enumerate() {
        if response.status != "needs_retry" {
            violations.push(violation(
                "rework_items_require_needs_retry",
                format!("rework_items[{index}]"),
                "structured rework items are only valid with status needs_retry",
            ));
        }
        if item.title.trim().is_empty() {
            violations.push(violation(
                "rework_item_title_required",
                format!("rework_items[{index}].title"),
                "rework items must include a task title",
            ));
        }
        if item.goal.trim().is_empty() {
            violations.push(violation(
                "rework_item_goal_required",
                format!("rework_items[{index}].goal"),
                "rework items must include an executable goal",
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
