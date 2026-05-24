use crate::artifact::hex_sha256;
use crate::checkpoint::TaskCheckpoint;
use crate::graph::{
    AtomicTask, ChildSubflowRef, ExecutionPolicySpec, ExecutorKind, PersonaRoutingSpec, TaskStatus,
    Workflow,
};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::BTreeSet;

const CONTEXT_SCHEMA_VERSION: &str = "forge.context.v24";
const ROUTING_FINGERPRINT_SCHEMA_VERSION: &str = "forge.context.routing_fingerprint.v1";
const ROUTING_CONTRACT_SCHEMA_VERSION: &str = "forge.context.routing_contract.v1";
const ROUTING_REPAIR_SCHEMA_VERSION: &str = "forge.context.routing_repair.v1";
const BUDGET_PLAN_SCHEMA_VERSION: &str = "forge.context.budget_plan.v1";
const PERSONA_PROFILE_SCHEMA_VERSION: &str = "forge.context.persona_profile.v1";
const PERSONA_CONTRACT_SCHEMA_VERSION: &str = "forge.context.persona_contract.v2";
const CONTEXT_DELTA_SCHEMA_VERSION: &str = "forge.context.delta.v1";
const CONTEXT_SELECTOR_VERSION: &str = "forge.context.selector.v1";
const EXECUTOR_PROFILE_SCHEMA_VERSION: &str = "forge.context.executor_profile.v1";
const CONTEXT_NEXT_ACTION_SCHEMA_VERSION: &str = "forge.inspect_context_action.v1";
const CONTEXT_ROUTING_QUALITY_SCHEMA_VERSION: &str = "forge.context_routing_quality.v1";
const CONTEXT_ROUTING_QUALITY_SUMMARY_SCHEMA_VERSION: &str =
    "forge.context_routing_quality_summary.v1";
const ROUTING_POLICY: &str =
    "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_v24";
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
    pub persona_profile: Option<ContextPersonaProfile>,
    pub persona_contract: Option<ContextPersonaContract>,
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
    pub routing_repair: ContextRoutingRepair,
    pub budget_plan: ContextBudgetPlan,
    pub routing_summary: ContextRoutingSummary,
    pub routing_quality: ContextRoutingQuality,
    pub next_action: ContextNextAction,
    pub context_delta: ContextDelta,
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
pub struct ContextRoutingRepair {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub current_effective_budget: usize,
    pub recommended_budget_bytes: usize,
    pub required_budget_deficit_bytes: usize,
    pub missing_required_sections: Vec<String>,
    pub budget_omitted_sections: Vec<String>,
    pub compressed_sections: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextBudgetPlan {
    pub schema_version: String,
    pub status: String,
    pub requested_budget: usize,
    pub effective_budget: usize,
    pub selected_bytes: usize,
    pub required_original_bytes: usize,
    pub required_minimum_bytes: usize,
    pub minimum_correct_budget_bytes: usize,
    pub optional_original_bytes: usize,
    pub profile_excluded_original_bytes: usize,
    pub omitted_required_bytes: usize,
    pub omitted_optional_bytes: usize,
    pub compression_saved_bytes: usize,
    pub recommended_budget_bytes: usize,
    pub missing_required_sections: Vec<String>,
    pub budget_omitted_sections: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextLineage {
    pub workflow_revision: u64,
    pub workflow_goal_sha256: String,
    pub task_goal_sha256: String,
    pub artifact_manifest_sha256: String,
    pub artifact_count: usize,
    pub persona_mode_sha256: String,
    pub persona_profile_sha256: String,
    pub persona_scope: String,
    pub revision_sources: Vec<String>,
    pub lineage_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPersonaProfile {
    pub schema_version: String,
    pub profile_id: String,
    pub mode: String,
    pub scope: String,
    pub instruction_source: String,
    pub voice: String,
    pub tone: String,
    pub validation_gate: String,
    pub routing_rationale: String,
    pub source_model_summaries: Vec<ContextPersonaSourceModelSummary>,
    pub auditable: bool,
    pub profile_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPersonaSourceModelSummary {
    pub model_id: String,
    pub source_kind: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPersonaContract {
    pub schema_version: String,
    pub profile_id: String,
    pub mode: String,
    pub scope: String,
    pub persona_scope: String,
    pub instruction_source: String,
    pub voice: String,
    pub tone: String,
    pub validation_gate: String,
    pub routing_rationale: String,
    pub source_models: Vec<String>,
    pub source_model_summaries: Vec<ContextPersonaSourceModelSummary>,
    pub auditable: bool,
    pub profile_sha256: String,
    pub lineage_sha256: String,
    pub persona_mode_sha256: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct ContextDelta {
    pub schema_version: String,
    pub status: String,
    pub can_reuse_checkpoint_context: bool,
    pub partial_retry_recommended: bool,
    pub checkpoint_id: Option<String>,
    pub checkpoint_workflow_revision: Option<u64>,
    pub current_workflow_revision: u64,
    pub checkpoint_context_sha256: Option<String>,
    pub current_context_sha256: String,
    pub checkpoint_context_routing_cache_key: Option<String>,
    pub current_context_routing_cache_key: String,
    pub changed_components: Vec<String>,
    pub reason: String,
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
    persona_profile_sha256: String,
    persona_scope: String,
    revision_sources: Vec<String>,
}

struct ContextLineageInput<'a> {
    workflow: &'a Workflow,
    task_id: &'a str,
    task_goal: &'a str,
    persona: Option<&'a PersonaRoutingSpec>,
    persona_profile_sha256: &'a str,
    workflow_revision: u64,
    revision_sources: Vec<String>,
    artifact_manifest: &'a str,
}

#[derive(Serialize)]
struct ContextPersonaProfileSeed {
    schema_version: &'static str,
    profile_id: String,
    mode: String,
    scope: String,
    instruction_source: String,
    voice: String,
    tone: String,
    validation_gate: String,
    routing_rationale: String,
    source_model_summaries: Vec<ContextPersonaSourceModelSummary>,
    auditable: bool,
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
    routing_repair: &'a ContextRoutingRepair,
    budget_plan: &'a ContextBudgetPlan,
    routing_quality: &'a ContextRoutingQuality,
    persona_profile: Option<&'a ContextPersonaProfile>,
    persona_contract: Option<&'a ContextPersonaContract>,
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
    let persona_profile = persona.as_ref().map(build_persona_profile).transpose()?;
    let persona_profile_sha256 = persona_profile
        .as_ref()
        .map(|profile| profile.profile_sha256.clone())
        .unwrap_or_else(|| hex_sha256(b"none"));
    let lineage = build_lineage(ContextLineageInput {
        workflow,
        task_id,
        task_goal: &task.goal,
        persona: persona.as_ref(),
        persona_profile_sha256: &persona_profile_sha256,
        workflow_revision,
        revision_sources,
        artifact_manifest: &artifact_manifest,
    })?;
    let (resume_context_status, resume_context_reason) =
        resume_context_status(latest_checkpoint.as_ref(), workflow_revision);
    let dependency_refs = build_dependency_refs(workflow, task);
    let dependency_summary = summarize_dependency_refs(&dependency_refs);
    let persona_contract = persona
        .as_ref()
        .zip(persona_profile.as_ref())
        .map(|(persona, profile)| build_persona_contract(persona, profile, &lineage));

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
                .zip(persona_profile.as_ref())
                .map(|(persona, profile)| render_persona_context(persona, profile))
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
    let routing_repair = build_routing_repair(
        &routing_summary,
        &shards,
        &missing_required_sections,
        effective_budget,
    );
    let budget_plan = build_budget_plan(
        &routing_summary,
        &shards,
        &missing_required_sections,
        budget,
        effective_budget,
    );
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
        routing_repair: &routing_repair,
        budget_plan: &budget_plan,
        routing_quality: &routing_quality,
        persona_profile: persona_profile.as_ref(),
        persona_contract: persona_contract.as_ref(),
        context_sha256: &context_sha256,
    })?;
    let next_action = build_context_next_action(
        &routing_fingerprint.cache_key,
        &handoff_blockers,
        context_ready,
        dependency_summary.ready,
        latest_checkpoint.as_ref(),
        resume_context_status,
        resume_context_reason,
    );
    let context_delta = build_context_delta(
        latest_checkpoint.as_ref(),
        workflow_revision,
        &context_sha256,
        &routing_fingerprint.cache_key,
        resume_context_status,
        resume_context_reason,
    );

    Ok(ContextPackage {
        schema_version: CONTEXT_SCHEMA_VERSION.to_string(),
        routing_policy: ROUTING_POLICY.to_string(),
        workflow_id: workflow.id.clone(),
        task_id: task.id.clone(),
        workflow_revision,
        artifact_count: workflow.artifacts.len(),
        lineage,
        persona,
        persona_profile,
        persona_contract,
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
        routing_repair,
        budget_plan,
        routing_summary,
        routing_quality,
        next_action,
        context_delta,
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
    package.next_action.clone()
}

fn build_context_next_action(
    current_route: &str,
    handoff_blockers: &[ContextHandoffBlocker],
    context_ready: bool,
    dependency_ready: bool,
    latest_checkpoint: Option<&TaskCheckpoint>,
    resume_context_status: &str,
    resume_context_reason: &str,
) -> ContextNextAction {
    let blocking_refs = handoff_blockers
        .iter()
        .flat_map(|blocker| blocker.refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    if !context_ready && !dependency_ready {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "repair_context_and_wait_for_dependencies".to_string(),
            ready_for_handoff: false,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route.to_string(),
            reason: "required context is missing and dependency tasks are not ready".to_string(),
            blocking_refs,
        };
    }

    if !context_ready {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "increase_context_budget".to_string(),
            ready_for_handoff: false,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route.to_string(),
            reason: "required context sections were omitted by budget or profile routing"
                .to_string(),
            blocking_refs,
        };
    }

    if !dependency_ready {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "wait_for_dependencies".to_string(),
            ready_for_handoff: false,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route.to_string(),
            reason: "dependency tasks must complete before executor handoff".to_string(),
            blocking_refs,
        };
    }

    let Some(checkpoint) = latest_checkpoint else {
        return ContextNextAction {
            schema_version: CONTEXT_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            action: "start_executor_handoff".to_string(),
            ready_for_handoff: true,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_context_sha256: None,
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: current_route.to_string(),
            reason: "context and dependencies are ready; acquire an executor handoff lease"
                .to_string(),
            blocking_refs,
        };
    };

    let checkpoint_route = checkpoint.context_routing_cache_key.clone();
    let (action, partial_retry_recommended, reason) = if resume_context_status == "checkpoint_stale"
    {
        (
            "refresh_context_before_resume",
            false,
            resume_context_reason,
        )
    } else if checkpoint_route.is_none() {
        (
            "refresh_context_before_resume",
            false,
            "checkpoint does not carry a context routing cache key",
        )
    } else if checkpoint_route.as_deref() == Some(current_route) {
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
        current_context_routing_cache_key: current_route.to_string(),
        reason: reason.to_string(),
        blocking_refs,
    }
}

fn build_context_delta(
    checkpoint: Option<&TaskCheckpoint>,
    workflow_revision: u64,
    context_sha256: &str,
    context_routing_cache_key: &str,
    resume_context_status: &str,
    resume_context_reason: &str,
) -> ContextDelta {
    let Some(checkpoint) = checkpoint else {
        return ContextDelta {
            schema_version: CONTEXT_DELTA_SCHEMA_VERSION.to_string(),
            status: "no_checkpoint".to_string(),
            can_reuse_checkpoint_context: false,
            partial_retry_recommended: false,
            checkpoint_id: None,
            checkpoint_workflow_revision: None,
            current_workflow_revision: workflow_revision,
            checkpoint_context_sha256: None,
            current_context_sha256: context_sha256.to_string(),
            checkpoint_context_routing_cache_key: None,
            current_context_routing_cache_key: context_routing_cache_key.to_string(),
            changed_components: Vec::new(),
            reason: "no checkpoint recorded for this workflow task".to_string(),
        };
    };

    let mut changed_components = Vec::new();
    if checkpoint.workflow_revision != workflow_revision {
        changed_components.push("workflow_revision".to_string());
    }
    if checkpoint.context_sha256 != context_sha256 {
        changed_components.push("context_payload".to_string());
    }
    match checkpoint.context_routing_cache_key.as_deref() {
        Some(cache_key) if cache_key != context_routing_cache_key => {
            changed_components.push("routing_cache_key".to_string());
        }
        None => changed_components.push("checkpoint_route_missing".to_string()),
        _ => {}
    }

    let status = if resume_context_status == "checkpoint_stale" {
        "checkpoint_stale"
    } else if checkpoint.context_routing_cache_key.is_none() {
        "checkpoint_route_unknown"
    } else if changed_components.is_empty() {
        "unchanged"
    } else if changed_components
        .iter()
        .any(|component| component == "routing_cache_key")
    {
        "route_changed"
    } else if changed_components
        .iter()
        .any(|component| component == "context_payload")
    {
        "content_changed"
    } else {
        "changed"
    };
    let reason = match status {
        "checkpoint_stale" => resume_context_reason,
        "checkpoint_route_unknown" => "checkpoint does not carry a context routing cache key",
        "unchanged" => "checkpoint context route matches the current context route",
        "route_changed" => "checkpoint context route differs from the current context route",
        "content_changed" => "checkpoint context payload differs from the current context payload",
        "changed" => "checkpoint context differs from current context",
        _ => "no checkpoint recorded for this workflow task",
    };

    ContextDelta {
        schema_version: CONTEXT_DELTA_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        can_reuse_checkpoint_context: status == "unchanged",
        partial_retry_recommended: status == "route_changed",
        checkpoint_id: Some(checkpoint.checkpoint_id.clone()),
        checkpoint_workflow_revision: Some(checkpoint.workflow_revision),
        current_workflow_revision: workflow_revision,
        checkpoint_context_sha256: Some(checkpoint.context_sha256.clone()),
        current_context_sha256: context_sha256.to_string(),
        checkpoint_context_routing_cache_key: checkpoint.context_routing_cache_key.clone(),
        current_context_routing_cache_key: context_routing_cache_key.to_string(),
        changed_components,
        reason: reason.to_string(),
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

fn build_routing_repair(
    summary: &ContextRoutingSummary,
    shards: &[ContextShard],
    missing_required_sections: &[String],
    effective_budget: usize,
) -> ContextRoutingRepair {
    let budget_omitted_sections = unique_sections(
        shards
            .iter()
            .filter(|shard| shard.routing_decision == "omitted_budget")
            .map(|shard| shard.section.as_str()),
    );
    let compressed_sections = unique_sections(
        shards
            .iter()
            .filter(|shard| shard.included && shard.compressed)
            .map(|shard| shard.section.as_str()),
    );

    if !missing_required_sections.is_empty() {
        let missing_required_budget = shards
            .iter()
            .filter(|shard| shard.missing_required)
            .map(minimum_routable_shard_bytes)
            .sum::<usize>();
        let recommended_budget_bytes = summary
            .selected_bytes
            .saturating_add(missing_required_budget)
            .max(effective_budget.saturating_add(1));

        return ContextRoutingRepair {
            schema_version: ROUTING_REPAIR_SCHEMA_VERSION.to_string(),
            status: "repair_required".to_string(),
            action: "increase_context_budget".to_string(),
            current_effective_budget: effective_budget,
            recommended_budget_bytes,
            required_budget_deficit_bytes: recommended_budget_bytes
                .saturating_sub(effective_budget),
            missing_required_sections: missing_required_sections.to_vec(),
            budget_omitted_sections,
            compressed_sections,
            reason: "required context sections were omitted before executor handoff".to_string(),
        };
    }

    if summary.budget_omitted_shards > 0 {
        return ContextRoutingRepair {
            schema_version: ROUTING_REPAIR_SCHEMA_VERSION.to_string(),
            status: "advisory".to_string(),
            action: "review_budget_pressure".to_string(),
            current_effective_budget: effective_budget,
            recommended_budget_bytes: effective_budget,
            required_budget_deficit_bytes: 0,
            missing_required_sections: Vec::new(),
            budget_omitted_sections,
            compressed_sections,
            reason: "optional context was omitted by budget; no required repair is needed"
                .to_string(),
        };
    }

    ContextRoutingRepair {
        schema_version: ROUTING_REPAIR_SCHEMA_VERSION.to_string(),
        status: "ready".to_string(),
        action: "none".to_string(),
        current_effective_budget: effective_budget,
        recommended_budget_bytes: effective_budget,
        required_budget_deficit_bytes: 0,
        missing_required_sections: Vec::new(),
        budget_omitted_sections,
        compressed_sections,
        reason: "required context fits within the effective budget".to_string(),
    }
}

fn build_budget_plan(
    summary: &ContextRoutingSummary,
    shards: &[ContextShard],
    missing_required_sections: &[String],
    requested_budget: usize,
    effective_budget: usize,
) -> ContextBudgetPlan {
    let required_original_bytes = shards
        .iter()
        .filter(|shard| shard.required)
        .map(|shard| shard.original_bytes)
        .sum();
    let required_minimum_bytes = shards
        .iter()
        .filter(|shard| shard.required)
        .map(minimum_routable_shard_bytes)
        .sum();
    let optional_original_bytes = shards
        .iter()
        .filter(|shard| !shard.required && !shard.profile_excluded)
        .map(|shard| shard.original_bytes)
        .sum();
    let profile_excluded_original_bytes = shards
        .iter()
        .filter(|shard| shard.profile_excluded)
        .map(|shard| shard.original_bytes)
        .sum();
    let omitted_required_bytes = shards
        .iter()
        .filter(|shard| shard.required && !shard.included)
        .map(|shard| shard.original_bytes)
        .sum();
    let omitted_optional_bytes = shards
        .iter()
        .filter(|shard| !shard.required && !shard.included && !shard.profile_excluded)
        .map(|shard| shard.original_bytes)
        .sum();
    let missing_required_minimum_bytes = shards
        .iter()
        .filter(|shard| shard.missing_required)
        .map(minimum_routable_shard_bytes)
        .sum::<usize>();
    let budget_omitted_sections = unique_sections(
        shards
            .iter()
            .filter(|shard| shard.routing_decision == "omitted_budget")
            .map(|shard| shard.section.as_str()),
    );
    let minimum_correct_budget_bytes = required_minimum_bytes;
    let recommended_budget_bytes = if !missing_required_sections.is_empty() {
        summary
            .selected_bytes
            .saturating_add(missing_required_minimum_bytes)
            .max(minimum_correct_budget_bytes)
            .max(effective_budget.saturating_add(1))
    } else if summary.budget_omitted_shards > 0 {
        effective_budget.max(minimum_correct_budget_bytes)
    } else {
        summary.selected_bytes.max(minimum_correct_budget_bytes)
    };
    let (status, reason) = if !missing_required_sections.is_empty() {
        (
            "repair_required",
            "minimum correct context cannot be routed until required sections fit",
        )
    } else if summary.budget_omitted_shards > 0 {
        (
            "advisory",
            "minimum correct context fits, but optional shards were omitted by budget",
        )
    } else {
        (
            "ready",
            "minimum correct context fits within the effective budget",
        )
    };

    ContextBudgetPlan {
        schema_version: BUDGET_PLAN_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        requested_budget,
        effective_budget,
        selected_bytes: summary.selected_bytes,
        required_original_bytes,
        required_minimum_bytes,
        minimum_correct_budget_bytes,
        optional_original_bytes,
        profile_excluded_original_bytes,
        omitted_required_bytes,
        omitted_optional_bytes,
        compression_saved_bytes: summary.compression_saved_bytes,
        recommended_budget_bytes,
        missing_required_sections: missing_required_sections.to_vec(),
        budget_omitted_sections,
        reason: reason.to_string(),
    }
}

fn minimum_routable_shard_bytes(shard: &ContextShard) -> usize {
    let compressed_bytes = format!("[compressed {}]\n{}\n", shard.section, shard.summary).len();
    shard.original_bytes.min(compressed_bytes)
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
    let routing_repair = serde_json::to_string(input.routing_repair)?;
    let budget_plan = serde_json::to_string(input.budget_plan)?;
    let routing_quality = serde_json::to_string(input.routing_quality)?;
    let persona_profile = serde_json::to_string(&input.persona_profile)?;
    let persona_contract = serde_json::to_string(&input.persona_contract)?;
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
        fingerprint_component("routing_repair", routing_repair),
        fingerprint_component("budget_plan", budget_plan),
        fingerprint_component("routing_quality", routing_quality),
        fingerprint_component("persona_profile", persona_profile),
        fingerprint_component("persona_contract", persona_contract),
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

fn build_lineage(input: ContextLineageInput<'_>) -> Result<ContextLineage> {
    let persona_mode = input
        .persona
        .map(|persona| persona.mode.as_str())
        .unwrap_or("none");
    let persona_scope = input
        .persona
        .map(|persona| persona.scope.clone())
        .unwrap_or_else(|| "none".to_string());
    let seed = ContextLineageSeed {
        workflow_id: input.workflow.id.clone(),
        task_id: input.task_id.to_string(),
        workflow_revision: input.workflow_revision,
        workflow_goal_sha256: hex_sha256(input.workflow.goal.as_bytes()),
        task_goal_sha256: hex_sha256(input.task_goal.as_bytes()),
        artifact_manifest_sha256: hex_sha256(input.artifact_manifest.as_bytes()),
        artifact_count: input.workflow.artifacts.len(),
        persona_mode_sha256: hex_sha256(persona_mode.as_bytes()),
        persona_profile_sha256: input.persona_profile_sha256.to_string(),
        persona_scope,
        revision_sources: input.revision_sources,
    };
    let lineage_sha256 = hex_sha256(serde_json::to_string(&seed)?.as_bytes());
    Ok(ContextLineage {
        workflow_revision: seed.workflow_revision,
        workflow_goal_sha256: seed.workflow_goal_sha256,
        task_goal_sha256: seed.task_goal_sha256,
        artifact_manifest_sha256: seed.artifact_manifest_sha256,
        artifact_count: seed.artifact_count,
        persona_mode_sha256: seed.persona_mode_sha256,
        persona_profile_sha256: seed.persona_profile_sha256,
        persona_scope: seed.persona_scope,
        revision_sources: seed.revision_sources,
        lineage_sha256,
    })
}

fn build_persona_profile(persona: &PersonaRoutingSpec) -> Result<ContextPersonaProfile> {
    let source_model_summaries = persona
        .source_models
        .iter()
        .map(|model_id| persona_source_model_summary(model_id))
        .collect::<Vec<_>>();
    let profile_id = persona_profile_id(&persona.mode);
    let routing_rationale = persona_routing_rationale(persona);
    let seed = ContextPersonaProfileSeed {
        schema_version: PERSONA_PROFILE_SCHEMA_VERSION,
        profile_id: profile_id.clone(),
        mode: persona.mode.clone(),
        scope: persona.scope.clone(),
        instruction_source: persona.instruction_source.clone(),
        voice: persona.voice.clone(),
        tone: persona.tone.clone(),
        validation_gate: persona.validation_gate.clone(),
        routing_rationale: routing_rationale.clone(),
        source_model_summaries: source_model_summaries.clone(),
        auditable: persona.auditable,
    };
    let profile_sha256 = hex_sha256(serde_json::to_string(&seed)?.as_bytes());

    Ok(ContextPersonaProfile {
        schema_version: PERSONA_PROFILE_SCHEMA_VERSION.to_string(),
        profile_id,
        mode: persona.mode.clone(),
        scope: persona.scope.clone(),
        instruction_source: persona.instruction_source.clone(),
        voice: persona.voice.clone(),
        tone: persona.tone.clone(),
        validation_gate: persona.validation_gate.clone(),
        routing_rationale,
        source_model_summaries,
        auditable: persona.auditable,
        profile_sha256,
    })
}

fn persona_profile_id(mode: &str) -> String {
    let sanitized = mode
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("persona.{sanitized}.v1")
}

fn persona_routing_rationale(_persona: &PersonaRoutingSpec) -> String {
    "node-scoped human-facing artifact uses explicit Codex developer/personality instructions and Paperclip soul, voice, tone and persona inputs while Forge keeps goals, validation and state authority".to_string()
}

fn persona_source_model_summary(model_id: &str) -> ContextPersonaSourceModelSummary {
    match model_id {
        "codex_developer_personality_instructions" => ContextPersonaSourceModelSummary {
            model_id: model_id.to_string(),
            source_kind: "developer_instructions".to_string(),
            summary: "Codex developer/personality instructions shape pragmatic voice, collaboration style and completion discipline for executor-facing behavior".to_string(),
        },
        "paperclip_soul_voice_tone_persona" => ContextPersonaSourceModelSummary {
            model_id: model_id.to_string(),
            source_kind: "soul_voice_tone_persona".to_string(),
            summary: "Paperclip soul, voice, tone and persona inputs shape human-facing artifact style without changing Forge state authority".to_string(),
        },
        _ => ContextPersonaSourceModelSummary {
            model_id: model_id.to_string(),
            source_kind: "declared_persona_source".to_string(),
            summary: "Declared persona source model contributes style constraints to this node-scoped profile".to_string(),
        },
    }
}

fn build_persona_contract(
    persona: &PersonaRoutingSpec,
    profile: &ContextPersonaProfile,
    lineage: &ContextLineage,
) -> ContextPersonaContract {
    ContextPersonaContract {
        schema_version: PERSONA_CONTRACT_SCHEMA_VERSION.to_string(),
        profile_id: profile.profile_id.clone(),
        mode: persona.mode.clone(),
        scope: persona.scope.clone(),
        persona_scope: lineage.persona_scope.clone(),
        instruction_source: persona.instruction_source.clone(),
        voice: persona.voice.clone(),
        tone: persona.tone.clone(),
        validation_gate: persona.validation_gate.clone(),
        routing_rationale: profile.routing_rationale.clone(),
        source_models: persona.source_models.clone(),
        source_model_summaries: profile.source_model_summaries.clone(),
        auditable: persona.auditable,
        profile_sha256: profile.profile_sha256.clone(),
        lineage_sha256: lineage.lineage_sha256.clone(),
        persona_mode_sha256: lineage.persona_mode_sha256.clone(),
    }
}

fn render_persona_context(persona: &PersonaRoutingSpec, profile: &ContextPersonaProfile) -> String {
    format!(
        "Persona mode: {}\nPersona profile: {}\nPersona scope: {}\nInstruction source: {}\nVoice: {}\nTone: {}\nValidation gate: {}\nRouting rationale: {}\nSource models: {}\nPersona profile sha256: {}\nAuditable: {}\n",
        persona.mode,
        profile.profile_id,
        persona.scope,
        persona.instruction_source,
        persona.voice,
        persona.tone,
        persona.validation_gate,
        profile.routing_rationale,
        persona.source_models.join(", "),
        profile.profile_sha256,
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
