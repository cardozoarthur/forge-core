use crate::request::{
    list_requests, load_run_record, mark_run_needs_attention, recover_stale_request,
    step_request_with_supervisor_fence, update_run_record, RequestDriveReport, RequestListRow,
    RequestStepReport, RequestSupervisorFence,
};
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

pub const REQUEST_SUPERVISOR_SCHEMA_VERSION: &str = "forge.request_supervisor.v1";
pub const DEFAULT_REQUEST_SUPERVISOR_TTL_SECONDS: u64 = 300;
pub const DEFAULT_REQUEST_SUPERVISOR_MAX_STEPS_PER_RUN: usize = 1;
pub const MAX_REQUEST_SUPERVISOR_STEPS_PER_RUN: usize = 16;

const MANUAL_ATTENTION_STATUSES: [&str; 4] = [
    "handoff_required",
    "completion_audit_required",
    "rework_required",
    "validation_failed",
];

#[derive(Debug, Clone)]
pub struct RequestSupervisorOptions {
    pub executor: String,
    pub origin: String,
    pub instance_id: String,
    pub ttl_seconds: u64,
    pub max_steps_per_run: usize,
}

impl Default for RequestSupervisorOptions {
    fn default() -> Self {
        Self::new(
            "forge-request-supervisor",
            "forge-request-supervisor",
            DEFAULT_REQUEST_SUPERVISOR_TTL_SECONDS,
            DEFAULT_REQUEST_SUPERVISOR_MAX_STEPS_PER_RUN,
        )
    }
}

impl RequestSupervisorOptions {
    pub fn new(
        executor: impl Into<String>,
        origin: impl Into<String>,
        ttl_seconds: u64,
        max_steps_per_run: usize,
    ) -> Self {
        let executor = executor.into();
        Self {
            instance_id: format!(
                "{}:{}:{}",
                executor,
                std::process::id(),
                Uuid::new_v4().simple()
            ),
            executor,
            origin: origin.into(),
            ttl_seconds,
            max_steps_per_run,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.executor.trim().is_empty() {
            anyhow::bail!("request supervisor executor cannot be empty");
        }
        if self.origin.trim().is_empty() {
            anyhow::bail!("request supervisor origin cannot be empty");
        }
        if self.instance_id.trim().is_empty() {
            anyhow::bail!("request supervisor instance_id cannot be empty");
        }
        if self.instance_id.len() > 512 {
            anyhow::bail!("request supervisor instance_id cannot exceed 512 bytes");
        }
        if self.ttl_seconds == 0 {
            anyhow::bail!("request supervisor ttl_seconds must be at least 1");
        }
        if self.max_steps_per_run == 0 {
            anyhow::bail!("request supervisor max_steps_per_run must be at least 1");
        }
        if self.max_steps_per_run > MAX_REQUEST_SUPERVISOR_STEPS_PER_RUN {
            anyhow::bail!(
                "request supervisor max_steps_per_run cannot exceed {}",
                MAX_REQUEST_SUPERVISOR_STEPS_PER_RUN
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RequestSupervisorCounts {
    pub scanned: usize,
    pub eligible: usize,
    pub recovered: usize,
    pub advanced: usize,
    pub advanced_steps: usize,
    pub needs_attention: usize,
    pub skipped_external_active: usize,
    pub skipped_lease_contended: usize,
    pub skipped_inactive: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestSupervisorRunOutcome {
    RecoveredStale,
    Advanced,
    Completed,
    NeedsAttention,
    SkippedExternalActive,
    SkippedLeaseContended,
    SkippedInactive,
    Observed,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RequestSupervisorAttentionReason {
    pub schema_version: String,
    pub source: String,
    pub status: String,
    pub action: String,
    pub reason: String,
}

impl RequestSupervisorAttentionReason {
    fn from_step_report(source: &'static str, status: &str, action: &str, reason: &str) -> Self {
        Self {
            schema_version: "forge.request_supervisor_attention_reason.v1".to_string(),
            source: source.to_string(),
            status: status.to_string(),
            action: action.to_string(),
            reason: reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestSupervisorRunResult {
    pub run_id: String,
    pub workflow_id: String,
    pub initial_status: String,
    pub final_status: String,
    pub initial_executor: Option<String>,
    pub final_executor: Option<String>,
    pub initial_heartbeat_status: String,
    pub outcome: RequestSupervisorRunOutcome,
    pub steps_attempted: usize,
    pub steps_advanced: usize,
    pub attention_reason: Option<RequestSupervisorAttentionReason>,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestSupervisorReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub instance_id: String,
    pub origin: String,
    pub ttl_seconds: u64,
    pub max_steps_per_run: usize,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub counts: RequestSupervisorCounts,
    pub runs: Vec<RequestSupervisorRunResult>,
}

pub fn supervise_requests_once(
    store: &ForgeStore,
    options: &RequestSupervisorOptions,
) -> Result<RequestSupervisorReport> {
    options.validate()?;
    let started_at = Utc::now();
    let listed = list_requests(store, None)?;
    let mut counts = RequestSupervisorCounts {
        scanned: listed.runs.len(),
        ..RequestSupervisorCounts::default()
    };
    let mut runs = Vec::with_capacity(listed.runs.len());

    for listed_run in listed.runs {
        let result = match supervise_run_once(store, &listed_run, options) {
            Ok(result) => result,
            Err(error) => failed_result(store, &listed_run, error),
        };
        update_counts(&mut counts, &result);
        runs.push(result);
    }

    Ok(RequestSupervisorReport {
        schema_version: REQUEST_SUPERVISOR_SCHEMA_VERSION.to_string(),
        status: if counts.failures == 0 {
            "request_supervisor_completed".to_string()
        } else {
            "request_supervisor_completed_with_failures".to_string()
        },
        executor: options.executor.clone(),
        instance_id: options.instance_id.clone(),
        origin: options.origin.clone(),
        ttl_seconds: options.ttl_seconds,
        max_steps_per_run: options.max_steps_per_run,
        started_at,
        completed_at: Utc::now(),
        counts,
        runs,
    })
}

pub fn supervise_request_once(
    store: &ForgeStore,
    run_id: &str,
    options: &RequestSupervisorOptions,
) -> Result<RequestSupervisorRunResult> {
    options.validate()?;
    let listed = list_requests(store, None)?
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id)
        .with_context(|| format!("request supervisor run not found: {run_id}"))?;
    supervise_run_once(store, &listed, options)
}

fn supervise_run_once(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    options: &RequestSupervisorOptions,
) -> Result<RequestSupervisorRunResult> {
    if listed_run.status == "running" {
        let current = load_run_record(store, &listed_run.run_id)?;
        let now = Utc::now();
        let live_lease_owner = match (
            current.supervisor_instance_id.as_deref(),
            current.supervisor_lease_expires_at,
        ) {
            (Some(owner), Some(expires_at)) if expires_at > now => Some(owner),
            _ => None,
        };
        if let Some(owner) = live_lease_owner {
            if owner != options.instance_id {
                return Ok(skipped_result(
                    listed_run,
                    RequestSupervisorRunOutcome::SkippedLeaseContended,
                    &format!(
                        "Request supervisor lease is held by live instance {owner} through fencing token {}; instance {} did not classify its heartbeat as stale or advance it.",
                        current.supervisor_fencing_token, options.instance_id
                    ),
                ));
            }
            return advance_eligible_run(store, listed_run, options);
        }
        if listed_run.activity.heartbeat_status == "stale" {
            return recover_stale_result(store, listed_run, options);
        }
    }

    if listed_run.status == "running" {
        let owned_by_supervisor =
            listed_run.activity.executor.as_deref() == Some(options.executor.as_str());
        let executor_is_active = matches!(
            listed_run.activity.heartbeat_status.as_str(),
            "fresh" | "process_alive"
        );
        if executor_is_active && !owned_by_supervisor {
            return Ok(skipped_result(
                listed_run,
                RequestSupervisorRunOutcome::SkippedExternalActive,
                "Fresh or live executor activity belongs to another executor; the request supervisor did not take over the run.",
            ));
        }
        if !owned_by_supervisor {
            return park_unsafe_running_state(
                store,
                listed_run,
                options,
                "running_executor_not_owned",
                "inspect_executor_ownership",
                "Running request has no active lease owned by this supervisor; Forge requires inspection instead of implicit takeover.",
            );
        }
    } else if !matches!(listed_run.status.as_str(), "accepted" | "resumed") {
        return Ok(skipped_result(
            listed_run,
            RequestSupervisorRunOutcome::SkippedInactive,
            "Run status is not eligible for automatic request supervision.",
        ));
    }

    advance_eligible_run(store, listed_run, options)
}

fn recover_stale_result(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    options: &RequestSupervisorOptions,
) -> Result<RequestSupervisorRunResult> {
    let recovered = recover_stale_request(store, &listed_run.run_id, &options.origin)?;
    Ok(RequestSupervisorRunResult {
        run_id: recovered.run_id,
        workflow_id: recovered.workflow_id,
        initial_status: listed_run.status.clone(),
        final_status: recovered.status,
        initial_executor: listed_run.activity.executor.clone(),
        final_executor: recovered.activity.executor,
        initial_heartbeat_status: listed_run.activity.heartbeat_status.clone(),
        outcome: RequestSupervisorRunOutcome::RecoveredStale,
        steps_attempted: 0,
        steps_advanced: 0,
        attention_reason: None,
        reason: recovered.recovery.reason,
        error: None,
    })
}

fn advance_eligible_run(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    options: &RequestSupervisorOptions,
) -> Result<RequestSupervisorRunResult> {
    let mut steps_attempted = 0;
    let mut steps_advanced = 0;
    let mut last_reason =
        "Eligible run was inspected without an automatic state transition.".to_string();

    for _ in 0..options.max_steps_per_run {
        steps_attempted += 1;
        let (step, fence) = match claim_and_step_request(store, &listed_run.run_id, options) {
            Ok(SupervisorStepAttempt::Stepped { step, fence }) => (*step, fence),
            Ok(SupervisorStepAttempt::Contended(reason)) => {
                return Ok(skipped_result(
                    listed_run,
                    RequestSupervisorRunOutcome::SkippedLeaseContended,
                    &reason,
                ));
            }
            Ok(SupervisorStepAttempt::Ineligible(reason)) => {
                return Ok(skipped_result(
                    listed_run,
                    RequestSupervisorRunOutcome::SkippedInactive,
                    &reason,
                ));
            }
            Ok(SupervisorStepAttempt::Stale) => {
                return recover_stale_result(store, listed_run, options);
            }
            Ok(SupervisorStepAttempt::Unsafe(reason)) => {
                return park_unsafe_running_state(
                    store,
                    listed_run,
                    options,
                    "supervisor_lease_unreconciled",
                    "inspect_supervisor_lease",
                    &reason,
                );
            }
            Err(error) => {
                return Ok(failed_result_with_progress(
                    store,
                    listed_run,
                    error,
                    steps_attempted,
                    steps_advanced,
                ));
            }
        };

        if step.status == "stepped" {
            steps_advanced += 1;
        }

        if let Some(attention_reason) = manual_attention_boundary(&step) {
            return match park_after_step(
                store,
                listed_run,
                options,
                Some(&fence),
                (steps_attempted, steps_advanced),
                "supervisor_manual_boundary",
                attention_reason,
            ) {
                Ok(result) => Ok(result),
                Err(error) => Ok(failed_result_with_progress(
                    store,
                    listed_run,
                    error,
                    steps_attempted,
                    steps_advanced,
                )),
            };
        }

        last_reason = step.reason.clone();
        match step.status.as_str() {
            "stepped" => continue,
            "complete" | "completed" => {
                return observed_step_result(
                    store,
                    listed_run,
                    RequestSupervisorRunOutcome::Completed,
                    steps_attempted,
                    steps_advanced,
                    step.reason,
                );
            }
            _ => {
                return observed_step_result(
                    store,
                    listed_run,
                    RequestSupervisorRunOutcome::Observed,
                    steps_attempted,
                    steps_advanced,
                    step.reason,
                );
            }
        }
    }

    observed_step_result(
        store,
        listed_run,
        RequestSupervisorRunOutcome::Advanced,
        steps_attempted,
        steps_advanced,
        format!(
            "Bounded request supervision stopped after {} step attempt(s). Last step: {}",
            options.max_steps_per_run, last_reason
        ),
    )
}

enum SupervisorStepAttempt {
    Stepped {
        step: Box<RequestStepReport>,
        fence: RequestSupervisorFence,
    },
    Contended(String),
    Ineligible(String),
    Stale,
    Unsafe(String),
}

enum SupervisorLeaseClaim {
    Claimed {
        fence: RequestSupervisorFence,
        lease_expires_at: DateTime<Utc>,
    },
    Contended(String),
    Ineligible(String),
    Stale,
    Unsafe(String),
}

fn claim_and_step_request(
    store: &ForgeStore,
    run_id: &str,
    options: &RequestSupervisorOptions,
) -> Result<SupervisorStepAttempt> {
    let claim = store.with_transaction(|| {
        let mut run = load_run_record(store, run_id)?;
        if !matches!(run.status.as_str(), "accepted" | "resumed" | "running") {
            return Ok(SupervisorLeaseClaim::Ineligible(format!(
                "Run status {} is no longer eligible for automatic request supervision.",
                run.status
            )));
        }
        if run.status == "running"
            && run.active_executor.as_deref() != Some(options.executor.as_str())
        {
            return Ok(SupervisorLeaseClaim::Contended(format!(
                "Running request is owned by executor {}; supervisor {} did not take it over.",
                run.active_executor.as_deref().unwrap_or("<unknown>"),
                options.instance_id
            )));
        }

        let now = Utc::now();
        let lease_is_live = run
            .supervisor_lease_expires_at
            .is_some_and(|expires_at| expires_at > now);
        let heartbeat_is_live = run
            .heartbeat_expires_at
            .is_some_and(|expires_at| expires_at > now);
        match (
            run.supervisor_instance_id.as_deref(),
            run.supervisor_lease_expires_at,
        ) {
            (None, Some(expires_at)) => {
                return Ok(SupervisorLeaseClaim::Unsafe(format!(
                    "Request supervisor lease metadata is inconsistent: lease expiry {expires_at} has no owner."
                )));
            }
            (Some(owner), None) => {
                return Ok(SupervisorLeaseClaim::Unsafe(format!(
                    "Request supervisor lease metadata is inconsistent: owner {owner} has no expiry."
                )));
            }
            _ => {}
        }
        if let Some(owner) = run.supervisor_instance_id.as_deref() {
            if owner != options.instance_id && lease_is_live {
                return Ok(SupervisorLeaseClaim::Contended(format!(
                    "Request supervisor lease is held by instance {owner} through fencing token {}; instance {} did not advance the run.",
                    run.supervisor_fencing_token, options.instance_id
                )));
            }
        }
        if run.status == "running"
            && run
                .heartbeat_expires_at
                .is_some_and(|expires_at| expires_at <= now)
            && !lease_is_live
        {
            return Ok(SupervisorLeaseClaim::Stale);
        }
        if run.status == "running"
            && run.heartbeat_expires_at.is_none()
            && run.supervisor_instance_id.is_none()
        {
            return Ok(SupervisorLeaseClaim::Unsafe(
                "Running request has no supervisor identity or heartbeat; Forge requires reconciliation instead of implicit takeover.".to_string(),
            ));
        }
        if let Some(owner) = run.supervisor_instance_id.as_deref() {
            if owner != options.instance_id && heartbeat_is_live {
                return Ok(SupervisorLeaseClaim::Contended(format!(
                    "Request supervisor lease is held by instance {owner} through fencing token {}; instance {} did not advance the run.",
                    run.supervisor_fencing_token, options.instance_id
                )));
            }
            if owner != options.instance_id && run.status == "running" {
                return Ok(SupervisorLeaseClaim::Unsafe(format!(
                    "Request supervisor lease for instance {owner} is no longer live and no stale heartbeat transition is available; Forge requires reconciliation before instance {} can advance the run.",
                    options.instance_id
                )));
            }
        }

        let lease_epoch_changed = run.supervisor_instance_id.as_deref()
            != Some(options.instance_id.as_str())
            || !lease_is_live;
        if lease_epoch_changed {
            run.supervisor_fencing_token =
                run.supervisor_fencing_token.checked_add(1).with_context(|| {
                    format!("request supervisor fencing token overflow for run {run_id}")
                })?;
        } else if run.supervisor_fencing_token == 0 {
            anyhow::bail!(
                "request supervisor lease for run {run_id} has an invalid zero fencing token"
            );
        }
        let fencing_token = run.supervisor_fencing_token;
        let lease_expires_at =
            now + Duration::seconds(options.ttl_seconds.min(i64::MAX as u64) as i64);
        run.supervisor_instance_id = Some(options.instance_id.clone());
        run.supervisor_lease_expires_at = Some(lease_expires_at);
        update_run_record(store, &run)?;
        if lease_epoch_changed {
            store.record_event(
                &run.workflow_id,
                "async_request_supervisor_lease_acquired",
                &serde_json::json!({
                    "schema_version": "forge.request_supervisor_lease.v1",
                    "run_id": run.run_id,
                    "executor": options.executor,
                    "instance_id": options.instance_id,
                    "fencing_token": fencing_token,
                    "lease_expires_at": lease_expires_at,
                    "origin": options.origin,
                }),
            )?;
        }

        Ok(SupervisorLeaseClaim::Claimed {
            fence: RequestSupervisorFence {
                instance_id: options.instance_id.clone(),
                fencing_token,
            },
            lease_expires_at,
        })
    })?;

    let (fence, lease_expires_at) = match claim {
        SupervisorLeaseClaim::Claimed {
            fence,
            lease_expires_at,
        } => (fence, lease_expires_at),
        SupervisorLeaseClaim::Contended(reason) => {
            return Ok(SupervisorStepAttempt::Contended(reason));
        }
        SupervisorLeaseClaim::Ineligible(reason) => {
            return Ok(SupervisorStepAttempt::Ineligible(reason));
        }
        SupervisorLeaseClaim::Stale => return Ok(SupervisorStepAttempt::Stale),
        SupervisorLeaseClaim::Unsafe(reason) => {
            return Ok(SupervisorStepAttempt::Unsafe(reason));
        }
    };

    let step = step_request_with_supervisor_fence(
        store,
        run_id,
        &options.executor,
        options.ttl_seconds,
        &options.origin,
        &fence,
    )?;

    store.with_transaction(|| {
        let mut current = load_run_record(store, run_id)?;
        if current.supervisor_instance_id.is_some() {
            if current.supervisor_instance_id.as_deref() != Some(options.instance_id.as_str())
                || current.supervisor_fencing_token != fence.fencing_token
            {
                anyhow::bail!(
                    "request supervisor instance {} was fenced while advancing run {run_id}",
                    options.instance_id
                );
            }
            let now = Utc::now();
            if current
                .supervisor_lease_expires_at
                .is_none_or(|expires_at| expires_at <= now)
            {
                anyhow::bail!(
                    "request supervisor instance {} lease expired while advancing run {run_id}; fencing token {} cannot be reused",
                    options.instance_id,
                    fence.fencing_token
                );
            }
            if current.status == "running" {
                let renewed_expires_at = current
                    .heartbeat_expires_at
                    .unwrap_or(lease_expires_at)
                    .max(lease_expires_at);
                if current.supervisor_lease_expires_at != Some(renewed_expires_at) {
                    current.supervisor_lease_expires_at = Some(renewed_expires_at);
                    update_run_record(store, &current)?;
                }
            }
        } else if current.status == "running" {
            anyhow::bail!(
                "request supervisor instance {} lost its lease while run {run_id} remained running",
                options.instance_id
            );
        }
        Ok(())
    })?;

    Ok(SupervisorStepAttempt::Stepped {
        step: Box::new(step),
        fence,
    })
}

fn manual_attention_boundary(step: &RequestStepReport) -> Option<RequestSupervisorAttentionReason> {
    boundary_from_fields("step", &step.status, &step.action, &step.reason)
        .or_else(|| {
            step.drive_after
                .as_ref()
                .and_then(|drive| boundary_from_drive("drive_after", drive))
        })
        .or_else(|| boundary_from_drive("drive_before", &step.drive_before))
}

fn boundary_from_drive(
    source: &'static str,
    drive: &RequestDriveReport,
) -> Option<RequestSupervisorAttentionReason> {
    boundary_from_fields(source, &drive.status, &drive.action, &drive.reason)
}

fn boundary_from_fields(
    source: &'static str,
    status: &str,
    action: &str,
    reason: &str,
) -> Option<RequestSupervisorAttentionReason> {
    MANUAL_ATTENTION_STATUSES
        .contains(&status)
        .then(|| RequestSupervisorAttentionReason::from_step_report(source, status, action, reason))
}

fn park_after_step(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    options: &RequestSupervisorOptions,
    supervisor_fence: Option<&RequestSupervisorFence>,
    progress: (usize, usize),
    reason_code: &str,
    attention_reason: RequestSupervisorAttentionReason,
) -> Result<RequestSupervisorRunResult> {
    let (steps_attempted, steps_advanced) = progress;
    let run = load_run_record(store, &listed_run.run_id)?;
    let workflow = store.load_workflow(&run.workflow_id)?;
    let reason = serde_json::to_value(&attention_reason)?;
    let parked = mark_run_needs_attention(
        store,
        &run,
        &workflow,
        supervisor_fence,
        &options.origin,
        reason_code,
        &reason,
    )?;
    Ok(RequestSupervisorRunResult {
        run_id: parked.run_id,
        workflow_id: parked.workflow_id,
        initial_status: listed_run.status.clone(),
        final_status: parked.status,
        initial_executor: listed_run.activity.executor.clone(),
        final_executor: parked.active_executor,
        initial_heartbeat_status: listed_run.activity.heartbeat_status.clone(),
        outcome: RequestSupervisorRunOutcome::NeedsAttention,
        steps_attempted,
        steps_advanced,
        reason: attention_reason.reason.clone(),
        attention_reason: Some(attention_reason),
        error: None,
    })
}

fn park_unsafe_running_state(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    options: &RequestSupervisorOptions,
    status: &str,
    action: &str,
    reason: &str,
) -> Result<RequestSupervisorRunResult> {
    let attention_reason =
        RequestSupervisorAttentionReason::from_step_report("scan", status, action, reason);
    park_after_step(
        store,
        listed_run,
        options,
        None,
        (0, 0),
        "supervisor_unsafe_running_state",
        attention_reason,
    )
}

fn observed_step_result(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    outcome: RequestSupervisorRunOutcome,
    steps_attempted: usize,
    steps_advanced: usize,
    reason: String,
) -> Result<RequestSupervisorRunResult> {
    let run = load_run_record(store, &listed_run.run_id)?;
    let outcome = match run.status.as_str() {
        "needs_attention" => RequestSupervisorRunOutcome::NeedsAttention,
        "complete" | "completed" => RequestSupervisorRunOutcome::Completed,
        _ => outcome,
    };
    Ok(RequestSupervisorRunResult {
        run_id: run.run_id,
        workflow_id: run.workflow_id,
        initial_status: listed_run.status.clone(),
        final_status: run.status,
        initial_executor: listed_run.activity.executor.clone(),
        final_executor: run.active_executor,
        initial_heartbeat_status: listed_run.activity.heartbeat_status.clone(),
        outcome,
        steps_attempted,
        steps_advanced,
        attention_reason: None,
        reason,
        error: None,
    })
}

fn skipped_result(
    listed_run: &RequestListRow,
    outcome: RequestSupervisorRunOutcome,
    reason: &str,
) -> RequestSupervisorRunResult {
    RequestSupervisorRunResult {
        run_id: listed_run.run_id.clone(),
        workflow_id: listed_run.workflow_id.clone(),
        initial_status: listed_run.status.clone(),
        final_status: listed_run.status.clone(),
        initial_executor: listed_run.activity.executor.clone(),
        final_executor: listed_run.activity.executor.clone(),
        initial_heartbeat_status: listed_run.activity.heartbeat_status.clone(),
        outcome,
        steps_attempted: 0,
        steps_advanced: 0,
        attention_reason: None,
        reason: reason.to_string(),
        error: None,
    }
}

fn failed_result(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    error: anyhow::Error,
) -> RequestSupervisorRunResult {
    failed_result_with_progress(store, listed_run, error, 0, 0)
}

fn failed_result_with_progress(
    store: &ForgeStore,
    listed_run: &RequestListRow,
    error: anyhow::Error,
    steps_attempted: usize,
    steps_advanced: usize,
) -> RequestSupervisorRunResult {
    let current = load_run_record(store, &listed_run.run_id).ok();
    let final_executor = current
        .as_ref()
        .map(|run| run.active_executor.clone())
        .unwrap_or_else(|| listed_run.activity.executor.clone());
    RequestSupervisorRunResult {
        run_id: listed_run.run_id.clone(),
        workflow_id: listed_run.workflow_id.clone(),
        initial_status: listed_run.status.clone(),
        final_status: current
            .as_ref()
            .map(|run| run.status.clone())
            .unwrap_or_else(|| listed_run.status.clone()),
        initial_executor: listed_run.activity.executor.clone(),
        final_executor,
        initial_heartbeat_status: listed_run.activity.heartbeat_status.clone(),
        outcome: RequestSupervisorRunOutcome::Failed,
        steps_attempted,
        steps_advanced,
        attention_reason: None,
        reason: "Request supervision failed for this run; no implicit resume or executor takeover was attempted after the error.".to_string(),
        error: Some(format!("{error:#}")),
    }
}

fn update_counts(counts: &mut RequestSupervisorCounts, result: &RequestSupervisorRunResult) {
    if result.steps_attempted > 0
        || matches!(
            result.outcome,
            RequestSupervisorRunOutcome::RecoveredStale
                | RequestSupervisorRunOutcome::NeedsAttention
                | RequestSupervisorRunOutcome::Failed
        )
    {
        counts.eligible += 1;
    }
    if result.steps_advanced > 0 {
        counts.advanced += 1;
        counts.advanced_steps += result.steps_advanced;
    }
    match result.outcome {
        RequestSupervisorRunOutcome::RecoveredStale => {
            counts.recovered += 1;
            counts.needs_attention += 1;
        }
        RequestSupervisorRunOutcome::NeedsAttention => counts.needs_attention += 1,
        RequestSupervisorRunOutcome::SkippedExternalActive => {
            counts.skipped_external_active += 1;
        }
        RequestSupervisorRunOutcome::SkippedLeaseContended => {
            counts.skipped_lease_contended += 1;
        }
        RequestSupervisorRunOutcome::SkippedInactive => counts.skipped_inactive += 1,
        RequestSupervisorRunOutcome::Failed => counts.failures += 1,
        RequestSupervisorRunOutcome::Advanced
        | RequestSupervisorRunOutcome::Completed
        | RequestSupervisorRunOutcome::Observed => {}
    }
}

#[cfg(test)]
mod lease_epoch_tests {
    use super::*;
    use crate::graph::{self, ExecutorKind};
    use crate::intent::parse_intent;
    use crate::request::{create_run_record, save_run_record};

    #[test]
    fn same_instance_reacquires_an_expired_lease_with_a_new_fencing_token() {
        let temporary = tempfile::tempdir().unwrap();
        let store = ForgeStore::open(temporary.path().join("forge.sqlite")).unwrap();
        let mut workflow =
            graph::create_workflow(parse_intent("Reacquire an expired supervisor lease"));
        workflow.tasks = vec![graph::task(
            "task-001",
            "Require a bounded executor handoff",
            &[],
            &[],
            vec![],
            "Bounded handoff",
            (ExecutorKind::Ai, 0.0),
        )];
        store.save_workflow(&workflow).unwrap();
        let mut run = create_run_record(&workflow, "test", "accepted");
        run.supervisor_instance_id = Some("same-supervisor".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() - Duration::seconds(1));
        run.supervisor_fencing_token = 7;
        save_run_record(&store, &run).unwrap();
        let options = RequestSupervisorOptions {
            executor: "forge-request-supervisor".to_string(),
            origin: "test".to_string(),
            instance_id: "same-supervisor".to_string(),
            ttl_seconds: 120,
            max_steps_per_run: 1,
        };

        let attempt = claim_and_step_request(&store, &run.run_id, &options).unwrap();
        let new_fence = match attempt {
            SupervisorStepAttempt::Stepped { fence, .. } => fence,
            _ => panic!("same supervisor instance should reacquire the expired lease"),
        };
        assert_eq!(new_fence.instance_id, "same-supervisor");
        assert_eq!(new_fence.fencing_token, 8);
        let current = load_run_record(&store, &run.run_id).unwrap();
        assert_eq!(current.supervisor_fencing_token, 8);
        assert_eq!(
            current.supervisor_instance_id.as_deref(),
            Some("same-supervisor")
        );
        let acquired_events = store
            .load_workflow_events(&workflow.id)
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == "async_request_supervisor_lease_acquired")
            .collect::<Vec<_>>();
        assert_eq!(acquired_events.len(), 1);
        assert_eq!(acquired_events[0].data["fencing_token"], 8);

        let run_before_old_fence = serde_json::to_value(&current).unwrap();
        let workflow_before_old_fence =
            serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap();
        let event_count_before_old_fence = store.load_workflow_events(&workflow.id).unwrap().len();
        let old_fence = RequestSupervisorFence {
            instance_id: "same-supervisor".to_string(),
            fencing_token: 7,
        };
        let error = step_request_with_supervisor_fence(
            &store,
            &run.run_id,
            &options.executor,
            options.ttl_seconds,
            &options.origin,
            &old_fence,
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not match live lease"));
        assert_eq!(
            serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap(),
            run_before_old_fence
        );
        assert_eq!(
            serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap(),
            workflow_before_old_fence
        );
        assert_eq!(
            store.load_workflow_events(&workflow.id).unwrap().len(),
            event_count_before_old_fence
        );
    }
}
