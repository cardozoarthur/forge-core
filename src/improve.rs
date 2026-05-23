use crate::artifact::write_json_artifact;
use crate::graph::Workflow;
use crate::storage::ForgeStore;
use crate::validation::validate_workflow;
use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::fs;

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementProposal {
    pub workflow_id: String,
    pub status: String,
    pub auto_promoted: bool,
    pub promotion_gate: String,
    pub target_version: String,
    pub artifact_path: String,
    pub changelog_path: String,
    pub candidate_changes: Vec<String>,
    pub evolution_domains: Vec<String>,
    pub metrics_used: Vec<String>,
}

pub fn generate_improvement(
    store: &ForgeStore,
    workflow: &Workflow,
    target_version: Option<String>,
) -> Result<ImprovementProposal> {
    let validation = validate_workflow(workflow);
    let target_version = target_version.unwrap_or_else(|| "next".to_string());
    let relative_path = format!(
        "artifacts/{}/improvement-{}.json",
        workflow.id,
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    let changelog_path = format!("artifacts/{}/changelog-{}.md", workflow.id, target_version);
    let evolution_domains = vec![
        "task_structure".to_string(),
        "prompt_system".to_string(),
        "process_runtime".to_string(),
        "validation_governance".to_string(),
        "executor_policy".to_string(),
    ];
    let candidate_changes = vec![
        "evolve task structure with backlog state, subtasks, impediments, ownership and acceptance criteria".to_string(),
        "version prompt packets so executor instructions can be benchmarked and rolled back".to_string(),
        "add process-level workflow policies for Scrum/SAFe-style planning, blocked work and promotion readiness".to_string(),
        "generate a strong changelog for every version with validation evidence, risk notes and migration guidance".to_string(),
    ];
    let payload = json!({
        "workflow_id": workflow.id,
        "generated_at": Utc::now().to_rfc3339(),
        "status": "experiment_generated",
        "auto_promoted": false,
        "promotion_gate": "benchmark_and_validation_required",
        "target_version": target_version,
        "baseline_validation_status": validation.status,
        "evolution_domains": evolution_domains,
        "metrics_used": [
            "completion_rate",
            "recovery_rate",
            "context_efficiency",
            "validation_pass_rate",
            "execution_latency",
            "blocked_work_age",
            "impediment_resolution_rate",
            "prompt_regression_rate"
        ],
        "candidate_changes": candidate_changes,
        "safety": {
            "unrestricted_self_modification": false,
            "requires_validation_before_promotion": true
        }
    });
    let (_full_path, sha256) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
    let changelog_full_path = store.base_dir().join(&changelog_path);
    if let Some(parent) = changelog_full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &changelog_full_path,
        render_changelog(&target_version, workflow, &candidate_changes),
    )?;
    store.record_event(
        &workflow.id,
        "improvement_experiment_generated",
        &json!({
            "artifact_path": relative_path,
            "changelog_path": changelog_path,
            "sha256": sha256
        }),
    )?;

    Ok(ImprovementProposal {
        workflow_id: workflow.id.clone(),
        status: "experiment_generated".to_string(),
        auto_promoted: false,
        promotion_gate: "benchmark_and_validation_required".to_string(),
        target_version,
        artifact_path: relative_path,
        changelog_path,
        candidate_changes,
        evolution_domains,
        metrics_used: vec![
            "completion_rate".to_string(),
            "recovery_rate".to_string(),
            "context_efficiency".to_string(),
            "validation_pass_rate".to_string(),
            "execution_latency".to_string(),
            "blocked_work_age".to_string(),
            "impediment_resolution_rate".to_string(),
            "prompt_regression_rate".to_string(),
        ],
    })
}

fn render_changelog(
    target_version: &str,
    workflow: &Workflow,
    candidate_changes: &[String],
) -> String {
    format!(
        r#"# Forge Core {target_version} Changelog

## Summary

This candidate version evolves Forge structurally instead of only tuning prompts or changing executor choices.

## Task Structure

- Adds backlog state, subtasks, impediments, owner role and acceptance criteria to atomic tasks.
- Keeps work visible as operational backlog, not just a flat execution list.
- Supports blocked-work tracking needed for Scrum/SAFe-style governance.

## Prompt System

- Treats prompts as versioned execution packets that can be benchmarked.
- Keeps rollback possible when a prompt/process change reduces validation quality.

## Process Runtime

- Uses workflow `{}` as the baseline for the experiment.
- Keeps promotion blocked until benchmark and validation gates pass.

## Candidate Changes

{}

## Validation

- `auto_promoted=false`
- `promotion_gate=benchmark_and_validation_required`
- Requires fresh validation evidence before this candidate can become the active runtime behavior.
"#,
        workflow.id,
        candidate_changes
            .iter()
            .map(|change| format!("- {change}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
