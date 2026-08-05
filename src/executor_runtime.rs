use crate::artifact::hex_sha256;
use crate::checkpoint::load_latest_task_checkpoint;
use crate::context::{
    build_context_package_with_checkpoint_and_project,
    build_context_package_with_checkpoint_project_and_worktree, DEFAULT_CONTEXT_BUDGET,
};
use crate::executor::{canonical_executor_id, load_executors, ExecutorState};
use crate::graph::{TaskStatus, Workflow};
use crate::lease::{validate_task_lease_for_execution, TaskLease};
use crate::request::{
    drive_request_with_context_budget, load_run_record, DispatchFrontier, RequestDriveReport,
};
use crate::security::{sanitize_prompt_secrets, SecretSanitizationOptions};
use crate::storage::{ExecutorRuntimeClaimWrite, FoundryStore};
use crate::worktree::bound_worktree_context;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const EXECUTOR_RUNTIME_RECEIPT_SCHEMA_VERSION: &str = "foundry.executor_runtime.receipt.v1";
pub const EXECUTOR_RUNTIME_GIT_OBSERVATION_SCHEMA_VERSION: &str =
    "foundry.executor_runtime.git_observation.v1";
pub const EXECUTOR_RUNTIME_CLAIMED_SCHEMA_VERSION: &str = "foundry.executor_runtime.claimed.v1";
pub const EXECUTOR_RUNTIME_STARTED_SCHEMA_VERSION: &str = "foundry.executor_runtime.started.v1";
pub const EXECUTOR_WAVE_REPORT_SCHEMA_VERSION: &str = "foundry.executor_wave.report.v1";
pub const REQUEST_EXECUTOR_WAVE_SCHEMA_VERSION: &str = "foundry.request_executor_wave.v1";
pub const EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS: u64 = 300;
pub const MAX_EXECUTOR_WAVE_WORKERS: usize = 64;
pub const DEFAULT_REQUEST_EXECUTOR_WAVE_EXECUTOR: &str = "auto";
pub const DEFAULT_REQUEST_EXECUTOR_WAVE_TTL_SECONDS: u64 = 300;
pub const DEFAULT_REQUEST_EXECUTOR_WAVE_TIMEOUT_SECONDS: u64 = 1_800;
pub const DEFAULT_REQUEST_EXECUTOR_WAVE_CONTEXT_BUDGET: usize = DEFAULT_CONTEXT_BUDGET;
pub const DEFAULT_REQUEST_EXECUTOR_WAVE_MAX_PARALLEL: usize = 8;

const MAX_EXECUTOR_RUNTIME_SECONDS: u64 = 3_600;
const MAX_EXECUTOR_PROMPT_BYTES: usize = 1_048_576;
const MAX_EXECUTOR_OUTPUT_EXCERPT_BYTES: usize = 8_192;
const MAX_EXECUTOR_USAGE_PARSE_BYTES: usize = 4 * 1_048_576;
const IMPLEMENTATION_WAVE_PARALLEL_GROUP: &str = "implementation-wave-001";
const IDEMPOTENT_REPLAY_WAIT_GRACE_SECONDS: u64 = 5;
const EXECUTOR_RUNTIME_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorRuntimeAuthorization {
    pub allow_non_interactive_execution: bool,
    pub approved_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutorRuntimeDispatchCorrelation {
    pub wave_id: String,
    pub workflow_revision: u64,
    pub task_version: u64,
    pub context_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorRuntimeRequest {
    pub workflow_id: String,
    pub run_id: String,
    pub task_id: String,
    pub lease_id: String,
    pub executor: String,
    pub cwd: PathBuf,
    pub prompt: String,
    pub timeout_seconds: u64,
    pub authorization: ExecutorRuntimeAuthorization,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<ExecutorRuntimeDispatchCorrelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorRuntimeStreamEvidence {
    pub sha256: String,
    pub total_bytes: usize,
    pub excerpt_bytes: usize,
    pub excerpt_truncated: bool,
    pub excerpt_redaction_count: usize,
    pub excerpt: String,
}

/// Provider-reported token counters normalized without changing their accounting semantics.
///
/// If a provider omits `total_tokens`, Foundry derives it only when both
/// `input_tokens` and `output_tokens` are present, using
/// `input_tokens + output_tokens`. Cache and reasoning/thinking counters are
/// details of those totals and are not added again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorTokenUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    pub source_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutorRuntimeGitObservation {
    pub schema_version: String,
    pub repository_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub base_head: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_is_ancestor: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_count: Option<u64>,
    #[serde(default)]
    pub changed_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clean: Option<bool>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorRuntimeReceipt {
    pub schema_version: String,
    pub execution_id: String,
    pub status: String,
    pub success: bool,
    pub workflow_id: String,
    pub run_id: String,
    pub task_id: String,
    pub lease_id: String,
    pub executor: String,
    pub command_path: String,
    pub command_argument_shape: Vec<String>,
    pub cwd: String,
    #[serde(default = "stdin_prompt_transport")]
    pub prompt_transport: String,
    pub prompt_sha256: String,
    pub prompt_bytes: usize,
    pub request_sha256: String,
    #[serde(default)]
    pub idempotent_replay: bool,
    pub authorization_opt_in: bool,
    pub approved_by: String,
    pub authorization_reason: String,
    pub timeout_seconds: u64,
    pub lease_expires_at: DateTime<Utc>,
    pub lease_grace_seconds: u64,
    pub lease_extended_for_runtime: bool,
    pub lease_preserved_for_validation: bool,
    pub worktree_id: String,
    pub workspace_binding_scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<ExecutorRuntimeDispatchCorrelation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<ExecutorRuntimeGitObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<ExecutorTokenUsage>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: u128,
    pub process_id: Option<u32>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_error: Option<String>,
    pub stdout: ExecutorRuntimeStreamEvidence,
    pub stderr: ExecutorRuntimeStreamEvidence,
    pub task_completion_attempted: bool,
    pub output_accepted_as_validation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorWaveError {
    pub request_index: usize,
    pub workflow_id: String,
    pub task_id: String,
    pub lease_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorWaveReport {
    pub schema_version: String,
    pub wave_id: String,
    pub status: String,
    pub success: bool,
    pub request_count: usize,
    pub unique_request_count: usize,
    pub deduplicated_request_count: usize,
    pub max_parallel: usize,
    pub worker_count: usize,
    pub initialized_worker_count: usize,
    pub worker_errors: Vec<String>,
    pub receipt_order: String,
    pub lease_grace_seconds: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: u128,
    pub receipts: Vec<ExecutorRuntimeReceipt>,
    pub errors: Vec<ExecutorWaveError>,
}

#[derive(Debug, Clone)]
pub struct RequestExecutorWaveOptions<'a> {
    pub run_id: &'a str,
    pub requested_executor: &'a str,
    pub ttl_seconds: u64,
    pub timeout_seconds: u64,
    pub context_budget: usize,
    pub max_parallel: Option<usize>,
    pub allow_exec: bool,
    pub approved_by: &'a str,
    pub reason: &'a str,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestExecutorWaveReport {
    pub schema_version: String,
    pub status: String,
    pub success: bool,
    pub run_id: String,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wave_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drive: Option<RequestDriveReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_frontier: Option<DispatchFrontier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_wave: Option<ExecutorWaveReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_commands: Vec<Vec<String>>,
    pub task_completion_attempted: bool,
    pub output_accepted_as_validation: bool,
    pub reason: String,
}

struct CapturedExecutorRuntimeStream {
    evidence: ExecutorRuntimeStreamEvidence,
    token_usage: Option<ExecutorTokenUsage>,
}

struct PreparedExecutorRuntime {
    executor: String,
    command_path: PathBuf,
    resolved_command_path: PathBuf,
    cwd: PathBuf,
    codex_git_common_dir: Option<PathBuf>,
    lease: TaskLease,
    lease_extended_for_runtime: bool,
}

struct ExecutorRuntimeGitBaseline {
    repository_root: String,
    branch: String,
    base_head: String,
}

struct WaveWorkerOutcome {
    request_index: usize,
    result: std::result::Result<ExecutorRuntimeReceipt, String>,
}

enum WaveWorkerMessage {
    Initialized,
    Outcome(Box<WaveWorkerOutcome>),
    InitializationError(String),
}

fn append_implementation_wave_prompt(prompt: &mut String, implementation_wave: bool) {
    if !implementation_wave {
        return;
    }
    prompt.push_str(
        "\n\nImplementation-wave requirements: limit all changes to the bounded scope/slice \
         assigned above; do not modify another lane's scope. Run the applicable validations and \
         report their observed exit codes. Create at least one semantic Git commit containing \
         this implementation, then leave the task worktree clean (`git status --porcelain` must \
         be empty) before reporting.",
    );
}

pub fn build_codex_runtime_command(
    command_path: &Path,
    cwd: &Path,
    git_common_dir: Option<&Path>,
    _prompt: &str,
) -> Command {
    let mut command = Command::new(command_path);
    command
        .arg("exec")
        .arg("--json")
        .arg("--ephemeral")
        .arg("--ignore-user-config")
        .arg("-c")
        .arg("allow_login_shell=false")
        .arg("--sandbox")
        .arg("workspace-write");
    if let Some(git_common_dir) = git_common_dir {
        command.arg("--add-dir").arg(git_common_dir);
    }
    command.arg("-C").arg(cwd).arg("-").current_dir(cwd);
    command
}

pub fn build_agy_runtime_command(
    command_path: &Path,
    cwd: &Path,
    prompt: &str,
    timeout_seconds: u64,
) -> Command {
    let mut command = Command::new(command_path);
    command
        .arg("--print")
        .arg(prompt)
        .arg("--print-timeout")
        .arg(format!("{timeout_seconds}s"))
        .arg("--mode")
        .arg("accept-edits")
        .arg("--sandbox")
        .arg("--output-format")
        .arg("json")
        .current_dir(cwd);
    command
}

pub fn execute_executor_wave(
    store: &FoundryStore,
    requests: Vec<ExecutorRuntimeRequest>,
    max_parallel: usize,
) -> Result<ExecutorWaveReport> {
    if !(1..=MAX_EXECUTOR_WAVE_WORKERS).contains(&max_parallel) {
        bail!("executor wave max_parallel must be between 1 and {MAX_EXECUTOR_WAVE_WORKERS}");
    }
    if requests.is_empty() {
        bail!("executor wave requires at least one runtime request");
    }
    let wave_id = requests[0]
        .dispatch
        .as_ref()
        .context("executor wave requests require dispatch correlation")?
        .wave_id
        .trim()
        .to_string();
    if wave_id.is_empty() {
        bail!("executor wave dispatch wave_id cannot be empty");
    }
    for (request_index, request) in requests.iter().enumerate() {
        let dispatch = request.dispatch.as_ref().with_context(|| {
            format!("executor wave request {request_index} requires dispatch correlation")
        })?;
        if dispatch.wave_id != wave_id {
            bail!(
                "executor wave request {request_index} belongs to wave {}, expected {}",
                dispatch.wave_id,
                wave_id
            );
        }
    }

    let started_at = Utc::now();
    let started = Instant::now();
    let request_count = requests.len();
    let request_metadata = requests
        .iter()
        .map(|request| {
            (
                request.workflow_id.clone(),
                request.task_id.clone(),
                request.lease_id.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut first_by_lease = BTreeMap::<(String, String, String), (usize, String)>::new();
    let mut duplicate_of = vec![None; request_count];
    let mut unique_jobs = VecDeque::new();
    for (request_index, request) in requests.into_iter().enumerate() {
        let key = (
            request.workflow_id.clone(),
            request.task_id.clone(),
            request.lease_id.clone(),
        );
        let fingerprint = executor_wave_request_fingerprint(&request)?;
        if let Some((first_index, first_fingerprint)) = first_by_lease.get(&key) {
            if first_fingerprint != &fingerprint {
                bail!(
                    "executor wave contains conflicting requests for workflow {} task {} lease {} at indexes {} and {}",
                    key.0,
                    key.1,
                    key.2,
                    first_index,
                    request_index
                );
            }
            duplicate_of[request_index] = Some(*first_index);
        } else {
            first_by_lease.insert(key, (request_index, fingerprint));
            unique_jobs.push_back((request_index, request));
        }
    }
    let unique_request_count = unique_jobs.len();
    let deduplicated_request_count = request_count - unique_request_count;
    let worker_count = unique_request_count.min(max_parallel);
    let queue = Arc::new(Mutex::new(unique_jobs));
    let (sender, receiver) = mpsc::channel::<WaveWorkerMessage>();
    let mut handles = Vec::with_capacity(worker_count);

    for worker_index in 0..worker_count {
        let queue = Arc::clone(&queue);
        let sender = sender.clone();
        let store_path = store.path().to_path_buf();
        handles.push(thread::spawn(move || {
            let worker_store = match FoundryStore::open(&store_path) {
                Ok(store) => store,
                Err(error) => {
                    let _ = sender.send(WaveWorkerMessage::InitializationError(
                        sanitize_runtime_text(&format!(
                            "executor wave worker {worker_index} failed to open store: {error:#}"
                        )),
                    ));
                    return;
                }
            };
            if sender.send(WaveWorkerMessage::Initialized).is_err() {
                return;
            }
            loop {
                let job = {
                    let mut queue = queue
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    queue.pop_front()
                };
                let Some((request_index, request)) = job else {
                    break;
                };
                let result = execute_executor_runtime(&worker_store, request)
                    .map_err(|error| sanitize_runtime_text(&format!("{error:#}")));
                if sender
                    .send(WaveWorkerMessage::Outcome(Box::new(WaveWorkerOutcome {
                        request_index,
                        result,
                    })))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    drop(sender);

    let mut outcomes = std::iter::repeat_with(|| None)
        .take(request_count)
        .collect::<Vec<Option<std::result::Result<ExecutorRuntimeReceipt, String>>>>();
    let mut initialization_errors = Vec::new();
    let mut initialized_worker_count = 0usize;
    for message in receiver {
        match message {
            WaveWorkerMessage::Initialized => initialized_worker_count += 1,
            WaveWorkerMessage::Outcome(outcome) => {
                outcomes[outcome.request_index] = Some(outcome.result);
            }
            WaveWorkerMessage::InitializationError(error) => initialization_errors.push(error),
        }
    }
    let panicked_workers = handles
        .into_iter()
        .map(|handle| handle.join().is_err())
        .filter(|panicked| *panicked)
        .count();

    for (request_index, first_index) in duplicate_of.iter().enumerate() {
        let Some(first_index) = first_index else {
            continue;
        };
        outcomes[request_index] = outcomes[*first_index].clone().map(|outcome| {
            outcome.map(|mut receipt| {
                receipt.idempotent_replay = true;
                receipt
            })
        });
    }

    let mut worker_errors = initialization_errors;
    if panicked_workers > 0 {
        worker_errors.push(format!(
            "{panicked_workers} executor wave worker(s) panicked"
        ));
    }
    let missing_outcome_error = if worker_errors.is_empty() {
        "executor wave worker ended without returning an outcome".to_string()
    } else {
        worker_errors.join("; ")
    };
    let mut receipts = Vec::with_capacity(request_count);
    let mut errors = Vec::new();
    for (request_index, outcome) in outcomes.into_iter().enumerate() {
        match outcome {
            Some(Ok(receipt)) => receipts.push(receipt),
            Some(Err(error)) => {
                let (workflow_id, task_id, lease_id) = &request_metadata[request_index];
                errors.push(ExecutorWaveError {
                    request_index,
                    workflow_id: workflow_id.clone(),
                    task_id: task_id.clone(),
                    lease_id: lease_id.clone(),
                    error,
                });
            }
            None => {
                let (workflow_id, task_id, lease_id) = &request_metadata[request_index];
                errors.push(ExecutorWaveError {
                    request_index,
                    workflow_id: workflow_id.clone(),
                    task_id: task_id.clone(),
                    lease_id: lease_id.clone(),
                    error: missing_outcome_error.clone(),
                });
            }
        }
    }

    let succeeded = receipts.iter().filter(|receipt| receipt.success).count();
    let success = errors.is_empty() && worker_errors.is_empty() && succeeded == request_count;
    let status = if success {
        "executor_wave_succeeded"
    } else if succeeded == 0 {
        "executor_wave_failed"
    } else {
        "executor_wave_partial_failure"
    };
    Ok(ExecutorWaveReport {
        schema_version: EXECUTOR_WAVE_REPORT_SCHEMA_VERSION.to_string(),
        wave_id,
        status: status.to_string(),
        success,
        request_count,
        unique_request_count,
        deduplicated_request_count,
        max_parallel,
        worker_count,
        initialized_worker_count,
        worker_errors,
        receipt_order: "request_order".to_string(),
        lease_grace_seconds: EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS,
        started_at,
        finished_at: Utc::now(),
        duration_ms: started.elapsed().as_millis(),
        receipts,
        errors,
    })
}

pub fn execute_request_executor_wave(
    store: &FoundryStore,
    options: &RequestExecutorWaveOptions<'_>,
) -> Result<RequestExecutorWaveReport> {
    if !options.allow_exec {
        bail!("request execute-wave requires explicit --allow-exec authorization");
    }
    let approved_by = options.approved_by.trim();
    if approved_by.is_empty() {
        bail!("request execute-wave requires a non-empty --approved-by value");
    }
    let authorization_reason = options.reason.trim();
    if authorization_reason.is_empty() {
        bail!("request execute-wave requires a non-empty authorization reason");
    }

    let drive = drive_request_with_context_budget(
        store,
        options.run_id,
        options.requested_executor,
        options.ttl_seconds,
        options.origin,
        Some(options.context_budget),
    )?;
    let Some(frontier) = drive.dispatch_frontier.clone() else {
        return Ok(RequestExecutorWaveReport {
            schema_version: REQUEST_EXECUTOR_WAVE_SCHEMA_VERSION.to_string(),
            status: "execution_not_started".to_string(),
            success: false,
            run_id: drive.run_id.clone(),
            workflow_id: drive.workflow_id.clone(),
            wave_id: None,
            drive: Some(drive),
            dispatch_frontier: None,
            executor_wave: None,
            validation_commands: Vec::new(),
            task_completion_attempted: false,
            output_accepted_as_validation: false,
            reason: "request drive did not produce an admitted dispatch frontier".to_string(),
        });
    };
    if frontier.wave.assignments.is_empty() {
        return Ok(RequestExecutorWaveReport {
            schema_version: REQUEST_EXECUTOR_WAVE_SCHEMA_VERSION.to_string(),
            status: "execution_not_started".to_string(),
            success: false,
            run_id: drive.run_id.clone(),
            workflow_id: drive.workflow_id.clone(),
            wave_id: Some(frontier.wave.wave_id.clone()),
            drive: Some(drive),
            dispatch_frontier: Some(frontier),
            executor_wave: None,
            validation_commands: Vec::new(),
            task_completion_attempted: false,
            output_accepted_as_validation: false,
            reason: "dispatch frontier admitted no executable task; inspect deferred tasks and worktree/quota/resource gates".to_string(),
        });
    }

    let admitted_count = frontier.wave.assignments.len();
    let max_parallel = options
        .max_parallel
        .unwrap_or_else(|| admitted_count.min(DEFAULT_REQUEST_EXECUTOR_WAVE_MAX_PARALLEL));
    if !(1..=MAX_EXECUTOR_WAVE_WORKERS).contains(&max_parallel) {
        bail!(
            "request execute-wave max_parallel must be between 1 and {MAX_EXECUTOR_WAVE_WORKERS}"
        );
    }

    let workflow = store.load_workflow(&drive.workflow_id)?;
    let authorization = ExecutorRuntimeAuthorization {
        allow_non_interactive_execution: true,
        approved_by: approved_by.to_string(),
        reason: authorization_reason.to_string(),
    };
    let mut requests = Vec::with_capacity(admitted_count);
    for assignment in &frontier.wave.assignments {
        let claim = assignment.workspace_claim.as_ref().with_context(|| {
            format!(
                "dispatch assignment {} has no task-scoped worktree claim",
                assignment.task_id
            )
        })?;
        if claim.binding_scope != "task" {
            bail!(
                "dispatch assignment {} resolved worktree {} with binding_scope={}; task scope is required",
                assignment.task_id,
                claim.worktree_id,
                claim.binding_scope
            );
        }
        let cwd = PathBuf::from(&claim.worktree_root);
        let latest_checkpoint =
            load_latest_task_checkpoint(store, &workflow.id, &assignment.task_id)?;
        let bound_worktree =
            bound_worktree_context(store, &workflow.id, Some(&assignment.task_id))?;
        let package = if bound_worktree.is_some() {
            build_context_package_with_checkpoint_project_and_worktree(
                &workflow,
                &assignment.task_id,
                options.context_budget,
                latest_checkpoint,
                Some(&cwd),
                bound_worktree,
            )?
        } else {
            build_context_package_with_checkpoint_and_project(
                &workflow,
                &assignment.task_id,
                options.context_budget,
                latest_checkpoint,
                Some(&cwd),
            )?
        };
        if !package.handoff_ready {
            bail!(
                "task {} context changed after admission and is no longer handoff-ready: {}",
                assignment.task_id,
                package.handoff_status
            );
        }
        if package.context_sha256 != assignment.context_sha256 {
            bail!(
                "task {} context drifted after dispatch: assignment={} current={}",
                assignment.task_id,
                assignment.context_sha256,
                package.context_sha256
            );
        }
        let implementation_wave = workflow
            .tasks
            .iter()
            .find(|task| task.id == assignment.task_id)
            .with_context(|| {
                format!(
                    "dispatch assignment references missing workflow task {}",
                    assignment.task_id
                )
            })?
            .node_brain_routing
            .agent_slots
            .iter()
            .any(|slot| slot.parallel_group == IMPLEMENTATION_WAVE_PARALLEL_GROUP);
        let prompt_packet = serde_json::to_string(&package.prompt_packet)?;
        let mut prompt = format!(
            "Foundry owns workflow state and final promotion. Execute only the bounded task below inside the task-bound worktree. Do not mark the Foundry task complete and do not treat process exit as validation. At the end, report changed files, validations actually run, observed exit codes and blockers.\n\nFoundry prompt packet:\n{prompt_packet}\n\nBounded task context (sha256={}):\n{}",
            package.context_sha256, package.content
        );
        append_implementation_wave_prompt(&mut prompt, implementation_wave);
        requests.push(ExecutorRuntimeRequest {
            workflow_id: workflow.id.clone(),
            run_id: drive.run_id.clone(),
            task_id: assignment.task_id.clone(),
            lease_id: assignment.lease_id.clone(),
            executor: assignment.selected_executor.clone(),
            cwd,
            prompt,
            timeout_seconds: options.timeout_seconds,
            authorization: authorization.clone(),
            dispatch: Some(ExecutorRuntimeDispatchCorrelation {
                wave_id: frontier.wave.wave_id.clone(),
                workflow_revision: frontier.wave.workflow_revision,
                task_version: assignment.task_version,
                context_sha256: assignment.context_sha256.clone(),
            }),
        });
    }

    store.record_event(
        &workflow.id,
        "request_executor_wave_started",
        &serde_json::json!({
            "schema_version": "foundry.request_executor_wave_event.v1",
            "run_id": drive.run_id,
            "wave_id": frontier.wave.wave_id,
            "workflow_revision": frontier.wave.workflow_revision,
            "task_ids": frontier.wave.assignments.iter().map(|assignment| assignment.task_id.as_str()).collect::<Vec<_>>(),
            "max_parallel": max_parallel,
            "approved_by": approved_by,
            "authorization_reason": authorization_reason,
            "origin": options.origin,
            "execution_started": true,
        }),
    )?;
    let wave = execute_executor_wave(store, requests, max_parallel)?;
    store.record_event(
        &workflow.id,
        "request_executor_wave_finished",
        &serde_json::json!({
            "schema_version": "foundry.request_executor_wave_event.v1",
            "run_id": drive.run_id,
            "wave_id": frontier.wave.wave_id,
            "origin": options.origin,
            "execution_started": true,
            "task_completion_attempted": false,
            "output_accepted_as_validation": false,
            "wave": &wave,
        }),
    )?;

    let validation_commands = wave
        .receipts
        .iter()
        .map(|receipt| {
            vec![
                "foundry".to_string(),
                "--store".to_string(),
                store.path().display().to_string(),
                "request".to_string(),
                "complete-task".to_string(),
                "--run".to_string(),
                drive.run_id.clone(),
                "--task".to_string(),
                receipt.task_id.clone(),
                "--executor".to_string(),
                receipt.executor.clone(),
                "--summary".to_string(),
                "<reviewed validated summary>".to_string(),
                "--evidence-command".to_string(),
                "<validation command actually run>".to_string(),
                "--evidence-exit-code".to_string(),
                "<observed exit code>".to_string(),
                "--evidence-summary".to_string(),
                format!("executor runtime receipt {} reviewed", receipt.execution_id),
                "--output".to_string(),
                "json".to_string(),
            ]
        })
        .collect::<Vec<_>>();

    Ok(RequestExecutorWaveReport {
        schema_version: REQUEST_EXECUTOR_WAVE_SCHEMA_VERSION.to_string(),
        status: wave.status.clone(),
        success: wave.success,
        run_id: drive.run_id,
        workflow_id: drive.workflow_id,
        wave_id: Some(frontier.wave.wave_id.clone()),
        drive: None,
        dispatch_frontier: Some(frontier),
        executor_wave: Some(wave),
        validation_commands,
        task_completion_attempted: false,
        output_accepted_as_validation: false,
        reason: "Foundry executed the admitted Codex/Agy processes in parallel and preserved every task lease for explicit validation; no task was promoted automatically.".to_string(),
    })
}

fn executor_wave_request_fingerprint(request: &ExecutorRuntimeRequest) -> Result<String> {
    let value = serde_json::json!({
        "workflow_id": request.workflow_id,
        "run_id": request.run_id,
        "task_id": request.task_id,
        "lease_id": request.lease_id,
        "executor": canonical_executor_id(&request.executor),
        "cwd": request.cwd,
        "prompt_sha256": hex_sha256(request.prompt.as_bytes()),
        "prompt_bytes": request.prompt.len(),
        "timeout_seconds": request.timeout_seconds,
        "authorization": request.authorization,
        "dispatch": request.dispatch,
    });
    Ok(hex_sha256(&serde_json::to_vec(&value)?))
}

fn executor_runtime_request_sha256(
    request: &ExecutorRuntimeRequest,
    prepared: &PreparedExecutorRuntime,
    prompt_sha256: &str,
) -> Result<String> {
    let value = serde_json::json!({
        "workflow_id": request.workflow_id,
        "run_id": request.run_id,
        "task_id": request.task_id,
        "lease_id": request.lease_id,
        "executor": prepared.executor,
        "command_path": prepared.command_path,
        "cwd": prepared.cwd,
        "codex_git_common_dir": prepared.codex_git_common_dir,
        "prompt_transport": executor_prompt_transport(&prepared.executor),
        "prompt_sha256": prompt_sha256,
        "prompt_bytes": request.prompt.len(),
        "timeout_seconds": request.timeout_seconds,
        "authorization": request.authorization,
        "dispatch": request.dispatch,
        "workspace_claim": prepared.lease.workspace_claim,
    });
    Ok(hex_sha256(&serde_json::to_vec(&value)?))
}

pub fn execute_executor_runtime(
    store: &FoundryStore,
    request: ExecutorRuntimeRequest,
) -> Result<ExecutorRuntimeReceipt> {
    let mut prepared = prepare_executor_runtime(store, &request)?;
    let execution_id = format!(
        "executor_runtime_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let owner_token = format!(
        "executor_runtime_owner_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let prompt_sha256 = hex_sha256(request.prompt.as_bytes());
    let request_sha256 = executor_runtime_request_sha256(&request, &prepared, &prompt_sha256)?;
    let command_argument_shape = command_argument_shape(&prepared.executor);
    let claimed_at = Utc::now();
    let started = Instant::now();
    let claimed_at_rfc3339 = claimed_at.to_rfc3339();
    let claimed = store.with_transaction(|| {
        let claimed = store.try_claim_executor_runtime(ExecutorRuntimeClaimWrite {
            workflow_id: &request.workflow_id,
            task_id: &request.task_id,
            lease_id: &request.lease_id,
            execution_id: &execution_id,
            owner_token: &owner_token,
            executor: &prepared.executor,
            request_sha256: &request_sha256,
            claimed_at: &claimed_at_rfc3339,
        })?;
        if claimed {
            store.record_event(
                &request.workflow_id,
                "executor_runtime_claimed",
                &serde_json::json!({
                    "schema_version": EXECUTOR_RUNTIME_CLAIMED_SCHEMA_VERSION,
                    "origin": "foundry_executor_runtime",
                    "execution_id": execution_id,
                    "workflow_id": request.workflow_id,
                    "run_id": request.run_id,
                    "task_id": request.task_id,
                    "lease_id": request.lease_id,
                    "executor": prepared.executor,
                    "request_sha256": request_sha256,
                    "cwd": prepared.cwd.display().to_string(),
                    "worktree_id": prepared.lease.workspace_claim.as_ref().map(|claim| &claim.worktree_id),
                    "workspace_binding_scope": prepared.lease.workspace_claim.as_ref().map(|claim| &claim.binding_scope),
                    "lease_expires_at": prepared.lease.expires_at,
                    "lease_grace_seconds": EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS,
                    "dispatch": request.dispatch,
                    "claimed_at": claimed_at,
                    "task_completion_attempted": false,
                    "output_accepted_as_validation": false,
                }),
            )?;
        }
        Ok(claimed)
    })?;
    if !claimed {
        return wait_for_idempotent_executor_receipt(store, &request, &prepared, &request_sha256);
    }

    if let Err(error) = revalidate_executor_runtime_before_spawn(store, &request, &mut prepared) {
        let error =
            sanitize_runtime_text(&format!("executor runtime preflight changed: {error:#}"));
        let mut receipt = build_receipt(
            &request,
            &prepared,
            execution_id,
            command_argument_shape,
            prompt_sha256,
            request_sha256,
            None,
            claimed_at,
            started.elapsed(),
            None,
            None,
            false,
            Some(error.clone()),
            None,
            empty_stream_evidence(),
            stream_evidence_from_text(&error),
        );
        receipt.status = "executor_runtime_preflight_failed".to_string();
        finalize_executor_runtime(store, &owner_token, &receipt)?;
        return Ok(receipt);
    }
    let git_baseline = capture_executor_runtime_git_baseline(&prepared);

    let started_at = Utc::now();
    let started_at_rfc3339 = started_at.to_rfc3339();
    store.with_transaction(|| {
        if !store.mark_executor_runtime_started(
            &request.workflow_id,
            &request.task_id,
            &request.lease_id,
            &owner_token,
            &started_at_rfc3339,
        )? {
            bail!(
                "executor runtime claim changed before start for workflow {} task {} lease {}",
                request.workflow_id,
                request.task_id,
                request.lease_id
            );
        }

        store.record_event(
            &request.workflow_id,
            "executor_runtime_started",
            &serde_json::json!({
                "schema_version": EXECUTOR_RUNTIME_STARTED_SCHEMA_VERSION,
                "origin": "foundry_executor_runtime",
                "execution_id": execution_id,
                "workflow_id": request.workflow_id,
                "run_id": request.run_id,
                "task_id": request.task_id,
                "lease_id": prepared.lease.lease_id,
                "executor": prepared.executor,
                "command_path": prepared.command_path.display().to_string(),
                "command_argument_shape": command_argument_shape,
                "cwd": prepared.cwd.display().to_string(),
                "prompt_transport": executor_prompt_transport(&prepared.executor),
                "prompt_sha256": prompt_sha256,
                "prompt_bytes": request.prompt.len(),
                "request_sha256": request_sha256,
                "authorization_opt_in": request.authorization.allow_non_interactive_execution,
                "approved_by": request.authorization.approved_by,
                "authorization_reason": request.authorization.reason,
                "timeout_seconds": request.timeout_seconds,
                "lease_expires_at": prepared.lease.expires_at,
                "lease_grace_seconds": EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS,
                "lease_preserved_for_validation": true,
                "dispatch": request.dispatch,
                "started_at": started_at,
                "task_completion_attempted": false,
                "output_accepted_as_validation": false,
            }),
        )
    })?;

    let mut command = match prepared.executor.as_str() {
        "codex" => build_codex_runtime_command(
            &prepared.command_path,
            &prepared.cwd,
            prepared.codex_git_common_dir.as_deref(),
            &request.prompt,
        ),
        "agy" => build_agy_runtime_command(
            &prepared.command_path,
            &prepared.cwd,
            &request.prompt,
            request.timeout_seconds,
        ),
        _ => unreachable!("executor canonicalization is validated during preparation"),
    };
    command
        .env("FOUNDRY_WORKFLOW_ID", &request.workflow_id)
        .env("FOUNDRY_RUN_ID", &request.run_id)
        .env("FOUNDRY_TASK_ID", &request.task_id)
        .env("FOUNDRY_TASK_LEASE_ID", &request.lease_id)
        .env("FOUNDRY_EXECUTOR_RUNTIME", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if prepared.executor == "codex" {
        harden_codex_git_environment(&mut command);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let error = sanitize_runtime_text(&format!("failed to spawn executor: {error}"));
            let git = Some(observe_executor_runtime_git(&prepared, git_baseline));
            let receipt = build_receipt(
                &request,
                &prepared,
                execution_id,
                command_argument_shape,
                prompt_sha256,
                request_sha256,
                git,
                started_at,
                started.elapsed(),
                None,
                None,
                false,
                Some(error.clone()),
                None,
                empty_stream_evidence(),
                stream_evidence_from_text(&error),
            );
            finalize_executor_runtime(store, &owner_token, &receipt)?;
            return Ok(receipt);
        }
    };
    let process_id = Some(child.id());
    let stdin_handle = if prepared.executor == "codex" {
        let stdin = child
            .stdin
            .take()
            .context("executor stdin pipe is missing")?;
        Some(spawn_prompt_writer(stdin, request.prompt.clone()))
    } else {
        drop(child.stdin.take());
        None
    };
    let stdout = child
        .stdout
        .take()
        .context("executor stdout pipe is missing")?;
    let stderr = child
        .stderr
        .take()
        .context("executor stderr pipe is missing")?;
    let stdout_handle = spawn_stream_reader(stdout, Some(prepared.executor.clone()));
    let stderr_handle = spawn_stream_reader(stderr, None);

    let timeout = Duration::from_secs(request.timeout_seconds);
    let (exit_status, timed_out, mut runtime_error) = wait_for_executor(&mut child, timeout);
    if let Some(stdin_handle) = stdin_handle {
        append_runtime_error(&mut runtime_error, join_prompt_writer(stdin_handle));
    }
    let (stdout_capture, stdout_error) = join_stream_reader(stdout_handle, "stdout");
    let (stderr_capture, stderr_error) = join_stream_reader(stderr_handle, "stderr");
    append_runtime_error(&mut runtime_error, stdout_error);
    append_runtime_error(&mut runtime_error, stderr_error);
    let git = Some(observe_executor_runtime_git(&prepared, git_baseline));

    let receipt = build_receipt(
        &request,
        &prepared,
        execution_id,
        command_argument_shape,
        prompt_sha256,
        request_sha256,
        git,
        started_at,
        started.elapsed(),
        process_id,
        exit_status,
        timed_out,
        runtime_error,
        stdout_capture.token_usage,
        stdout_capture.evidence,
        stderr_capture.evidence,
    );
    finalize_executor_runtime(store, &owner_token, &receipt)?;
    Ok(receipt)
}

fn wait_for_idempotent_executor_receipt(
    store: &FoundryStore,
    request: &ExecutorRuntimeRequest,
    prepared: &PreparedExecutorRuntime,
    request_sha256: &str,
) -> Result<ExecutorRuntimeReceipt> {
    let wait_seconds = request
        .timeout_seconds
        .saturating_add(IDEMPOTENT_REPLAY_WAIT_GRACE_SECONDS);
    let deadline = Instant::now() + Duration::from_secs(wait_seconds);
    loop {
        let claim = store
            .load_executor_runtime_claim(&request.workflow_id, &request.task_id, &request.lease_id)?
            .with_context(|| {
                format!(
                    "executor runtime claim disappeared for workflow {} task {} lease {}",
                    request.workflow_id, request.task_id, request.lease_id
                )
            })?;
        if claim.executor != prepared.executor || claim.request_sha256 != request_sha256 {
            bail!(
                "executor runtime lease {} was already claimed by a different request",
                request.lease_id
            );
        }
        match claim.state.as_str() {
            "finished" => {
                let receipt_json = claim.receipt_json.with_context(|| {
                    format!(
                        "finished executor runtime claim {} has no persisted receipt",
                        claim.execution_id
                    )
                })?;
                let mut receipt = serde_json::from_str::<ExecutorRuntimeReceipt>(&receipt_json)
                    .context("persisted executor runtime receipt is invalid")?;
                if receipt.request_sha256 != request_sha256
                    || receipt.workflow_id != request.workflow_id
                    || receipt.task_id != request.task_id
                    || receipt.lease_id != request.lease_id
                {
                    bail!(
                        "persisted executor runtime receipt does not match lease {}",
                        request.lease_id
                    );
                }
                receipt.idempotent_replay = true;
                return Ok(receipt);
            }
            "claimed" | "running" => {}
            state => bail!(
                "executor runtime claim {} has unsupported state {state}",
                claim.execution_id
            ),
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for idempotent executor runtime receipt for lease {}",
                request.lease_id
            );
        }
        thread::sleep(EXECUTOR_RUNTIME_POLL_INTERVAL);
    }
}

fn finalize_executor_runtime(
    store: &FoundryStore,
    owner_token: &str,
    receipt: &ExecutorRuntimeReceipt,
) -> Result<()> {
    let receipt_value = serde_json::to_value(receipt)?;
    let finished_at = receipt.finished_at.to_rfc3339();
    store.with_transaction(|| {
        record_executor_runtime_finished(store, receipt)?;
        if !store.finish_executor_runtime_claim(
            &receipt.workflow_id,
            &receipt.task_id,
            &receipt.lease_id,
            owner_token,
            &receipt_value,
            &finished_at,
        )? {
            bail!(
                "executor runtime claim changed before finish for workflow {} task {} lease {}",
                receipt.workflow_id,
                receipt.task_id,
                receipt.lease_id
            );
        }
        Ok(())
    })
}

fn prepare_executor_runtime(
    store: &FoundryStore,
    request: &ExecutorRuntimeRequest,
) -> Result<PreparedExecutorRuntime> {
    require_text(&request.workflow_id, "workflow id")?;
    require_text(&request.run_id, "run id")?;
    require_text(&request.task_id, "task id")?;
    require_text(&request.lease_id, "lease id")?;
    require_text(&request.executor, "executor")?;
    require_text(&request.prompt, "executor prompt")?;
    require_text(&request.authorization.approved_by, "executor approver")?;
    require_text(
        &request.authorization.reason,
        "executor authorization reason",
    )?;
    if !request.authorization.allow_non_interactive_execution {
        bail!("executor runtime requires explicit non-interactive execution opt-in");
    }
    if request.prompt.len() > MAX_EXECUTOR_PROMPT_BYTES {
        bail!("executor prompt exceeds bounded maximum of {MAX_EXECUTOR_PROMPT_BYTES} bytes");
    }
    if !(1..=MAX_EXECUTOR_RUNTIME_SECONDS).contains(&request.timeout_seconds) {
        bail!(
            "executor runtime timeout must be between 1 and {MAX_EXECUTOR_RUNTIME_SECONDS} seconds"
        );
    }

    let executor = canonical_executor_id(&request.executor);
    if !matches!(executor.as_str(), "codex" | "agy") {
        bail!("executor runtime supports only canonical executors codex and agy, not {executor}");
    }
    let state = load_executor_state(store, &executor)?;
    let mut policy_failures = Vec::new();
    if !state.installed {
        policy_failures.push("installed=false");
    }
    if !state.configured {
        policy_failures.push("configured=false");
    }
    if !state.allowed {
        policy_failures.push("allowed=false");
    }
    if !state.non_interactive_ready {
        policy_failures.push("non_interactive_ready=false");
    }
    if !policy_failures.is_empty() {
        bail!(
            "executor runtime policy blocks {executor}: {}",
            policy_failures.join(", ")
        );
    }
    let command_path = state
        .command_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .with_context(|| format!("executor policy for {executor} has no command_path"))?;
    let resolved_command_path = validate_executor_command_path(&command_path, &executor)?;

    let cwd = fs::canonicalize(&request.cwd).with_context(|| {
        format!(
            "failed to canonicalize executor cwd {}",
            request.cwd.display()
        )
    })?;
    if !cwd.is_dir() {
        bail!("executor cwd {} is not a directory", cwd.display());
    }

    let workflow = store.load_workflow(&request.workflow_id)?;
    if is_terminal_status(&workflow.status) {
        bail!(
            "executor runtime cannot start for terminal workflow {} with status {}",
            request.workflow_id,
            workflow.status
        );
    }
    validate_runtime_task_and_dependencies(&workflow, &request.task_id)?;
    validate_executor_runtime_dispatch(&workflow, &request.task_id, request.dispatch.as_ref())?;
    let run = load_run_record(store, &request.run_id)?;
    if run.workflow_id != request.workflow_id {
        bail!(
            "run {} belongs to workflow {}, not {}",
            request.run_id,
            run.workflow_id,
            request.workflow_id
        );
    }
    if is_terminal_status(&run.status) {
        bail!(
            "executor runtime cannot start for terminal run {} with status {}",
            request.run_id,
            run.status
        );
    }

    let lease_value = store
        .load_task_lease(&request.workflow_id, &request.task_id)?
        .with_context(|| {
            format!(
                "active task lease is required for workflow {} task {}",
                request.workflow_id, request.task_id
            )
        })?;
    let persisted_lease = serde_json::from_value::<TaskLease>(lease_value)
        .context("persisted executor task lease is invalid")?;
    if persisted_lease.lease_id != request.lease_id {
        bail!(
            "task lease id mismatch for workflow {} task {}: expected {}, found {}",
            request.workflow_id,
            request.task_id,
            request.lease_id,
            persisted_lease.lease_id
        );
    }
    if canonical_executor_id(&persisted_lease.executor) != executor {
        bail!(
            "task lease {} belongs to executor {}, not {}",
            persisted_lease.lease_id,
            persisted_lease.executor,
            executor
        );
    }
    let lease = validate_task_lease_for_execution(
        store,
        &request.workflow_id,
        &request.task_id,
        &persisted_lease.executor,
        &cwd,
    )?;
    if lease.lease_id != request.lease_id {
        bail!("validated task lease changed before executor runtime preparation");
    }
    require_task_scoped_workspace_claim(&lease)?;
    let (lease, lease_extended_for_runtime) = ensure_executor_runtime_lease_window(
        store,
        request,
        &persisted_lease.executor,
        &cwd,
        lease,
    )?;
    let codex_git_common_dir = if executor == "codex" {
        Some(resolve_codex_git_common_dir(&cwd, &lease)?)
    } else {
        None
    };

    Ok(PreparedExecutorRuntime {
        executor,
        command_path,
        resolved_command_path,
        cwd,
        codex_git_common_dir,
        lease,
        lease_extended_for_runtime,
    })
}

fn validate_runtime_task_and_dependencies(workflow: &Workflow, task_id: &str) -> Result<()> {
    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .with_context(|| format!("task {task_id} is missing from workflow {}", workflow.id))?;
    if task.status != TaskStatus::Pending {
        bail!(
            "executor runtime requires pending task {task_id}, found {}",
            task_status_name(&task.status)
        );
    }

    let mut blockers = Vec::new();
    for dependency_id in &task.dependencies {
        let dependency = workflow
            .tasks
            .iter()
            .find(|candidate| candidate.id == *dependency_id)
            .with_context(|| {
                format!(
                    "task {task_id} references missing dependency {dependency_id} in workflow {}",
                    workflow.id
                )
            })?;
        if dependency.status != TaskStatus::Completed {
            blockers.push(format!(
                "{}={}",
                dependency.id,
                task_status_name(&dependency.status)
            ));
        }
    }
    if !blockers.is_empty() {
        bail!(
            "executor runtime dependencies are not completed for task {task_id}: {}",
            blockers.join(", ")
        );
    }
    Ok(())
}

fn validate_executor_runtime_dispatch(
    workflow: &Workflow,
    task_id: &str,
    dispatch: Option<&ExecutorRuntimeDispatchCorrelation>,
) -> Result<()> {
    let Some(dispatch) = dispatch else {
        return Ok(());
    };
    require_text(&dispatch.wave_id, "executor runtime dispatch wave id")?;
    if dispatch.context_sha256.len() != 64
        || !dispatch
            .context_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("executor runtime dispatch context_sha256 must be a 64-character hex digest");
    }
    let workflow_revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0);
    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .with_context(|| format!("task {task_id} is missing from workflow {}", workflow.id))?;
    if dispatch.workflow_revision != workflow_revision || dispatch.task_version != task.version {
        bail!(
            "executor runtime dispatch drifted for workflow {} task {}: wave={} dispatched_revision={} current_revision={} dispatched_task_version={} current_task_version={}",
            workflow.id,
            task_id,
            dispatch.wave_id,
            dispatch.workflow_revision,
            workflow_revision,
            dispatch.task_version,
            task.version
        );
    }
    Ok(())
}

fn task_status_name(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
}

fn require_task_scoped_workspace_claim(lease: &TaskLease) -> Result<()> {
    let claim = lease.workspace_claim.as_ref().with_context(|| {
        format!(
            "executor runtime requires a task-scoped workspace claim for lease {}",
            lease.lease_id
        )
    })?;
    if claim.mode != "exclusive_mutation" {
        bail!(
            "executor runtime lease {} requires exclusive_mutation workspace mode, found {}",
            lease.lease_id,
            claim.mode
        );
    }
    if claim.binding_scope != "task" {
        bail!(
            "executor runtime lease {} requires task-scoped workspace binding, found {}",
            lease.lease_id,
            claim.binding_scope
        );
    }
    Ok(())
}

fn resolve_codex_git_common_dir(cwd: &Path, lease: &TaskLease) -> Result<PathBuf> {
    let claim = lease.workspace_claim.as_ref().with_context(|| {
        format!(
            "Codex Git write access requires a task-scoped workspace claim for lease {}",
            lease.lease_id
        )
    })?;
    let repository_root = fs::canonicalize(&claim.repository_root).with_context(|| {
        format!(
            "failed to canonicalize claimed repository root {}",
            claim.repository_root
        )
    })?;
    let claimed_worktree = fs::canonicalize(&claim.worktree_root).with_context(|| {
        format!(
            "failed to canonicalize claimed worktree root {}",
            claim.worktree_root
        )
    })?;
    if cwd != claimed_worktree {
        bail!(
            "Codex cwd {} does not match claimed task worktree {}",
            cwd.display(),
            claimed_worktree.display()
        );
    }

    let worktree_common_dir = executor_runtime_git_path(cwd, &["rev-parse", "--git-common-dir"])?;
    let repository_common_dir =
        executor_runtime_git_path(&repository_root, &["rev-parse", "--git-common-dir"])?;
    if worktree_common_dir != repository_common_dir {
        bail!(
            "task worktree Git common dir {} does not match claimed repository Git common dir {}",
            worktree_common_dir.display(),
            repository_common_dir.display()
        );
    }
    if !repository_common_dir.starts_with(&repository_root) {
        bail!(
            "Codex Git common dir {} escapes claimed repository root {}",
            repository_common_dir.display(),
            repository_root.display()
        );
    }
    Ok(repository_common_dir)
}

fn executor_runtime_git_path(cwd: &Path, args: &[&str]) -> Result<PathBuf> {
    let raw = executor_runtime_git_text(cwd, args)?;
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    fs::canonicalize(&path)
        .with_context(|| format!("failed to canonicalize Git path {}", path.display()))
}

fn ensure_executor_runtime_lease_window(
    store: &FoundryStore,
    request: &ExecutorRuntimeRequest,
    lease_executor: &str,
    cwd: &Path,
    lease: TaskLease,
) -> Result<(TaskLease, bool)> {
    let now = Utc::now();
    let required_seconds = request
        .timeout_seconds
        .saturating_add(EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS);
    let required_seconds = i64::try_from(required_seconds)
        .context("executor runtime lease window exceeds supported duration")?;
    let required_expires_at = now + ChronoDuration::seconds(required_seconds);
    if lease.expires_at >= required_expires_at {
        return Ok((lease, false));
    }

    let mut extended_lease = lease.clone();
    extended_lease.expires_at = required_expires_at;
    let extended = store.try_extend_task_lease_for_runtime(
        &request.workflow_id,
        &request.task_id,
        &request.lease_id,
        &now.to_rfc3339(),
        &required_expires_at.to_rfc3339(),
        &serde_json::to_value(&extended_lease)?,
    )?;
    let refreshed = validate_task_lease_for_execution(
        store,
        &request.workflow_id,
        &request.task_id,
        lease_executor,
        cwd,
    )?;
    if refreshed.lease_id != request.lease_id {
        bail!("task lease changed while extending executor runtime window");
    }
    require_task_scoped_workspace_claim(&refreshed)?;
    if refreshed.expires_at < required_expires_at {
        bail!(
            "task lease {} does not cover executor timeout plus {} seconds of validation grace",
            refreshed.lease_id,
            EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS
        );
    }
    Ok((refreshed, extended))
}

fn revalidate_executor_runtime_before_spawn(
    store: &FoundryStore,
    request: &ExecutorRuntimeRequest,
    prepared: &mut PreparedExecutorRuntime,
) -> Result<()> {
    let resolved_command_path =
        validate_executor_command_path(&prepared.command_path, &prepared.executor)?;
    if resolved_command_path != prepared.resolved_command_path {
        bail!(
            "executor policy command_path {} changed target from {} to {} before spawn",
            prepared.command_path.display(),
            prepared.resolved_command_path.display(),
            resolved_command_path.display()
        );
    }
    let workflow = store.load_workflow(&request.workflow_id)?;
    if is_terminal_status(&workflow.status) {
        bail!(
            "workflow {} became terminal with status {}",
            workflow.id,
            workflow.status
        );
    }
    validate_runtime_task_and_dependencies(&workflow, &request.task_id)?;
    validate_executor_runtime_dispatch(&workflow, &request.task_id, request.dispatch.as_ref())?;
    let run = load_run_record(store, &request.run_id)?;
    if run.workflow_id != request.workflow_id || is_terminal_status(&run.status) {
        bail!(
            "run {} is no longer executable for workflow {} (status={})",
            request.run_id,
            request.workflow_id,
            run.status
        );
    }
    let lease = validate_task_lease_for_execution(
        store,
        &request.workflow_id,
        &request.task_id,
        &prepared.lease.executor,
        &prepared.cwd,
    )?;
    if lease.lease_id != request.lease_id {
        bail!("task lease changed immediately before executor spawn");
    }
    require_task_scoped_workspace_claim(&lease)?;
    let lease_executor = lease.executor.clone();
    let (lease, extended) = ensure_executor_runtime_lease_window(
        store,
        request,
        &lease_executor,
        &prepared.cwd,
        lease,
    )?;
    let current_codex_git_common_dir = if prepared.executor == "codex" {
        Some(resolve_codex_git_common_dir(&prepared.cwd, &lease)?)
    } else {
        None
    };
    if current_codex_git_common_dir != prepared.codex_git_common_dir {
        bail!(
            "Codex Git common dir changed before executor spawn: prepared={} current={}",
            prepared
                .codex_git_common_dir
                .as_deref()
                .map_or_else(|| "none".to_string(), |path| path.display().to_string()),
            current_codex_git_common_dir
                .as_deref()
                .map_or_else(|| "none".to_string(), |path| path.display().to_string())
        );
    }
    prepared.lease = lease;
    prepared.codex_git_common_dir = current_codex_git_common_dir;
    prepared.lease_extended_for_runtime |= extended;
    Ok(())
}

fn harden_codex_git_environment(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
}

fn load_executor_state(store: &FoundryStore, executor: &str) -> Result<ExecutorState> {
    load_executors(store)?
        .executors
        .into_iter()
        .find(|state| state.id == executor)
        .with_context(|| {
            format!("executor runtime policy is missing canonical executor {executor}")
        })
}

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(value)
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "complete" | "cancelled" | "canceled" | "failed"
    )
}

fn validate_executor_command_path(command_path: &Path, executor: &str) -> Result<PathBuf> {
    if !command_path.is_absolute() {
        bail!("executor policy command_path must be absolute for {executor}");
    }
    let resolved_command_path = fs::canonicalize(command_path).with_context(|| {
        format!(
            "failed to resolve executor policy command_path {} for {executor}",
            command_path.display()
        )
    })?;
    if !resolved_command_path.is_file() {
        bail!(
            "executor policy command_path {} for {executor} is not a file",
            resolved_command_path.display()
        );
    }
    ensure_executable(&resolved_command_path, executor)?;
    Ok(resolved_command_path)
}

fn ensure_executable(command_path: &Path, executor: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(command_path)?.permissions().mode() & 0o111 == 0 {
            bail!(
                "executor policy command_path {} for {executor} is not executable",
                command_path.display()
            );
        }
    }
    #[cfg(not(unix))]
    let _ = executor;
    Ok(())
}

fn command_argument_shape(executor: &str) -> Vec<String> {
    let shape = match executor {
        "codex" => vec![
            "exec",
            "--json",
            "--ephemeral",
            "--ignore-user-config",
            "-c",
            "allow_login_shell=false",
            "--sandbox",
            "workspace-write",
            "--add-dir",
            "<git-common-dir>",
            "-C",
            "<cwd>",
            "-",
        ],
        "agy" => vec![
            "--print",
            "<prompt>",
            "--print-timeout",
            "<timeout_seconds>s",
            "--mode",
            "accept-edits",
            "--sandbox",
            "--output-format",
            "json",
        ],
        _ => unreachable!("command shape requested for unsupported executor"),
    };
    shape.into_iter().map(str::to_string).collect()
}

fn stdin_prompt_transport() -> String {
    "stdin".to_string()
}

fn executor_prompt_transport(executor: &str) -> &'static str {
    match executor {
        "agy" => "argument",
        _ => "stdin",
    }
}

fn spawn_prompt_writer(
    mut stdin: std::process::ChildStdin,
    prompt: String,
) -> thread::JoinHandle<std::io::Result<()>> {
    thread::spawn(move || {
        stdin.write_all(prompt.as_bytes())?;
        stdin.flush()
    })
}

fn join_prompt_writer(handle: thread::JoinHandle<std::io::Result<()>>) -> Option<String> {
    match handle.join() {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(format!("failed to write executor prompt to stdin: {error}")),
        Err(_) => Some("executor stdin writer panicked".to_string()),
    }
}

fn spawn_stream_reader<R>(
    reader: R,
    usage_executor: Option<String>,
) -> thread::JoinHandle<std::io::Result<CapturedExecutorRuntimeStream>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || capture_stream(reader, usage_executor.as_deref()))
}

fn capture_stream(
    mut reader: impl Read,
    usage_executor: Option<&str>,
) -> std::io::Result<CapturedExecutorRuntimeStream> {
    let mut hasher = Sha256::new();
    let mut excerpt = Vec::with_capacity(MAX_EXECUTOR_OUTPUT_EXCERPT_BYTES);
    let mut usage_parse_bytes = VecDeque::new();
    let mut total_bytes = 0usize;
    let mut buffer = [0u8; 8_192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read);
        hasher.update(&buffer[..read]);
        let remaining = MAX_EXECUTOR_OUTPUT_EXCERPT_BYTES.saturating_sub(excerpt.len());
        excerpt.extend_from_slice(&buffer[..read.min(remaining)]);
        if usage_executor.is_some() {
            let overflow = usage_parse_bytes
                .len()
                .saturating_add(read)
                .saturating_sub(MAX_EXECUTOR_USAGE_PARSE_BYTES);
            usage_parse_bytes.drain(..overflow.min(usage_parse_bytes.len()));
            usage_parse_bytes.extend(&buffer[..read]);
        }
    }
    let token_usage = usage_executor.and_then(|executor| {
        let raw_usage_bytes = usage_parse_bytes.make_contiguous();
        parse_executor_token_usage(executor, raw_usage_bytes)
    });
    let raw_excerpt = String::from_utf8_lossy(&excerpt);
    let sanitized = sanitize_prompt_secrets(
        &raw_excerpt,
        runtime_secret_sanitization_options("executor_runtime_output"),
    );
    Ok(CapturedExecutorRuntimeStream {
        evidence: ExecutorRuntimeStreamEvidence {
            sha256: format!("{:x}", hasher.finalize()),
            total_bytes,
            excerpt_bytes: excerpt.len(),
            excerpt_truncated: total_bytes > excerpt.len(),
            excerpt_redaction_count: sanitized.detection_count,
            excerpt: sanitized.sanitized_text,
        },
        token_usage,
    })
}

fn parse_executor_token_usage(executor: &str, raw_stdout: &[u8]) -> Option<ExecutorTokenUsage> {
    match executor {
        "codex" => parse_codex_token_usage(raw_stdout),
        "agy" => parse_agy_token_usage(raw_stdout),
        _ => None,
    }
}

fn parse_codex_token_usage(raw_stdout: &[u8]) -> Option<ExecutorTokenUsage> {
    String::from_utf8_lossy(raw_stdout)
        .lines()
        .filter_map(parse_json_value_from_line)
        .filter(|value| {
            value.get("type").and_then(serde_json::Value::as_str) == Some("turn.completed")
        })
        .filter_map(|value| {
            normalized_token_usage(
                value.get("usage")?,
                "codex.exec.jsonl.turn.completed.usage",
                &["cached_input_tokens"],
                &["cache_write_input_tokens"],
                &["reasoning_output_tokens"],
                &["thinking_tokens"],
            )
        })
        .next_back()
}

fn parse_agy_token_usage(raw_stdout: &[u8]) -> Option<ExecutorTokenUsage> {
    let whole_document = serde_json::from_slice::<serde_json::Value>(raw_stdout)
        .ok()
        .and_then(|value| value.get("usage").cloned());
    let line_document = || {
        String::from_utf8_lossy(raw_stdout)
            .lines()
            .filter_map(parse_json_value_from_line)
            .filter_map(|value| value.get("usage").cloned())
            .next_back()
    };
    whole_document
        .or_else(line_document)
        .or_else(|| extract_named_json_object(raw_stdout, b"\"usage\""))
        .and_then(|usage| {
            normalized_token_usage(
                &usage,
                "agy.json.usage",
                &["cached_input_tokens", "cache_read_tokens"],
                &[
                    "cache_write_input_tokens",
                    "cache_write_tokens",
                    "cache_creation_input_tokens",
                ],
                &["reasoning_output_tokens"],
                &["thinking_tokens"],
            )
        })
}

fn normalized_token_usage(
    usage: &serde_json::Value,
    source_format: &str,
    cached_input_keys: &[&str],
    cache_write_input_keys: &[&str],
    reasoning_output_keys: &[&str],
    thinking_keys: &[&str],
) -> Option<ExecutorTokenUsage> {
    let input_tokens = json_u64(usage, &["input_tokens"]);
    let output_tokens = json_u64(usage, &["output_tokens"]);
    let cached_input_tokens = json_u64(usage, cached_input_keys);
    let cache_write_input_tokens = json_u64(usage, cache_write_input_keys);
    let reasoning_output_tokens = json_u64(usage, reasoning_output_keys);
    let thinking_tokens = json_u64(usage, thinking_keys);
    let total_tokens = json_u64(usage, &["total_tokens"]).or_else(|| {
        input_tokens
            .zip(output_tokens)
            .and_then(|(input, output)| input.checked_add(output))
    });
    if input_tokens.is_none()
        && output_tokens.is_none()
        && cached_input_tokens.is_none()
        && cache_write_input_tokens.is_none()
        && reasoning_output_tokens.is_none()
        && thinking_tokens.is_none()
        && total_tokens.is_none()
    {
        return None;
    }
    Some(ExecutorTokenUsage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        thinking_tokens,
        total_tokens,
        source_format: source_format.to_string(),
    })
}

fn json_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_u64))
}

fn parse_json_value_from_line(line: &str) -> Option<serde_json::Value> {
    let line = line.trim();
    serde_json::from_str(line).ok().or_else(|| {
        let start = line.find('{')?;
        let end = line.rfind('}')?;
        serde_json::from_str(&line[start..=end]).ok()
    })
}

fn extract_named_json_object(raw: &[u8], key: &[u8]) -> Option<serde_json::Value> {
    let mut offset = 0usize;
    let mut found = None;
    while offset.saturating_add(key.len()) <= raw.len() {
        let Some(relative_key_start) = raw[offset..]
            .windows(key.len())
            .position(|window| window == key)
        else {
            break;
        };
        let key_start = offset.saturating_add(relative_key_start);
        let mut cursor = key_start.saturating_add(key.len());
        while raw.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor = cursor.saturating_add(1);
        }
        if raw.get(cursor) != Some(&b':') {
            offset = key_start.saturating_add(1);
            continue;
        }
        cursor = cursor.saturating_add(1);
        while raw.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor = cursor.saturating_add(1);
        }
        if raw.get(cursor) != Some(&b'{') {
            offset = key_start.saturating_add(1);
            continue;
        }
        if let Some(end) = json_object_end(raw, cursor) {
            if let Ok(value) = serde_json::from_slice(&raw[cursor..=end]) {
                found = Some(value);
            }
        }
        offset = key_start.saturating_add(1);
    }
    found
}

fn json_object_end(raw: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (relative, byte) in raw.get(start..)?.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(start.saturating_add(relative));
                }
            }
            _ => {}
        }
    }
    None
}

fn join_stream_reader(
    handle: thread::JoinHandle<std::io::Result<CapturedExecutorRuntimeStream>>,
    stream_name: &str,
) -> (CapturedExecutorRuntimeStream, Option<String>) {
    match handle.join() {
        Ok(Ok(capture)) => (capture, None),
        Ok(Err(error)) => (
            empty_captured_stream(),
            Some(format!("failed to capture executor {stream_name}: {error}")),
        ),
        Err(_) => (
            empty_captured_stream(),
            Some(format!("executor {stream_name} reader panicked")),
        ),
    }
}

fn wait_for_executor(
    child: &mut Child,
    timeout: Duration,
) -> (Option<ExitStatus>, bool, Option<String>) {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let error = terminate_executor_process_group(child, false)
                    .err()
                    .map(|error| sanitize_runtime_text(&error.to_string()));
                return (Some(status), false, error);
            }
            Ok(None) if started.elapsed() >= timeout => {
                let mut error = terminate_executor_process_group(child, true)
                    .err()
                    .map(|error| sanitize_runtime_text(&error.to_string()));
                let status = match child.wait() {
                    Ok(status) => Some(status),
                    Err(wait_error) => {
                        append_runtime_error(
                            &mut error,
                            Some(format!("failed to reap timed-out executor: {wait_error}")),
                        );
                        None
                    }
                };
                return (status, true, error);
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(wait_error) => {
                let mut error = Some(format!("failed to inspect executor process: {wait_error}"));
                append_runtime_error(
                    &mut error,
                    terminate_executor_process_group(child, true)
                        .err()
                        .map(|error| error.to_string()),
                );
                let status = child.wait().ok();
                return (
                    status,
                    false,
                    error.map(|value| sanitize_runtime_text(&value)),
                );
            }
        }
    }
}

fn terminate_executor_process_group(child: &mut Child, leader_running: bool) -> Result<()> {
    #[cfg(unix)]
    {
        let process_group = -(child.id() as i32);
        // SAFETY: the child was started in its own process group with process_group(0).
        let result = unsafe { libc::kill(process_group, libc::SIGKILL) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error).context("failed to terminate executor process group");
            }
        }
    }
    #[cfg(not(unix))]
    if leader_running {
        child
            .kill()
            .context("failed to terminate executor process")?;
    }
    #[cfg(unix)]
    let _ = leader_running;
    Ok(())
}

fn capture_executor_runtime_git_baseline(
    prepared: &PreparedExecutorRuntime,
) -> Result<ExecutorRuntimeGitBaseline> {
    let workspace_claim = prepared
        .lease
        .workspace_claim
        .as_ref()
        .context("prepared executor runtime has no workspace claim for Git observation")?;
    let repository_root =
        fs::canonicalize(&workspace_claim.repository_root).with_context(|| {
            format!(
                "failed to resolve claimed Git repository root {}",
                workspace_claim.repository_root
            )
        })?;
    let expected_worktree =
        fs::canonicalize(&workspace_claim.worktree_root).with_context(|| {
            format!(
                "failed to resolve claimed Git worktree root {}",
                workspace_claim.worktree_root
            )
        })?;
    if prepared.cwd != expected_worktree {
        bail!(
            "executor cwd {} does not match claimed Git worktree {}",
            prepared.cwd.display(),
            expected_worktree.display()
        );
    }

    let observed_worktree =
        executor_runtime_git_text(&prepared.cwd, &["rev-parse", "--show-toplevel"])?;
    let observed_worktree = fs::canonicalize(&observed_worktree).with_context(|| {
        format!("failed to resolve observed Git worktree root {observed_worktree}")
    })?;
    if observed_worktree != expected_worktree {
        bail!(
            "observed Git worktree {} does not match claimed worktree {}",
            observed_worktree.display(),
            expected_worktree.display()
        );
    }

    let base_head = workspace_claim.head.trim().to_string();
    if base_head.is_empty() {
        bail!("claimed Git base HEAD is empty");
    }
    let observed_head =
        executor_runtime_git_text(&prepared.cwd, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    if observed_head != base_head {
        bail!("observed Git HEAD {observed_head} does not match claimed base HEAD {base_head}");
    }
    let branch = executor_runtime_git_text(&prepared.cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;

    Ok(ExecutorRuntimeGitBaseline {
        repository_root: repository_root.display().to_string(),
        branch,
        base_head,
    })
}

fn observe_executor_runtime_git(
    prepared: &PreparedExecutorRuntime,
    baseline: Result<ExecutorRuntimeGitBaseline>,
) -> ExecutorRuntimeGitObservation {
    let baseline = match baseline {
        Ok(baseline) => baseline,
        Err(error) => {
            return failed_executor_runtime_git_observation(
                prepared,
                None,
                &format!("Git baseline observation failed: {error:#}"),
            );
        }
    };
    match build_executor_runtime_git_observation(prepared, &baseline) {
        Ok(observation) => observation,
        Err(error) => failed_executor_runtime_git_observation(
            prepared,
            Some(&baseline),
            &format!("Git post-execution observation failed: {error:#}"),
        ),
    }
}

fn build_executor_runtime_git_observation(
    prepared: &PreparedExecutorRuntime,
    baseline: &ExecutorRuntimeGitBaseline,
) -> Result<ExecutorRuntimeGitObservation> {
    let head =
        executor_runtime_git_text(&prepared.cwd, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let branch = executor_runtime_git_text(&prepared.cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let base_is_ancestor =
        executor_runtime_git_base_is_ancestor(&prepared.cwd, &baseline.base_head, &head)?;
    let revision_range = format!("{}..{head}", baseline.base_head);
    let commit_count = executor_runtime_git_text(
        &prepared.cwd,
        &["rev-list", "--count", revision_range.as_str()],
    )?
    .parse::<u64>()
    .context("Git commit count is not an unsigned integer")?;
    let changed_paths = executor_runtime_git_text(
        &prepared.cwd,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            revision_range.as_str(),
        ],
    )?
    .lines()
    .map(|path| path.trim_end_matches('\r'))
    .filter(|path| !path.is_empty())
    .map(str::to_string)
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect();
    let dirty = !executor_runtime_git_bytes(
        &prepared.cwd,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty();

    Ok(ExecutorRuntimeGitObservation {
        schema_version: EXECUTOR_RUNTIME_GIT_OBSERVATION_SCHEMA_VERSION.to_string(),
        repository_root: baseline.repository_root.clone(),
        branch: Some(branch),
        base_head: baseline.base_head.clone(),
        head: Some(head),
        base_is_ancestor: Some(base_is_ancestor),
        commit_count: Some(commit_count),
        changed_paths,
        dirty: Some(dirty),
        clean: Some(!dirty),
        status: "observed".to_string(),
        observation_error: None,
    })
}

fn failed_executor_runtime_git_observation(
    prepared: &PreparedExecutorRuntime,
    baseline: Option<&ExecutorRuntimeGitBaseline>,
    error: &str,
) -> ExecutorRuntimeGitObservation {
    let workspace_claim = prepared
        .lease
        .workspace_claim
        .as_ref()
        .expect("prepared executor runtime must retain a workspace claim");
    ExecutorRuntimeGitObservation {
        schema_version: EXECUTOR_RUNTIME_GIT_OBSERVATION_SCHEMA_VERSION.to_string(),
        repository_root: baseline.map_or_else(
            || workspace_claim.repository_root.clone(),
            |baseline| baseline.repository_root.clone(),
        ),
        branch: baseline.map(|baseline| baseline.branch.clone()),
        base_head: baseline.map_or_else(
            || workspace_claim.head.clone(),
            |baseline| baseline.base_head.clone(),
        ),
        head: None,
        base_is_ancestor: None,
        commit_count: None,
        changed_paths: Vec::new(),
        dirty: None,
        clean: None,
        status: "observation_failed".to_string(),
        observation_error: Some(sanitize_runtime_text(error)),
    }
}

fn executor_runtime_git_base_is_ancestor(cwd: &Path, base: &str, head: &str) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["merge-base", "--is-ancestor", base, head])
        .output()
        .with_context(|| {
            format!(
                "failed to execute git merge-base --is-ancestor in {}",
                cwd.display()
            )
        })?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!(
            "git merge-base --is-ancestor failed in {}: {}",
            cwd.display(),
            sanitize_runtime_text(&String::from_utf8_lossy(&output.stderr))
        ),
    }
}

fn executor_runtime_git_text(cwd: &Path, args: &[&str]) -> Result<String> {
    let bytes = executor_runtime_git_bytes(cwd, args)?;
    let text = String::from_utf8(bytes).context("Git output is not valid UTF-8")?;
    Ok(text.trim_end_matches(['\n', '\r']).to_string())
}

fn executor_runtime_git_bytes(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .with_context(|| {
            format!(
                "failed to execute git {} in {}",
                args.join(" "),
                cwd.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            cwd.display(),
            sanitize_runtime_text(&String::from_utf8_lossy(&output.stderr))
        );
    }
    Ok(output.stdout)
}

#[allow(clippy::too_many_arguments)]
fn build_receipt(
    request: &ExecutorRuntimeRequest,
    prepared: &PreparedExecutorRuntime,
    execution_id: String,
    command_argument_shape: Vec<String>,
    prompt_sha256: String,
    request_sha256: String,
    git: Option<ExecutorRuntimeGitObservation>,
    started_at: DateTime<Utc>,
    duration: Duration,
    process_id: Option<u32>,
    exit_status: Option<ExitStatus>,
    timed_out: bool,
    runtime_error: Option<String>,
    token_usage: Option<ExecutorTokenUsage>,
    stdout: ExecutorRuntimeStreamEvidence,
    stderr: ExecutorRuntimeStreamEvidence,
) -> ExecutorRuntimeReceipt {
    let workspace_claim = prepared
        .lease
        .workspace_claim
        .as_ref()
        .expect("prepared executor runtime must retain a workspace claim");
    let success = !timed_out
        && runtime_error.is_none()
        && exit_status.as_ref().is_some_and(ExitStatus::success);
    let status = if timed_out {
        "executor_runtime_timed_out"
    } else if process_id.is_none() {
        "executor_runtime_spawn_failed"
    } else if runtime_error.is_some() {
        "executor_runtime_failed"
    } else if success {
        "executor_runtime_succeeded"
    } else {
        "executor_runtime_exit_failed"
    };
    ExecutorRuntimeReceipt {
        schema_version: EXECUTOR_RUNTIME_RECEIPT_SCHEMA_VERSION.to_string(),
        execution_id,
        status: status.to_string(),
        success,
        workflow_id: request.workflow_id.clone(),
        run_id: request.run_id.clone(),
        task_id: request.task_id.clone(),
        lease_id: request.lease_id.clone(),
        executor: prepared.executor.clone(),
        command_path: prepared.command_path.display().to_string(),
        command_argument_shape,
        cwd: prepared.cwd.display().to_string(),
        prompt_transport: executor_prompt_transport(&prepared.executor).to_string(),
        prompt_sha256,
        prompt_bytes: request.prompt.len(),
        request_sha256,
        idempotent_replay: false,
        authorization_opt_in: request.authorization.allow_non_interactive_execution,
        approved_by: request.authorization.approved_by.clone(),
        authorization_reason: request.authorization.reason.clone(),
        timeout_seconds: request.timeout_seconds,
        lease_expires_at: prepared.lease.expires_at,
        lease_grace_seconds: EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS,
        lease_extended_for_runtime: prepared.lease_extended_for_runtime,
        lease_preserved_for_validation: true,
        worktree_id: workspace_claim.worktree_id.clone(),
        workspace_binding_scope: workspace_claim.binding_scope.clone(),
        dispatch: request.dispatch.clone(),
        git,
        token_usage,
        started_at,
        finished_at: Utc::now(),
        duration_ms: duration.as_millis(),
        process_id,
        exit_code: exit_status.and_then(|status| status.code()),
        timed_out,
        runtime_error: runtime_error.map(|error| sanitize_runtime_text(&error)),
        stdout,
        stderr,
        task_completion_attempted: false,
        output_accepted_as_validation: false,
    }
}

fn record_executor_runtime_finished(
    store: &FoundryStore,
    receipt: &ExecutorRuntimeReceipt,
) -> Result<()> {
    let mut event = serde_json::to_value(receipt)?;
    event["origin"] = serde_json::Value::String("foundry_executor_runtime".to_string());
    store.record_event(&receipt.workflow_id, "executor_runtime_finished", &event)
}

fn empty_captured_stream() -> CapturedExecutorRuntimeStream {
    CapturedExecutorRuntimeStream {
        evidence: empty_stream_evidence(),
        token_usage: None,
    }
}

fn empty_stream_evidence() -> ExecutorRuntimeStreamEvidence {
    ExecutorRuntimeStreamEvidence {
        sha256: hex_sha256(b""),
        total_bytes: 0,
        excerpt_bytes: 0,
        excerpt_truncated: false,
        excerpt_redaction_count: 0,
        excerpt: String::new(),
    }
}

fn stream_evidence_from_text(text: &str) -> ExecutorRuntimeStreamEvidence {
    let bytes = text.as_bytes();
    let captured = &bytes[..bytes.len().min(MAX_EXECUTOR_OUTPUT_EXCERPT_BYTES)];
    let raw_excerpt = String::from_utf8_lossy(captured);
    let sanitized = sanitize_prompt_secrets(
        &raw_excerpt,
        runtime_secret_sanitization_options("executor_runtime_error"),
    );
    ExecutorRuntimeStreamEvidence {
        sha256: hex_sha256(bytes),
        total_bytes: bytes.len(),
        excerpt_bytes: captured.len(),
        excerpt_truncated: bytes.len() > captured.len(),
        excerpt_redaction_count: sanitized.detection_count,
        excerpt: sanitized.sanitized_text,
    }
}

fn sanitize_runtime_text(text: &str) -> String {
    sanitize_prompt_secrets(
        text,
        runtime_secret_sanitization_options("executor_runtime_error"),
    )
    .sanitized_text
}

fn runtime_secret_sanitization_options(scope: &str) -> SecretSanitizationOptions {
    SecretSanitizationOptions {
        scope: scope.to_string(),
        enable_regex: true,
        enable_entropy: true,
        enable_local_ai_fallback: false,
        allow_external_ai: false,
        entropy_threshold: 4.2,
    }
}

fn append_runtime_error(target: &mut Option<String>, addition: Option<String>) {
    let Some(addition) = addition.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    match target {
        Some(current) => {
            current.push_str("; ");
            current.push_str(&addition);
        }
        None => *target = Some(addition),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_observed_codex_turn_completed_usage() {
        let raw = br#"not-json
{"type":"thread.started","thread_id":"thread-1"}
{"type":"turn.completed","usage":{"input_tokens":16459,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0}}
trailing noise"#;

        let usage = parse_executor_token_usage("codex", raw).expect("Codex token usage");

        assert_eq!(
            usage,
            ExecutorTokenUsage {
                input_tokens: Some(16_459),
                output_tokens: Some(5),
                cached_input_tokens: Some(0),
                cache_write_input_tokens: Some(0),
                reasoning_output_tokens: Some(0),
                thinking_tokens: None,
                total_tokens: Some(16_464),
                source_format: "codex.exec.jsonl.turn.completed.usage".to_string(),
            }
        );
    }

    #[test]
    fn parses_observed_agy_usage() {
        let raw = br#"{"result":"done","usage":{"input_tokens":19241,"output_tokens":300,"thinking_tokens":294,"cache_read_tokens":0,"total_tokens":19541}}"#;

        let usage = parse_executor_token_usage("agy", raw).expect("Agy token usage");

        assert_eq!(
            usage,
            ExecutorTokenUsage {
                input_tokens: Some(19_241),
                output_tokens: Some(300),
                cached_input_tokens: Some(0),
                cache_write_input_tokens: None,
                reasoning_output_tokens: None,
                thinking_tokens: Some(294),
                total_tokens: Some(19_541),
                source_format: "agy.json.usage".to_string(),
            }
        );
    }

    #[test]
    fn absent_or_noisy_usage_does_not_create_zero_counters() {
        assert_eq!(
            parse_executor_token_usage("codex", b"noise\n{\"type\":\"turn.completed\"}"),
            None
        );
        assert_eq!(
            parse_executor_token_usage("agy", b"noise\n{\"result\":\"done\"}\nmore noise"),
            None
        );
    }

    #[test]
    fn legacy_receipt_without_token_usage_still_deserializes() {
        let legacy = json!({
            "schema_version": EXECUTOR_RUNTIME_RECEIPT_SCHEMA_VERSION,
            "execution_id": "executor_runtime_legacy",
            "status": "executor_runtime_succeeded",
            "success": true,
            "workflow_id": "wf-legacy",
            "run_id": "run-legacy",
            "task_id": "task-legacy",
            "lease_id": "lease-legacy",
            "executor": "codex",
            "command_path": "/usr/bin/codex",
            "command_argument_shape": ["exec", "--ephemeral", "--sandbox", "workspace-write", "-C", "<cwd>", "-"],
            "cwd": "/tmp/worktree",
            "prompt_transport": "stdin",
            "prompt_sha256": "prompt-sha",
            "prompt_bytes": 7,
            "request_sha256": "request-sha",
            "authorization_opt_in": true,
            "approved_by": "operator",
            "authorization_reason": "legacy receipt fixture",
            "timeout_seconds": 60,
            "lease_expires_at": "2026-07-29T12:00:00Z",
            "lease_grace_seconds": 300,
            "lease_extended_for_runtime": false,
            "lease_preserved_for_validation": true,
            "worktree_id": "wt-legacy",
            "workspace_binding_scope": "task",
            "started_at": "2026-07-29T11:59:00Z",
            "finished_at": "2026-07-29T11:59:01Z",
            "duration_ms": 1000,
            "process_id": 42,
            "exit_code": 0,
            "timed_out": false,
            "runtime_error": null,
            "stdout": {
                "sha256": "stdout-sha",
                "total_bytes": 4,
                "excerpt_bytes": 4,
                "excerpt_truncated": false,
                "excerpt_redaction_count": 0,
                "excerpt": "done"
            },
            "stderr": {
                "sha256": "stderr-sha",
                "total_bytes": 0,
                "excerpt_bytes": 0,
                "excerpt_truncated": false,
                "excerpt_redaction_count": 0,
                "excerpt": ""
            },
            "task_completion_attempted": false,
            "output_accepted_as_validation": false
        });

        let receipt: ExecutorRuntimeReceipt =
            serde_json::from_value(legacy).expect("legacy receipt remains compatible");

        assert_eq!(receipt.token_usage, None);
    }

    #[test]
    fn codex_command_shape_uses_headless_config_without_changing_stdin_or_sandbox() {
        let command = build_codex_runtime_command(
            Path::new("/usr/bin/codex"),
            Path::new("/tmp/codex-worktree"),
            Some(Path::new("/tmp/codex-repository/.git")),
            "bounded task",
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let expected = vec![
            "exec",
            "--json",
            "--ephemeral",
            "--ignore-user-config",
            "-c",
            "allow_login_shell=false",
            "--sandbox",
            "workspace-write",
            "--add-dir",
            "/tmp/codex-repository/.git",
            "-C",
            "/tmp/codex-worktree",
            "-",
        ];

        assert_eq!(args, expected);
        assert_eq!(
            command_argument_shape("codex"),
            vec![
                "exec",
                "--json",
                "--ephemeral",
                "--ignore-user-config",
                "-c",
                "allow_login_shell=false",
                "--sandbox",
                "workspace-write",
                "--add-dir",
                "<git-common-dir>",
                "-C",
                "<cwd>",
                "-"
            ]
        );
    }

    #[test]
    fn agy_command_passes_the_prompt_as_the_print_argument() {
        let command = build_agy_runtime_command(
            Path::new("/usr/bin/agy"),
            Path::new("/tmp/agy-worktree"),
            "bounded task",
            17,
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "--print",
                "bounded task",
                "--print-timeout",
                "17s",
                "--mode",
                "accept-edits",
                "--sandbox",
                "--output-format",
                "json",
            ]
        );
        assert_eq!(
            command_argument_shape("agy"),
            vec![
                "--print",
                "<prompt>",
                "--print-timeout",
                "<timeout_seconds>s",
                "--mode",
                "accept-edits",
                "--sandbox",
                "--output-format",
                "json",
            ]
        );
        assert_eq!(executor_prompt_transport("agy"), "argument");
    }

    #[test]
    fn implementation_wave_prompt_requires_bounded_commit_and_clean_worktree() {
        let mut prompt = "bounded task".to_string();

        append_implementation_wave_prompt(&mut prompt, true);

        assert!(prompt.contains("bounded scope/slice"));
        assert!(prompt.contains("applicable validations"));
        assert!(prompt.contains("at least one semantic Git commit"));
        assert!(prompt.contains("git status --porcelain"));

        let mut ordinary_prompt = "bounded task".to_string();
        append_implementation_wave_prompt(&mut ordinary_prompt, false);
        assert_eq!(ordinary_prompt, "bounded task");
    }
}
