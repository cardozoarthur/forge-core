use crate::artifact::hex_sha256;
use crate::checkpoint::TaskCheckpoint;
use crate::graph::{
    AtomicTask, ChildSubflowRef, ExecutionPolicySpec, ExecutorKind, PersonaRoutingSpec, Workflow,
};
use anyhow::{bail, Result};
use serde::Serialize;

const CONTEXT_SCHEMA_VERSION: &str = "forge.context.v9";
const ROUTING_POLICY: &str =
    "task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_budget_decisions_v9";
const DETERMINISTIC_CONTEXT_BUDGET: usize = 640;
const NOTIFICATION_CONTEXT_BUDGET: usize = 900;
const ALL_CONTEXT_SECTIONS: &[&str] = &[
    "local_objective",
    "workflow_goal",
    "persona_routing",
    "execution_policy",
    "child_subflows",
    "checkpoint",
    "context_requirements",
    "validation_rules",
    "dependencies",
    "work_item",
    "constraints",
];
const NO_AI_CONTEXT_SECTIONS: &[&str] = &[
    "local_objective",
    "workflow_goal",
    "execution_policy",
    "child_subflows",
    "checkpoint",
    "context_requirements",
    "validation_rules",
    "dependencies",
];
const NOTIFICATION_CONTEXT_SECTIONS: &[&str] = &[
    "local_objective",
    "workflow_goal",
    "persona_routing",
    "execution_policy",
    "child_subflows",
    "checkpoint",
    "context_requirements",
    "validation_rules",
    "dependencies",
];

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
    pub executor_profile: ContextExecutorProfile,
    pub execution_policy: ExecutionPolicySpec,
    pub child_subflow_count: usize,
    pub child_subflows: Vec<ChildSubflowRef>,
    pub latest_checkpoint: Option<TaskCheckpoint>,
    pub resume_context_status: String,
    pub resume_context_reason: String,
    pub requested_budget: usize,
    pub effective_budget: usize,
    pub context_bytes: usize,
    pub context_sha256: String,
    pub included_sections: Vec<String>,
    pub omitted_sections: Vec<String>,
    pub profile_omitted_sections: Vec<String>,
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
    pub compressed: bool,
    pub profile_excluded: bool,
    pub routing_decision: String,
    pub decision_reason: String,
    pub bytes: usize,
    pub original_bytes: usize,
    pub content_sha256: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextExecutorProfile {
    pub id: String,
    pub executor: String,
    pub reasoning_allowed: bool,
    pub deterministic: bool,
    pub max_context_bytes: Option<usize>,
    pub allowed_sections: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct ExecutorContextProfile {
    id: &'static str,
    reasoning_allowed: bool,
    deterministic: bool,
    max_context_bytes: Option<usize>,
    allowed_sections: &'static [&'static str],
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
    build_context_package_with_checkpoint(workflow, task_id, budget, None)
}

pub fn build_context_package_with_checkpoint(
    workflow: &Workflow,
    task_id: &str,
    budget: usize,
    latest_checkpoint: Option<TaskCheckpoint>,
) -> Result<ContextPackage> {
    let task = workflow
        .tasks
        .iter()
        .find(|candidate| candidate.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?;
    if budget < 128 {
        bail!("context budget must be at least 128 bytes");
    }

    let profile = executor_context_profile(task);
    let effective_budget = profile
        .max_context_bytes
        .map(|max_bytes| budget.min(max_bytes))
        .unwrap_or(budget);
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
    let (resume_context_status, resume_context_reason) =
        resume_context_status(latest_checkpoint.as_ref(), workflow_revision);

    let mut candidates = vec![
        ContextShardCandidate {
            section: "local_objective",
            source: "task",
            priority: priority_for_profile(&profile, "local_objective", 100),
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
            priority: priority_for_profile(&profile, "workflow_goal", 95),
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
            priority: priority_for_profile(&profile, "persona_routing", 92),
            content: persona
                .as_ref()
                .map(render_persona_context)
                .unwrap_or_default(),
        },
        ContextShardCandidate {
            section: "execution_policy",
            source: "execution_policy",
            priority: priority_for_profile(&profile, "execution_policy", 91),
            content: render_execution_policy_context(&task.execution_policy),
        },
        ContextShardCandidate {
            section: "child_subflows",
            source: "subflow_registry",
            priority: priority_for_profile(&profile, "child_subflows", 89),
            content: render_child_subflows_context(&task.child_subflows),
        },
        ContextShardCandidate {
            section: "checkpoint",
            source: "checkpoint",
            priority: priority_for_profile(&profile, "checkpoint", 88),
            content: latest_checkpoint
                .as_ref()
                .map(|checkpoint| render_checkpoint_context(checkpoint, resume_context_status))
                .unwrap_or_default(),
        },
        ContextShardCandidate {
            section: "context_requirements",
            source: "task",
            priority: priority_for_profile(&profile, "context_requirements", 90),
            content: format!(
                "Context requirements: {}\n",
                task.context_requirements.join("; ")
            ),
        },
        ContextShardCandidate {
            section: "validation_rules",
            source: "validation",
            priority: priority_for_profile(&profile, "validation_rules", 80),
            content: format!(
                "Validation rules: {}\n",
                serde_json::to_string(&task.validation_rules)?
            ),
        },
        ContextShardCandidate {
            section: "dependencies",
            source: "graph",
            priority: priority_for_profile(&profile, "dependencies", 70),
            content: format!("Dependencies: {}\n", task.dependencies.join(", ")),
        },
        ContextShardCandidate {
            section: "work_item",
            source: "task",
            priority: priority_for_profile(&profile, "work_item", 60),
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
            priority: priority_for_profile(&profile, "constraints", 40),
            content: format!("Constraints: {}\n", workflow.intent.constraints.join("; ")),
        },
    ];

    let mut content = String::new();
    let mut included_sections = Vec::new();
    let mut omitted_sections = Vec::new();
    let mut profile_omitted_sections = Vec::new();
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
        let summary = summarize_shard(&candidate.content);
        let original_bytes = candidate.content.len();
        if !profile.allowed_sections.contains(&candidate.section) {
            omitted_sections.push(candidate.section.to_string());
            profile_omitted_sections.push(candidate.section.to_string());
            shards.push(ContextShard {
                section: candidate.section.to_string(),
                source: candidate.source.to_string(),
                priority: candidate.priority,
                included: false,
                compressed: false,
                profile_excluded: true,
                routing_decision: "omitted_profile".to_string(),
                decision_reason: format!(
                    "section is not allowed by executor profile {}",
                    profile.id
                ),
                bytes: 0,
                original_bytes,
                content_sha256: hex_sha256(b""),
                summary,
            });
            continue;
        }

        let compressed_content = compress_shard(&candidate, &summary);
        let (included, compressed, selected_content, routing_decision, decision_reason) =
            if content.len() + candidate.content.len() <= effective_budget {
                (
                    true,
                    false,
                    candidate.content.clone(),
                    "included_full",
                    "full shard fits within remaining effective budget",
                )
            } else if compressed_content.len() < original_bytes
                && content.len() + compressed_content.len() <= effective_budget
            {
                (
                    true,
                    true,
                    compressed_content,
                    "included_compressed",
                    "compressed shard fits within remaining effective budget",
                )
            } else {
                (
                    false,
                    false,
                    String::new(),
                    "omitted_budget",
                    "full and compressed shard exceed remaining effective budget",
                )
            };

        if included {
            content.push_str(&selected_content);
            included_sections.push(candidate.section.to_string());
        } else {
            omitted_sections.push(candidate.section.to_string());
        }

        shards.push(ContextShard {
            section: candidate.section.to_string(),
            source: candidate.source.to_string(),
            priority: candidate.priority,
            included,
            compressed,
            profile_excluded: false,
            routing_decision: routing_decision.to_string(),
            decision_reason: decision_reason.to_string(),
            bytes: selected_content.len(),
            original_bytes,
            content_sha256: hex_sha256(selected_content.as_bytes()),
            summary,
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
        executor_profile: profile.to_public(&task.executor),
        execution_policy: task.execution_policy.clone(),
        child_subflow_count: task.child_subflows.len(),
        child_subflows: task.child_subflows.clone(),
        latest_checkpoint,
        resume_context_status: resume_context_status.to_string(),
        resume_context_reason: resume_context_reason.to_string(),
        requested_budget: budget,
        effective_budget,
        context_bytes: content.len(),
        context_sha256: hex_sha256(content.as_bytes()),
        included_sections,
        omitted_sections,
        profile_omitted_sections,
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

fn render_execution_policy_context(policy: &ExecutionPolicySpec) -> String {
    let Some(runtime) = &policy.code_runtime else {
        return format!(
            "Policy: {}\nAI: {} deterministic: {}\nGate: {}\n",
            policy.mode, policy.ai_allowed, policy.deterministic, policy.validation_gate
        );
    };

    format!(
        "Execution policy mode: {}\nAI allowed: {} deterministic: {}\nCode runtime: {} via {}\nReuse hint: {}\nValidation gate: {}\n",
        policy.mode,
        policy.ai_allowed,
        policy.deterministic,
        runtime.language,
        runtime.entrypoint,
        policy.reuse_hint,
        policy.validation_gate
    )
}

fn render_child_subflows_context(child_subflows: &[ChildSubflowRef]) -> String {
    child_subflows
        .iter()
        .map(|subflow| {
            format!(
                "Child subflow: {}/{}\nBinding status: {}\n",
                subflow.workflow_id, subflow.task_id, subflow.binding_status
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_checkpoint_context(checkpoint: &TaskCheckpoint, resume_status: &str) -> String {
    format!(
        "Latest checkpoint: {}\nTask: {}\nExecutor: {}\nState: {}\nWorkflow revision: {}\nContext sha256: {}\nResume status: {}\nSummary: {}\n",
        checkpoint.checkpoint_id,
        checkpoint.task_id,
        checkpoint.executor,
        checkpoint.state,
        checkpoint.workflow_revision,
        checkpoint.context_sha256,
        resume_status,
        checkpoint.summary
    )
}

fn resume_context_status(
    checkpoint: Option<&TaskCheckpoint>,
    workflow_revision: u64,
) -> (&'static str, &'static str) {
    let Some(checkpoint) = checkpoint else {
        return (
            "no_checkpoint",
            "no checkpoint recorded for this workflow task",
        );
    };
    if checkpoint.workflow_revision == workflow_revision {
        return (
            "checkpoint_current",
            "checkpoint workflow revision matches current workflow revision",
        );
    }
    (
        "checkpoint_stale",
        "checkpoint workflow revision differs from current workflow revision",
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

fn compress_shard(candidate: &ContextShardCandidate, summary: &str) -> String {
    format!("[compressed {}]\n{}\n", candidate.section, summary)
}

fn executor_context_profile(task: &AtomicTask) -> ExecutorContextProfile {
    match task.executor {
        ExecutorKind::Command | ExecutorKind::Wait => ExecutorContextProfile {
            id: "no_ai_deterministic",
            reasoning_allowed: false,
            deterministic: true,
            max_context_bytes: Some(DETERMINISTIC_CONTEXT_BUDGET),
            allowed_sections: NO_AI_CONTEXT_SECTIONS,
        },
        ExecutorKind::Notification => ExecutorContextProfile {
            id: "no_ai_notification",
            reasoning_allowed: false,
            deterministic: true,
            max_context_bytes: Some(NOTIFICATION_CONTEXT_BUDGET),
            allowed_sections: NOTIFICATION_CONTEXT_SECTIONS,
        },
        ExecutorKind::Ai => ExecutorContextProfile {
            id: "ai_reasoning",
            reasoning_allowed: true,
            deterministic: false,
            max_context_bytes: None,
            allowed_sections: ALL_CONTEXT_SECTIONS,
        },
        ExecutorKind::Mixed => ExecutorContextProfile {
            id: "mixed_execution",
            reasoning_allowed: true,
            deterministic: false,
            max_context_bytes: None,
            allowed_sections: ALL_CONTEXT_SECTIONS,
        },
    }
}

impl ExecutorContextProfile {
    fn to_public(self, executor: &ExecutorKind) -> ContextExecutorProfile {
        ContextExecutorProfile {
            id: self.id.to_string(),
            executor: executor_kind(executor).to_string(),
            reasoning_allowed: self.reasoning_allowed,
            deterministic: self.deterministic,
            max_context_bytes: self.max_context_bytes,
            allowed_sections: self
                .allowed_sections
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
        }
    }
}

fn executor_kind(executor: &ExecutorKind) -> &'static str {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
}

fn priority_for_profile(
    profile: &ExecutorContextProfile,
    section: &'static str,
    default_priority: u8,
) -> u8 {
    match profile.id {
        "no_ai_deterministic" => match section {
            "local_objective" => 100,
            "execution_policy" => 98,
            "child_subflows" => 97,
            "checkpoint" => 97,
            "validation_rules" => 96,
            "workflow_goal" => 95,
            "context_requirements" => 90,
            "dependencies" => 85,
            _ => default_priority,
        },
        "no_ai_notification" => match section {
            "local_objective" => 100,
            "persona_routing" => 96,
            "execution_policy" => 94,
            "child_subflows" => 92,
            "checkpoint" => 92,
            "validation_rules" => 90,
            "workflow_goal" => 85,
            "context_requirements" => 80,
            "dependencies" => 70,
            _ => default_priority,
        },
        _ => default_priority,
    }
}
