use crate::graph::{ArtifactRecord, TaskStatus, Workflow};
use serde::Serialize;

pub const FINAL_COMPLETION_AUDIT_KIND: &str = "final_completion_audit";

const OUTCOME_STATUS_SCHEMA_VERSION: &str = "foundry.outcome_status.v1";
const OUTCOME_REGISTRY_SCHEMA_VERSION: &str = "foundry.outcome_registry_summary.v1";
const SUPPORT_ONLY_FINAL_AUDIT_BLOCK_REASON: &str = "Final completion audit cannot verify a user-facing outcome because the workflow declares only support deliverables.";

#[derive(Debug, Clone, Serialize)]
pub struct OutcomeStatusReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub reason: String,
    pub declared_deliverable_count: usize,
    pub user_facing_deliverable_count: usize,
    pub evidenced_user_facing_deliverable_count: usize,
    pub missing_user_facing_deliverable_count: usize,
    pub support_deliverable_count: usize,
    pub user_facing_artifact_count: usize,
    pub support_artifact_count: usize,
    pub final_completion_audit_required: bool,
    pub final_completion_audit_evaluated: bool,
    pub final_completion_audit_present: bool,
    pub final_completion_audit_passed: bool,
    pub final_completion_audit_block_reason: Option<String>,
    pub deliverables: Vec<OutcomeDeliverableStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutcomeDeliverableStatus {
    pub name: String,
    pub kind: String,
    pub status: String,
    pub artifact_refs: Vec<String>,
    pub completed_task_refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OutcomeEvidenceDeliverable {
    pub name: String,
    pub artifact_ref: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OutcomeRegistrySummary {
    pub schema_version: String,
    pub workflows: usize,
    pub support_only_workflows: usize,
    pub user_facing_workflows: usize,
    pub workflows_missing_user_evidence: usize,
    pub workflows_requiring_final_audit: usize,
    pub workflows_with_final_audit_passed: usize,
    pub user_facing_deliverables: usize,
    pub evidenced_user_facing_deliverables: usize,
}

impl OutcomeRegistrySummary {
    pub fn empty() -> Self {
        Self {
            schema_version: OUTCOME_REGISTRY_SCHEMA_VERSION.to_string(),
            ..Self::default()
        }
    }

    pub fn add(&mut self, status: &OutcomeStatusReport) {
        self.workflows += 1;
        if status.user_facing_deliverable_count == 0 {
            self.support_only_workflows += 1;
        } else {
            self.user_facing_workflows += 1;
        }
        if status.missing_user_facing_deliverable_count > 0 {
            self.workflows_missing_user_evidence += 1;
        }
        if status.final_completion_audit_required {
            self.workflows_requiring_final_audit += 1;
        }
        if status.final_completion_audit_passed {
            self.workflows_with_final_audit_passed += 1;
        }
        self.user_facing_deliverables += status.user_facing_deliverable_count;
        self.evidenced_user_facing_deliverables += status.evidenced_user_facing_deliverable_count;
    }
}

pub fn assess_workflow_outcome_metadata(workflow: &Workflow) -> OutcomeStatusReport {
    assess_workflow_outcome(workflow, false, None)
}

pub fn assess_workflow_outcome(
    workflow: &Workflow,
    final_completion_audit_evaluated: bool,
    final_completion_audit_block_reason: Option<&str>,
) -> OutcomeStatusReport {
    assess_workflow_outcome_with_evidence(
        workflow,
        final_completion_audit_evaluated,
        final_completion_audit_block_reason,
        &[],
    )
}

pub fn assess_workflow_outcome_with_evidence(
    workflow: &Workflow,
    final_completion_audit_evaluated: bool,
    final_completion_audit_block_reason: Option<&str>,
    evidence_deliverables: &[OutcomeEvidenceDeliverable],
) -> OutcomeStatusReport {
    let final_completion_audit_required = workflow_requires_final_outcome_audit(workflow);
    let final_completion_audit_present = workflow
        .artifacts
        .iter()
        .any(is_final_completion_audit_artifact);
    let final_completion_audit_artifact_passed = final_completion_audit_required
        && final_completion_audit_evaluated
        && final_completion_audit_block_reason.is_none();
    let (user_facing_artifact_count, support_artifact_count) =
        count_artifact_kinds(&workflow.artifacts);

    let mut deliverables = workflow
        .intent
        .deliverables
        .iter()
        .map(|deliverable| {
            deliverable_status(
                workflow,
                deliverable,
                final_completion_audit_artifact_passed,
                final_completion_audit_present,
            )
        })
        .collect::<Vec<_>>();
    merge_evidence_deliverables(&mut deliverables, evidence_deliverables);
    let user_facing_deliverable_count = deliverables
        .iter()
        .filter(|deliverable| deliverable.kind == "user_facing")
        .count();
    let support_deliverable_count = deliverables
        .iter()
        .filter(|deliverable| deliverable.kind == "support")
        .count();
    let evidenced_user_facing_deliverable_count = deliverables
        .iter()
        .filter(|deliverable| {
            deliverable.kind == "user_facing" && deliverable.status == "evidence_present"
        })
        .count();
    let missing_user_facing_deliverable_count =
        user_facing_deliverable_count.saturating_sub(evidenced_user_facing_deliverable_count);
    let final_completion_audit_passed =
        final_completion_audit_artifact_passed && user_facing_deliverable_count > 0;
    let effective_final_completion_audit_block_reason =
        if final_completion_audit_artifact_passed && user_facing_deliverable_count == 0 {
            Some(SUPPORT_ONLY_FINAL_AUDIT_BLOCK_REASON.to_string())
        } else {
            final_completion_audit_block_reason.map(str::to_string)
        };

    let all_tasks_completed = !workflow.tasks.is_empty()
        && workflow
            .tasks
            .iter()
            .all(|task| task.status == TaskStatus::Completed);
    let (status, action, reason) = if user_facing_deliverable_count == 0 {
        (
            "support_only".to_string(),
            "define_user_facing_deliverables".to_string(),
            if final_completion_audit_required {
                "Workflow has explicit final criteria, but its intent only declares support deliverables; final user outcome is not explicit."
                    .to_string()
            } else {
                "Workflow intent only declares support deliverables; final user outcome is not explicit."
                    .to_string()
            },
        )
    } else if final_completion_audit_passed {
        (
            "final_outcome_verified".to_string(),
            "none".to_string(),
            "User-facing deliverables have final completion audit evidence.".to_string(),
        )
    } else if missing_user_facing_deliverable_count > 0 {
        (
            "needs_user_delivery_evidence".to_string(),
            "produce_user_facing_deliverables".to_string(),
            "One or more user-facing deliverables do not have artifact or final-audit evidence."
                .to_string(),
        )
    } else if final_completion_audit_required && !final_completion_audit_evaluated {
        (
            "needs_final_outcome_audit_evaluation".to_string(),
            "run_request_drive_or_status_with_audit".to_string(),
            "User-facing deliverables have evidence metadata, but the final audit JSON has not been evaluated."
                .to_string(),
        )
    } else if final_completion_audit_required && final_completion_audit_block_reason.is_some() {
        (
            "needs_final_outcome_audit".to_string(),
            "attach_final_completion_audit".to_string(),
            final_completion_audit_block_reason
                .unwrap_or("Final completion audit is required.")
                .to_string(),
        )
    } else if all_tasks_completed {
        (
            "user_outcome_evidenced".to_string(),
            "run_final_validation".to_string(),
            "All workflow tasks are complete and user-facing deliverable evidence is present."
                .to_string(),
        )
    } else {
        (
            "in_progress_with_user_deliverables".to_string(),
            "continue_workflow".to_string(),
            "Workflow declares user-facing deliverables and still has incomplete tasks."
                .to_string(),
        )
    };

    OutcomeStatusReport {
        schema_version: OUTCOME_STATUS_SCHEMA_VERSION.to_string(),
        status,
        action,
        reason,
        declared_deliverable_count: deliverables.len(),
        user_facing_deliverable_count,
        evidenced_user_facing_deliverable_count,
        missing_user_facing_deliverable_count,
        support_deliverable_count,
        user_facing_artifact_count,
        support_artifact_count,
        final_completion_audit_required,
        final_completion_audit_evaluated,
        final_completion_audit_present,
        final_completion_audit_passed,
        final_completion_audit_block_reason: effective_final_completion_audit_block_reason,
        deliverables,
    }
}

fn merge_evidence_deliverables(
    deliverables: &mut Vec<OutcomeDeliverableStatus>,
    evidence_deliverables: &[OutcomeEvidenceDeliverable],
) {
    for evidence in evidence_deliverables {
        let name = evidence.name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(existing) = deliverables
            .iter_mut()
            .find(|deliverable| deliverable.name.eq_ignore_ascii_case(name))
        {
            existing.kind = "user_facing".to_string();
            existing.status = "evidence_present".to_string();
            if !existing
                .artifact_refs
                .iter()
                .any(|artifact_ref| artifact_ref == &evidence.artifact_ref)
            {
                existing.artifact_refs.push(evidence.artifact_ref.clone());
            }
            continue;
        }

        deliverables.push(OutcomeDeliverableStatus {
            name: name.to_string(),
            kind: "user_facing".to_string(),
            status: "evidence_present".to_string(),
            artifact_refs: vec![evidence.artifact_ref.clone()],
            completed_task_refs: Vec::new(),
        });
    }
}

pub fn workflow_requires_final_outcome_audit(workflow: &Workflow) -> bool {
    workflow_has_explicit_final_criteria(workflow)
        || workflow_has_user_facing_deliverables(workflow)
}

pub fn workflow_has_explicit_final_criteria(workflow: &Workflow) -> bool {
    let mut text = workflow.goal.clone();
    if let Some(initial_goal) = &workflow.initial_goal {
        text.push(' ');
        text.push_str(initial_goal);
    }
    let normalized = text.to_lowercase();
    [
        "critério final",
        "criterio final",
        "workflow só termina",
        "workflow so termina",
        "só termina quando",
        "so termina quando",
        "stopping rule",
        "definition of done",
        "only complete when",
        "only completes when",
        "only finish when",
        "only finishes when",
        "must only complete when",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub fn workflow_has_user_facing_deliverables(workflow: &Workflow) -> bool {
    workflow
        .intent
        .deliverables
        .iter()
        .any(|deliverable| deliverable_kind(deliverable) == "user_facing")
}

pub fn is_final_completion_audit_artifact(artifact: &ArtifactRecord) -> bool {
    let normalized_kind = artifact.kind.to_lowercase().replace('-', "_");
    let normalized_path = artifact.path.to_lowercase().replace('-', "_");
    normalized_kind == FINAL_COMPLETION_AUDIT_KIND
        || normalized_kind == "completion_audit"
        || normalized_path.contains("final_completion_audit")
}

fn deliverable_status(
    workflow: &Workflow,
    deliverable: &str,
    final_completion_audit_passed: bool,
    final_completion_audit_present: bool,
) -> OutcomeDeliverableStatus {
    let kind = deliverable_kind(deliverable).to_string();
    let artifact_refs = matching_artifact_refs(workflow, deliverable);
    let completed_task_refs = matching_completed_task_refs(workflow, deliverable);
    let status = if kind == "support" {
        "support_tracking"
    } else if !artifact_refs.is_empty() || final_completion_audit_passed {
        "evidence_present"
    } else if final_completion_audit_present {
        "final_audit_present_but_not_passed"
    } else if !completed_task_refs.is_empty() {
        "task_completed_needs_artifact_evidence"
    } else {
        "missing_evidence"
    };

    OutcomeDeliverableStatus {
        name: deliverable.to_string(),
        kind,
        status: status.to_string(),
        artifact_refs,
        completed_task_refs,
    }
}

fn matching_artifact_refs(workflow: &Workflow, deliverable: &str) -> Vec<String> {
    let tokens = significant_tokens(deliverable);
    workflow
        .artifacts
        .iter()
        .filter(|artifact| text_matches_tokens(&artifact_text(artifact), &tokens))
        .map(|artifact| artifact.path.clone())
        .collect()
}

fn matching_completed_task_refs(workflow: &Workflow, deliverable: &str) -> Vec<String> {
    let tokens = significant_tokens(deliverable);
    workflow
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .filter(|task| {
            let text = format!("{} {} {}", task.title, task.goal, task.expected_output);
            text_matches_tokens(&text, &tokens)
        })
        .map(|task| task.id.clone())
        .collect()
}

fn count_artifact_kinds(artifacts: &[ArtifactRecord]) -> (usize, usize) {
    let mut user_facing = 0;
    let mut support = 0;
    for artifact in artifacts {
        if artifact_is_support(artifact) {
            support += 1;
        } else {
            user_facing += 1;
        }
    }
    (user_facing, support)
}

fn artifact_is_support(artifact: &ArtifactRecord) -> bool {
    let text = artifact_text(artifact);
    [
        "auto_step_output",
        "execution_trace",
        "checkpoint",
        "patch_plan",
        "patch_apply",
        "patch_revert",
        "validation",
        "executor_response",
        "context",
        "self_evolution",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn artifact_text(artifact: &ArtifactRecord) -> String {
    format!("{} {}", artifact.kind, artifact.path)
        .to_lowercase()
        .replace('-', "_")
}

fn deliverable_kind(deliverable: &str) -> &'static str {
    let lower = deliverable.to_lowercase();
    let support = [
        "atomic task graph",
        "validation plan",
        "artifact manifest",
        "persistent runtime state",
        "interface contract",
        "foundry primitive promotion recommendation",
        "n8n primitive research catalog",
        "explicit goal loop node",
        "per-goal research subflow lineage",
        "buffered deadline improvement loop",
    ];
    if support.iter().any(|needle| lower.contains(needle)) {
        return "support";
    }

    let user_facing = [
        "pdf",
        "telegram",
        "report",
        "documentation",
        "pitch",
        "mvp",
        "backlog",
        "delivery",
        "deploy",
        "invoice",
        "checkout",
        "gateway",
        "sdk",
        "dashboard",
        "client",
        "server",
        "api",
        "compliance",
        "viability decision",
    ];
    if user_facing.iter().any(|needle| lower.contains(needle)) {
        return "user_facing";
    }

    "user_facing"
}

fn text_matches_tokens(text: &str, tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    let normalized = text.to_lowercase().replace('-', "_");
    tokens
        .iter()
        .any(|token| normalized.contains(&token.replace('-', "_")))
}

fn significant_tokens(input: &str) -> Vec<String> {
    let stop_words = [
        "and",
        "the",
        "with",
        "para",
        "com",
        "sem",
        "uma",
        "um",
        "artifact",
        "artifacts",
        "package",
        "payload",
        "final",
        "durable",
        "explicit",
        "native",
        "workflow",
        "foundry",
    ];
    input
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|token| token.chars().count() >= 3)
        .filter(|token| !stop_words.contains(&token.as_str()))
        .collect()
}
