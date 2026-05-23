use crate::artifact::write_json_artifact;
use crate::graph::Workflow;
use crate::storage::ForgeStore;
use crate::validation::validate_workflow;
use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementProposal {
    pub workflow_id: String,
    pub status: String,
    pub auto_promoted: bool,
    pub promotion_gate: String,
    pub artifact_path: String,
    pub candidate_changes: Vec<String>,
    pub metrics_used: Vec<String>,
}

pub fn generate_improvement(
    store: &ForgeStore,
    workflow: &Workflow,
) -> Result<ImprovementProposal> {
    let validation = validate_workflow(workflow);
    let relative_path = format!(
        "artifacts/{}/improvement-{}.json",
        workflow.id,
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    let payload = json!({
        "workflow_id": workflow.id,
        "generated_at": Utc::now().to_rfc3339(),
        "status": "experiment_generated",
        "auto_promoted": false,
        "promotion_gate": "benchmark_and_validation_required",
        "baseline_validation_status": validation.status,
        "metrics_used": [
            "completion_rate",
            "recovery_rate",
            "context_efficiency",
            "validation_pass_rate",
            "execution_latency"
        ],
        "candidate_changes": [
            "raise context priority for failed validation dependencies",
            "split high-risk tasks into smaller retryable units",
            "add benchmark comparison before promotion"
        ],
        "safety": {
            "unrestricted_self_modification": false,
            "requires_validation_before_promotion": true
        }
    });
    let (_full_path, sha256) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
    store.record_event(
        &workflow.id,
        "improvement_experiment_generated",
        &json!({ "artifact_path": relative_path, "sha256": sha256 }),
    )?;

    Ok(ImprovementProposal {
        workflow_id: workflow.id.clone(),
        status: "experiment_generated".to_string(),
        auto_promoted: false,
        promotion_gate: "benchmark_and_validation_required".to_string(),
        artifact_path: relative_path,
        candidate_changes: vec![
            "raise context priority for failed validation dependencies".to_string(),
            "split high-risk tasks into smaller retryable units".to_string(),
            "add benchmark comparison before promotion".to_string(),
        ],
        metrics_used: vec![
            "completion_rate".to_string(),
            "recovery_rate".to_string(),
            "context_efficiency".to_string(),
            "validation_pass_rate".to_string(),
            "execution_latency".to_string(),
        ],
    })
}
