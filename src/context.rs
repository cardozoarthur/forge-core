use crate::artifact::hex_sha256;
use crate::graph::{PersonaRoutingSpec, Workflow};
use anyhow::{bail, Result};
use serde::Serialize;

const CONTEXT_SCHEMA_VERSION: &str = "forge.context.v3";
const ROUTING_POLICY: &str = "task_local_revisioned_persona_budget_v3";

#[derive(Debug, Clone, Serialize)]
pub struct ContextPackage {
    pub schema_version: String,
    pub routing_policy: String,
    pub workflow_id: String,
    pub task_id: String,
    pub workflow_revision: u64,
    pub artifact_count: usize,
    pub lineage: ContextLineage,
    pub persona: Option<PersonaRoutingSpec>,
    pub context_bytes: usize,
    pub context_sha256: String,
    pub included_sections: Vec<String>,
    pub omitted_sections: Vec<String>,
    pub shards: Vec<ContextShard>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextLineage {
    pub workflow_revision: u64,
    pub workflow_goal_sha256: String,
    pub task_goal_sha256: String,
    pub artifact_manifest_sha256: String,
    pub artifact_count: usize,
    pub persona_mode_sha256: String,
    pub persona_scope: String,
    pub revision_sources: Vec<String>,
    pub lineage_sha256: String,
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

#[derive(Serialize)]
struct ContextLineageSeed {
    workflow_id: String,
    task_id: String,
    workflow_revision: u64,
    workflow_goal_sha256: String,
    task_goal_sha256: String,
    artifact_manifest_sha256: String,
    artifact_count: usize,
    persona_mode_sha256: String,
    persona_scope: String,
    revision_sources: Vec<String>,
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

    let workflow_revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0);
    let artifact_manifest = serde_json::to_string(&workflow.artifacts)?;
    let revision_sources = workflow
        .revisions
        .iter()
        .map(|revision| revision.origin.clone())
        .collect::<Vec<_>>();
    let persona = task.persona.clone();
    let lineage = build_lineage(
        workflow,
        task_id,
        &task.goal,
        persona.as_ref(),
        workflow_revision,
        revision_sources,
        &artifact_manifest,
    )?;

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
            section: "workflow_goal",
            source: "workflow",
            priority: 95,
            content: format!(
                "Current workflow goal: {}\nInitial workflow goal: {}\nWorkflow revision: {}\nArtifact count: {}\n",
                workflow.goal,
                workflow
                    .initial_goal
                    .as_deref()
                    .unwrap_or(workflow.goal.as_str()),
                workflow_revision,
                workflow.artifacts.len()
            ),
        },
        ContextShardCandidate {
            section: "persona_routing",
            source: "persona",
            priority: 92,
            content: persona
                .as_ref()
                .map(render_persona_context)
                .unwrap_or_default(),
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
        if candidate.content.is_empty() {
            continue;
        }
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
        workflow_revision,
        artifact_count: workflow.artifacts.len(),
        lineage,
        persona,
        context_bytes: content.len(),
        context_sha256: hex_sha256(content.as_bytes()),
        included_sections,
        omitted_sections,
        shards,
        content,
    })
}

fn build_lineage(
    workflow: &Workflow,
    task_id: &str,
    task_goal: &str,
    persona: Option<&PersonaRoutingSpec>,
    workflow_revision: u64,
    revision_sources: Vec<String>,
    artifact_manifest: &str,
) -> Result<ContextLineage> {
    let persona_mode = persona
        .map(|persona| persona.mode.as_str())
        .unwrap_or("none");
    let persona_scope = persona
        .map(|persona| persona.scope.clone())
        .unwrap_or_else(|| "none".to_string());
    let seed = ContextLineageSeed {
        workflow_id: workflow.id.clone(),
        task_id: task_id.to_string(),
        workflow_revision,
        workflow_goal_sha256: hex_sha256(workflow.goal.as_bytes()),
        task_goal_sha256: hex_sha256(task_goal.as_bytes()),
        artifact_manifest_sha256: hex_sha256(artifact_manifest.as_bytes()),
        artifact_count: workflow.artifacts.len(),
        persona_mode_sha256: hex_sha256(persona_mode.as_bytes()),
        persona_scope,
        revision_sources,
    };
    let lineage_sha256 = hex_sha256(serde_json::to_string(&seed)?.as_bytes());
    Ok(ContextLineage {
        workflow_revision: seed.workflow_revision,
        workflow_goal_sha256: seed.workflow_goal_sha256,
        task_goal_sha256: seed.task_goal_sha256,
        artifact_manifest_sha256: seed.artifact_manifest_sha256,
        artifact_count: seed.artifact_count,
        persona_mode_sha256: seed.persona_mode_sha256,
        persona_scope: seed.persona_scope,
        revision_sources: seed.revision_sources,
        lineage_sha256,
    })
}

fn render_persona_context(persona: &PersonaRoutingSpec) -> String {
    format!(
        "Persona mode: {}\nPersona scope: {}\nInstruction source: {}\nVoice: {}\nTone: {}\nValidation gate: {}\nSource models: {}\nAuditable: {}\n",
        persona.mode,
        persona.scope,
        persona.instruction_source,
        persona.voice,
        persona.tone,
        persona.validation_gate,
        persona.source_models.join(", "),
        persona.auditable
    )
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
