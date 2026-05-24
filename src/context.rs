use crate::artifact::hex_sha256;
use crate::checkpoint::TaskCheckpoint;
use crate::graph::{
    AtomicTask, ChildSubflowRef, ExecutionPolicySpec, ExecutorKind, PersonaRoutingSpec, TaskStatus,
    Workflow,
};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::BTreeSet;

const CONTEXT_SCHEMA_VERSION: &str = "forge.context.v18";
const ROUTING_FINGERPRINT_SCHEMA_VERSION: &str = "forge.context.routing_fingerprint.v1";
const ROUTING_CONTRACT_SCHEMA_VERSION: &str = "forge.context.routing_contract.v1";
const CONTEXT_SELECTOR_VERSION: &str = "forge.context.selector.v1";
const EXECUTOR_PROFILE_SCHEMA_VERSION: &str = "forge.context.executor_profile.v1";
const CONTEXT_NEXT_ACTION_SCHEMA_VERSION: &str = "forge.inspect_context_action.v1";
const CONTEXT_ROUTING_QUALITY_SCHEMA_VERSION: &str = "forge.context_routing_quality.v1";
const CONTEXT_ROUTING_QUALITY_SUMMARY_SCHEMA_VERSION: &str =
    "forge.context_routing_quality_summary.v1";
const ROUTING_POLICY: &str =
    "task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_v18";
const MINIMUM_CONTEXT_BUDGET_BYTES: usize = 128;
pub const DEFAULT_CONTEXT_BUDGET: usize = 1200;
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
const REASONING_REQUIRED_CONTEXT_SECTIONS: &[&str] = &[
    "local_objective",
    "workflow_goal",
    "execution_policy",
    "context_requirements",
    "validation_rules",
];
const NO_AI_REQUIRED_CONTEXT_SECTIONS: &[&str] = &[
    "local_objective",
    "execution_policy",
    "child_subflows",
    "context_requirements",
    "validation_rules",
];
const NOTIFICATION_REQUIRED_CONTEXT_SECTIONS: &[&str] = &[
    "local_objective",
    "persona_routing",
    "execution_policy",
    "context_requirements",
    "validation_rules",
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
    pub dependency_summary: ContextDependencySummary,
    pub dependency_refs: Vec<ContextDependencyRef>,
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub handoff_blockers: Vec<ContextHandoffBlocker>,
    pub child_subflow_count: usize,
    pub child_subflows: Vec<ChildSubflowRef>,
    pub latest_checkpoint: Option<TaskCheckpoint>,
    pub resume_context_status: String,
    pub resume_context_reason: String,
    pub requested_budget: usize,
    pub effective_budget: usize,
    pub context_bytes: usize,
    pub context_sha256: String,
    pub routing_fingerprint: ContextRoutingFingerprint,
    pub routing_contract: ContextRoutingContract,
    pub routing_summary: ContextRoutingSummary,
    pub routing_quality: ContextRoutingQuality,
    pub context_ready: bool,
    pub required_sections: Vec<String>,
    pub missing_required_sections: Vec<String>,
    pub included_sections: Vec<String>,
    pub omitted_sections: Vec<String>,
    pub profile_omitted_sections: Vec<String>,
    pub shards: Vec<ContextShard>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextRoutingFingerprint {
    pub schema_version: String,
    pub cache_key: String,
    pub workflow_revision: u64,
    pub executor_profile_id: String,
    pub context_sha256: String,
    pub lineage_sha256: String,
    pub components: Vec<ContextRoutingFingerprintComponent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextRoutingFingerprintComponent {
    pub name: String,
    pub value: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextRoutingContract {
    pub schema_version: String,
    pub selector_version: String,
    pub profile_version: String,
    pub profile_id: String,
    pub selection_strategy: String,
    pub requested_budget: usize,
    pub effective_budget: usize,
    pub minimum_budget_bytes: usize,
    pub max_context_bytes: Option<usize>,
    pub compression_allowed: bool,
    pub allowed_sections: Vec<String>,
    pub required_sections: Vec<String>,
    pub optional_sections: Vec<String>,
    pub profile_sha256: String,
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
    pub sequence: usize,
    pub shard_id: String,
    pub section: String,
    pub source: String,
    pub priority: u8,
    pub required: bool,
    pub included: bool,
    pub compressed: bool,
    pub profile_excluded: bool,
    pub missing_required: bool,
    pub routing_decision: String,
    pub decision_reason: String,
    pub remaining_budget_before: usize,
    pub remaining_budget_after: usize,
    pub bytes: usize,
    pub original_bytes: usize,
    pub source_sha256: String,
    pub content_sha256: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextDependencySummary {
    pub total: usize,
    pub completed: usize,
    pub running: usize,
    pub pending: usize,
    pub blocked: usize,
    pub failed: usize,
    pub missing: usize,
    pub ready: bool,
    pub blocking_task_ids: Vec<String>,
    pub missing_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextDependencyRef {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub required: bool,
    pub blocking: bool,
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextHandoffBlocker {
    pub kind: String,
    pub message: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextHandoffSummary {
    pub total: usize,
    pub ready: usize,
    pub blocked: usize,
    pub blocked_missing_context: usize,
    pub blocked_dependencies: usize,
    pub blocked_missing_context_and_dependencies: usize,
    pub routing_quality: ContextRoutingQualitySummary,
    pub tasks: Vec<ContextHandoffTask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextHandoffTask {
    pub task_id: String,
    pub title: String,
    pub executor: String,
    pub context_ready: bool,
    pub dependency_ready: bool,
    pub handoff_ready: bool,
    pub handoff_status: String,
    pub handoff_blockers: Vec<ContextHandoffBlocker>,
    pub blocking_refs: Vec<String>,
    pub context_sha256: String,
    pub resume_context_status: String,
    pub routing_quality: ContextRoutingQuality,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextRoutingSummary {
    pub total_shards: usize,
    pub included_shards: usize,
    pub omitted_shards: usize,
    pub compressed_shards: usize,
    pub required_shards: usize,
    pub required_omitted_shards: usize,
    pub profile_omitted_shards: usize,
    pub budget_omitted_shards: usize,
    pub selected_bytes: usize,
    pub original_bytes: usize,
    pub omitted_bytes: usize,
    pub compression_saved_bytes: usize,
    pub effective_budget: usize,
    pub remaining_budget: usize,
    pub budget_utilization_bps: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextRoutingQuality {
    pub schema_version: String,
    pub status: String,
    pub score_bps: u32,
    pub warnings: Vec<ContextRoutingQualityWarning>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextRoutingQualityWarning {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub recommendation: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextRoutingQualitySummary {
    pub schema_version: String,
    pub tasks: usize,
    pub passed: usize,
    pub warning: usize,
    pub blocked: usize,
    pub total_warnings: usize,
    pub blocking_warnings: usize,
    pub warning_warnings: usize,
    pub advisory_warnings: usize,
    pub min_score_bps: u32,
    pub average_score_bps: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextExecutorProfile {
    pub id: String,
    pub executor: String,
    pub reasoning_allowed: bool,
    pub deterministic: bool,
    pub max_context_bytes: Option<usize>,
    pub allowed_sections: Vec<String>,
    pub required_sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextNextAction {
    pub schema_version: String,
    pub action: String,
    pub ready_for_handoff: bool,
    pub partial_retry_recommended: bool,
    pub checkpoint_id: Option<String>,
    pub checkpoint_context_sha256: Option<String>,
    pub checkpoint_context_routing_cache_key: Option<String>,
    pub current_context_routing_cache_key: String,
    pub reason: String,
    pub blocking_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct ExecutorContextProfile {
    id: &'static str,
    reasoning_allowed: bool,
    deterministic: bool,
    max_context_bytes: Option<usize>,
    allowed_sections: &'static [&'static str],
    required_sections: &'static [&'static str],
}

struct ContextShardCandidate {
    section: &'static str,
    source: &'static str,
    priority: u8,
    content: String,
}

#[derive(Serialize)]
struct ContextRoutingFingerprintSeed<'a> {
    schema_version: &'static str,
    workflow_id: &'a str,
    task_id: &'a str,
    workflow_revision: u64,
    executor_profile_id: &'static str,
    context_sha256: &'a str,
    lineage_sha256: &'a str,
    components: &'a [ContextRoutingFingerprintComponent],
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

#[derive(Serialize)]
struct ContextRoutingContractProfileSeed {
    schema_version: &'static str,
    selector_version: &'static str,
    profile_version: &'static str,
    profile_id: String,
    selection_strategy: &'static str,
    reasoning_allowed: bool,
    deterministic: bool,
    max_context_bytes: Option<usize>,
    compression_allowed: bool,
    allowed_sections: Vec<String>,
    required_sections: Vec<String>,
}

struct RoutingFingerprintInput<'a> {
    workflow_id: &'a str,
    task_id: &'a str,
    workflow_revision: u64,
    profile: &'a ExecutorContextProfile,
    requested_budget: usize,
    effective_budget: usize,
    lineage: &'a ContextLineage,
    dependency_summary: &'a ContextDependencySummary,
    included_sections: &'a [String],
    omitted_sections: &'a [String],
    missing_required_sections: &'a [String],
    shards: &'a [ContextShard],
    child_subflows: &'a [ChildSubflowRef],
    resume_context_status: &'a str,
    routing_contract: &'a ContextRoutingContract,
    routing_quality: &'a ContextRoutingQuality,
    context_sha256: &'a str,
}

pub fn build_context_package(
    workflow: &Workflow,
    task_id: &str,
    budget: usize,
) -> Result<ContextPackage> {
    build_context_package_with_checkpoint(workflow, task_id, budget, None)
}

pub fn build_context_handoff_summary(
    workflow: &Workflow,
    budget: usize,
    checkpoints: &[TaskCheckpoint],
) -> Result<ContextHandoffSummary> {
    let mut tasks = Vec::new();

    for task in &workflow.tasks {
        let latest_checkpoint = checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.task_id == task.id)
            .cloned();
        let package =
            build_context_package_with_checkpoint(workflow, &task.id, budget, latest_checkpoint)?;
        let blocking_refs = package
            .handoff_blockers
            .iter()
            .flat_map(|blocker| blocker.refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        tasks.push(ContextHandoffTask {
            task_id: task.id.clone(),
            title: task.title.clone(),
            executor: executor_kind(&task.executor).to_string(),
            context_ready: package.context_ready,
            dependency_ready: package.dependency_summary.ready,
            handoff_ready: package.handoff_ready,
            handoff_status: package.handoff_status,
            handoff_blockers: package.handoff_blockers,
            blocking_refs,
            context_sha256: package.context_sha256,
            resume_context_status: package.resume_context_status,
            routing_quality: package.routing_quality,
        });
    }

    Ok(summarize_context_handoff_tasks(tasks))
}

pub fn summarize_context_handoff_tasks(tasks: Vec<ContextHandoffTask>) -> ContextHandoffSummary {
    let ready = tasks.iter().filter(|task| task.handoff_ready).count();
    let blocked_missing_context = tasks
        .iter()
        .filter(|task| task.handoff_status == "blocked_missing_context")
        .count();
    let blocked_dependencies = tasks
        .iter()
        .filter(|task| task.handoff_status == "blocked_dependencies")
        .count();
    let blocked_missing_context_and_dependencies = tasks
        .iter()
        .filter(|task| task.handoff_status == "blocked_missing_context_and_dependencies")
        .count();
    let total = tasks.len();
    let routing_quality = summarize_routing_quality(&tasks);

    ContextHandoffSummary {
        total,
        ready,
        blocked: total.saturating_sub(ready),
        blocked_missing_context,
        blocked_dependencies,
        blocked_missing_context_and_dependencies,
        routing_quality,
        tasks,
    }
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
    if budget < MINIMUM_CONTEXT_BUDGET_BYTES {
        bail!(
            "context budget must be at least {} bytes",
            MINIMUM_CONTEXT_BUDGET_BYTES
        );
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
    let dependency_refs = build_dependency_refs(workflow, task);
    let dependency_summary = summarize_dependency_refs(&dependency_refs);

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
            content: render_dependencies_context(&dependency_refs, &dependency_summary),
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
    let mut required_sections = Vec::new();
    let mut shards = Vec::new();

    candidates.sort_by(|left, right| {
        let left_required = profile.required_sections.contains(&left.section);
        let right_required = profile.required_sections.contains(&right.section);
        right_required
            .cmp(&left_required)
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left.section.cmp(right.section))
    });

    for candidate in candidates {
        if candidate.content.is_empty() {
            continue;
        }
        let required = profile.required_sections.contains(&candidate.section);
        if required {
            required_sections.push(candidate.section.to_string());
        }
        let summary = summarize_shard(&candidate.content);
        let original_bytes = candidate.content.len();
        let source_sha256 = hex_sha256(candidate.content.as_bytes());
        let shard_id = build_shard_id(
            &workflow.id,
            &task.id,
            workflow_revision,
            &profile,
            candidate.section,
            candidate.source,
            &source_sha256,
        );
        let sequence = shards.len();
        let remaining_budget_before = effective_budget.saturating_sub(content.len());
        if !profile.allowed_sections.contains(&candidate.section) {
            omitted_sections.push(candidate.section.to_string());
            profile_omitted_sections.push(candidate.section.to_string());
            shards.push(ContextShard {
                sequence,
                shard_id,
                section: candidate.section.to_string(),
                source: candidate.source.to_string(),
                priority: candidate.priority,
                required,
                included: false,
                compressed: false,
                profile_excluded: true,
                missing_required: required,
                routing_decision: "omitted_profile".to_string(),
                decision_reason: format!(
                    "section is not allowed by executor profile {}",
                    profile.id
                ),
                remaining_budget_before,
                remaining_budget_after: remaining_budget_before,
                bytes: 0,
                original_bytes,
                source_sha256,
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
        let remaining_budget_after = remaining_budget_before.saturating_sub(selected_content.len());

        shards.push(ContextShard {
            sequence,
            shard_id,
            section: candidate.section.to_string(),
            source: candidate.source.to_string(),
            priority: candidate.priority,
            required,
            included,
            compressed,
            profile_excluded: false,
            missing_required: required && !included,
            routing_decision: routing_decision.to_string(),
            decision_reason: decision_reason.to_string(),
            remaining_budget_before,
            remaining_budget_after,
            bytes: selected_content.len(),
            original_bytes,
            source_sha256,
            content_sha256: hex_sha256(selected_content.as_bytes()),
            summary,
        });
    }

    let routing_summary = build_routing_summary(&shards, effective_budget, content.len());
    let missing_required_sections = shards
        .iter()
        .filter(|shard| shard.missing_required)
        .map(|shard| shard.section.clone())
        .collect::<Vec<_>>();
    let context_ready = missing_required_sections.is_empty();
    let handoff_blockers = build_handoff_blockers(&missing_required_sections, &dependency_summary);
    let handoff_ready = handoff_blockers.is_empty();
    let handoff_status = derive_handoff_status(&handoff_blockers);
    let context_sha256 = hex_sha256(content.as_bytes());
    let routing_contract =
        build_routing_contract(&profile, budget, effective_budget, &required_sections)?;
    let routing_quality = build_routing_quality(
        &routing_summary,
        &shards,
        &missing_required_sections,
        &profile_omitted_sections,
    );
    let routing_fingerprint = build_routing_fingerprint(RoutingFingerprintInput {
        workflow_id: &workflow.id,
        task_id: &task.id,
        workflow_revision,
        profile: &profile,
        requested_budget: budget,
        effective_budget,
        lineage: &lineage,
        dependency_summary: &dependency_summary,
        included_sections: &included_sections,
        omitted_sections: &omitted_sections,
        missing_required_sections: &missing_required_sections,
        shards: &shards,
        child_subflows: &task.child_subflows,
        resume_context_status,
        routing_contract: &routing_contract,
        routing_quality: &routing_quality,
        context_sha256: &context_sha256,
    })?;

    Ok(ContextPackage {
        schema_version: CONTEXT_SCHEMA_VERSION.to_string(),
        routing_policy: ROUTING_POLICY.to_string(),
        workflow_id: workflow.id.clone(),
        task_id: task.id.clone(),
        workflow_revision,
        artifact_count: workflow.artifacts.len(),
        lineage,
        persona,
        executor_profile: profile.to_public(&task.executor, &required_sections),
        execution_policy: task.execution_policy.clone(),
        dependency_summary,
        dependency_refs,
        handoff_ready,
        handoff_status: handoff_status.to_string(),
        handoff_blockers,
        child_subflow_count: task.child_subflows.len(),
        child_subflows: task.child_subflows.clone(),
        latest_checkpoint,
        resume_context_status: resume_context_status.to_string(),
        resume_context_reason: resume_context_reason.to_string(),
        requested_budget: budget,
        effective_budget,
        context_bytes: content.len(),
        context_sha256,
        routing_fingerprint,
        routing_contract,
        routing_summary,
        routing_quality,
        context_ready,
        required_sections,
        missing_required_sections,
        included_sections,
        omitted_sections,
        profile_omitted_sections,
        shards,
        content,
    })
}

pub fn context_next_action(package: &ContextPackage) -> ContextNextAction {
    let current_route = package.routing_fingerprint.cache_key.clone();
    let blocking_refs = package
        .handoff_blockers
        .iter()
        .flat_map(|blocker| blocker.refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    if !package.context_ready && !package.dependency_summary.ready {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "repair_context_and_wait_for_dependencies".to_string(),
            ready_for_handoff: false,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route,
            reason: "required context is missing and dependency tasks are not ready".to_string(),
            blocking_refs,
        };
    }

    if !package.context_ready {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "increase_context_budget".to_string(),
            ready_for_handoff: false,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route,
            reason: "required context sections were omitted by budget or profile routing"
                .to_string(),
            blocking_refs,
        };
    }

    if !package.dependency_summary.ready {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "wait_for_dependencies".to_string(),
            ready_for_handoff: false,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route,
            reason: "dependency tasks must complete before executor handoff".to_string(),
            blocking_refs,
        };
    }

    let Some(checkpoint) = &package.latest_checkpoint else {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "start_executor_handoff".to_string(),
            ready_for_handoff: true,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route,
            reason: "context and dependencies are ready; acquire an executor handoff lease"
                .to_string(),
            blocking_refs,
        };
    };

    let checkpoint_route = checkpoint.context_routing_cache_key.clone();
    let (action, partial_retry_recommended, reason) =
        if package.resume_context_status == "checkpoint_stale" {
            (
                "refresh_context_before_resume",
                false,
                package.resume_context_reason.as_str(),
            )
        } else if checkpoint_route.is_none() {
            (
                "refresh_context_before_resume",
                false,
                "checkpoint does not carry a context routing cache key",
            )
        } else if checkpoint_route.as_deref() == Some(current_route.as_str()) {
            (
                "resume_from_checkpoint",
                false,
                "checkpoint route matches current context route",
            )
        } else {
            (
                "partial_retry_with_fresh_context",
                true,
                "checkpoint route differs from current context route",
            )
        };

    ContextNextAction {
        schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
        action: action.to_string(),
        ready_for_handoff: true,
        partial_retry_recommended,
        checkpoint_id: Some(checkpoint.checkpoint_id.clone()),
        checkpoint_context_sha256: Some(checkpoint.context_sha256.clone()),
        checkpoint_context_routing_cache_key: checkpoint_route,
        current_context_routing_cache_key: current_route,
        reason: reason.to_string(),
        blocking_refs,
    }
}

fn build_handoff_blockers(
    missing_required_sections: &[String],
    dependency_summary: &ContextDependencySummary,
) -> Vec<ContextHandoffBlocker> {
    let mut blockers = Vec::new();

    if !missing_required_sections.is_empty() {
        blockers.push(ContextHandoffBlocker {
            kind: "missing_required_context".to_string(),
            message: "required context sections were omitted by budget or profile routing"
                .to_string(),
            refs: missing_required_sections.to_vec(),
        });
    }

    if !dependency_summary.ready {
        blockers.push(ContextHandoffBlocker {
            kind: "dependency_not_ready".to_string(),
            message: "dependency tasks are not ready for executor handoff".to_string(),
            refs: dependency_summary.blocking_task_ids.clone(),
        });
    }

    blockers
}

fn derive_handoff_status(blockers: &[ContextHandoffBlocker]) -> &'static str {
    let missing_context = blockers
        .iter()
        .any(|blocker| blocker.kind == "missing_required_context");
    let blocked_dependencies = blockers
        .iter()
        .any(|blocker| blocker.kind == "dependency_not_ready");

    match (missing_context, blocked_dependencies) {
        (false, false) => "ready",
        (true, false) => "blocked_missing_context",
        (false, true) => "blocked_dependencies",
        (true, true) => "blocked_missing_context_and_dependencies",
    }
}

fn build_routing_summary(
    shards: &[ContextShard],
    effective_budget: usize,
    selected_bytes: usize,
) -> ContextRoutingSummary {
    let total_shards = shards.len();
    let included_shards = shards.iter().filter(|shard| shard.included).count();
    let compressed_shards = shards.iter().filter(|shard| shard.compressed).count();
    let required_shards = shards.iter().filter(|shard| shard.required).count();
    let required_omitted_shards = shards
        .iter()
        .filter(|shard| shard.required && !shard.included)
        .count();
    let profile_omitted_shards = shards.iter().filter(|shard| shard.profile_excluded).count();
    let budget_omitted_shards = shards
        .iter()
        .filter(|shard| shard.routing_decision == "omitted_budget")
        .count();
    let original_bytes = shards.iter().map(|shard| shard.original_bytes).sum();
    let omitted_bytes = shards
        .iter()
        .filter(|shard| !shard.included)
        .map(|shard| shard.original_bytes)
        .sum();
    let compression_saved_bytes = shards
        .iter()
        .filter(|shard| shard.included && shard.compressed)
        .map(|shard| shard.original_bytes.saturating_sub(shard.bytes))
        .sum();
    let budget_utilization_bps = if effective_budget == 0 {
        0
    } else {
        ((selected_bytes as u128 * 10_000) / effective_budget as u128)
            .min(10_000)
            .try_into()
            .unwrap_or(10_000)
    };

    ContextRoutingSummary {
        total_shards,
        included_shards,
        omitted_shards: total_shards.saturating_sub(included_shards),
        compressed_shards,
        required_shards,
        required_omitted_shards,
        profile_omitted_shards,
        budget_omitted_shards,
        selected_bytes,
        original_bytes,
        omitted_bytes,
        compression_saved_bytes,
        effective_budget,
        remaining_budget: effective_budget.saturating_sub(selected_bytes),
        budget_utilization_bps,
    }
}

fn build_routing_quality(
    summary: &ContextRoutingSummary,
    shards: &[ContextShard],
    missing_required_sections: &[String],
    profile_omitted_sections: &[String],
) -> ContextRoutingQuality {
    let mut warnings = Vec::new();

    if !missing_required_sections.is_empty() {
        warnings.push(routing_quality_warning(
            "required_context_missing",
            "blocking",
            "required context sections were omitted before executor handoff",
            "increase_context_budget",
            missing_required_sections.to_vec(),
        ));
    }

    if summary.budget_omitted_shards > 0 {
        warnings.push(routing_quality_warning(
            "budget_pressure",
            "warning",
            "one or more context shards were omitted by the effective budget",
            "increase_context_budget",
            unique_sections(
                shards
                    .iter()
                    .filter(|shard| shard.routing_decision == "omitted_budget")
                    .map(|shard| shard.section.as_str()),
            ),
        ));
    }

    if summary.compressed_shards > 0 {
        warnings.push(routing_quality_warning(
            "compressed_context",
            "advisory",
            "one or more shards were summarized to fit the context budget",
            "review_context_summary_before_reuse",
            unique_sections(
                shards
                    .iter()
                    .filter(|shard| shard.included && shard.compressed)
                    .map(|shard| shard.section.as_str()),
            ),
        ));
    }

    if summary.profile_omitted_shards > 0 {
        warnings.push(routing_quality_warning(
            "profile_filtered_optional_context",
            "advisory",
            "the executor profile filtered context sections that are not part of its contract",
            "verify_executor_profile_matches_node_policy",
            unique_sections(profile_omitted_sections.iter().map(String::as_str)),
        ));
    }

    let status = if warnings
        .iter()
        .any(|warning| warning.severity == "blocking")
    {
        "blocked"
    } else if warnings.is_empty() {
        "pass"
    } else {
        "warning"
    };
    let penalty = warnings
        .iter()
        .map(|warning| match warning.severity.as_str() {
            "blocking" => 5_000,
            "warning" => 1_000,
            "advisory" => 250,
            _ => 0,
        })
        .sum::<u32>();

    ContextRoutingQuality {
        schema_version: CONTEXT_ROUTING_QUALITY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        score_bps: 10_000_u32.saturating_sub(penalty),
        warnings,
    }
}

fn build_routing_contract(
    profile: &ExecutorContextProfile,
    requested_budget: usize,
    effective_budget: usize,
    required_sections: &[String],
) -> Result<ContextRoutingContract> {
    let allowed_sections = profile
        .allowed_sections
        .iter()
        .map(|section| (*section).to_string())
        .collect::<Vec<_>>();
    let required_sections = required_sections.to_vec();
    let optional_sections = allowed_sections
        .iter()
        .filter(|section| {
            !required_sections
                .iter()
                .any(|required| required.as_str() == section.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let seed = ContextRoutingContractProfileSeed {
        schema_version: ROUTING_CONTRACT_SCHEMA_VERSION,
        selector_version: CONTEXT_SELECTOR_VERSION,
        profile_version: EXECUTOR_PROFILE_SCHEMA_VERSION,
        profile_id: profile.id.to_string(),
        selection_strategy: "required_first_priority_budgeted_compression",
        reasoning_allowed: profile.reasoning_allowed,
        deterministic: profile.deterministic,
        max_context_bytes: profile.max_context_bytes,
        compression_allowed: true,
        allowed_sections: allowed_sections.clone(),
        required_sections: required_sections.clone(),
    };

    Ok(ContextRoutingContract {
        schema_version: ROUTING_CONTRACT_SCHEMA_VERSION.to_string(),
        selector_version: CONTEXT_SELECTOR_VERSION.to_string(),
        profile_version: EXECUTOR_PROFILE_SCHEMA_VERSION.to_string(),
        profile_id: profile.id.to_string(),
        selection_strategy: seed.selection_strategy.to_string(),
        requested_budget,
        effective_budget,
        minimum_budget_bytes: MINIMUM_CONTEXT_BUDGET_BYTES,
        max_context_bytes: profile.max_context_bytes,
        compression_allowed: true,
        allowed_sections,
        required_sections,
        optional_sections,
        profile_sha256: hex_sha256(serde_json::to_string(&seed)?.as_bytes()),
    })
}

fn routing_quality_warning(
    code: &str,
    severity: &str,
    message: &str,
    recommendation: &str,
    refs: Vec<String>,
) -> ContextRoutingQualityWarning {
    ContextRoutingQualityWarning {
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.to_string(),
        recommendation: recommendation.to_string(),
        refs,
    }
}

fn unique_sections<'a>(sections: impl Iterator<Item = &'a str>) -> Vec<String> {
    sections
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn summarize_routing_quality(tasks: &[ContextHandoffTask]) -> ContextRoutingQualitySummary {
    let mut summary = ContextRoutingQualitySummary {
        schema_version: CONTEXT_ROUTING_QUALITY_SUMMARY_SCHEMA_VERSION.to_string(),
        tasks: tasks.len(),
        passed: 0,
        warning: 0,
        blocked: 0,
        total_warnings: 0,
        blocking_warnings: 0,
        warning_warnings: 0,
        advisory_warnings: 0,
        min_score_bps: 10_000,
        average_score_bps: 0,
    };

    if tasks.is_empty() {
        return summary;
    }

    let mut score_total = 0_u64;
    for task in tasks {
        match task.routing_quality.status.as_str() {
            "pass" => summary.passed += 1,
            "warning" => summary.warning += 1,
            "blocked" => summary.blocked += 1,
            _ => {}
        }
        summary.total_warnings += task.routing_quality.warnings.len();
        for warning in &task.routing_quality.warnings {
            match warning.severity.as_str() {
                "blocking" => summary.blocking_warnings += 1,
                "warning" => summary.warning_warnings += 1,
                "advisory" => summary.advisory_warnings += 1,
                _ => {}
            }
        }
        summary.min_score_bps = summary.min_score_bps.min(task.routing_quality.score_bps);
        score_total += u64::from(task.routing_quality.score_bps);
    }
    summary.average_score_bps = (score_total / tasks.len() as u64) as u32;

    summary
}

fn build_routing_fingerprint(
    input: RoutingFingerprintInput<'_>,
) -> Result<ContextRoutingFingerprint> {
    let dependency_state = serde_json::to_string(input.dependency_summary)?;
    let child_subflows = serde_json::to_string(input.child_subflows)?;
    let routing_contract = serde_json::to_string(input.routing_contract)?;
    let routing_quality = serde_json::to_string(input.routing_quality)?;
    let source_shards = input
        .shards
        .iter()
        .map(|shard| {
            format!(
                "{}:{}:{}:{}",
                shard.sequence, shard.section, shard.source, shard.source_sha256
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    let budget_ledger = input
        .shards
        .iter()
        .map(|shard| {
            format!(
                "{}:{}:{}:{}:{}",
                shard.sequence,
                shard.section,
                shard.routing_decision,
                shard.remaining_budget_before,
                shard.remaining_budget_after
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    let components = vec![
        fingerprint_component("routing_policy", ROUTING_POLICY.to_string()),
        fingerprint_component(
            "executor_profile",
            format!(
                "{}:reasoning={}:deterministic={}",
                input.profile.id, input.profile.reasoning_allowed, input.profile.deterministic
            ),
        ),
        fingerprint_component("lineage", input.lineage.lineage_sha256.clone()),
        fingerprint_component(
            "budget",
            format!(
                "requested={}:effective={}",
                input.requested_budget, input.effective_budget
            ),
        ),
        fingerprint_component("source_shards", source_shards),
        fingerprint_component("selected_sections", input.included_sections.join(",")),
        fingerprint_component("omitted_sections", input.omitted_sections.join(",")),
        fingerprint_component(
            "missing_required_sections",
            input.missing_required_sections.join(","),
        ),
        fingerprint_component("dependency_state", dependency_state),
        fingerprint_component("child_subflows", child_subflows),
        fingerprint_component("resume_context", input.resume_context_status.to_string()),
        fingerprint_component("budget_ledger", budget_ledger),
        fingerprint_component("routing_contract", routing_contract),
        fingerprint_component("routing_quality", routing_quality),
        fingerprint_component("context_payload", input.context_sha256.to_string()),
    ];
    let seed = ContextRoutingFingerprintSeed {
        schema_version: ROUTING_FINGERPRINT_SCHEMA_VERSION,
        workflow_id: input.workflow_id,
        task_id: input.task_id,
        workflow_revision: input.workflow_revision,
        executor_profile_id: input.profile.id,
        context_sha256: input.context_sha256,
        lineage_sha256: &input.lineage.lineage_sha256,
        components: &components,
    };
    let cache_key = hex_sha256(serde_json::to_string(&seed)?.as_bytes());

    Ok(ContextRoutingFingerprint {
        schema_version: ROUTING_FINGERPRINT_SCHEMA_VERSION.to_string(),
        cache_key,
        workflow_revision: input.workflow_revision,
        executor_profile_id: input.profile.id.to_string(),
        context_sha256: input.context_sha256.to_string(),
        lineage_sha256: input.lineage.lineage_sha256.clone(),
        components,
    })
}

fn build_shard_id(
    workflow_id: &str,
    task_id: &str,
    workflow_revision: u64,
    profile: &ExecutorContextProfile,
    section: &str,
    source: &str,
    source_sha256: &str,
) -> String {
    hex_sha256(
        format!(
            "{workflow_id}:{task_id}:{workflow_revision}:{}:{section}:{source}:{source_sha256}",
            profile.id
        )
        .as_bytes(),
    )
}

fn fingerprint_component(name: &str, value: String) -> ContextRoutingFingerprintComponent {
    ContextRoutingFingerprintComponent {
        name: name.to_string(),
        sha256: hex_sha256(value.as_bytes()),
        value,
    }
}

fn build_dependency_refs(workflow: &Workflow, task: &AtomicTask) -> Vec<ContextDependencyRef> {
    task.dependencies
        .iter()
        .map(|dependency_id| {
            let Some(dependency) = workflow
                .tasks
                .iter()
                .find(|candidate| &candidate.id == dependency_id)
            else {
                return ContextDependencyRef {
                    task_id: dependency_id.clone(),
                    title: "missing dependency".to_string(),
                    status: "missing".to_string(),
                    required: true,
                    blocking: true,
                    missing: true,
                };
            };

            let blocking = dependency.status != TaskStatus::Completed;
            ContextDependencyRef {
                task_id: dependency.id.clone(),
                title: dependency.title.clone(),
                status: task_status(&dependency.status).to_string(),
                required: true,
                blocking,
                missing: false,
            }
        })
        .collect()
}

fn summarize_dependency_refs(dependencies: &[ContextDependencyRef]) -> ContextDependencySummary {
    let completed = dependencies
        .iter()
        .filter(|dependency| dependency.status == "completed")
        .count();
    let running = dependencies
        .iter()
        .filter(|dependency| dependency.status == "running")
        .count();
    let pending = dependencies
        .iter()
        .filter(|dependency| dependency.status == "pending")
        .count();
    let blocked = dependencies
        .iter()
        .filter(|dependency| dependency.status == "blocked")
        .count();
    let failed = dependencies
        .iter()
        .filter(|dependency| dependency.status == "failed")
        .count();
    let missing = dependencies
        .iter()
        .filter(|dependency| dependency.missing)
        .count();
    let blocking_task_ids = dependencies
        .iter()
        .filter(|dependency| dependency.blocking)
        .map(|dependency| dependency.task_id.clone())
        .collect::<Vec<_>>();
    let missing_task_ids = dependencies
        .iter()
        .filter(|dependency| dependency.missing)
        .map(|dependency| dependency.task_id.clone())
        .collect::<Vec<_>>();

    ContextDependencySummary {
        total: dependencies.len(),
        completed,
        running,
        pending,
        blocked,
        failed,
        missing,
        ready: blocking_task_ids.is_empty(),
        blocking_task_ids,
        missing_task_ids,
    }
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

fn render_dependencies_context(
    dependencies: &[ContextDependencyRef],
    summary: &ContextDependencySummary,
) -> String {
    if dependencies.is_empty() {
        return "Dependency readiness: ready\nDependencies: none\n".to_string();
    }

    let mut lines = vec![
        format!(
            "Dependency readiness: {}",
            if summary.ready { "ready" } else { "blocked" }
        ),
        format!(
            "Dependencies total: {} completed: {} pending: {} running: {} blocked: {} failed: {} missing: {}",
            summary.total,
            summary.completed,
            summary.pending,
            summary.running,
            summary.blocked,
            summary.failed,
            summary.missing
        ),
    ];

    for dependency in dependencies {
        let marker = if dependency.missing {
            "missing"
        } else if dependency.blocking {
            "blocking"
        } else {
            "ready"
        };
        lines.push(format!(
            "- {} {} [{}] {}",
            dependency.task_id, dependency.title, dependency.status, marker
        ));
    }

    format!("{}\n", lines.join("\n"))
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

fn task_status(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
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
            required_sections: NO_AI_REQUIRED_CONTEXT_SECTIONS,
        },
        ExecutorKind::Notification => ExecutorContextProfile {
            id: "no_ai_notification",
            reasoning_allowed: false,
            deterministic: true,
            max_context_bytes: Some(NOTIFICATION_CONTEXT_BUDGET),
            allowed_sections: NOTIFICATION_CONTEXT_SECTIONS,
            required_sections: NOTIFICATION_REQUIRED_CONTEXT_SECTIONS,
        },
        ExecutorKind::Ai => ExecutorContextProfile {
            id: "ai_reasoning",
            reasoning_allowed: true,
            deterministic: false,
            max_context_bytes: None,
            allowed_sections: ALL_CONTEXT_SECTIONS,
            required_sections: REASONING_REQUIRED_CONTEXT_SECTIONS,
        },
        ExecutorKind::Mixed => ExecutorContextProfile {
            id: "mixed_execution",
            reasoning_allowed: true,
            deterministic: false,
            max_context_bytes: None,
            allowed_sections: ALL_CONTEXT_SECTIONS,
            required_sections: REASONING_REQUIRED_CONTEXT_SECTIONS,
        },
    }
}

impl ExecutorContextProfile {
    fn to_public(
        self,
        executor: &ExecutorKind,
        required_sections: &[String],
    ) -> ContextExecutorProfile {
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
            required_sections: required_sections.to_vec(),
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
