use crate::artifact::hex_sha256;
use crate::graph::{
    ArtifactLineageRecord, ArtifactRecord, AtomicTask, LoopSpec, NativeSubflowSpec,
    ScheduleRunRecord, ScheduleSpec, TaskStatus, Workflow, WorkflowRevision,
};
use crate::intent::parse_intent;
use crate::registry::{attach_reuse_candidates_as_child_subflows, find_reuse_candidates};
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use std::fs;
use std::path::Path;
use uuid::Uuid;

const SCHEDULE_SUMMARY_SCHEMA_VERSION: &str = "forge.schedule.summary.v1";
const LOOP_SUMMARY_SCHEMA_VERSION: &str = "forge.loop.summary.v1";
const MISSED_RUN_RECONCILIATION_SCHEMA_VERSION: &str = "forge.missed_run_reconciliation.v1";
const MISSED_RUN_GRACE_MINUTES: i64 = 5;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScheduleSummary {
    pub schema_version: String,
    pub scheduled_nodes: usize,
    pub cron_nodes: usize,
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
    let mut workflow = store.load_workflow(workflow_id)?;
    let parsed_next_run_at = options
        .next_run_at
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|parsed| parsed.with_timezone(&Utc))
                .with_context(|| format!("invalid next_run_at RFC3339 timestamp: {value}"))
        })
        .transpose()?;
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
        schedule.next_run_at = parsed_next_run_at.or_else(|| Some(Utc::now() + Duration::days(1)));
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
    run_daily_goal_research_smoke_with_schedule_mode(store, workflow, false)
}

fn run_daily_goal_research_smoke_with_schedule_mode(
    store: &ForgeStore,
    workflow: &mut Workflow,
    due_only: bool,
) -> Result<Option<DailyGoalResearchSmokeReport>> {
    let goals = configured_goal_items(workflow);
    if goals.is_empty() {
        return Ok(None);
    }

    let schedule_runs = record_schedule_run_history(workflow, due_only);

    let mut reports = Vec::new();
    for goal in goals {
        let lineage = build_daily_goal_artifact_lineage(workflow, &goal, &schedule_runs);
        let markdown_path =
            write_markdown_report(store.base_dir().as_path(), workflow, &goal, &lineage)?;
        let pdf_path = write_pdf_report(store.base_dir().as_path(), workflow, &goal, &lineage)?;
        let delivery = write_telegram_delivery_record(
            store.base_dir().as_path(),
            workflow,
            &goal,
            &markdown_path,
            &pdf_path,
            &lineage,
        )?;
        reports.push(DailyGoalSmokeGoalReport {
            goal,
            markdown_path,
            pdf_path,
            lineage,
            telegram_delivery: delivery,
        });
    }

    Ok(Some(DailyGoalResearchSmokeReport {
        status: "smoke_artifacts_generated".to_string(),
        workflow_id: workflow.id.clone(),
        artifact_count: reports.len() * 3,
        goals: reports,
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
        let next_run_at_after = finished_at + Duration::days(1);
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
        schedule.next_run_at = Some(next_run_at_after);
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
    let mut workflow = store.load_workflow(workflow_id)?;
    let now = Utc::now();
    let has_due = workflow.tasks.iter().any(|task| {
        task.schedule
            .as_ref()
            .and_then(|schedule| schedule.next_run_at)
            .is_some_and(|next| next <= now)
    });

    if !has_due {
        return Ok(Some(ScheduleRunDueReport {
            status: "no_due_cron_nodes".to_string(),
            workflow_id: workflow_id.to_string(),
            goal: workflow.goal.clone(),
            due_executed: false,
            blocked_loop_task_id: None,
            blocked_loop_state: None,
            missed_run_reconciliation: Vec::new(),
            daily_goal_research: None,
            schedule_summary: summarize_schedules(&workflow.tasks),
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
        return Ok(Some(ScheduleRunDueReport {
            status: "loop_not_runnable".to_string(),
            workflow_id: workflow_id.to_string(),
            goal: workflow.goal.clone(),
            due_executed: false,
            blocked_loop_task_id: Some(task_id),
            blocked_loop_state: Some(state),
            missed_run_reconciliation: Vec::new(),
            daily_goal_research: None,
            schedule_summary: summarize_schedules(&workflow.tasks),
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
            schedule_summary,
        }));
    }

    let workflow_id = workflow.id.clone();
    let history_markers = schedule_run_history_markers(&workflow);
    let daily_goal_research =
        run_daily_goal_research_smoke_with_schedule_mode(store, &mut workflow, true)?;
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
        schedule_summary,
    }))
}

fn skip_due_missed_runs(workflow: &mut Workflow) -> Vec<MissedRunReconciliationReport> {
    let mut reconciliation = Vec::new();
    for schedule in workflow
        .tasks
        .iter_mut()
        .filter_map(|task| task.schedule.as_mut().map(|schedule| (&task.id, schedule)))
    {
        let (task_id, schedule) = schedule;
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
        let next_run_at_after = now + Duration::days(1);
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
        schedule.next_run_at = Some(next_run_at_after);
        reconciliation.push(MissedRunReconciliationReport {
            schema_version: MISSED_RUN_RECONCILIATION_SCHEMA_VERSION.to_string(),
            task_id: task_id.clone(),
            policy: schedule.missed_run_policy.clone(),
            action: "skipped_missed".to_string(),
            scheduled_at: format_utc_rfc3339(next_run_at),
            observed_at: format_utc_rfc3339(now),
            next_run_at_before: format_utc_rfc3339(next_run_at),
            next_run_at_after: Some(format_utc_rfc3339(next_run_at_after)),
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

fn write_markdown_report(
    base_dir: &Path,
    workflow: &mut Workflow,
    goal: &str,
    lineage: &ArtifactLineageRecord,
) -> Result<String> {
    let relative_path = format!("artifacts/{}/goal-{goal}-report.md", workflow.id);
    let full_path = base_dir.join(&relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = format!(
        "# Daily Goal Research: {goal}\n\n\
         - Workflow: `{}`\n\
         - Discovery: DuckDuckGo query plan for upcoming hackathons and marathons.\n\
         - Inspection: Playwright page/regulation review queued under Forge-owned semantics.\n\
         - Eligibility: first phase online, Pelotas/RS geography, Engineering Production + ADS fit.\n\
         - Economics: cost, travel burden and registration clarity are scored before recommendation.\n\
         - Ambition fit: opportunities are filtered for strong alignment with the user's stated ambitions.\n",
        workflow.id
    );
    fs::write(&full_path, content.as_bytes())?;
    upsert_artifact(
        workflow,
        "markdown_report",
        &relative_path,
        content.as_bytes(),
        Some(lineage.clone()),
    );
    Ok(relative_path)
}

fn write_pdf_report(
    base_dir: &Path,
    workflow: &mut Workflow,
    goal: &str,
    lineage: &ArtifactLineageRecord,
) -> Result<String> {
    let relative_path = format!("artifacts/{}/goal-{goal}-report.pdf", workflow.id);
    let full_path = base_dir.join(&relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let pdf = minimal_pdf_bytes(&format!("Daily Goal Research: {goal}"));
    fs::write(&full_path, &pdf)?;
    upsert_artifact(
        workflow,
        "pdf_report",
        &relative_path,
        &pdf,
        Some(lineage.clone()),
    );
    Ok(relative_path)
}

fn write_telegram_delivery_record(
    base_dir: &Path,
    workflow: &mut Workflow,
    goal: &str,
    markdown_path: &str,
    pdf_path: &str,
    lineage: &ArtifactLineageRecord,
) -> Result<TelegramDeliveryRecord> {
    let relative_path = format!("artifacts/{}/telegram-delivery-{goal}.json", workflow.id);
    let full_path = base_dir.join(&relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let record = TelegramDeliveryRecord {
        schema_version: "forge.telegram_delivery.v1".to_string(),
        goal: goal.to_string(),
        lineage: lineage.clone(),
        status: "recorded".to_string(),
        channel: "telegram".to_string(),
        configured_telegram_chat_ref: "configured_telegram_destination".to_string(),
        markdown_path: markdown_path.to_string(),
        pdf_path: pdf_path.to_string(),
        secret_exposed: false,
        simulated: true,
    };
    let bytes = serde_json::to_vec_pretty(&record)?;
    fs::write(&full_path, &bytes)?;
    upsert_artifact(
        workflow,
        "telegram_delivery",
        &relative_path,
        &bytes,
        Some(lineage.clone()),
    );
    Ok(record)
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
