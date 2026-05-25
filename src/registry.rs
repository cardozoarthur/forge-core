use crate::artifact::hex_sha256;
use crate::checkpoint::{load_workflow_checkpoints, TaskCheckpoint};
use crate::context::{
    build_context_handoff_summary, build_context_package_with_checkpoint, context_next_action,
    ContextHandoffSummary, ContextNextAction, DEFAULT_CONTEXT_BUDGET,
};
use crate::graph::{
    AtomicTask, ChildSubflowRef, ExecutionPolicySpec, ExecutorKind, TaskStatus, Workflow,
};
use crate::interaction::{summarize_human_interactions, HumanInteractionSummary};
use crate::request::RunRecord;
use crate::schedule::{summarize_loops, summarize_schedules, LoopSummary, ScheduleSummary};
use crate::storage::ForgeStore;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const REGISTRY_CONTEXT_HANDOFF_SCHEMA_VERSION: &str = "forge.registry_context_handoff.v1";
const REGISTRY_CONTEXT_ACTION_SCHEMA_VERSION: &str = "forge.registry_context_action.v1";
const REGISTRY_CONTEXT_QUALITY_SCHEMA_VERSION: &str = "forge.registry_context_quality.v1";
const REGISTRY_EXECUTION_POLICY_SCHEMA_VERSION: &str = "forge.registry_execution_policy.v1";
const REGISTRY_CONTEXT_ACTION_REF_SCHEMA_VERSION: &str = "forge.registry_context_action_ref.v1";
const REGISTRY_QUALITY_ACTION_SCHEMA_VERSION: &str = "forge.registry_quality_action.v1";
const REGISTRY_CONTEXT_ACTION_CATALOG_SCHEMA_VERSION: &str =
    "forge.registry_context_action_catalog.v1";
const REGISTRY_QUALITY_ACTION_CATALOG_SCHEMA_VERSION: &str =
    "forge.registry_quality_action_catalog.v1";

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRegistryReport {
    pub status: String,
    pub filter: WorkflowRegistryFilterReport,
    pub summary: WorkflowRegistrySummary,
    pub workflows: Vec<WorkflowRegistryRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRegistryFilterReport {
    pub lifecycle: String,
    pub context_action: Option<String>,
    pub quality_action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowLifecycleFilter {
    All,
    Running,
    NonRunning,
}

#[derive(Debug, Clone)]
pub struct WorkflowRegistryFilters {
    pub lifecycle: WorkflowLifecycleFilter,
    pub context_action: Option<String>,
    pub quality_action: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkflowRegistrySummary {
    pub total: usize,
    pub running: usize,
    pub non_running: usize,
    pub reusable_subflows: usize,
    pub execution_policy: RegistryExecutionPolicySummary,
    pub context_handoff: RegistryContextHandoffSummary,
    pub context_actions: RegistryContextActionSummary,
    pub context_quality: RegistryContextQualitySummary,
    pub human_interaction: HumanInteractionSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRegistryRow {
    pub workflow_id: String,
    pub run_ids: Vec<String>,
    pub run_statuses: Vec<String>,
    pub initial_request: String,
    pub current_goal: String,
    pub workflow_status: String,
    pub lifecycle_state: String,
    pub running: bool,
    pub workflow_revision: u64,
    pub artifact_count: usize,
    pub task_summary: RegistryTaskStatusSummary,
    pub execution_policy: RegistryExecutionPolicySummary,
    pub context_handoff: RegistryContextHandoffSummary,
    pub context_actions: RegistryContextActionSummary,
    pub context_action_refs: Vec<RegistryContextActionRef>,
    pub context_quality: RegistryContextQualitySummary,
    pub schedule_summary: ScheduleSummary,
    pub loop_summary: LoopSummary,
    pub human_interaction_summary: HumanInteractionSummary,
    pub quality_action: RegistryQualityAction,
    pub reusable_subflows: Vec<ReusableSubflowRef>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryTaskStatusSummary {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub blocked: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryExecutionPolicySummary {
    pub schema_version: String,
    pub workflows: usize,
    pub total_tasks: usize,
    pub ai_tasks: usize,
    pub command_tasks: usize,
    pub wait_tasks: usize,
    pub notification_tasks: usize,
    pub mixed_tasks: usize,
    pub ai_allowed_tasks: usize,
    pub no_ai_tasks: usize,
    pub deterministic_tasks: usize,
    pub model_call_required_tasks: usize,
    pub model_call_avoided_tasks: usize,
    pub local_code_nodes: usize,
    pub reusable_local_code_nodes: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryContextHandoffSummary {
    pub schema_version: String,
    pub workflows: usize,
    pub total_tasks: usize,
    pub ready_tasks: usize,
    pub blocked_tasks: usize,
    pub blocked_missing_context: usize,
    pub blocked_dependencies: usize,
    pub blocked_missing_context_and_dependencies: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryContextActionSummary {
    pub schema_version: String,
    pub workflows: usize,
    pub total_tasks: usize,
    pub ready_for_handoff: usize,
    pub blocked_tasks: usize,
    pub start_executor_handoff: usize,
    pub wait_for_dependencies: usize,
    pub increase_context_budget: usize,
    pub repair_context_and_wait_for_dependencies: usize,
    pub refresh_context_before_resume: usize,
    pub resume_from_checkpoint: usize,
    pub partial_retry_with_fresh_context: usize,
    pub partial_retry_recommended: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryContextActionRef {
    pub schema_version: String,
    pub task_id: String,
    pub title: String,
    pub executor: String,
    pub action: String,
    pub ready_for_handoff: bool,
    pub partial_retry_recommended: bool,
    pub context_ready: bool,
    pub dependency_ready: bool,
    pub handoff_status: String,
    pub routing_quality_status: String,
    pub blocking_refs: Vec<String>,
    pub checkpoint_id: Option<String>,
    pub checkpoint_context_sha256: Option<String>,
    pub checkpoint_context_routing_cache_key: Option<String>,
    pub current_context_routing_cache_key: String,
    pub context_sha256: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryContextQualitySummary {
    pub schema_version: String,
    pub workflows: usize,
    pub total_tasks: usize,
    pub passed: usize,
    pub warning: usize,
    pub blocked: usize,
    pub total_warnings: usize,
    pub blocking_warnings: usize,
    pub warning_warnings: usize,
    pub advisory_warnings: usize,
    pub required_context_missing: usize,
    pub budget_pressure: usize,
    pub compressed_context: usize,
    pub profile_filtered_optional_context: usize,
    pub min_score_bps: u32,
    pub average_score_bps: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryQualityAction {
    pub schema_version: String,
    pub action: String,
    pub priority: String,
    pub affected_tasks: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryQualityActionCatalog {
    pub status: String,
    pub schema_version: String,
    pub filter_field: String,
    pub actions: Vec<RegistryQualityActionCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryContextActionCatalog {
    pub status: String,
    pub schema_version: String,
    pub filter_field: String,
    pub actions: Vec<RegistryContextActionCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryQualityActionCatalogEntry {
    pub action: String,
    pub filter_value: String,
    pub possible_priorities: Vec<String>,
    pub description: String,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryContextActionCatalogEntry {
    pub action: String,
    pub filter_value: String,
    pub readiness: String,
    pub description: String,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReusableSubflowRef {
    pub workflow_id: String,
    pub task_id: String,
    pub title: String,
    pub executor: String,
    pub policy_mode: String,
    pub reuse_hint: String,
    pub reuse_key: String,
    pub context_lineage_sha256: String,
    pub language: Option<String>,
    pub entrypoint: Option<String>,
    pub validation_gate: String,
    pub lifecycle_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowReuseCandidate {
    pub requested_task_id: String,
    pub requested_title: String,
    pub candidate_workflow_id: String,
    pub candidate_task_id: String,
    pub candidate_title: String,
    pub reuse_key: String,
    pub context_lineage_sha256: String,
    pub policy_mode: String,
    pub validation_gate: String,
    pub candidate_lifecycle_state: String,
    pub attachable_as_child_subflow: bool,
    pub reason: String,
}

struct RegistryContextActionProjection {
    summary: RegistryContextActionSummary,
    refs: Vec<RegistryContextActionRef>,
}

pub fn list_workflows(store: &ForgeStore) -> Result<WorkflowRegistryReport> {
    list_workflows_filtered(store, WorkflowLifecycleFilter::All)
}

pub fn list_workflows_filtered(
    store: &ForgeStore,
    filter: WorkflowLifecycleFilter,
) -> Result<WorkflowRegistryReport> {
    list_workflows_with_filters(store, WorkflowRegistryFilters::new(filter))
}

pub fn list_workflows_with_filters(
    store: &ForgeStore,
    filters: WorkflowRegistryFilters,
) -> Result<WorkflowRegistryReport> {
    let workflows = store.load_workflows()?;
    let runs_by_workflow = load_runs_by_workflow(store)?;
    let mut rows = Vec::new();

    for workflow in workflows {
        let runs = runs_by_workflow
            .get(&workflow.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let checkpoints = load_workflow_checkpoints(store, &workflow.id)?;
        let handoff_summary =
            build_context_handoff_summary(&workflow, DEFAULT_CONTEXT_BUDGET, &checkpoints)?;
        let action_projection = registry_context_action_projection(&workflow, &checkpoints)?;
        rows.push(registry_row(
            &workflow,
            runs,
            &handoff_summary,
            &action_projection,
        ));
    }

    rows.retain(|row| filters.matches(row));
    let running = rows.iter().filter(|row| row.running).count();
    let reusable_subflows = rows.iter().map(|row| row.reusable_subflows.len()).sum();
    let execution_policy =
        rows.iter()
            .fold(RegistryExecutionPolicySummary::empty(), |mut total, row| {
                total.add(&row.execution_policy);
                total
            });
    let context_handoff =
        rows.iter()
            .fold(RegistryContextHandoffSummary::empty(), |mut total, row| {
                total.add(&row.context_handoff);
                total
            });
    let context_actions =
        rows.iter()
            .fold(RegistryContextActionSummary::empty(), |mut total, row| {
                total.add(&row.context_actions);
                total
            });
    let context_quality =
        rows.iter()
            .fold(RegistryContextQualitySummary::empty(), |mut total, row| {
                total.add(&row.context_quality);
                total
            });
    let human_interaction =
        rows.iter()
            .fold(empty_human_interaction_summary(), |mut total, row| {
                add_human_interaction_summary(&mut total, &row.human_interaction_summary);
                total
            });
    let summary = WorkflowRegistrySummary {
        total: rows.len(),
        running,
        non_running: rows.len().saturating_sub(running),
        reusable_subflows,
        execution_policy,
        context_handoff,
        context_actions,
        context_quality,
        human_interaction,
    };

    Ok(WorkflowRegistryReport {
        status: "loaded".to_string(),
        filter: WorkflowRegistryFilterReport {
            lifecycle: filters.lifecycle.label().to_string(),
            context_action: filters.context_action,
            quality_action: filters.quality_action,
        },
        summary,
        workflows: rows,
    })
}

pub fn quality_action_catalog() -> RegistryQualityActionCatalog {
    RegistryQualityActionCatalog {
        status: "quality_actions_loaded".to_string(),
        schema_version: REGISTRY_QUALITY_ACTION_CATALOG_SCHEMA_VERSION.to_string(),
        filter_field: "quality_action".to_string(),
        actions: vec![
            quality_action_catalog_entry(
                "repair_context_and_wait_for_dependencies",
                &["blocking"],
                "Repair required context while waiting for dependency tasks to become ready.",
                "required context is missing and dependency tasks are not ready",
            ),
            quality_action_catalog_entry(
                "increase_context_budget",
                &["blocking", "warning"],
                "Increase the context budget or reduce routed context pressure before executor handoff.",
                "required context was omitted or routing quality reports budget pressure",
            ),
            quality_action_catalog_entry(
                "wait_for_dependencies",
                &["blocking"],
                "Wait for prerequisite tasks before attempting executor handoff.",
                "dependency tasks must complete before executor handoff",
            ),
            quality_action_catalog_entry(
                "verify_executor_profile",
                &["advisory"],
                "Review executor profile selection when optional context is filtered by policy.",
                "executor profile filtered optional context sections",
            ),
            quality_action_catalog_entry(
                "review_context_summary_before_reuse",
                &["advisory"],
                "Review compressed context summaries before reuse or long-running continuation.",
                "one or more context shards were compressed to fit the route",
            ),
            quality_action_catalog_entry(
                "start_executor_handoff",
                &["ready"],
                "Start a bounded executor handoff for tasks whose context and dependencies are ready.",
                "context quality and dependencies allow executor handoff",
            ),
        ],
    }
}

pub fn context_action_catalog() -> RegistryContextActionCatalog {
    RegistryContextActionCatalog {
        status: "context_actions_loaded".to_string(),
        schema_version: REGISTRY_CONTEXT_ACTION_CATALOG_SCHEMA_VERSION.to_string(),
        filter_field: "context_action".to_string(),
        actions: vec![
            context_action_catalog_entry(
                "ready_for_handoff",
                "ready",
                "Find workflows with at least one task whose context and dependencies are ready.",
                "one or more tasks are ready for executor handoff",
            ),
            context_action_catalog_entry(
                "blocked_tasks",
                "blocked",
                "Find workflows with tasks blocked before executor handoff.",
                "one or more tasks are not ready for handoff",
            ),
            context_action_catalog_entry(
                "start_executor_handoff",
                "ready",
                "Find workflows where an executor handoff can be started now.",
                "context, quality and dependencies allow executor handoff",
            ),
            context_action_catalog_entry(
                "wait_for_dependencies",
                "blocked",
                "Find workflows whose next step is waiting for dependency tasks.",
                "dependencies are not ready for executor handoff",
            ),
            context_action_catalog_entry(
                "increase_context_budget",
                "blocked",
                "Find workflows where required context needs more budget before handoff.",
                "required context was omitted by the active context budget",
            ),
            context_action_catalog_entry(
                "repair_context_and_wait_for_dependencies",
                "blocked",
                "Find workflows that need context repair while dependencies are still blocked.",
                "required context is missing and dependencies are not ready",
            ),
            context_action_catalog_entry(
                "refresh_context_before_resume",
                "resume",
                "Find workflows that need fresh context before resuming a checkpointed task.",
                "checkpoint context is stale after workflow mutation",
            ),
            context_action_catalog_entry(
                "resume_from_checkpoint",
                "resume",
                "Find workflows that can resume from a current checkpoint.",
                "checkpoint context is current for the task route",
            ),
            context_action_catalog_entry(
                "partial_retry_with_fresh_context",
                "retry",
                "Find workflows where a partial retry should rebuild context before handoff.",
                "checkpoint route differs from current context route",
            ),
            context_action_catalog_entry(
                "partial_retry_recommended",
                "retry",
                "Find workflows with any checkpointed task that recommends partial retry.",
                "a checkpointed task requires fresh context before continuation",
            ),
        ],
    }
}

fn context_action_catalog_entry(
    action: &str,
    readiness: &str,
    description: &str,
    trigger: &str,
) -> RegistryContextActionCatalogEntry {
    RegistryContextActionCatalogEntry {
        action: action.to_string(),
        filter_value: action.to_string(),
        readiness: readiness.to_string(),
        description: description.to_string(),
        trigger: trigger.to_string(),
    }
}

fn quality_action_catalog_entry(
    action: &str,
    possible_priorities: &[&str],
    description: &str,
    trigger: &str,
) -> RegistryQualityActionCatalogEntry {
    RegistryQualityActionCatalogEntry {
        action: action.to_string(),
        filter_value: action.to_string(),
        possible_priorities: possible_priorities
            .iter()
            .map(|priority| (*priority).to_string())
            .collect(),
        description: description.to_string(),
        trigger: trigger.to_string(),
    }
}

impl WorkflowRegistryFilters {
    pub fn new(lifecycle: WorkflowLifecycleFilter) -> Self {
        Self {
            lifecycle,
            context_action: None,
            quality_action: None,
        }
    }

    pub fn with_context_action(mut self, context_action: Option<String>) -> Self {
        self.context_action = context_action;
        self
    }

    pub fn with_quality_action(mut self, quality_action: Option<String>) -> Self {
        self.quality_action = quality_action;
        self
    }

    fn matches(&self, row: &WorkflowRegistryRow) -> bool {
        if !self.lifecycle.matches(row) {
            return false;
        }

        if let Some(action) = &self.context_action {
            if !row_has_context_action(row, action) {
                return false;
            }
        }

        match &self.quality_action {
            Some(action) => row.quality_action.action == *action,
            None => true,
        }
    }
}

fn row_has_context_action(row: &WorkflowRegistryRow, action: &str) -> bool {
    let summary = &row.context_actions;
    match action {
        "ready_for_handoff" => summary.ready_for_handoff > 0,
        "blocked_tasks" => summary.blocked_tasks > 0,
        "start_executor_handoff" => summary.start_executor_handoff > 0,
        "wait_for_dependencies" => summary.wait_for_dependencies > 0,
        "increase_context_budget" => summary.increase_context_budget > 0,
        "repair_context_and_wait_for_dependencies" => {
            summary.repair_context_and_wait_for_dependencies > 0
        }
        "refresh_context_before_resume" => summary.refresh_context_before_resume > 0,
        "resume_from_checkpoint" => summary.resume_from_checkpoint > 0,
        "partial_retry_with_fresh_context" => summary.partial_retry_with_fresh_context > 0,
        "partial_retry_recommended" => summary.partial_retry_recommended > 0,
        _ => false,
    }
}

impl WorkflowLifecycleFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Running => "running",
            Self::NonRunning => "non-running",
        }
    }

    fn matches(self, row: &WorkflowRegistryRow) -> bool {
        match self {
            Self::All => true,
            Self::Running => row.running,
            Self::NonRunning => !row.running,
        }
    }
}

fn load_runs_by_workflow(store: &ForgeStore) -> Result<BTreeMap<String, Vec<RunRecord>>> {
    let mut runs_by_workflow: BTreeMap<String, Vec<RunRecord>> = BTreeMap::new();
    for value in store.load_runs()? {
        let run: RunRecord = serde_json::from_value(value)?;
        runs_by_workflow
            .entry(run.workflow_id.clone())
            .or_default()
            .push(run);
    }
    Ok(runs_by_workflow)
}

pub fn find_reuse_candidates(
    store: &ForgeStore,
    requested_workflow: &Workflow,
) -> Result<Vec<WorkflowReuseCandidate>> {
    let requested_subflows = reusable_subflows(requested_workflow, "candidate");
    if requested_subflows.is_empty() {
        return Ok(Vec::new());
    }

    let registry = list_workflows(store)?;
    let mut candidates = Vec::new();
    for requested in &requested_subflows {
        for row in &registry.workflows {
            if row.workflow_id == requested.workflow_id {
                continue;
            }
            for existing in &row.reusable_subflows {
                if existing.reuse_key != requested.reuse_key
                    || existing.context_lineage_sha256 != requested.context_lineage_sha256
                {
                    continue;
                }
                let attachable = attachable_lifecycle(&existing.lifecycle_state);
                candidates.push(WorkflowReuseCandidate {
                    requested_task_id: requested.task_id.clone(),
                    requested_title: requested.title.clone(),
                    candidate_workflow_id: existing.workflow_id.clone(),
                    candidate_task_id: existing.task_id.clone(),
                    candidate_title: existing.title.clone(),
                    reuse_key: existing.reuse_key.clone(),
                    context_lineage_sha256: existing.context_lineage_sha256.clone(),
                    policy_mode: existing.policy_mode.clone(),
                    validation_gate: existing.validation_gate.clone(),
                    candidate_lifecycle_state: existing.lifecycle_state.clone(),
                    attachable_as_child_subflow: attachable,
                    reason: if attachable {
                        "compatible deterministic code node can be attached as a child subflow"
                            .to_string()
                    } else {
                        "compatible deterministic code node exists but lifecycle is not attachable"
                            .to_string()
                    },
                });
            }
        }
    }

    candidates.sort_by(|left, right| {
        right
            .attachable_as_child_subflow
            .cmp(&left.attachable_as_child_subflow)
            .then_with(|| left.candidate_workflow_id.cmp(&right.candidate_workflow_id))
            .then_with(|| left.candidate_task_id.cmp(&right.candidate_task_id))
    });
    Ok(candidates)
}

pub fn attach_reuse_candidates_as_child_subflows(
    workflow: &mut Workflow,
    candidates: &[WorkflowReuseCandidate],
) -> usize {
    let mut attached_task_ids = BTreeSet::new();
    let mut attached = 0;

    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.attachable_as_child_subflow)
    {
        if !attached_task_ids.insert(candidate.requested_task_id.clone()) {
            continue;
        }

        let Some(task) = workflow
            .tasks
            .iter_mut()
            .find(|task| task.id == candidate.requested_task_id)
        else {
            continue;
        };

        if task.child_subflows.iter().any(|subflow| {
            subflow.workflow_id == candidate.candidate_workflow_id
                && subflow.task_id == candidate.candidate_task_id
        }) {
            continue;
        }

        task.child_subflows.push(ChildSubflowRef {
            workflow_id: candidate.candidate_workflow_id.clone(),
            task_id: candidate.candidate_task_id.clone(),
            title: candidate.candidate_title.clone(),
            binding_status: "proposed".to_string(),
            lifecycle_state: candidate.candidate_lifecycle_state.clone(),
            reuse_key: candidate.reuse_key.clone(),
            context_lineage_sha256: candidate.context_lineage_sha256.clone(),
            validation_gate: candidate.validation_gate.clone(),
            reason: candidate.reason.clone(),
        });
        attached += 1;
    }

    attached
}

fn registry_row(
    workflow: &Workflow,
    runs: &[RunRecord],
    handoff_summary: &ContextHandoffSummary,
    action_projection: &RegistryContextActionProjection,
) -> WorkflowRegistryRow {
    let task_summary = summarize_tasks(workflow);
    let lifecycle_state = derive_lifecycle_state(workflow, &task_summary);
    let running = lifecycle_state == "running";
    let workflow_revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0);
    let reusable_subflows = reusable_subflows(workflow, &lifecycle_state);
    let execution_policy = RegistryExecutionPolicySummary::from_workflow(workflow);
    let context_quality = RegistryContextQualitySummary::from_handoff_summary(handoff_summary);
    let quality_action = RegistryQualityAction::from_summaries(handoff_summary, &context_quality);
    let schedule_summary = summarize_schedules(&workflow.tasks);
    let loop_summary = summarize_loops(&workflow.tasks);
    let human_interaction_summary = summarize_human_interactions(&workflow.tasks);

    WorkflowRegistryRow {
        workflow_id: workflow.id.clone(),
        run_ids: runs.iter().map(|run| run.run_id.clone()).collect(),
        run_statuses: runs.iter().map(|run| run.status.clone()).collect(),
        initial_request: initial_request(workflow, runs),
        current_goal: workflow.goal.clone(),
        workflow_status: workflow.status.clone(),
        lifecycle_state,
        running,
        workflow_revision,
        artifact_count: workflow.artifacts.len(),
        task_summary,
        execution_policy,
        context_handoff: RegistryContextHandoffSummary::from_handoff_summary(handoff_summary),
        context_actions: action_projection.summary.clone(),
        context_action_refs: action_projection.refs.clone(),
        context_quality,
        schedule_summary,
        loop_summary,
        human_interaction_summary,
        quality_action,
        reusable_subflows,
        created_at: workflow.created_at,
    }
}

fn empty_human_interaction_summary() -> HumanInteractionSummary {
    HumanInteractionSummary {
        schema_version: "forge.human_interaction.summary.v1".to_string(),
        ..HumanInteractionSummary::default()
    }
}

fn add_human_interaction_summary(
    total: &mut HumanInteractionSummary,
    other: &HumanInteractionSummary,
) {
    total.total += other.total;
    total.required += other.required;
    total.pending += other.pending;
    total.answered += other.answered;
    total.timed_out += other.timed_out;
    total.pending_required += other.pending_required;
    total.timed_out_required += other.timed_out_required;
}

impl RegistryExecutionPolicySummary {
    fn empty() -> Self {
        Self {
            schema_version: REGISTRY_EXECUTION_POLICY_SCHEMA_VERSION.to_string(),
            workflows: 0,
            total_tasks: 0,
            ai_tasks: 0,
            command_tasks: 0,
            wait_tasks: 0,
            notification_tasks: 0,
            mixed_tasks: 0,
            ai_allowed_tasks: 0,
            no_ai_tasks: 0,
            deterministic_tasks: 0,
            model_call_required_tasks: 0,
            model_call_avoided_tasks: 0,
            local_code_nodes: 0,
            reusable_local_code_nodes: 0,
        }
    }

    fn from_workflow(workflow: &Workflow) -> Self {
        let mut summary = Self {
            workflows: 1,
            ..Self::empty()
        };

        for task in &workflow.tasks {
            summary.add_task(task);
        }

        summary
    }

    fn add_task(&mut self, task: &AtomicTask) {
        self.total_tasks += 1;

        match task.executor {
            ExecutorKind::Ai => self.ai_tasks += 1,
            ExecutorKind::Command => self.command_tasks += 1,
            ExecutorKind::Wait => self.wait_tasks += 1,
            ExecutorKind::Notification => self.notification_tasks += 1,
            ExecutorKind::Mixed => self.mixed_tasks += 1,
        }

        let policy = &task.execution_policy;
        if policy.ai_allowed {
            self.ai_allowed_tasks += 1;
        } else {
            self.no_ai_tasks += 1;
        }
        if policy.deterministic {
            self.deterministic_tasks += 1;
        }

        if policy.ai_allowed && !policy.deterministic {
            self.model_call_required_tasks += 1;
        } else if !policy.ai_allowed {
            self.model_call_avoided_tasks += 1;
        }

        if is_local_code_node(policy) {
            self.local_code_nodes += 1;
            if policy.reuse_hint == "reuse_compatible_code_node" {
                self.reusable_local_code_nodes += 1;
            }
        }
    }

    fn add(&mut self, other: &Self) {
        self.workflows += other.workflows;
        self.total_tasks += other.total_tasks;
        self.ai_tasks += other.ai_tasks;
        self.command_tasks += other.command_tasks;
        self.wait_tasks += other.wait_tasks;
        self.notification_tasks += other.notification_tasks;
        self.mixed_tasks += other.mixed_tasks;
        self.ai_allowed_tasks += other.ai_allowed_tasks;
        self.no_ai_tasks += other.no_ai_tasks;
        self.deterministic_tasks += other.deterministic_tasks;
        self.model_call_required_tasks += other.model_call_required_tasks;
        self.model_call_avoided_tasks += other.model_call_avoided_tasks;
        self.local_code_nodes += other.local_code_nodes;
        self.reusable_local_code_nodes += other.reusable_local_code_nodes;
    }
}

impl RegistryContextHandoffSummary {
    fn empty() -> Self {
        Self {
            schema_version: REGISTRY_CONTEXT_HANDOFF_SCHEMA_VERSION.to_string(),
            workflows: 0,
            total_tasks: 0,
            ready_tasks: 0,
            blocked_tasks: 0,
            blocked_missing_context: 0,
            blocked_dependencies: 0,
            blocked_missing_context_and_dependencies: 0,
        }
    }

    fn from_handoff_summary(summary: &ContextHandoffSummary) -> Self {
        Self {
            schema_version: REGISTRY_CONTEXT_HANDOFF_SCHEMA_VERSION.to_string(),
            workflows: 1,
            total_tasks: summary.total,
            ready_tasks: summary.ready,
            blocked_tasks: summary.blocked,
            blocked_missing_context: summary.blocked_missing_context,
            blocked_dependencies: summary.blocked_dependencies,
            blocked_missing_context_and_dependencies: summary
                .blocked_missing_context_and_dependencies,
        }
    }

    fn add(&mut self, other: &Self) {
        self.workflows += other.workflows;
        self.total_tasks += other.total_tasks;
        self.ready_tasks += other.ready_tasks;
        self.blocked_tasks += other.blocked_tasks;
        self.blocked_missing_context += other.blocked_missing_context;
        self.blocked_dependencies += other.blocked_dependencies;
        self.blocked_missing_context_and_dependencies +=
            other.blocked_missing_context_and_dependencies;
    }
}

impl RegistryContextActionSummary {
    fn empty() -> Self {
        Self {
            schema_version: REGISTRY_CONTEXT_ACTION_SCHEMA_VERSION.to_string(),
            workflows: 0,
            total_tasks: 0,
            ready_for_handoff: 0,
            blocked_tasks: 0,
            start_executor_handoff: 0,
            wait_for_dependencies: 0,
            increase_context_budget: 0,
            repair_context_and_wait_for_dependencies: 0,
            refresh_context_before_resume: 0,
            resume_from_checkpoint: 0,
            partial_retry_with_fresh_context: 0,
            partial_retry_recommended: 0,
        }
    }

    fn for_workflow() -> Self {
        Self {
            workflows: 1,
            ..Self::empty()
        }
    }

    fn add_action(&mut self, action: &ContextNextAction) {
        self.total_tasks += 1;
        if action.ready_for_handoff {
            self.ready_for_handoff += 1;
        } else {
            self.blocked_tasks += 1;
        }
        if action.partial_retry_recommended {
            self.partial_retry_recommended += 1;
        }

        match action.action.as_str() {
            "start_executor_handoff" => self.start_executor_handoff += 1,
            "wait_for_dependencies" => self.wait_for_dependencies += 1,
            "increase_context_budget" => self.increase_context_budget += 1,
            "repair_context_and_wait_for_dependencies" => {
                self.repair_context_and_wait_for_dependencies += 1;
            }
            "refresh_context_before_resume" => self.refresh_context_before_resume += 1,
            "resume_from_checkpoint" => self.resume_from_checkpoint += 1,
            "partial_retry_with_fresh_context" => self.partial_retry_with_fresh_context += 1,
            _ => {}
        }
    }

    fn add(&mut self, other: &Self) {
        self.workflows += other.workflows;
        self.total_tasks += other.total_tasks;
        self.ready_for_handoff += other.ready_for_handoff;
        self.blocked_tasks += other.blocked_tasks;
        self.start_executor_handoff += other.start_executor_handoff;
        self.wait_for_dependencies += other.wait_for_dependencies;
        self.increase_context_budget += other.increase_context_budget;
        self.repair_context_and_wait_for_dependencies +=
            other.repair_context_and_wait_for_dependencies;
        self.refresh_context_before_resume += other.refresh_context_before_resume;
        self.resume_from_checkpoint += other.resume_from_checkpoint;
        self.partial_retry_with_fresh_context += other.partial_retry_with_fresh_context;
        self.partial_retry_recommended += other.partial_retry_recommended;
    }
}

impl RegistryContextQualitySummary {
    fn empty() -> Self {
        Self {
            schema_version: REGISTRY_CONTEXT_QUALITY_SCHEMA_VERSION.to_string(),
            workflows: 0,
            total_tasks: 0,
            passed: 0,
            warning: 0,
            blocked: 0,
            total_warnings: 0,
            blocking_warnings: 0,
            warning_warnings: 0,
            advisory_warnings: 0,
            required_context_missing: 0,
            budget_pressure: 0,
            compressed_context: 0,
            profile_filtered_optional_context: 0,
            min_score_bps: 10_000,
            average_score_bps: 0,
        }
    }

    fn from_handoff_summary(summary: &ContextHandoffSummary) -> Self {
        let routing_quality = &summary.routing_quality;
        let mut quality = Self {
            schema_version: REGISTRY_CONTEXT_QUALITY_SCHEMA_VERSION.to_string(),
            workflows: 1,
            total_tasks: routing_quality.tasks,
            passed: routing_quality.passed,
            warning: routing_quality.warning,
            blocked: routing_quality.blocked,
            total_warnings: routing_quality.total_warnings,
            blocking_warnings: routing_quality.blocking_warnings,
            warning_warnings: routing_quality.warning_warnings,
            advisory_warnings: routing_quality.advisory_warnings,
            required_context_missing: 0,
            budget_pressure: 0,
            compressed_context: 0,
            profile_filtered_optional_context: 0,
            min_score_bps: routing_quality.min_score_bps,
            average_score_bps: routing_quality.average_score_bps,
        };

        for task in &summary.tasks {
            for warning in &task.routing_quality.warnings {
                match warning.code.as_str() {
                    "required_context_missing" => quality.required_context_missing += 1,
                    "budget_pressure" => quality.budget_pressure += 1,
                    "compressed_context" => quality.compressed_context += 1,
                    "profile_filtered_optional_context" => {
                        quality.profile_filtered_optional_context += 1;
                    }
                    _ => {}
                }
            }
        }

        quality
    }

    fn add(&mut self, other: &Self) {
        let previous_tasks = self.total_tasks;
        let previous_score_total = u64::from(self.average_score_bps) * previous_tasks as u64;
        let other_score_total = u64::from(other.average_score_bps) * other.total_tasks as u64;

        self.workflows += other.workflows;
        self.total_tasks += other.total_tasks;
        self.passed += other.passed;
        self.warning += other.warning;
        self.blocked += other.blocked;
        self.total_warnings += other.total_warnings;
        self.blocking_warnings += other.blocking_warnings;
        self.warning_warnings += other.warning_warnings;
        self.advisory_warnings += other.advisory_warnings;
        self.required_context_missing += other.required_context_missing;
        self.budget_pressure += other.budget_pressure;
        self.compressed_context += other.compressed_context;
        self.profile_filtered_optional_context += other.profile_filtered_optional_context;

        if other.total_tasks > 0 {
            self.min_score_bps = self.min_score_bps.min(other.min_score_bps);
        }
        self.average_score_bps = if self.total_tasks == 0 {
            0
        } else {
            ((previous_score_total + other_score_total) / self.total_tasks as u64) as u32
        };
    }
}

impl RegistryQualityAction {
    fn from_summaries(
        handoff: &ContextHandoffSummary,
        quality: &RegistryContextQualitySummary,
    ) -> Self {
        if quality.required_context_missing > 0 && handoff.blocked_dependencies > 0 {
            return Self::new(
                "repair_context_and_wait_for_dependencies",
                "blocking",
                quality.required_context_missing,
                "required context is missing and dependency tasks are not ready",
            );
        }

        if quality.required_context_missing > 0 || handoff.blocked_missing_context > 0 {
            return Self::new(
                "increase_context_budget",
                "blocking",
                quality
                    .required_context_missing
                    .max(handoff.blocked_missing_context),
                "required context was omitted before executor handoff",
            );
        }

        if quality.budget_pressure > 0 {
            return Self::new(
                "increase_context_budget",
                "warning",
                quality.budget_pressure,
                "routing quality reports budget pressure for one or more tasks",
            );
        }

        if handoff.blocked_dependencies > 0 {
            return Self::new(
                "wait_for_dependencies",
                "blocking",
                handoff.blocked_dependencies,
                "dependency tasks must complete before executor handoff",
            );
        }

        if quality.profile_filtered_optional_context > 0 {
            return Self::new(
                "verify_executor_profile",
                "advisory",
                quality.profile_filtered_optional_context,
                "executor profile filtered optional context sections",
            );
        }

        if quality.compressed_context > 0 {
            return Self::new(
                "review_context_summary_before_reuse",
                "advisory",
                quality.compressed_context,
                "one or more context shards were compressed to fit the route",
            );
        }

        Self::new(
            "start_executor_handoff",
            "ready",
            handoff.ready,
            "context quality and dependencies allow executor handoff",
        )
    }

    fn new(action: &str, priority: &str, affected_tasks: usize, reason: &str) -> Self {
        Self {
            schema_version: REGISTRY_QUALITY_ACTION_SCHEMA_VERSION.to_string(),
            action: action.to_string(),
            priority: priority.to_string(),
            affected_tasks,
            reason: reason.to_string(),
        }
    }
}

fn registry_context_action_projection(
    workflow: &Workflow,
    checkpoints: &[TaskCheckpoint],
) -> Result<RegistryContextActionProjection> {
    let mut summary = RegistryContextActionSummary::for_workflow();
    let mut refs = Vec::new();

    for task in &workflow.tasks {
        let latest_checkpoint = checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.task_id == task.id)
            .cloned();
        let package = build_context_package_with_checkpoint(
            workflow,
            &task.id,
            DEFAULT_CONTEXT_BUDGET,
            latest_checkpoint,
        )?;
        let action = context_next_action(&package);
        summary.add_action(&action);
        refs.push(registry_context_action_ref(task, &package, &action));
    }

    Ok(RegistryContextActionProjection { summary, refs })
}

fn registry_context_action_ref(
    task: &AtomicTask,
    package: &crate::context::ContextPackage,
    action: &ContextNextAction,
) -> RegistryContextActionRef {
    RegistryContextActionRef {
        schema_version: REGISTRY_CONTEXT_ACTION_REF_SCHEMA_VERSION.to_string(),
        task_id: task.id.clone(),
        title: task.title.clone(),
        executor: executor_kind(&task.executor).to_string(),
        action: action.action.clone(),
        ready_for_handoff: action.ready_for_handoff,
        partial_retry_recommended: action.partial_retry_recommended,
        context_ready: package.context_ready,
        dependency_ready: package.dependency_summary.ready,
        handoff_status: package.handoff_status.clone(),
        routing_quality_status: package.routing_quality.status.clone(),
        blocking_refs: action.blocking_refs.clone(),
        checkpoint_id: action.checkpoint_id.clone(),
        checkpoint_context_sha256: action.checkpoint_context_sha256.clone(),
        checkpoint_context_routing_cache_key: action.checkpoint_context_routing_cache_key.clone(),
        current_context_routing_cache_key: action.current_context_routing_cache_key.clone(),
        context_sha256: package.context_sha256.clone(),
        reason: action.reason.clone(),
    }
}

fn initial_request(workflow: &Workflow, runs: &[RunRecord]) -> String {
    workflow
        .initial_goal
        .as_ref()
        .filter(|goal| !goal.trim().is_empty())
        .cloned()
        .or_else(|| runs.first().map(|run| run.goal.clone()))
        .unwrap_or_else(|| workflow.goal.clone())
}

fn derive_lifecycle_state(workflow: &Workflow, task_summary: &RegistryTaskStatusSummary) -> String {
    if workflow.status == "failed" || task_summary.failed > 0 {
        return "failed".to_string();
    }
    if workflow.status == "blocked" || task_summary.blocked > 0 {
        return "blocked".to_string();
    }
    if task_summary.running > 0 {
        return "running".to_string();
    }
    if workflow.status == "scaled_to_zero" {
        return "scaled_to_zero".to_string();
    }
    if workflow.status == "completed" {
        if task_summary.total == task_summary.completed {
            return "scaled_to_zero".to_string();
        }
        return "completed".to_string();
    }
    "idle".to_string()
}

fn reusable_subflows(workflow: &Workflow, lifecycle_state: &str) -> Vec<ReusableSubflowRef> {
    workflow
        .tasks
        .iter()
        .filter_map(|task| reusable_subflow(workflow, lifecycle_state, task))
        .collect()
}

fn reusable_subflow(
    workflow: &Workflow,
    lifecycle_state: &str,
    task: &AtomicTask,
) -> Option<ReusableSubflowRef> {
    let policy = &task.execution_policy;
    if policy.reuse_hint != "reuse_compatible_code_node" {
        return None;
    }
    let reuse_key = execution_policy_reuse_key(policy)?;
    let runtime = policy.code_runtime.as_ref();
    Some(ReusableSubflowRef {
        workflow_id: workflow.id.clone(),
        task_id: task.id.clone(),
        title: task.title.clone(),
        executor: executor_kind(&task.executor).to_string(),
        policy_mode: policy.mode.clone(),
        reuse_hint: policy.reuse_hint.clone(),
        reuse_key,
        context_lineage_sha256: context_lineage_key(task),
        language: runtime.map(|runtime| runtime.language.clone()),
        entrypoint: runtime.map(|runtime| runtime.entrypoint.clone()),
        validation_gate: policy.validation_gate.clone(),
        lifecycle_state: lifecycle_state.to_string(),
    })
}

fn execution_policy_reuse_key(policy: &ExecutionPolicySpec) -> Option<String> {
    if !is_local_code_node(policy) {
        return None;
    }
    let runtime = policy.code_runtime.as_ref()?;
    Some(format!(
        "{}:{}:{}:{}",
        policy.mode, runtime.language, runtime.entrypoint, policy.validation_gate
    ))
}

fn is_local_code_node(policy: &ExecutionPolicySpec) -> bool {
    policy.mode == "local_code_node"
        && !policy.ai_allowed
        && policy.deterministic
        && policy.code_runtime.is_some()
}

fn context_lineage_key(task: &AtomicTask) -> String {
    let validation_rules = task
        .validation_rules
        .iter()
        .map(|rule| {
            format!(
                "{}:{}:{}",
                rule.kind,
                rule.command.as_deref().unwrap_or(""),
                rule.expected
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let seed = format!(
        "{}\n{}\n{}\n{}",
        task.title,
        task.expected_output,
        task.context_requirements.join("\n"),
        validation_rules
    );
    hex_sha256(seed.as_bytes())
}

fn attachable_lifecycle(lifecycle_state: &str) -> bool {
    matches!(lifecycle_state, "idle" | "completed" | "scaled_to_zero")
}

fn summarize_tasks(workflow: &Workflow) -> RegistryTaskStatusSummary {
    let mut summary = RegistryTaskStatusSummary {
        total: workflow.tasks.len(),
        ..RegistryTaskStatusSummary::default()
    };
    for task in &workflow.tasks {
        match &task.status {
            TaskStatus::Pending => summary.pending += 1,
            TaskStatus::Running => summary.running += 1,
            TaskStatus::Completed => summary.completed += 1,
            TaskStatus::Blocked => summary.blocked += 1,
            TaskStatus::Failed => summary.failed += 1,
        }
    }
    summary
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
