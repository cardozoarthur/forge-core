use crate::artifact::hex_sha256;
use crate::graph::Workflow;
use anyhow::{bail, Result};
use serde::Serialize;

const CONTEXT_SCHEMA_VERSION: &str = "forge.context.v1";
const ROUTING_POLICY: &str = "task_local_priority_budget_v1";

#[derive(Debug, Clone, Serialize)]
pub struct ContextPackage {
    pub schema_version: String,
    pub routing_policy: String,
    pub workflow_id: String,
    pub task_id: String,
    pub context_bytes: usize,
    pub context_sha256: String,
    pub included_sections: Vec<String>,
    pub omitted_sections: Vec<String>,
    pub shards: Vec<ContextShard>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextShard {
    pub section: String,
    pub source: String,
    pub priority: u8,
    pub included: bool,
    pub bytes: usize,
    pub content_sha256: String,
    pub summary: String,
}

struct ContextShardCandidate {
    section: &'static str,
    source: &'static str,
    priority: u8,
    content: String,
}

pub fn build_context_package(
    workflow: &Workflow,
    task_id: &str,
    budget: usize,
) -> Result<ContextPackage> {
    let task = workflow
        .tasks
        .iter()
        .find(|candidate| candidate.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?;
    if budget < 128 {
        bail!("context budget must be at least 128 bytes");
    }

    let mut candidates = vec![
        ContextShardCandidate {
            section: "local_objective",
            source: "task",
            priority: 100,
            content: format!(
                "Task {}: {}\nGoal: {}\nExpected output: {}\nDefinition of ready: {}\n",
                task.id,
                task.title,
                task.goal,
                task.expected_output,
                task.work_item.goal_validation.evidence_required.join("; ")
            ),
        },
        ContextShardCandidate {
            section: "context_requirements",
            source: "task",
            priority: 90,
            content: format!(
                "Context requirements: {}\n",
                task.context_requirements.join("; ")
            ),
        },
        ContextShardCandidate {
            section: "validation_rules",
            source: "validation",
            priority: 80,
            content: format!(
                "Validation rules: {}\n",
                serde_json::to_string(&task.validation_rules)?
            ),
        },
        ContextShardCandidate {
            section: "dependencies",
            source: "graph",
            priority: 70,
            content: format!("Dependencies: {}\n", task.dependencies.join(", ")),
        },
        ContextShardCandidate {
            section: "work_item",
            source: "task",
            priority: 60,
            content: format!(
                "Backlog state: {}\nImpediments: {}\nAcceptance criteria: {}\n",
                task.work_item.backlog_state,
                task.work_item.impediments.join("; "),
                task.work_item.acceptance_criteria.join("; ")
            ),
        },
        ContextShardCandidate {
            section: "constraints",
            source: "intent",
            priority: 40,
            content: format!("Constraints: {}\n", workflow.intent.constraints.join("; ")),
        },
    ];

    let mut content = String::new();
    let mut included_sections = Vec::new();
    let mut omitted_sections = Vec::new();
    let mut shards = Vec::new();

    candidates.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.section.cmp(right.section))
    });

    for candidate in candidates {
        let included = content.len() + candidate.content.len() <= budget;
        if included {
            content.push_str(&candidate.content);
            included_sections.push(candidate.section.to_string());
        } else {
            omitted_sections.push(candidate.section.to_string());
        }

        shards.push(ContextShard {
            section: candidate.section.to_string(),
            source: candidate.source.to_string(),
            priority: candidate.priority,
            included,
            bytes: candidate.content.len(),
            content_sha256: hex_sha256(candidate.content.as_bytes()),
            summary: summarize_shard(&candidate.content),
        });
    }

    Ok(ContextPackage {
        schema_version: CONTEXT_SCHEMA_VERSION.to_string(),
        routing_policy: ROUTING_POLICY.to_string(),
        workflow_id: workflow.id.clone(),
        task_id: task.id.clone(),
        context_bytes: content.len(),
        context_sha256: hex_sha256(content.as_bytes()),
        included_sections,
        omitted_sections,
        shards,
        content,
    })
}

fn summarize_shard(content: &str) -> String {
    content
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect()
}
