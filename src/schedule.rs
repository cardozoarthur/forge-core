use crate::artifact::hex_sha256;
use crate::graph::{
    ArtifactLineageRecord, ArtifactRecord, AtomicTask, LoopSpec, NativeSubflowSpec,
    ScheduleRunRecord, ScheduleSpec, TaskStatus, Workflow, WorkflowRevision,
};
use crate::identity::ensure_workflow_policy;
use crate::intent::parse_intent;
use crate::lease::{acquire_task_lease, release_task_lease, TaskLease};
use crate::registry::{attach_reuse_candidates_as_child_subflows, find_reuse_candidates};
use crate::storage::ForgeStore;
use crate::worker::{Job, WorkerPool, WorkerPoolReport};
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

const SCHEDULE_SUMMARY_SCHEMA_VERSION: &str = "forge.schedule.summary.v1";
const LOOP_SUMMARY_SCHEMA_VERSION: &str = "forge.loop.summary.v1";
const MISSED_RUN_RECONCILIATION_SCHEMA_VERSION: &str = "forge.missed_run_reconciliation.v1";
const SCALE_TO_ZERO_DECISION_SCHEMA_VERSION: &str = "forge.scale_to_zero_decision.v1";
const SCHEDULE_SCAN_DUE_SCHEMA_VERSION: &str = "forge.schedule.scan_due.v1";
const SCHEDULE_WORKER_STATUS_SCHEMA_VERSION: &str = "forge.schedule.worker_status.v1";
const DAILY_GOAL_EXECUTION_SCHEMA_VERSION: &str = "forge.daily_goal_research.execution.v1";
const DAILY_GOAL_MAX_ARTIFACT_WORKERS: usize = 4;
const MISSED_RUN_GRACE_MINUTES: i64 = 5;
const ARTIFACT_DIRECTORY_NAME: &str = "artifacts";
const ENCODED_ARTIFACT_COMPONENT_PREFIX: &str = "sha256-";
const SCHEDULE_RUN_ARTIFACT_PREFIX: &str = "schedule-run-";

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScheduleSummary {
    pub schema_version: String,
    pub scheduled_nodes: usize,
    pub cron_nodes: usize,
    #[serde(default)]
    pub wait_until_nodes: usize,
    #[serde(default)]
    pub delay_nodes: usize,
    pub due_nodes: usize,
    pub missed_run_nodes: usize,
    pub scale_to_zero_when_idle_nodes: usize,
    pub next_run_at: Option<String>,
    pub timezones: Vec<String>,
    pub missed_run_policies: Vec<String>,
    pub missed_run_reconciliation_actions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LoopSummary {
    pub schema_version: String,
    pub loop_nodes: usize,
    pub loop_over_items_nodes: usize,
    pub bounded_repeat_nodes: usize,
    pub retry_backoff_nodes: usize,
    pub while_until_nodes: usize,
    pub infinite_recurring_subflow_nodes: usize,
    pub total_items: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyGoalResearchWorkflowReport {
    pub status: String,
    pub workflow_id: String,
    pub origin: String,
    pub goals: Vec<String>,
    pub workflow: Workflow,
    pub attached_subflows: usize,
    pub schedule_summary: ScheduleSummary,
    pub loop_summary: LoopSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub revision: u64,
    pub schedule: ScheduleSpec,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScheduleUpdateOptions<'a> {
    pub cron: Option<&'a str>,
    pub timezone: Option<&'a str>,
    pub missed_run_policy: Option<&'a str>,
    pub next_run_at: Option<&'a str>,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyGoalResearchSmokeReport {
    pub status: String,
    pub workflow_id: String,
    pub goals: Vec<DailyGoalSmokeGoalReport>,
    pub artifact_count: usize,
    pub execution: DailyGoalSmokeExecutionReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyGoalSmokeExecutionReport {
    pub schema_version: String,
    pub mode: String,
    pub max_workers: usize,
    pub worker_count: usize,
    pub total_goals: usize,
    pub bounded: bool,
    pub concurrency_used: bool,
    pub deterministic_output_order: bool,
    pub goal_order: Vec<String>,
    pub waves: Vec<DailyGoalSmokeExecutionWave>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyGoalSmokeExecutionWave {
    pub level: usize,
    pub worker_count: usize,
    pub goals: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyGoalSmokeGoalReport {
    pub goal: String,
    pub markdown_path: String,
    pub pdf_path: String,
    pub lineage: ArtifactLineageRecord,
    pub telegram_delivery: TelegramDeliveryRecord,
}

#[derive(Debug, Clone, Serialize)]
pub struct TelegramDeliveryRecord {
    pub schema_version: String,
    pub goal: String,
    pub lineage: ArtifactLineageRecord,
    pub status: String,
    pub channel: String,
    pub configured_telegram_chat_ref: String,
    pub markdown_path: String,
    pub pdf_path: String,
    pub secret_exposed: bool,
    pub simulated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissedRunReconciliationReport {
    pub schema_version: String,
    pub task_id: String,
    pub policy: String,
    pub action: String,
    pub scheduled_at: String,
    pub observed_at: String,
    pub next_run_at_before: String,
    pub next_run_at_after: Option<String>,
    pub run_id: String,
    pub run_status: String,
    pub missed: bool,
    pub artifacts_allowed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScaleToZeroDecision {
    pub schema_version: String,
    pub applied: bool,
    pub reason: String,
    pub next_wakeup_at: Option<String>,
    pub scheduled_nodes: usize,
    pub due_nodes: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScheduleScanDueSummary {
    pub scanned_workflows: usize,
    pub due_workflows: usize,
    pub leased_workflows: usize,
    pub lease_conflicts: usize,
    pub executed_workflows: usize,
    pub idle_workflows: usize,
    pub scale_to_zero_workflows: usize,
    pub skipped_workflows: usize,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub max_workers: usize,
    #[serde(default)]
    pub wave_count: usize,
    #[serde(default)]
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleScanWorkflowReport {
    pub workflow_id: String,
    pub goal: String,
    pub status: String,
    pub schedule_task_id: Option<String>,
    pub due_nodes: usize,
    pub lease_status: String,
    pub lease_id: Option<String>,
    pub lease_released: bool,
    pub current_lease_id: Option<String>,
    pub run_due: Option<ScheduleRunDueReport>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleScanDueReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub ttl_seconds: u64,
    pub summary: ScheduleScanDueSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_pool: Option<WorkerPoolReport>,
    pub results: Vec<ScheduleScanWorkflowReport>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScheduleWorkerStatusSummary {
    pub scanned_workflows: usize,
    pub due_workflows: usize,
    pub runnable_due_workflows: usize,
    pub blocked_due_workflows: usize,
    pub idle_workflows: usize,
    pub scale_to_zero_workflows: usize,
    pub paused_or_stopped_loop_workflows: usize,
    pub scheduled_nodes: usize,
    #[serde(default)]
    pub cron_nodes: usize,
    #[serde(default)]
    pub wait_until_nodes: usize,
    #[serde(default)]
    pub delay_nodes: usize,
    pub due_nodes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerPoolStatus {
    pub max_workers: usize,
    pub available_workers: usize,
    pub assignable_due_workflows: usize,
    pub worker_kind: String,
    pub deterministic: bool,
    pub assignment_plan: ScheduleWorkerAssignmentPlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerAssignmentPlan {
    pub schema_version: String,
    pub max_workers: usize,
    pub assigned: Vec<ScheduleWorkerAssignment>,
    pub queued: Vec<ScheduleWorkerAssignment>,
    pub deterministic_ordering: bool,
    pub ordering_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerAssignment {
    pub workflow_id: String,
    pub goal: String,
    pub schedule_task_id: String,
    pub due_nodes: usize,
    pub next_run_at: Option<String>,
    pub lease_scope: String,
    pub wave: usize,
    pub queue_position: usize,
    pub executor: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerSleepPlan {
    pub sleep_until_next_wakeup: bool,
    pub next_wakeup_at: Option<String>,
    pub sleep_seconds: u64,
    pub mode: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerBackpressure {
    pub active: bool,
    pub queued_due_workflows: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerCancellation {
    pub supported: bool,
    pub lease_ttl_seconds: u64,
    pub safe_points: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerWorkflowStatus {
    pub workflow_id: String,
    pub goal: String,
    pub status: String,
    pub due_nodes: usize,
    pub next_wakeup_at: Option<String>,
    pub scale_to_zero_eligible: bool,
    pub blocked_loop_task_id: Option<String>,
    pub blocked_loop_state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWorkerStatusReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub observed_at: String,
    pub ttl_seconds: u64,
    pub summary: ScheduleWorkerStatusSummary,
    pub worker_pool: ScheduleWorkerPoolStatus,
    pub sleep: ScheduleWorkerSleepPlan,
    pub backpressure: ScheduleWorkerBackpressure,
    pub cancellation: ScheduleWorkerCancellation,
    pub workflows: Vec<ScheduleWorkerWorkflowStatus>,
}

pub fn summarize_schedules(tasks: &[AtomicTask]) -> ScheduleSummary {
    let mut summary = ScheduleSummary {
        schema_version: SCHEDULE_SUMMARY_SCHEMA_VERSION.to_string(),
        ..ScheduleSummary::default()
    };
    let now = Utc::now();

    for schedule in tasks.iter().filter_map(|task| task.schedule.as_ref()) {
        summary.scheduled_nodes += 1;
        if schedule.kind == "cron" {
            summary.cron_nodes += 1;
        } else if schedule.kind == "wait_until" {
            summary.wait_until_nodes += 1;
        } else if schedule.kind == "delay" {
            summary.delay_nodes += 1;
        }
        if schedule
            .next_run_at
            .as_ref()
            .is_some_and(|next| *next <= now)
        {
            summary.due_nodes += 1;
        }
        if schedule.run_history.iter().any(|run| run.missed) {
            summary.missed_run_nodes += 1;
        }
        if !summary
            .missed_run_policies
            .contains(&schedule.missed_run_policy)
        {
            summary
                .missed_run_policies
                .push(schedule.missed_run_policy.clone());
        }
        for action in schedule
            .run_history
            .iter()
            .filter(|run| run.missed)
            .map(|run| run.reconciliation_action.clone())
        {
            if !summary.missed_run_reconciliation_actions.contains(&action) {
                summary.missed_run_reconciliation_actions.push(action);
            }
        }
        if schedule.scale_to_zero_when_idle {
            summary.scale_to_zero_when_idle_nodes += 1;
        }
        if !summary.timezones.contains(&schedule.timezone) {
            summary.timezones.push(schedule.timezone.clone());
        }
        let next_run = schedule
            .next_run_at
            .as_ref()
            .map(|value| value.to_rfc3339());
        summary.next_run_at = earliest_next_run(summary.next_run_at.take(), next_run);
    }

    summary
}

pub fn summarize_loops(tasks: &[AtomicTask]) -> LoopSummary {
    let mut summary = LoopSummary {
        schema_version: LOOP_SUMMARY_SCHEMA_VERSION.to_string(),
        ..LoopSummary::default()
    };

    for loop_control in tasks.iter().filter_map(|task| task.loop_control.as_ref()) {
        summary.loop_nodes += 1;
        summary.total_items += loop_control.items.len();
        match loop_control.kind.as_str() {
            "loop_over_items" => summary.loop_over_items_nodes += 1,
            "bounded_repeat" => summary.bounded_repeat_nodes += 1,
            "retry_backoff" => summary.retry_backoff_nodes += 1,
            "while_until" => summary.while_until_nodes += 1,
            "infinite_recurring_subflow" => summary.infinite_recurring_subflow_nodes += 1,
            _ => {}
        }
    }

    summary
}

pub fn create_daily_goal_research_workflow(
    store: &ForgeStore,
    goals: Vec<String>,
    timezone: &str,
    cron: &str,
    origin: &str,
) -> Result<DailyGoalResearchWorkflowReport> {
    let goals = normalize_goals(goals);
    let goal_text = format!(
        "Create daily Goal research workflow for Goals: {} in {} cron {}",
        goals.join(", "),
        timezone,
        cron
    );
    let intent = parse_intent(&goal_text);
    let mut workflow = crate::graph::create_workflow(intent);
    let reuse_candidates = find_reuse_candidates(store, &workflow)?;
    let attached_subflows =
        attach_reuse_candidates_as_child_subflows(&mut workflow, &reuse_candidates);
    let workflow_id = workflow.id.clone();
    store.with_transaction(|| {
        store.save_workflow(&workflow)?;
        store.record_event(
            &workflow.id,
            "daily_goal_research_workflow_created",
            &serde_json::json!({
                "origin": origin,
                "goals": goals,
                "timezone": timezone,
                "cron": cron,
                "attached_subflows": attached_subflows
            }),
        )?;
        Ok(())
    })?;

    Ok(DailyGoalResearchWorkflowReport {
        status: "daily_goal_research_workflow_created".to_string(),
        workflow_id,
        origin: origin.to_string(),
        goals,
        schedule_summary: summarize_schedules(&workflow.tasks),
        loop_summary: summarize_loops(&workflow.tasks),
        workflow,
        attached_subflows,
    })
}

pub fn update_workflow_schedule(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    options: ScheduleUpdateOptions<'_>,
) -> Result<ScheduleUpdateReport> {
    let parsed_next_run_at = options
        .next_run_at
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|parsed| parsed.with_timezone(&Utc))
                .with_context(|| format!("invalid next_run_at RFC3339 timestamp: {value}"))
        })
        .transpose()?;
    let (schedule, revision) = store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "schedule update")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        let schedule = {
            let task = workflow
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .with_context(|| {
                    format!("scheduled task not found in workflow {workflow_id}: {task_id}")
                })?;
            let schedule = task
                .schedule
                .as_mut()
                .with_context(|| format!("task {task_id} is not a scheduled node"))?;
            if let Some(cron) = options.cron {
                schedule.cron = cron.to_string();
            }
            if let Some(timezone) = options.timezone {
                schedule.timezone = timezone.to_string();
            }
            if let Some(missed_run_policy) = options.missed_run_policy {
                schedule.missed_run_policy = missed_run_policy.to_string();
            }
            schedule.next_run_at =
                parsed_next_run_at.or_else(|| Some(Utc::now() + Duration::days(1)));
            schedule.clone()
        };
        let revision = push_schedule_revision(
            &mut workflow,
            options.origin,
            &format!("updated schedule for task {task_id}"),
        );
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "schedule_updated",
            &serde_json::json!({
                "origin": options.origin,
                "task_id": task_id,
                "revision": revision,
                "schedule": schedule
            }),
        )?;
        Ok((schedule, revision))
    })?;

    Ok(ScheduleUpdateReport {
        status: "schedule_updated".to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: options.origin.to_string(),
        revision,
        schedule,
    })
}

pub fn run_daily_goal_research_smoke(
    store: &ForgeStore,
    workflow: &mut Workflow,
) -> Result<Option<DailyGoalResearchSmokeReport>> {
    run_daily_goal_research_smoke_with_schedule_mode(store, workflow, false, None)
}

fn run_daily_goal_research_smoke_with_schedule_mode(
    store: &ForgeStore,
    workflow: &mut Workflow,
    due_only: bool,
    artifact_journal: Option<&ArtifactWriteJournal>,
) -> Result<Option<DailyGoalResearchSmokeReport>> {
    let goals = configured_goal_items(workflow);
    if goals.is_empty() {
        return Ok(None);
    }

    let schedule_runs = record_schedule_run_history(workflow, due_only);

    let execution = build_daily_goal_execution_report(&goals);
    let goal_lineage = goals
        .iter()
        .map(|goal| {
            (
                goal.clone(),
                build_daily_goal_artifact_lineage(workflow, goal, &schedule_runs),
            )
        })
        .collect::<Vec<_>>();
    let generated = generate_daily_goal_artifacts_bounded(
        store.base_dir().as_path(),
        &workflow.id,
        &goal_lineage,
        &execution,
        due_only,
        artifact_journal,
    )?;
    let mut reports = generated
        .into_iter()
        .map(|artifacts| register_generated_goal_artifacts(workflow, artifacts))
        .collect::<Vec<_>>();
    reports.sort_by_key(|report| {
        execution
            .goal_order
            .iter()
            .position(|goal| goal == &report.goal)
            .unwrap_or(usize::MAX)
    });

    Ok(Some(DailyGoalResearchSmokeReport {
        status: "smoke_artifacts_generated".to_string(),
        workflow_id: workflow.id.clone(),
        artifact_count: reports.len() * 3,
        goals: reports,
        execution,
    }))
}

#[derive(Debug, Clone)]
struct ScheduleRunEmission {
    schedule_task_id: String,
    run_id: String,
}

fn record_schedule_run_history(
    workflow: &mut Workflow,
    due_only: bool,
) -> Vec<ScheduleRunEmission> {
    record_schedule_run_history_with_status(workflow, due_only, "completed")
}

fn record_schedule_run_history_with_status(
    workflow: &mut Workflow,
    due_only: bool,
    status: &str,
) -> Vec<ScheduleRunEmission> {
    let started_at = Utc::now();
    let finished_at = Utc::now();
    let mut emissions = Vec::new();
    for task in workflow.tasks.iter_mut() {
        let Some(schedule) = task.schedule.as_mut() else {
            continue;
        };
        let scheduled_at = if due_only {
            let Some(next_run_at) = schedule.next_run_at else {
                continue;
            };
            if next_run_at > started_at {
                continue;
            }
            next_run_at
        } else {
            started_at
        };
        let missed = is_missed_run(scheduled_at, started_at);
        let missed_run_policy = schedule.missed_run_policy.clone();
        let reconciliation_action = missed_run_action(&missed_run_policy, missed, status);
        let run_id = format!("run_{}", Uuid::new_v4().to_string().replace('-', ""));
        let one_shot_wait = is_one_shot_wait_kind(&schedule.kind);
        let next_run_at_after = (!one_shot_wait).then_some(finished_at + Duration::days(1));
        schedule.run_history.push(ScheduleRunRecord {
            run_id: run_id.clone(),
            scheduled_at,
            started_at: Some(started_at),
            finished_at: Some(finished_at),
            status: status.to_string(),
            missed,
            missed_run_policy: missed_run_policy.clone(),
            reconciliation_action: reconciliation_action.clone(),
        });
        schedule.next_run_at = next_run_at_after;
        if one_shot_wait {
            task.status = TaskStatus::Completed;
        }
        emissions.push(ScheduleRunEmission {
            schedule_task_id: task.id.clone(),
            run_id,
        });
    }
    emissions
}

pub fn configured_goal_items(workflow: &Workflow) -> Vec<String> {
    let mut goals = Vec::new();
    for loop_control in workflow
        .tasks
        .iter()
        .filter_map(|task| task.loop_control.as_ref())
    {
        for item in &loop_control.items {
            if !goals.contains(item) {
                goals.push(item.clone());
            }
        }
    }
    goals
}

pub fn task_loop_kind(loop_control: Option<&LoopSpec>) -> Option<&str> {
    loop_control.map(|loop_control| loop_control.kind.as_str())
}

pub fn task_subflow_id(native_subflow: Option<&NativeSubflowSpec>) -> Option<&str> {
    native_subflow.map(|subflow| subflow.subflow_id.as_str())
}

fn earliest_next_run(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn normalize_goals(goals: Vec<String>) -> Vec<String> {
    let mut normalized = goals
        .into_iter()
        .map(|goal| goal.trim().to_ascii_lowercase())
        .filter(|goal| !goal.is_empty())
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        normalized.push("hackathon".to_string());
    }
    normalized.sort();
    normalized.dedup();
    normalized
}

#[derive(Debug, Clone, Serialize)]
pub struct LoopStateUpdateReport {
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub previous_state: String,
    pub new_state: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleRunDueReport {
    pub status: String,
    pub workflow_id: String,
    pub goal: String,
    pub due_executed: bool,
    pub blocked_loop_task_id: Option<String>,
    pub blocked_loop_state: Option<String>,
    pub missed_run_reconciliation: Vec<MissedRunReconciliationReport>,
    pub daily_goal_research: Option<DailyGoalResearchSmokeReport>,
    pub schedule_summary: ScheduleSummary,
    pub scale_to_zero: ScaleToZeroDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregateSummaryReport {
    pub schema_version: String,
    pub summary: ScheduleSummary,
    pub loop_summary: LoopSummary,
    pub workflow_count: usize,
    pub scale_to_zero_workflows: usize,
    pub paused_or_stopped_loop_workflows: usize,
}

pub fn aggregate_summary(tasks_by_workflow: &[&[AtomicTask]]) -> AggregateSummaryReport {
    let mut total_schedule = ScheduleSummary {
        schema_version: SCHEDULE_SUMMARY_SCHEMA_VERSION.to_string(),
        ..ScheduleSummary::default()
    };
    let mut total_loop = LoopSummary {
        schema_version: LOOP_SUMMARY_SCHEMA_VERSION.to_string(),
        ..LoopSummary::default()
    };
    let mut workflow_count = 0usize;
    let mut scale_to_zero_count = 0usize;
    let mut paused_or_stopped_count = 0usize;

    for tasks in tasks_by_workflow {
        let s = summarize_schedules(tasks);
        total_schedule.scheduled_nodes += s.scheduled_nodes;
        total_schedule.cron_nodes += s.cron_nodes;
        total_schedule.wait_until_nodes += s.wait_until_nodes;
        total_schedule.delay_nodes += s.delay_nodes;
        total_schedule.due_nodes += s.due_nodes;
        total_schedule.missed_run_nodes += s.missed_run_nodes;
        total_schedule.scale_to_zero_when_idle_nodes += s.scale_to_zero_when_idle_nodes;
        for tz in s.timezones {
            if !total_schedule.timezones.contains(&tz) {
                total_schedule.timezones.push(tz);
            }
        }
        for p in s.missed_run_policies {
            if !total_schedule.missed_run_policies.contains(&p) {
                total_schedule.missed_run_policies.push(p);
            }
        }
        for a in s.missed_run_reconciliation_actions {
            if !total_schedule
                .missed_run_reconciliation_actions
                .contains(&a)
            {
                total_schedule.missed_run_reconciliation_actions.push(a);
            }
        }
        total_schedule.next_run_at =
            earliest_next_run(total_schedule.next_run_at.take(), s.next_run_at);

        let l = summarize_loops(tasks);
        total_loop.loop_nodes += l.loop_nodes;
        total_loop.loop_over_items_nodes += l.loop_over_items_nodes;
        total_loop.bounded_repeat_nodes += l.bounded_repeat_nodes;
        total_loop.retry_backoff_nodes += l.retry_backoff_nodes;
        total_loop.while_until_nodes += l.while_until_nodes;
        total_loop.infinite_recurring_subflow_nodes += l.infinite_recurring_subflow_nodes;
        total_loop.total_items += l.total_items;

        workflow_count += 1;
        if s.due_nodes == 0
            && s.scale_to_zero_when_idle_nodes == s.scheduled_nodes
            && s.scheduled_nodes > 0
        {
            scale_to_zero_count += 1;
        }
        if l.loop_nodes > 0 {
            let all_paused_or_stopped = tasks
                .iter()
                .filter_map(|t| t.loop_control.as_ref())
                .all(|lc| lc.state == "paused" || lc.state == "stopped");
            if all_paused_or_stopped {
                paused_or_stopped_count += 1;
            }
        }
    }

    AggregateSummaryReport {
        schema_version: "forge.schedule.aggregate_summary.v1".to_string(),
        summary: total_schedule,
        loop_summary: total_loop,
        workflow_count,
        scale_to_zero_workflows: scale_to_zero_count,
        paused_or_stopped_loop_workflows: paused_or_stopped_count,
    }
}

pub fn update_loop_state(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    new_state: &str,
    origin: &str,
) -> Result<LoopStateUpdateReport> {
    let valid_states = ["active", "paused", "stopped"];
    if !valid_states.contains(&new_state) {
        anyhow::bail!("invalid loop state: {new_state}. Valid states: active, paused, stopped");
    }

    let (previous_state, revision) = store.with_transaction(|| {
        ensure_workflow_policy(store, workflow_id, "loop state update")?;
        let mut workflow = store.load_workflow(workflow_id)?;
        let previous_state = {
            let task = workflow
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .with_context(|| format!("task not found in workflow {workflow_id}: {task_id}"))?;
            let loop_control = task
                .loop_control
                .as_mut()
                .with_context(|| format!("task {task_id} is not a loop node"))?;
            let previous = loop_control.state.clone();
            loop_control.state = new_state.to_string();
            previous
        };

        let revision = push_loop_revision(
            &mut workflow,
            origin,
            &format!("loop state changed from {previous_state} to {new_state} for task {task_id}"),
        );
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "loop_state_updated",
            &serde_json::json!({
                "origin": origin,
                "task_id": task_id,
                "previous_state": previous_state,
                "new_state": new_state,
                "revision": revision
            }),
        )?;
        Ok((previous_state, revision))
    })?;

    Ok(LoopStateUpdateReport {
        status: "loop_state_updated".to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: origin.to_string(),
        previous_state,
        new_state: new_state.to_string(),
        revision,
    })
}

pub fn run_due_workflow(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<Option<ScheduleRunDueReport>> {
    let artifact_journal = ArtifactWriteJournal::default();
    let result = store.with_transaction(|| {
        run_due_workflow_in_transaction(store, workflow_id, Some(&artifact_journal))
    });
    rollback_artifacts_on_error(result, &artifact_journal)
}

fn run_due_workflow_in_transaction(
    store: &ForgeStore,
    workflow_id: &str,
    artifact_journal: Option<&ArtifactWriteJournal>,
) -> Result<Option<ScheduleRunDueReport>> {
    ensure_workflow_policy(store, workflow_id, "run due workflow")?;
    let mut workflow = store.load_workflow(workflow_id)?;
    reconcile_uncommitted_schedule_artifacts(store.base_dir().as_path(), &workflow)?;
    let now = Utc::now();
    let has_due = workflow.tasks.iter().any(|task| {
        task.schedule
            .as_ref()
            .and_then(|schedule| schedule.next_run_at)
            .is_some_and(|next| next <= now)
    });

    if !has_due {
        let schedule_summary = summarize_schedules(&workflow.tasks);
        let scale_to_zero = scale_to_zero_decision(
            &schedule_summary,
            can_scale_to_zero_when_idle(&schedule_summary),
            "finite_workflow_has_no_due_scheduled_work",
        );
        if scale_to_zero.applied {
            workflow.status = "scaled_to_zero".to_string();
            store.save_workflow(&workflow)?;
            store.record_event(
                workflow_id,
                "workflow_scaled_to_zero",
                &serde_json::json!({
                    "workflow_id": workflow_id,
                    "reason": scale_to_zero.reason,
                    "next_wakeup_at": scale_to_zero.next_wakeup_at,
                }),
            )?;
        }

        return Ok(Some(ScheduleRunDueReport {
            status: no_due_schedule_status(&schedule_summary),
            workflow_id: workflow_id.to_string(),
            goal: workflow.goal.clone(),
            due_executed: false,
            blocked_loop_task_id: None,
            blocked_loop_state: None,
            missed_run_reconciliation: Vec::new(),
            daily_goal_research: None,
            schedule_summary,
            scale_to_zero,
        }));
    }

    if let Some((task_id, state)) = workflow.tasks.iter().find_map(|task| {
        task.loop_control.as_ref().and_then(|loop_control| {
            if matches!(loop_control.state.as_str(), "paused" | "stopped") {
                Some((task.id.clone(), loop_control.state.clone()))
            } else {
                None
            }
        })
    }) {
        let schedule_summary = summarize_schedules(&workflow.tasks);
        return Ok(Some(ScheduleRunDueReport {
            status: "loop_not_runnable".to_string(),
            workflow_id: workflow_id.to_string(),
            goal: workflow.goal.clone(),
            due_executed: false,
            blocked_loop_task_id: Some(task_id),
            blocked_loop_state: Some(state),
            missed_run_reconciliation: Vec::new(),
            daily_goal_research: None,
            scale_to_zero: scale_to_zero_decision(
                &schedule_summary,
                false,
                "loop_node_paused_or_stopped",
            ),
            schedule_summary,
        }));
    }

    let skipped_reconciliation = skip_due_missed_runs(&mut workflow);
    if !skipped_reconciliation.is_empty() {
        let workflow_id = workflow.id.clone();
        let schedule_summary = summarize_schedules(&workflow.tasks);
        store.save_workflow(&workflow)?;
        store.record_event(
            &workflow_id,
            "missed_runs_skipped",
            &serde_json::json!({
                "workflow_id": workflow_id,
                "policy": "skip_missed",
                "reconciliation": skipped_reconciliation,
            }),
        )?;

        return Ok(Some(ScheduleRunDueReport {
            status: "missed_runs_skipped".to_string(),
            workflow_id,
            goal: workflow.goal.clone(),
            due_executed: false,
            blocked_loop_task_id: None,
            blocked_loop_state: None,
            missed_run_reconciliation: skipped_reconciliation,
            daily_goal_research: None,
            scale_to_zero: scale_to_zero_decision(
                &schedule_summary,
                false,
                "missed_runs_reconciled_without_artifacts",
            ),
            schedule_summary,
        }));
    }

    let workflow_id = workflow.id.clone();
    let history_markers = schedule_run_history_markers(&workflow);
    let daily_goal_research = run_daily_goal_research_smoke_with_schedule_mode(
        store,
        &mut workflow,
        true,
        artifact_journal,
    )?;
    if daily_goal_research.is_none() {
        record_schedule_run_history(&mut workflow, true);
    }
    let missed_run_reconciliation = collect_new_missed_run_reconciliations(
        &workflow,
        &history_markers,
        daily_goal_research.is_some(),
    );
    let schedule_summary = summarize_schedules(&workflow.tasks);
    store.save_workflow(&workflow)?;
    store.record_event(
        &workflow_id,
        "due_workflow_executed",
        &serde_json::json!({
            "workflow_id": workflow_id,
            "missed_run_reconciliation": missed_run_reconciliation,
        }),
    )?;

    Ok(Some(ScheduleRunDueReport {
        status: "due_workflow_executed".to_string(),
        workflow_id,
        goal: workflow.goal.clone(),
        due_executed: true,
        blocked_loop_task_id: None,
        blocked_loop_state: None,
        missed_run_reconciliation,
        daily_goal_research,
        scale_to_zero: scale_to_zero_decision(&schedule_summary, false, "due_work_executed"),
        schedule_summary,
    }))
}

fn run_due_workflow_and_release_lease(
    store: &ForgeStore,
    workflow_id: &str,
    schedule_task_id: &str,
    lease_id: Option<&str>,
    executor: &str,
) -> Result<(Option<ScheduleRunDueReport>, bool)> {
    let Some(lease_id) = lease_id else {
        return run_due_workflow(store, workflow_id).map(|report| (report, false));
    };

    let artifact_journal = ArtifactWriteJournal::default();
    let result = store.with_transaction(|| {
        validate_schedule_lease(
            store,
            workflow_id,
            schedule_task_id,
            lease_id,
            executor,
            Utc::now(),
        )?;
        let report = run_due_workflow_in_transaction(store, workflow_id, Some(&artifact_journal))?;
        validate_schedule_lease(
            store,
            workflow_id,
            schedule_task_id,
            lease_id,
            executor,
            Utc::now(),
        )?;
        let release = release_task_lease(store, workflow_id, schedule_task_id, lease_id, executor)?;
        if !release.released {
            anyhow::bail!(
                "schedule lease fencing rejected commit: lease {lease_id} is no longer current"
            );
        }
        Ok((report, true))
    });
    match rollback_artifacts_on_error(result, &artifact_journal) {
        Ok(report) => Ok(report),
        Err(error) => {
            let release_result = store.with_transaction(|| {
                release_task_lease(store, workflow_id, schedule_task_id, lease_id, executor)
            });
            match release_result {
                Ok(_) => Err(error),
                Err(release_error) => Err(error.context(format!(
                    "failed to release schedule lease {lease_id} after the due workflow transaction rolled back: {release_error:#}"
                ))),
            }
        }
    }
}

fn validate_schedule_lease(
    store: &ForgeStore,
    workflow_id: &str,
    schedule_task_id: &str,
    lease_id: &str,
    executor: &str,
    observed_at: DateTime<Utc>,
) -> Result<TaskLease> {
    let current_value = store
        .load_task_lease(workflow_id, schedule_task_id)?
        .with_context(|| {
            format!(
                "schedule lease fencing rejected: no current lease for {workflow_id}/{schedule_task_id}"
            )
        })?;
    let current: TaskLease = serde_json::from_value(current_value)
        .context("schedule lease fencing rejected: current lease is invalid")?;
    if current.workflow_id != workflow_id
        || current.task_id != schedule_task_id
        || current.lease_id != lease_id
        || current.executor != executor
    {
        anyhow::bail!(
            "schedule lease fencing rejected: expected lease {lease_id} owned by {executor}, current lease is {} owned by {}",
            current.lease_id,
            current.executor
        );
    }
    if current.expires_at <= observed_at {
        anyhow::bail!(
            "schedule lease fencing rejected: lease {lease_id} owned by {executor} expired at {}",
            current.expires_at.to_rfc3339()
        );
    }
    Ok(current)
}

pub fn scan_due_workflows(
    store: &ForgeStore,
    executor: &str,
    ttl_seconds: u64,
) -> Result<ScheduleScanDueReport> {
    let mut summary = ScheduleScanDueSummary::default();
    let mut results = Vec::new();

    for workflow in store.load_workflows()? {
        let schedule_summary = summarize_schedules(&workflow.tasks);
        if schedule_summary.scheduled_nodes == 0 {
            continue;
        }
        summary.scanned_workflows += 1;

        let due_schedule_task_id = first_due_schedule_task_id(&workflow);
        if let Some(schedule_task_id) = due_schedule_task_id {
            summary.due_workflows += 1;
            let lease_report = acquire_task_lease(
                store,
                &workflow.id,
                &schedule_task_id,
                executor,
                ttl_seconds,
            )?;
            if !lease_report.allowed {
                summary.lease_conflicts += 1;
                summary.skipped_workflows += 1;
                results.push(ScheduleScanWorkflowReport {
                    workflow_id: workflow.id,
                    goal: workflow.goal,
                    status: "lease_conflict".to_string(),
                    schedule_task_id: Some(schedule_task_id),
                    due_nodes: schedule_summary.due_nodes,
                    lease_status: lease_report.status,
                    lease_id: None,
                    lease_released: false,
                    current_lease_id: lease_report.current_lease.map(|lease| lease.lease_id),
                    run_due: None,
                    reason: "scheduled task already has an active lease".to_string(),
                });
                continue;
            }

            summary.leased_workflows += 1;
            let lease_id = lease_report
                .lease
                .as_ref()
                .map(|lease| lease.lease_id.clone());
            let (run_due, lease_released) = run_due_workflow_and_release_lease(
                store,
                &workflow.id,
                &schedule_task_id,
                lease_id.as_deref(),
                executor,
            )?;
            if run_due.as_ref().is_some_and(|report| report.due_executed) {
                summary.executed_workflows += 1;
            }
            if run_due
                .as_ref()
                .is_some_and(|report| report.scale_to_zero.applied)
            {
                summary.scale_to_zero_workflows += 1;
            }
            let status = run_due
                .as_ref()
                .map(|report| report.status.clone())
                .unwrap_or_else(|| "no_scheduled_work".to_string());
            results.push(ScheduleScanWorkflowReport {
                workflow_id: workflow.id,
                goal: workflow.goal,
                status,
                schedule_task_id: Some(schedule_task_id),
                due_nodes: schedule_summary.due_nodes,
                lease_status: lease_report.status,
                lease_id,
                lease_released,
                current_lease_id: None,
                run_due,
                reason: "due scheduled workflow executed under local lease".to_string(),
            });
            continue;
        }

        summary.idle_workflows += 1;
        let run_due = run_due_workflow(store, &workflow.id)?;
        if run_due
            .as_ref()
            .is_some_and(|report| report.scale_to_zero.applied)
        {
            summary.scale_to_zero_workflows += 1;
        }
        let status = run_due
            .as_ref()
            .map(|report| report.status.clone())
            .unwrap_or_else(|| "no_scheduled_work".to_string());
        results.push(ScheduleScanWorkflowReport {
            workflow_id: workflow.id,
            goal: workflow.goal,
            status,
            schedule_task_id: None,
            due_nodes: schedule_summary.due_nodes,
            lease_status: "not_required".to_string(),
            lease_id: None,
            lease_released: false,
            current_lease_id: None,
            run_due,
            reason: no_due_schedule_reason(&schedule_summary),
        });
    }

    Ok(ScheduleScanDueReport {
        schema_version: SCHEDULE_SCAN_DUE_SCHEMA_VERSION.to_string(),
        status: "schedule_scan_completed".to_string(),
        executor: executor.to_string(),
        ttl_seconds,
        summary,
        worker_pool: None,
        results,
    })
}

pub fn scan_due_workflows_parallel(
    store: &ForgeStore,
    executor: &str,
    max_workers: usize,
    ttl_seconds: u64,
) -> Result<ScheduleScanDueReport> {
    let max_workers = max_workers.max(1);
    let pool = WorkerPool::new(max_workers);
    let mut summary = ScheduleScanDueSummary::default();
    let results = Arc::new(std::sync::Mutex::new(Vec::new()));
    let store_path = store.path().to_path_buf();

    let all_workflows = store.load_workflows()?;
    let mut due_candidates = Vec::new();
    let mut idle_candidates = Vec::new();

    for workflow in &all_workflows {
        let schedule_summary = summarize_schedules(&workflow.tasks);
        if schedule_summary.scheduled_nodes == 0 {
            continue;
        }
        summary.scanned_workflows += 1;
        if first_due_schedule_task_id(workflow).is_some() {
            due_candidates.push(workflow.id.clone());
        } else {
            summary.idle_workflows += 1;
            idle_candidates.push(workflow.id.clone());
        }
    }

    summary.due_workflows = due_candidates.len();
    let mut idle_results = idle_candidates
        .iter()
        .map(|workflow_id| scan_idle_workflow_reconciliation(store, workflow_id))
        .collect::<Result<Vec<_>>>()?;

    let jobs: Vec<_> = due_candidates
        .into_iter()
        .map(|workflow_id| {
            let executor = executor.to_string();
            let store_path = store_path.clone();
            let results = Arc::clone(&results);
            Box::new(move || {
                match ForgeStore::open(&store_path).and_then(|worker_store| {
                    scan_due_workflow_dispatch(&worker_store, &workflow_id, &executor, ttl_seconds)
                }) {
                    Ok(report) => {
                        if let Ok(mut guard) = results.lock() {
                            guard.push(report);
                        }
                        Ok(())
                    }
                    Err(error) => Err(format!(
                        "failed to scan due workflow {workflow_id}: {error}"
                    )),
                }
            }) as crate::worker::Job
        })
        .collect();

    let worker_report = pool.execute(jobs);
    summary.parallel = true;
    summary.max_workers = max_workers;
    summary.wave_count = worker_report.wave_count;
    summary.duration_ms = worker_report.duration_ms;

    let due_results: Vec<ScheduleScanWorkflowReport> = Arc::try_unwrap(results)
        .unwrap_or_else(|_| std::sync::Mutex::new(Vec::new()))
        .into_inner()
        .unwrap_or_default();
    let mut final_results = Vec::new();
    final_results.append(&mut idle_results);
    final_results.extend(due_results);

    final_results.sort_by(|left, right| left.workflow_id.cmp(&right.workflow_id));

    let executed = final_results
        .iter()
        .filter(|r| r.run_due.as_ref().is_some_and(|d| d.due_executed))
        .count();
    let scale_to_zero = final_results
        .iter()
        .filter(|r| r.run_due.as_ref().is_some_and(|d| d.scale_to_zero.applied))
        .count();
    let lease_conflicts = final_results
        .iter()
        .filter(|r| r.status == "lease_conflict")
        .count();
    let skipped = final_results
        .iter()
        .filter(|r| r.status == "missed_runs_skipped" || r.status.starts_with("no_due_"))
        .count();
    summary.executed_workflows = executed;
    summary.scale_to_zero_workflows = scale_to_zero;
    summary.lease_conflicts = lease_conflicts;
    summary.leased_workflows = final_results
        .iter()
        .filter(|report| report.lease_id.is_some())
        .count();
    summary.skipped_workflows = skipped;

    Ok(ScheduleScanDueReport {
        schema_version: SCHEDULE_SCAN_DUE_SCHEMA_VERSION.to_string(),
        status: if worker_report.failed_jobs > 0 {
            "schedule_scan_completed_with_worker_failures"
        } else if executed > 0 {
            "schedule_scan_completed_with_execution"
        } else if scale_to_zero > 0 {
            "schedule_scan_completed_scale_to_zero"
        } else {
            "schedule_scan_completed"
        }
        .to_string(),
        executor: executor.to_string(),
        ttl_seconds,
        summary,
        worker_pool: Some(worker_report),
        results: final_results,
    })
}

fn scan_idle_workflow_reconciliation(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<ScheduleScanWorkflowReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let schedule_summary = summarize_schedules(&workflow.tasks);
    let run_due = run_due_workflow(store, workflow_id)?;
    let status = run_due
        .as_ref()
        .map(|report| report.status.clone())
        .unwrap_or_else(|| "no_scheduled_work".to_string());

    Ok(ScheduleScanWorkflowReport {
        workflow_id: workflow.id,
        goal: workflow.goal,
        status,
        schedule_task_id: None,
        due_nodes: schedule_summary.due_nodes,
        lease_status: "not_required".to_string(),
        lease_id: None,
        lease_released: false,
        current_lease_id: None,
        run_due,
        reason: no_due_schedule_reason(&schedule_summary),
    })
}

fn scan_due_workflow_dispatch(
    store: &ForgeStore,
    workflow_id: &str,
    executor: &str,
    ttl_seconds: u64,
) -> Result<ScheduleScanWorkflowReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let schedule_summary = summarize_schedules(&workflow.tasks);

    let due_schedule_task_id = first_due_schedule_task_id(&workflow);
    let Some(schedule_task_id) = due_schedule_task_id else {
        return Ok(ScheduleScanWorkflowReport {
            workflow_id: workflow.id.clone(),
            goal: workflow.goal.clone(),
            status: "idle".to_string(),
            schedule_task_id: None,
            due_nodes: schedule_summary.due_nodes,
            lease_status: "not_required".to_string(),
            lease_id: None,
            lease_released: false,
            current_lease_id: None,
            run_due: None,
            reason: no_due_schedule_reason(&schedule_summary),
        });
    };

    let lease_report =
        acquire_task_lease(store, workflow_id, &schedule_task_id, executor, ttl_seconds)?;
    if !lease_report.allowed {
        return Ok(ScheduleScanWorkflowReport {
            workflow_id: workflow.id,
            goal: workflow.goal,
            status: "lease_conflict".to_string(),
            schedule_task_id: Some(schedule_task_id),
            due_nodes: schedule_summary.due_nodes,
            lease_status: lease_report.status,
            lease_id: None,
            lease_released: false,
            current_lease_id: lease_report.current_lease.map(|lease| lease.lease_id),
            run_due: None,
            reason: "scheduled task already has an active lease".to_string(),
        });
    }

    let lease_id = lease_report
        .lease
        .as_ref()
        .map(|lease| lease.lease_id.clone());

    let (run_due, lease_released) = run_due_workflow_and_release_lease(
        store,
        workflow_id,
        &schedule_task_id,
        lease_id.as_deref(),
        executor,
    )?;

    let due_executed = run_due.as_ref().is_some_and(|r| r.due_executed);
    let scale_to_zero = run_due.as_ref().is_some_and(|r| r.scale_to_zero.applied);

    let status = if scale_to_zero {
        "scale_to_zero".to_string()
    } else if due_executed {
        "executed".to_string()
    } else {
        run_due
            .as_ref()
            .map(|r| r.status.clone())
            .unwrap_or_else(|| "no_scheduled_work".to_string())
    };

    let workflow = store.load_workflow(workflow_id)?;

    Ok(ScheduleScanWorkflowReport {
        workflow_id: workflow.id,
        goal: workflow.goal,
        status,
        schedule_task_id: Some(schedule_task_id),
        due_nodes: schedule_summary.due_nodes,
        lease_status: lease_report.status,
        lease_id,
        lease_released,
        current_lease_id: None,
        run_due,
        reason: "due scheduled workflow executed under bounded concurrent lease".to_string(),
    })
}

pub fn build_schedule_worker_status(
    store: &ForgeStore,
    executor: &str,
    max_workers: usize,
    ttl_seconds: u64,
) -> Result<ScheduleWorkerStatusReport> {
    let observed_at = Utc::now();
    let max_workers = max_workers.max(1);
    let workflows = store.load_workflows()?;
    let mut summary = ScheduleWorkerStatusSummary::default();
    let mut workflow_reports = Vec::new();
    let mut runnable_due = Vec::new();
    let mut next_wakeup_at: Option<DateTime<Utc>> = None;

    for workflow in workflows {
        let schedule_summary = summarize_schedules(&workflow.tasks);
        if schedule_summary.scheduled_nodes == 0 {
            continue;
        }

        summary.scanned_workflows += 1;
        summary.scheduled_nodes += schedule_summary.scheduled_nodes;
        summary.cron_nodes += schedule_summary.cron_nodes;
        summary.wait_until_nodes += schedule_summary.wait_until_nodes;
        summary.delay_nodes += schedule_summary.delay_nodes;
        summary.due_nodes += schedule_summary.due_nodes;
        let scale_to_zero_eligible = can_scale_to_zero_when_idle(&schedule_summary);
        if scale_to_zero_eligible {
            summary.scale_to_zero_workflows += 1;
        }

        let blocked_loop = first_blocking_loop_state(&workflow);
        let has_due = schedule_summary.due_nodes > 0;
        if has_due {
            summary.due_workflows += 1;
            if blocked_loop.is_some() {
                summary.blocked_due_workflows += 1;
            } else {
                summary.runnable_due_workflows += 1;
                if let Some(schedule_task_id) = first_due_schedule_task_id(&workflow) {
                    runnable_due.push(ScheduleWorkerAssignmentCandidate {
                        workflow_id: workflow.id.clone(),
                        goal: workflow.goal.clone(),
                        schedule_task_id,
                        due_nodes: schedule_summary.due_nodes,
                        next_run_at: schedule_summary.next_run_at.clone(),
                    });
                }
            }
        } else {
            summary.idle_workflows += 1;
        }
        if blocked_loop.is_some() {
            summary.paused_or_stopped_loop_workflows += 1;
        }

        for next in future_schedule_wakeups(&workflow, observed_at) {
            next_wakeup_at = Some(match next_wakeup_at {
                Some(current) => current.min(next),
                None => next,
            });
        }

        let status = if has_due && blocked_loop.is_some() {
            "blocked_by_loop_state"
        } else if has_due {
            "due"
        } else if scale_to_zero_eligible {
            "idle_scale_to_zero"
        } else {
            "idle"
        };
        let (blocked_loop_task_id, blocked_loop_state) =
            blocked_loop.unwrap_or((String::new(), String::new()));
        workflow_reports.push(ScheduleWorkerWorkflowStatus {
            workflow_id: workflow.id,
            goal: workflow.goal,
            status: status.to_string(),
            due_nodes: schedule_summary.due_nodes,
            next_wakeup_at: schedule_summary.next_run_at,
            scale_to_zero_eligible,
            blocked_loop_task_id: (!blocked_loop_task_id.is_empty())
                .then_some(blocked_loop_task_id),
            blocked_loop_state: (!blocked_loop_state.is_empty()).then_some(blocked_loop_state),
        });
    }

    let assignable_due_workflows = summary.runnable_due_workflows.min(max_workers);
    let queued_due_workflows = summary
        .runnable_due_workflows
        .saturating_sub(assignable_due_workflows);
    let status = worker_status_label(&summary, next_wakeup_at);
    let sleep = build_worker_sleep_plan(&summary, observed_at, next_wakeup_at);
    let assignment_plan = build_assignment_plan(runnable_due, max_workers, executor);

    Ok(ScheduleWorkerStatusReport {
        schema_version: SCHEDULE_WORKER_STATUS_SCHEMA_VERSION.to_string(),
        status,
        executor: executor.to_string(),
        observed_at: format_utc_rfc3339(observed_at),
        ttl_seconds,
        summary,
        worker_pool: ScheduleWorkerPoolStatus {
            max_workers,
            available_workers: max_workers,
            assignable_due_workflows,
            worker_kind: "bounded_local_schedule_worker_pool".to_string(),
            deterministic: true,
            assignment_plan,
        },
        sleep,
        backpressure: ScheduleWorkerBackpressure {
            active: queued_due_workflows > 0,
            queued_due_workflows,
            reason: if queued_due_workflows > 0 {
                "runnable due workflows exceed configured max_workers".to_string()
            } else {
                "runnable due workflows fit within configured max_workers".to_string()
            },
        },
        cancellation: ScheduleWorkerCancellation {
            supported: true,
            lease_ttl_seconds: ttl_seconds,
            safe_points: vec![
                "before_acquiring_task_lease".to_string(),
                "between_due_workflow_leases".to_string(),
                "before_starting_executor_handoff".to_string(),
            ],
            reason: "worker status is read-only; scan-due can be cancelled between lease boundaries without mutating external resources".to_string(),
        },
        workflows: workflow_reports,
    })
}

#[derive(Debug, Clone)]
struct ScheduleWorkerAssignmentCandidate {
    workflow_id: String,
    goal: String,
    schedule_task_id: String,
    due_nodes: usize,
    next_run_at: Option<String>,
}

fn build_assignment_plan(
    mut candidates: Vec<ScheduleWorkerAssignmentCandidate>,
    max_workers: usize,
    executor: &str,
) -> ScheduleWorkerAssignmentPlan {
    let max_workers = max_workers.max(1);
    candidates.sort_by(|left, right| {
        (
            left.next_run_at.as_deref().unwrap_or(""),
            left.workflow_id.as_str(),
            left.schedule_task_id.as_str(),
        )
            .cmp(&(
                right.next_run_at.as_deref().unwrap_or(""),
                right.workflow_id.as_str(),
                right.schedule_task_id.as_str(),
            ))
    });

    let mut assigned = Vec::new();
    let mut queued = Vec::new();
    for (index, candidate) in candidates.into_iter().enumerate() {
        let assignment = ScheduleWorkerAssignment {
            workflow_id: candidate.workflow_id,
            goal: candidate.goal,
            schedule_task_id: candidate.schedule_task_id,
            due_nodes: candidate.due_nodes,
            next_run_at: candidate.next_run_at,
            lease_scope: "schedule_task".to_string(),
            wave: (index / max_workers) + 1,
            queue_position: index + 1,
            executor: executor.to_string(),
        };
        if index < max_workers {
            assigned.push(assignment);
        } else {
            queued.push(assignment);
        }
    }

    ScheduleWorkerAssignmentPlan {
        schema_version: "forge.schedule.assignment_plan.v1".to_string(),
        max_workers,
        assigned,
        queued,
        deterministic_ordering: true,
        ordering_key: "next_run_at,workflow_id,schedule_task_id".to_string(),
    }
}

fn can_scale_to_zero_when_idle(summary: &ScheduleSummary) -> bool {
    summary.scheduled_nodes > 0
        && summary.due_nodes == 0
        && summary.scale_to_zero_when_idle_nodes == summary.scheduled_nodes
}

fn is_one_shot_wait_kind(kind: &str) -> bool {
    matches!(
        kind,
        "wait_until" | "delay" | "sleep_until" | "one_shot_wait"
    )
}

fn no_due_schedule_status(summary: &ScheduleSummary) -> String {
    if summary.cron_nodes > 0 {
        "no_due_cron_nodes".to_string()
    } else if summary.wait_until_nodes > 0 || summary.delay_nodes > 0 {
        "no_due_wait_nodes".to_string()
    } else {
        "no_due_scheduled_nodes".to_string()
    }
}

fn no_due_schedule_reason(summary: &ScheduleSummary) -> String {
    if summary.cron_nodes > 0 {
        "no due cron nodes; recorded idle schedule state".to_string()
    } else if summary.wait_until_nodes > 0 || summary.delay_nodes > 0 {
        "no due wait nodes; recorded idle wait state".to_string()
    } else {
        "no due scheduled nodes; recorded idle schedule state".to_string()
    }
}

fn first_blocking_loop_state(workflow: &Workflow) -> Option<(String, String)> {
    workflow.tasks.iter().find_map(|task| {
        task.loop_control.as_ref().and_then(|loop_control| {
            matches!(loop_control.state.as_str(), "paused" | "stopped")
                .then(|| (task.id.clone(), loop_control.state.clone()))
        })
    })
}

fn future_schedule_wakeups(workflow: &Workflow, now: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    workflow
        .tasks
        .iter()
        .filter_map(|task| {
            task.schedule
                .as_ref()
                .and_then(|schedule| schedule.next_run_at)
        })
        .filter(|next_run_at| *next_run_at > now)
        .collect()
}

fn worker_status_label(
    summary: &ScheduleWorkerStatusSummary,
    next_wakeup_at: Option<DateTime<Utc>>,
) -> String {
    if summary.scanned_workflows == 0 {
        "no_scheduled_workflows".to_string()
    } else if summary.runnable_due_workflows > 0 {
        "ready_due_work".to_string()
    } else if summary.blocked_due_workflows > 0 {
        "blocked_due_work".to_string()
    } else if next_wakeup_at.is_some() {
        "sleeping_until_next_wakeup".to_string()
    } else {
        "idle_no_wakeup".to_string()
    }
}

fn build_worker_sleep_plan(
    summary: &ScheduleWorkerStatusSummary,
    observed_at: DateTime<Utc>,
    next_wakeup_at: Option<DateTime<Utc>>,
) -> ScheduleWorkerSleepPlan {
    if summary.runnable_due_workflows > 0 || summary.blocked_due_workflows > 0 {
        return ScheduleWorkerSleepPlan {
            sleep_until_next_wakeup: false,
            next_wakeup_at: next_wakeup_at.map(format_utc_rfc3339),
            sleep_seconds: 0,
            mode: "ready".to_string(),
            reason: "due scheduled work is present, so the worker should not sleep".to_string(),
        };
    }

    let sleep_seconds = next_wakeup_at
        .and_then(|next| (next - observed_at).to_std().ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    ScheduleWorkerSleepPlan {
        sleep_until_next_wakeup: next_wakeup_at.is_some(),
        next_wakeup_at: next_wakeup_at.map(format_utc_rfc3339),
        sleep_seconds,
        mode: if next_wakeup_at.is_some() {
            "sleep_until_next_wakeup".to_string()
        } else {
            "idle_without_wakeup".to_string()
        },
        reason: if next_wakeup_at.is_some() {
            "no due work exists; the worker can scale to zero until the next Forge-owned schedule wakeup".to_string()
        } else {
            "no scheduled workflows have a future wakeup".to_string()
        },
    }
}

fn first_due_schedule_task_id(workflow: &Workflow) -> Option<String> {
    let now = Utc::now();
    workflow.tasks.iter().find_map(|task| {
        task.schedule.as_ref().and_then(|schedule| {
            schedule
                .next_run_at
                .filter(|next_run_at| *next_run_at <= now)
                .map(|_| task.id.clone())
        })
    })
}

fn scale_to_zero_decision(
    summary: &ScheduleSummary,
    applied: bool,
    reason: &str,
) -> ScaleToZeroDecision {
    ScaleToZeroDecision {
        schema_version: SCALE_TO_ZERO_DECISION_SCHEMA_VERSION.to_string(),
        applied,
        reason: reason.to_string(),
        next_wakeup_at: summary.next_run_at.clone(),
        scheduled_nodes: summary.scheduled_nodes,
        due_nodes: summary.due_nodes,
    }
}

fn skip_due_missed_runs(workflow: &mut Workflow) -> Vec<MissedRunReconciliationReport> {
    let mut reconciliation = Vec::new();
    for task in workflow.tasks.iter_mut() {
        let task_id = task.id.clone();
        let Some(schedule) = task.schedule.as_mut() else {
            continue;
        };
        let Some(next_run_at) = schedule.next_run_at else {
            continue;
        };
        let now = Utc::now();
        if next_run_at > now
            || !is_missed_run(next_run_at, now)
            || !should_skip_missed_run(&schedule.missed_run_policy)
        {
            continue;
        }

        let run_id = format!("run_{}", Uuid::new_v4().to_string().replace('-', ""));
        let one_shot_wait = is_one_shot_wait_kind(&schedule.kind);
        let next_run_at_after = (!one_shot_wait).then_some(now + Duration::days(1));
        schedule.run_history.push(ScheduleRunRecord {
            run_id: run_id.clone(),
            scheduled_at: next_run_at,
            started_at: Some(now),
            finished_at: Some(now),
            status: "skipped_missed".to_string(),
            missed: true,
            missed_run_policy: schedule.missed_run_policy.clone(),
            reconciliation_action: "skipped_missed".to_string(),
        });
        schedule.next_run_at = next_run_at_after;
        if one_shot_wait {
            task.status = TaskStatus::Completed;
        }
        reconciliation.push(MissedRunReconciliationReport {
            schema_version: MISSED_RUN_RECONCILIATION_SCHEMA_VERSION.to_string(),
            task_id,
            policy: schedule.missed_run_policy.clone(),
            action: "skipped_missed".to_string(),
            scheduled_at: format_utc_rfc3339(next_run_at),
            observed_at: format_utc_rfc3339(now),
            next_run_at_before: format_utc_rfc3339(next_run_at),
            next_run_at_after: next_run_at_after.map(format_utc_rfc3339),
            run_id,
            run_status: "skipped_missed".to_string(),
            missed: true,
            artifacts_allowed: false,
        });
    }
    reconciliation
}

fn schedule_run_history_markers(workflow: &Workflow) -> Vec<(String, usize)> {
    workflow
        .tasks
        .iter()
        .filter_map(|task| {
            task.schedule
                .as_ref()
                .map(|schedule| (task.id.clone(), schedule.run_history.len()))
        })
        .collect()
}

fn collect_new_missed_run_reconciliations(
    workflow: &Workflow,
    history_markers: &[(String, usize)],
    artifacts_allowed: bool,
) -> Vec<MissedRunReconciliationReport> {
    let mut reconciliation = Vec::new();
    for task in &workflow.tasks {
        let Some(schedule) = task.schedule.as_ref() else {
            continue;
        };
        let history_start = history_markers
            .iter()
            .find_map(|(task_id, len)| (task_id == &task.id).then_some(*len))
            .unwrap_or(0);
        for run in schedule
            .run_history
            .iter()
            .skip(history_start)
            .filter(|run| run.missed)
        {
            let observed_at = run
                .started_at
                .or(run.finished_at)
                .unwrap_or(run.scheduled_at);
            reconciliation.push(MissedRunReconciliationReport {
                schema_version: MISSED_RUN_RECONCILIATION_SCHEMA_VERSION.to_string(),
                task_id: task.id.clone(),
                policy: run.missed_run_policy.clone(),
                action: run.reconciliation_action.clone(),
                scheduled_at: format_utc_rfc3339(run.scheduled_at),
                observed_at: format_utc_rfc3339(observed_at),
                next_run_at_before: format_utc_rfc3339(run.scheduled_at),
                next_run_at_after: schedule.next_run_at.map(format_utc_rfc3339),
                run_id: run.run_id.clone(),
                run_status: run.status.clone(),
                missed: run.missed,
                artifacts_allowed,
            });
        }
    }
    reconciliation
}

fn is_missed_run(scheduled_at: DateTime<Utc>, started_at: DateTime<Utc>) -> bool {
    scheduled_at + Duration::minutes(MISSED_RUN_GRACE_MINUTES) < started_at
}

fn should_skip_missed_run(policy: &str) -> bool {
    matches!(policy, "skip_missed" | "skip_and_resume")
}

fn format_utc_rfc3339(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn missed_run_action(policy: &str, missed: bool, status: &str) -> String {
    if !missed {
        return "on_time_due_run".to_string();
    }
    if status == "skipped_missed" || should_skip_missed_run(policy) {
        return "skipped_missed".to_string();
    }
    match policy {
        "run_once_then_resume" => "ran_once_then_resumed".to_string(),
        _ => "ran_missed_due".to_string(),
    }
}

fn push_schedule_revision(workflow: &mut Workflow, origin: &str, summary: &str) -> u64 {
    let revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision + 1)
        .unwrap_or(1);
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: "schedule_update".to_string(),
        summary: summary.to_string(),
        created_at: Utc::now(),
    });
    revision
}

fn push_loop_revision(workflow: &mut Workflow, origin: &str, summary: &str) -> u64 {
    let revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision + 1)
        .unwrap_or(1);
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: "loop_state_update".to_string(),
        summary: summary.to_string(),
        created_at: Utc::now(),
    });
    revision
}

fn build_daily_goal_artifact_lineage(
    workflow: &Workflow,
    goal: &str,
    schedule_runs: &[ScheduleRunEmission],
) -> ArtifactLineageRecord {
    let schedule_task_id = schedule_runs
        .first()
        .map(|run| run.schedule_task_id.clone())
        .or_else(|| {
            workflow
                .tasks
                .iter()
                .find(|task| task.schedule.is_some())
                .map(|task| task.id.clone())
        })
        .unwrap_or_else(|| "unscheduled".to_string());
    let run_id = schedule_runs
        .first()
        .map(|run| run.run_id.clone())
        .unwrap_or_else(|| format!("run_{}", Uuid::new_v4().to_string().replace('-', "")));
    let loop_task_id = workflow
        .tasks
        .iter()
        .find(|task| {
            task.loop_control
                .as_ref()
                .is_some_and(|loop_control| loop_control.items.iter().any(|item| item == goal))
        })
        .map(|task| task.id.clone())
        .unwrap_or_else(|| "unknown_loop".to_string());
    let subflow = workflow.tasks.iter().find_map(|task| {
        task.native_subflow
            .as_ref()
            .filter(|subflow| subflow.goal == goal)
    });
    let subflow_id = subflow
        .map(|subflow| subflow.subflow_id.clone())
        .unwrap_or_else(|| format!("goal_research:{goal}"));
    let triggered_by = subflow
        .map(|subflow| subflow.triggered_by.clone())
        .unwrap_or_else(|| format!("loop:{loop_task_id}"));

    ArtifactLineageRecord {
        schema_version: "forge.artifact_lineage.v1".to_string(),
        workflow_id: workflow.id.clone(),
        run_id,
        schedule_task_id,
        loop_task_id,
        goal: goal.to_string(),
        subflow_id,
        triggered_by,
    }
}

fn portable_artifact_component(value: &str) -> String {
    let portable = !value.is_empty()
        && value.len() <= 96
        && value != "."
        && value != ".."
        && !value.starts_with(ENCODED_ARTIFACT_COMPONENT_PREFIX)
        && !value.starts_with(' ')
        && !value.ends_with([' ', '.'])
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b' '));
    if portable {
        value.to_string()
    } else {
        format!(
            "{ENCODED_ARTIFACT_COMPONENT_PREFIX}{}",
            hex_sha256(value.as_bytes())
        )
    }
}

fn workflow_artifact_relative_dir(workflow_id: &str) -> PathBuf {
    Path::new(ARTIFACT_DIRECTORY_NAME).join(portable_artifact_component(workflow_id))
}

fn artifact_relative_path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .with_context(|| format!("artifact path must be valid UTF-8: {}", path.display()))
}

fn contained_artifact_components(relative_path: &Path, label: &str) -> Result<Vec<OsString>> {
    if relative_path.as_os_str().is_empty() || relative_path.is_absolute() {
        anyhow::bail!("{label} must be a contained relative artifact path");
    }
    let mut components = Vec::new();
    for component in relative_path.components() {
        match component {
            Component::Normal(value) => components.push(value.to_os_string()),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                anyhow::bail!("{label} escapes the artifact root")
            }
        }
    }
    if components.len() < 2
        || components
            .first()
            .is_none_or(|component| component != ARTIFACT_DIRECTORY_NAME)
    {
        anyhow::bail!("{label} must be rooted below `{ARTIFACT_DIRECTORY_NAME}`");
    }
    Ok(components)
}

fn canonical_schedule_base_dir(base_dir: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(base_dir).with_context(|| {
        format!(
            "failed to resolve schedule artifact base directory {}",
            base_dir.display()
        )
    })?;
    let metadata = fs::metadata(&canonical).with_context(|| {
        format!(
            "failed to inspect schedule artifact base directory {}",
            canonical.display()
        )
    })?;
    if !metadata.is_dir() {
        anyhow::bail!(
            "schedule artifact base path is not a directory: {}",
            canonical.display()
        );
    }
    Ok(canonical)
}

fn resolve_real_artifact_directory(
    path: &Path,
    create: bool,
    label: &str,
) -> Result<Option<PathBuf>> {
    loop {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    anyhow::bail!(
                        "{label} must be a real directory, not a symlink: {}",
                        path.display()
                    );
                }
                return fs::canonicalize(path).map(Some).with_context(|| {
                    format!("failed to resolve {label} directory {}", path.display())
                });
            }
            Err(error) if error.kind() == ErrorKind::NotFound && !create => return Ok(None),
            Err(error) if error.kind() == ErrorKind::NotFound => match fs::create_dir(path) {
                Ok(()) => continue,
                Err(create_error) if create_error.kind() == ErrorKind::AlreadyExists => continue,
                Err(create_error) => {
                    return Err(create_error)
                        .with_context(|| format!("failed to create {label} {}", path.display()));
                }
            },
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {label} {}", path.display()));
            }
        }
    }
}

fn resolve_artifact_directory(
    base_dir: &Path,
    relative_dir: &Path,
    create: bool,
) -> Result<Option<(PathBuf, PathBuf)>> {
    let components = contained_artifact_components(relative_dir, "schedule artifact directory")?;
    let canonical_base = canonical_schedule_base_dir(base_dir)?;
    let mut current = canonical_base.clone();
    let mut artifact_root = None;

    for (index, component) in components.iter().enumerate() {
        let candidate = current.join(component);
        let Some(canonical_candidate) =
            resolve_real_artifact_directory(&candidate, create, "schedule artifact")?
        else {
            return Ok(None);
        };
        if index == 0 {
            if canonical_candidate.parent() != Some(canonical_base.as_path()) {
                anyhow::bail!(
                    "schedule artifact root escapes its base directory: {}",
                    canonical_candidate.display()
                );
            }
            artifact_root = Some(canonical_candidate.clone());
        } else if artifact_root
            .as_ref()
            .is_none_or(|root| !canonical_candidate.starts_with(root))
        {
            anyhow::bail!(
                "schedule artifact directory escapes its canonical root: {}",
                canonical_candidate.display()
            );
        }
        current = canonical_candidate;
    }

    let artifact_root = artifact_root.context("schedule artifact root was not resolved")?;
    Ok(Some((current, artifact_root)))
}

#[derive(Debug, Clone)]
struct ResolvedArtifactFile {
    path: PathBuf,
    parent: PathBuf,
    artifact_root: PathBuf,
}

fn resolve_artifact_file(
    base_dir: &Path,
    relative_path: &Path,
    create_parent: bool,
) -> Result<ResolvedArtifactFile> {
    let components = contained_artifact_components(relative_path, "schedule artifact file")?;
    if components.len() < 3 {
        anyhow::bail!("schedule artifact file requires a workflow directory and file name");
    }
    let file_name = components
        .last()
        .context("schedule artifact file requires a file name")?;
    let mut relative_parent = PathBuf::new();
    for component in &components[..components.len() - 1] {
        relative_parent.push(component);
    }
    let (parent, artifact_root) =
        resolve_artifact_directory(base_dir, &relative_parent, create_parent)?
            .context("schedule artifact parent directory does not exist")?;
    let path = parent.join(file_name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                anyhow::bail!(
                    "schedule artifact target must be a regular non-symlink file: {}",
                    path.display()
                );
            }
            let canonical = fs::canonicalize(&path).with_context(|| {
                format!("failed to resolve schedule artifact {}", path.display())
            })?;
            if !canonical.starts_with(&artifact_root) {
                anyhow::bail!(
                    "schedule artifact target escapes its canonical root: {}",
                    path.display()
                );
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to inspect schedule artifact {}", path.display())
            });
        }
    }
    Ok(ResolvedArtifactFile {
        path,
        parent,
        artifact_root,
    })
}

fn artifact_file_matches(resolved: &ResolvedArtifactFile, expected_bytes: &[u8]) -> Result<bool> {
    let metadata = fs::symlink_metadata(&resolved.path).with_context(|| {
        format!(
            "failed to inspect existing schedule artifact {}",
            resolved.path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!(
            "existing schedule artifact must be a regular non-symlink file: {}",
            resolved.path.display()
        );
    }
    let canonical = fs::canonicalize(&resolved.path).with_context(|| {
        format!(
            "failed to resolve existing schedule artifact {}",
            resolved.path.display()
        )
    })?;
    if !canonical.starts_with(&resolved.artifact_root) {
        anyhow::bail!(
            "existing schedule artifact escapes its canonical root: {}",
            resolved.path.display()
        );
    }
    if metadata.len() != u64::try_from(expected_bytes.len()).unwrap_or(u64::MAX) {
        return Ok(false);
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    let mut file = options.open(&resolved.path).with_context(|| {
        format!(
            "failed to open existing schedule artifact without following symlinks: {}",
            resolved.path.display()
        )
    })?;
    if !file.metadata()?.is_file() {
        anyhow::bail!(
            "existing schedule artifact is not a regular file: {}",
            resolved.path.display()
        );
    }
    let mut actual = Vec::with_capacity(expected_bytes.len());
    (&mut file)
        .take(
            u64::try_from(expected_bytes.len())
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
        .read_to_end(&mut actual)?;
    Ok(actual == expected_bytes)
}

fn remove_contained_artifact_file(artifact_root: &Path, path: &Path) -> Result<bool> {
    let parent = path
        .parent()
        .with_context(|| format!("artifact path has no parent: {}", path.display()))?;
    let canonical_parent = fs::canonicalize(parent)
        .with_context(|| format!("failed to resolve artifact parent {}", parent.display()))?;
    if !canonical_parent.starts_with(artifact_root) {
        anyhow::bail!(
            "refusing to remove schedule artifact outside its canonical root: {}",
            path.display()
        );
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                anyhow::bail!(
                    "refusing to remove non-regular schedule artifact: {}",
                    path.display()
                );
            }
            fs::remove_file(path).with_context(|| {
                format!("failed to remove schedule artifact {}", path.display())
            })?;
            sync_directory(&canonical_parent)?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error)
            .with_context(|| format!("failed to inspect schedule artifact {}", path.display())),
    }
}

fn reconcile_uncommitted_schedule_artifacts(base_dir: &Path, workflow: &Workflow) -> Result<()> {
    let relative_artifact_dir = workflow_artifact_relative_dir(&workflow.id);
    let Some((artifact_dir, artifact_root)) =
        resolve_artifact_directory(base_dir, &relative_artifact_dir, false)?
    else {
        return Ok(());
    };

    let referenced = workflow
        .artifacts
        .iter()
        .map(|artifact| artifact.path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut removed = false;
    for entry in fs::read_dir(&artifact_dir).with_context(|| {
        format!(
            "failed to reconcile schedule artifacts in {}",
            artifact_dir.display()
        )
    })? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let is_committed_name = file_name.starts_with(SCHEDULE_RUN_ARTIFACT_PREFIX);
        let is_staged_name = file_name.starts_with(&format!(".{SCHEDULE_RUN_ARTIFACT_PREFIX}"))
            && file_name.contains(".tmp-");
        if !is_committed_name && !is_staged_name {
            continue;
        }
        if entry.file_type()?.is_symlink() {
            anyhow::bail!(
                "schedule artifact reconciliation refuses symlink {}",
                entry.path().display()
            );
        }
        let relative_path =
            artifact_relative_path_string(&relative_artifact_dir.join(entry.file_name()))?;
        if !referenced.contains(relative_path.as_str()) {
            removed |= remove_contained_artifact_file(&artifact_root, &entry.path())?;
        }
    }
    if removed {
        sync_directory(&artifact_dir)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct ArtifactWriteJournal {
    created_files: Arc<std::sync::Mutex<Vec<JournaledArtifactFile>>>,
}

#[derive(Debug, Clone)]
struct JournaledArtifactFile {
    path: PathBuf,
    artifact_root: PathBuf,
}

impl ArtifactWriteJournal {
    fn record_created(&self, resolved: &ResolvedArtifactFile) -> Result<()> {
        self.created_files
            .lock()
            .map_err(|_| anyhow::anyhow!("artifact write journal lock poisoned"))?
            .push(JournaledArtifactFile {
                path: resolved.path.clone(),
                artifact_root: resolved.artifact_root.clone(),
            });
        Ok(())
    }

    fn rollback(&self) -> Result<()> {
        let mut created_files = self
            .created_files
            .lock()
            .map_err(|_| anyhow::anyhow!("artifact write journal lock poisoned"))?;
        let files = std::mem::take(&mut *created_files);
        drop(created_files);

        for file in files.into_iter().rev() {
            remove_contained_artifact_file(&file.artifact_root, &file.path).with_context(|| {
                format!(
                    "failed to roll back schedule artifact {}",
                    file.path.display()
                )
            })?;
        }
        Ok(())
    }
}

fn rollback_artifacts_on_error<T>(
    result: Result<T>,
    artifact_journal: &ArtifactWriteJournal,
) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => match artifact_journal.rollback() {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(anyhow::anyhow!(
                "{error:#}; schedule artifact rollback also failed: {rollback_error:#}"
            )),
        },
    }
}

#[derive(Debug, Clone, Copy)]
enum ArtifactWriteMode {
    AtomicReplace,
    NoOverwrite,
}

#[derive(Debug, Clone)]
struct GeneratedDailyGoalArtifacts {
    goal: String,
    lineage: ArtifactLineageRecord,
    markdown_path: String,
    markdown_bytes: Vec<u8>,
    pdf_path: String,
    pdf_bytes: Vec<u8>,
    telegram_delivery_path: String,
    telegram_delivery_bytes: Vec<u8>,
    telegram_delivery: TelegramDeliveryRecord,
}

fn build_daily_goal_execution_report(goals: &[String]) -> DailyGoalSmokeExecutionReport {
    let worker_count = goals.len().clamp(1, DAILY_GOAL_MAX_ARTIFACT_WORKERS);
    let waves = goals
        .chunks(worker_count)
        .enumerate()
        .map(|(index, goals)| DailyGoalSmokeExecutionWave {
            level: index + 1,
            worker_count: goals.len(),
            goals: goals.to_vec(),
        })
        .collect::<Vec<_>>();

    DailyGoalSmokeExecutionReport {
        schema_version: DAILY_GOAL_EXECUTION_SCHEMA_VERSION.to_string(),
        mode: "bounded_parallel_goal_artifacts".to_string(),
        max_workers: DAILY_GOAL_MAX_ARTIFACT_WORKERS,
        worker_count,
        total_goals: goals.len(),
        bounded: true,
        concurrency_used: worker_count > 1,
        deterministic_output_order: true,
        goal_order: goals.to_vec(),
        waves,
    }
}

fn generate_daily_goal_artifacts_bounded(
    base_dir: &Path,
    workflow_id: &str,
    goal_lineage: &[(String, ArtifactLineageRecord)],
    execution: &DailyGoalSmokeExecutionReport,
    due_only: bool,
    artifact_journal: Option<&ArtifactWriteJournal>,
) -> Result<Vec<GeneratedDailyGoalArtifacts>> {
    let max_workers = execution.max_workers.max(1);
    let pool = WorkerPool::new(max_workers);
    let base_dir = base_dir.to_path_buf();
    let workflow_id = workflow_id.to_string();
    let results = Arc::new(std::sync::Mutex::new(Vec::new()));

    let jobs: Vec<Job> = goal_lineage
        .iter()
        .map(|(goal, lineage)| {
            let base_dir = base_dir.clone();
            let workflow_id = workflow_id.clone();
            let goal = goal.clone();
            let lineage = lineage.clone();
            let results = Arc::clone(&results);
            let artifact_journal = artifact_journal.cloned();
            Box::new(move || {
                match generate_daily_goal_artifacts(
                    &base_dir,
                    &workflow_id,
                    &goal,
                    lineage,
                    due_only,
                    artifact_journal.as_ref(),
                ) {
                    Ok(artifacts) => {
                        if let Ok(mut guard) = results.lock() {
                            guard.push(artifacts);
                        }
                        Ok(())
                    }
                    Err(error) => Err(format!("failed to generate {goal} artifacts: {error}")),
                }
            }) as Job
        })
        .collect();

    let worker_report = pool.execute(jobs);

    let mut generated: Vec<GeneratedDailyGoalArtifacts> = Arc::try_unwrap(results)
        .unwrap_or_else(|_| std::sync::Mutex::new(Vec::new()))
        .into_inner()
        .unwrap_or_default();

    if worker_report.failed_jobs > 0 {
        anyhow::bail!(
            "{} daily Goal artifact worker(s) failed; artifact batch was not committed",
            worker_report.failed_jobs
        );
    }

    generated.sort_by_key(|artifacts| {
        execution
            .goal_order
            .iter()
            .position(|goal| goal == &artifacts.goal)
            .unwrap_or(usize::MAX)
    });
    Ok(generated)
}

fn generate_daily_goal_artifacts(
    base_dir: &Path,
    workflow_id: &str,
    goal: &str,
    lineage: ArtifactLineageRecord,
    due_only: bool,
    artifact_journal: Option<&ArtifactWriteJournal>,
) -> Result<GeneratedDailyGoalArtifacts> {
    let artifact_dir = workflow_artifact_relative_dir(workflow_id);
    let goal_component = portable_artifact_component(goal);
    let file_prefix = if due_only {
        format!(
            "{SCHEDULE_RUN_ARTIFACT_PREFIX}{}--",
            portable_artifact_component(&lineage.run_id)
        )
    } else {
        String::new()
    };
    let write_mode = if due_only {
        ArtifactWriteMode::NoOverwrite
    } else {
        ArtifactWriteMode::AtomicReplace
    };

    let markdown_relative =
        artifact_dir.join(format!("{file_prefix}goal-{goal_component}-report.md"));
    let markdown_path = artifact_relative_path_string(&markdown_relative)?;
    let markdown_bytes = daily_goal_markdown_report_bytes(workflow_id, goal);
    write_artifact_file(
        base_dir,
        &markdown_path,
        &markdown_bytes,
        write_mode,
        artifact_journal,
    )?;

    let pdf_relative = artifact_dir.join(format!("{file_prefix}goal-{goal_component}-report.pdf"));
    let pdf_path = artifact_relative_path_string(&pdf_relative)?;
    let pdf_bytes = minimal_pdf_bytes(&format!("Daily Goal Research: {goal}"));
    write_artifact_file(
        base_dir,
        &pdf_path,
        &pdf_bytes,
        write_mode,
        artifact_journal,
    )?;

    let telegram_delivery_relative = artifact_dir.join(format!(
        "{file_prefix}telegram-delivery-{goal_component}.json"
    ));
    let telegram_delivery_path = artifact_relative_path_string(&telegram_delivery_relative)?;
    let telegram_delivery =
        telegram_delivery_record(goal, &markdown_path, &pdf_path, lineage.clone());
    let telegram_delivery_bytes = serde_json::to_vec_pretty(&telegram_delivery)?;
    write_artifact_file(
        base_dir,
        &telegram_delivery_path,
        &telegram_delivery_bytes,
        write_mode,
        artifact_journal,
    )?;

    Ok(GeneratedDailyGoalArtifacts {
        goal: goal.to_string(),
        lineage,
        markdown_path,
        markdown_bytes,
        pdf_path,
        pdf_bytes,
        telegram_delivery_path,
        telegram_delivery_bytes,
        telegram_delivery,
    })
}

fn write_artifact_file(
    base_dir: &Path,
    relative_path: &str,
    bytes: &[u8],
    mode: ArtifactWriteMode,
    artifact_journal: Option<&ArtifactWriteJournal>,
) -> Result<()> {
    let resolved = resolve_artifact_file(base_dir, Path::new(relative_path), true)?;
    let file_name = resolved
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .context("artifact path has no UTF-8 file name")?;
    let temporary_path = resolved.parent.join(format!(
        ".{file_name}.tmp-{}",
        Uuid::new_v4().to_string().replace('-', "")
    ));

    let write_result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
        }
        let mut temporary = options.open(&temporary_path).with_context(|| {
            format!(
                "failed to create staged schedule artifact {}",
                temporary_path.display()
            )
        })?;
        temporary.write_all(bytes)?;
        temporary.sync_all()?;
        drop(temporary);

        match mode {
            ArtifactWriteMode::AtomicReplace => {
                fs::rename(&temporary_path, &resolved.path).with_context(|| {
                    format!(
                        "failed to atomically publish schedule artifact {}",
                        resolved.path.display()
                    )
                })?;
            }
            ArtifactWriteMode::NoOverwrite => {
                match fs::hard_link(&temporary_path, &resolved.path) {
                    Ok(()) => {
                        if let Some(journal) = artifact_journal {
                            if let Err(error) = journal.record_created(&resolved) {
                                let _ = remove_contained_artifact_file(
                                    &resolved.artifact_root,
                                    &resolved.path,
                                );
                                return Err(error);
                            }
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                        if !artifact_file_matches(&resolved, bytes)? {
                            anyhow::bail!(
                                "schedule artifact no-overwrite conflict at {}",
                                resolved.path.display()
                            );
                        }
                    }
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!(
                                "failed to publish no-overwrite schedule artifact {}",
                                resolved.path.display()
                            )
                        });
                    }
                }
            }
        }
        sync_directory(&resolved.parent)
    })();

    match fs::remove_file(&temporary_path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(cleanup_error) if write_result.is_ok() => {
            return Err(cleanup_error).with_context(|| {
                format!(
                    "failed to remove staged schedule artifact {}",
                    temporary_path.display()
                )
            });
        }
        Err(_) => {}
    }
    write_result
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("failed to open artifact directory {}", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync artifact directory {}", path.display()))
}

fn daily_goal_markdown_report_bytes(workflow_id: &str, goal: &str) -> Vec<u8> {
    format!(
        "# Daily Goal Research: {goal}\n\n\
         - Workflow: `{workflow_id}`\n\
         - Discovery: DuckDuckGo query plan for upcoming hackathons and marathons.\n\
         - Inspection: Playwright page/regulation review queued under Forge-owned semantics.\n\
         - Eligibility: first phase online, Pelotas/RS geography, Engineering Production + ADS fit.\n\
         - Economics: cost, travel burden and registration clarity are scored before recommendation.\n\
         - Ambition fit: opportunities are filtered for strong alignment with the user's stated ambitions.\n",
    )
    .into_bytes()
}

fn telegram_delivery_record(
    goal: &str,
    markdown_path: &str,
    pdf_path: &str,
    lineage: ArtifactLineageRecord,
) -> TelegramDeliveryRecord {
    TelegramDeliveryRecord {
        schema_version: "forge.telegram_delivery.v1".to_string(),
        goal: goal.to_string(),
        lineage,
        status: "recorded".to_string(),
        channel: "telegram".to_string(),
        configured_telegram_chat_ref: "configured_telegram_destination".to_string(),
        markdown_path: markdown_path.to_string(),
        pdf_path: pdf_path.to_string(),
        secret_exposed: false,
        simulated: true,
    }
}

fn register_generated_goal_artifacts(
    workflow: &mut Workflow,
    artifacts: GeneratedDailyGoalArtifacts,
) -> DailyGoalSmokeGoalReport {
    upsert_artifact(
        workflow,
        "markdown_report",
        &artifacts.markdown_path,
        &artifacts.markdown_bytes,
        Some(artifacts.lineage.clone()),
    );
    upsert_artifact(
        workflow,
        "pdf_report",
        &artifacts.pdf_path,
        &artifacts.pdf_bytes,
        Some(artifacts.lineage.clone()),
    );
    upsert_artifact(
        workflow,
        "telegram_delivery",
        &artifacts.telegram_delivery_path,
        &artifacts.telegram_delivery_bytes,
        Some(artifacts.lineage.clone()),
    );
    DailyGoalSmokeGoalReport {
        goal: artifacts.goal,
        markdown_path: artifacts.markdown_path,
        pdf_path: artifacts.pdf_path,
        lineage: artifacts.lineage,
        telegram_delivery: artifacts.telegram_delivery,
    }
}

fn upsert_artifact(
    workflow: &mut Workflow,
    kind: &str,
    relative_path: &str,
    bytes: &[u8],
    lineage: Option<ArtifactLineageRecord>,
) {
    workflow
        .artifacts
        .retain(|artifact| artifact.path != relative_path);
    workflow.artifacts.push(ArtifactRecord {
        id: format!("artifact_{}", Uuid::new_v4().to_string().replace('-', "")),
        kind: kind.to_string(),
        path: relative_path.to_string(),
        sha256: hex_sha256(bytes),
        tags: Vec::new(),
        created_at: Utc::now(),
        lineage,
    });
    workflow
        .tasks
        .iter_mut()
        .filter(|task| task.native_subflow.is_some())
        .for_each(|task| {
            if task.title.contains("Generate")
                || task.title.contains("Record")
                || task.title.contains("Search")
                || task.title.contains("Inspect")
            {
                task.status = TaskStatus::Completed;
            }
        });
}

fn minimal_pdf_bytes(title: &str) -> Vec<u8> {
    let escaped = title.replace('(', "\\(").replace(')', "\\)");
    format!(
        "%PDF-1.4\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >> endobj\n\
         4 0 obj << /Length 72 >> stream\n\
         BT /F1 18 Tf 72 720 Td ({escaped}) Tj ET\n\
         endstream endobj\n\
         trailer << /Root 1 0 R >>\n%%EOF\n"
    )
    .into_bytes()
}

#[cfg(test)]
mod schedule_concurrency_tests {
    use super::*;
    use rusqlite::{params, Connection};
    use tempfile::tempdir;

    fn create_due_schedule(store: &ForgeStore) -> (String, String) {
        let created = create_daily_goal_research_workflow(
            store,
            vec!["lease fencing".to_string()],
            "UTC",
            "0 9 * * *",
            "schedule-test",
        )
        .unwrap();
        let mut workflow = store.load_workflow(&created.workflow_id).unwrap();
        let task_id = workflow
            .tasks
            .iter_mut()
            .find_map(|task| {
                task.schedule.as_mut().map(|schedule| {
                    schedule.next_run_at = Some(Utc::now() - Duration::seconds(1));
                    task.id.clone()
                })
            })
            .unwrap();
        store.save_workflow(&workflow).unwrap();
        (created.workflow_id, task_id)
    }

    #[test]
    fn stale_schedule_lease_is_fenced_after_reacquisition() {
        let temp = tempdir().unwrap();
        let store_path = temp.path().join("forge.sqlite");
        let store = ForgeStore::open(&store_path).unwrap();
        let (workflow_id, task_id) = create_due_schedule(&store);

        let first = acquire_task_lease(&store, &workflow_id, &task_id, "worker-old", 60)
            .unwrap()
            .lease
            .unwrap();
        let mut expired = first.clone();
        expired.expires_at = Utc::now() - Duration::seconds(1);
        let connection = Connection::open(&store_path).unwrap();
        connection
            .execute(
                "UPDATE task_leases
                 SET expires_at = ?1, data_json = ?2
                 WHERE workflow_id = ?3 AND task_id = ?4",
                params![
                    expired.expires_at.to_rfc3339(),
                    serde_json::to_string(&expired).unwrap(),
                    workflow_id,
                    task_id
                ],
            )
            .unwrap();
        drop(connection);

        let replacement = acquire_task_lease(&store, &workflow_id, &task_id, "worker-new", 60)
            .unwrap()
            .lease
            .unwrap();
        let error = run_due_workflow_and_release_lease(
            &store,
            &workflow_id,
            &task_id,
            Some(&first.lease_id),
            "worker-old",
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("schedule lease fencing rejected"),
            "{error:#}"
        );

        let current: TaskLease = serde_json::from_value(
            store
                .load_task_lease(&workflow_id, &task_id)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(current.lease_id, replacement.lease_id);
        assert_eq!(current.executor, "worker-new");
        let still_due = store
            .load_workflow(&workflow_id)
            .unwrap()
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| task.schedule.as_ref())
            .and_then(|schedule| schedule.next_run_at)
            .is_some_and(|next_run_at| next_run_at <= Utc::now());
        assert!(still_due, "stale worker must not advance the schedule");

        let (report, released) = run_due_workflow_and_release_lease(
            &store,
            &workflow_id,
            &task_id,
            Some(&replacement.lease_id),
            "worker-new",
        )
        .unwrap();
        assert!(report.unwrap().due_executed);
        assert!(released);
        assert!(store
            .load_task_lease(&workflow_id, &task_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn no_overwrite_artifact_conflict_preserves_committed_bytes() {
        let temp = tempdir().unwrap();
        let journal = ArtifactWriteJournal::default();
        let path = "artifacts/workflow/runs/run-1/report.md";
        write_artifact_file(
            temp.path(),
            path,
            b"committed",
            ArtifactWriteMode::NoOverwrite,
            Some(&journal),
        )
        .unwrap();

        let error = write_artifact_file(
            temp.path(),
            path,
            b"stale-worker",
            ArtifactWriteMode::NoOverwrite,
            Some(&journal),
        )
        .unwrap_err();
        assert!(error.to_string().contains("no-overwrite conflict"));
        assert_eq!(fs::read(temp.path().join(path)).unwrap(), b"committed");
    }

    #[test]
    fn artifact_writer_rejects_traversal_absolute_and_separator_paths_in_both_modes() {
        let temp = tempdir().unwrap();
        let base = temp.path().join("base");
        fs::create_dir(&base).unwrap();
        let sentinel = temp.path().join("sentinel");
        fs::write(&sentinel, b"external sentinel").unwrap();
        let absolute = sentinel.to_string_lossy().to_string();
        let paths = [
            "../sentinel",
            "artifacts/workflow/../../../sentinel",
            r"artifacts\workflow\..\sentinel",
            absolute.as_str(),
        ];

        for mode in [
            ArtifactWriteMode::AtomicReplace,
            ArtifactWriteMode::NoOverwrite,
        ] {
            for path in paths {
                let error = write_artifact_file(&base, path, b"attacker", mode, None).unwrap_err();
                assert!(error.to_string().contains("artifact"), "{path}: {error:#}");
                assert_eq!(fs::read(&sentinel).unwrap(), b"external sentinel");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn artifact_writer_rejects_symlink_ancestors_and_symlink_targets_in_both_modes() {
        use std::os::unix::fs::symlink;

        for target_is_ancestor in [true, false] {
            for mode in [
                ArtifactWriteMode::AtomicReplace,
                ArtifactWriteMode::NoOverwrite,
            ] {
                let temp = tempdir().unwrap();
                let external = temp.path().join("external");
                let artifact_root = temp.path().join("artifacts");
                fs::create_dir(&external).unwrap();
                fs::create_dir(&artifact_root).unwrap();
                let sentinel = external.join("sentinel");
                fs::write(&sentinel, b"external sentinel").unwrap();

                let relative = "artifacts/workflow/report.md";
                if target_is_ancestor {
                    symlink(&external, artifact_root.join("workflow")).unwrap();
                } else {
                    fs::create_dir(artifact_root.join("workflow")).unwrap();
                    symlink(&sentinel, temp.path().join(relative)).unwrap();
                }

                let error =
                    write_artifact_file(temp.path(), relative, b"external sentinel", mode, None)
                        .unwrap_err();
                assert!(error.to_string().contains("symlink"), "{error:#}");
                assert_eq!(fs::read(&sentinel).unwrap(), b"external sentinel");
            }
        }
    }
}
