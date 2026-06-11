use crate::artifact::hex_sha256;
use crate::harness::{inspect_cli_harness_shim_status, CliShimStatusOptions};
use crate::intent::OperatingContextSpec;
use crate::storage::{ForgeStore, GlobalEventWrite};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ExecutorSyncOptions {
    pub home: PathBuf,
    pub executor_paths: Vec<PathBuf>,
    pub shim_dirs: Vec<PathBuf>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub prompt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorState {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub installed: bool,
    pub configured: bool,
    pub command_path: Option<String>,
    pub config_evidence: Vec<String>,
    #[serde(default)]
    pub non_interactive_ready: bool,
    #[serde(default)]
    pub probe_evidence: Vec<String>,
    #[serde(default)]
    pub forge_first_ready: bool,
    #[serde(default)]
    pub forge_first_entrypoint: Option<Vec<String>>,
    #[serde(default)]
    pub harness_status: Option<ExecutorHarnessStatus>,
    pub allowed: bool,
    pub decision_source: String,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorHarnessStatus {
    pub schema_version: String,
    pub status: String,
    pub shim_dir: String,
    pub shim_path: String,
    pub path_precedence: String,
    pub forge_owned: bool,
    pub executable: bool,
    pub would_recurse: bool,
    pub real_command: Option<String>,
    pub store_path: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorIntegration {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub enabled: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorSyncReport {
    pub status: String,
    pub home: String,
    pub needs_human_approval: bool,
    pub usable: Vec<String>,
    pub executors: Vec<ExecutorState>,
    pub integrations: Vec<ExecutorIntegration>,
    pub brain_router: BrainRouterReport,
    pub quota_policy: ExecutorQuotaPolicyReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainRouterReport {
    pub schema_version: String,
    pub controller: String,
    pub controller_role: String,
    pub orchestrator_brain: String,
    pub brain_role: String,
    pub node_brain_role: String,
    pub routing_principle: String,
    pub node_brain_routing_policy: String,
    pub parallel_agent_policy: String,
    pub hot_swap_policy: String,
    pub selected_brain: Option<String>,
    pub forge_controlled_surfaces: Vec<String>,
    pub brain_owned_surfaces: Vec<String>,
    pub brains: Vec<BrainCandidate>,
    pub shell_sessions: Vec<BrainShellSessionSpec>,
    pub safety_gates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainCandidate {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub status: String,
    pub execution_mode: String,
    pub session_role: String,
    pub persistent_state_owner: String,
    pub context_source: String,
    pub memory_source: String,
    pub skills_source: String,
    pub mcp_source: String,
    pub installed: bool,
    pub configured: bool,
    pub allowed: bool,
    pub non_interactive_ready: bool,
    pub forge_first_ready: bool,
    pub forge_first_entrypoint: Option<Vec<String>>,
    pub harness_status: Option<ExecutorHarnessStatus>,
    pub shell_entrypoints: Vec<Vec<String>>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainShellSessionSpec {
    pub id: String,
    pub brain_id: String,
    pub entry_command: Vec<String>,
    pub attachable: bool,
    pub launch_mode: String,
    pub forge_first_ready: bool,
    pub forge_first_entrypoint: Option<Vec<String>>,
    pub role: String,
    pub state_boundary: String,
    pub safety_note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellLaunchPlanReport {
    pub schema_version: String,
    pub status: String,
    pub controller: String,
    pub executor_filter: Option<String>,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub context_budget: usize,
    pub ttl_seconds: u64,
    pub execution: String,
    pub launch_plans: Vec<ShellLaunchPlan>,
    pub safety_gates: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellLaunchPlan {
    pub session_id: String,
    pub brain_id: String,
    pub readiness: String,
    pub entry_command: Vec<String>,
    pub attachable: bool,
    pub launch_mode: String,
    pub forge_first_ready: bool,
    pub forge_first_entrypoint: Option<Vec<String>>,
    pub harness_status: Option<ExecutorHarnessStatus>,
    pub prompt_packet_gate_policy: ShellPromptPacketGatePolicy,
    pub dry_run: bool,
    pub execution_boundary: String,
    pub context_command: Option<Vec<String>>,
    pub handoff_command: Option<Vec<String>>,
    pub heartbeat_command: Option<Vec<String>>,
    pub preflight_commands: Vec<Vec<String>>,
    pub state_boundary: String,
    pub safety_note: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellPromptPacketGatePolicy {
    pub schema_version: String,
    pub context_source: String,
    pub required_gates: Vec<String>,
    pub policy: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct ShellLaunchPlanOptions {
    pub executor_filter: Option<String>,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub context_budget: Option<usize>,
    pub ttl_seconds: Option<u64>,
}

const PROMPT_PACKET_REQUIRED_GATES: [&str; 3] = [
    "organization_context_required",
    "personality_decision_required",
    "company_work_decision_required",
];

fn prompt_packet_required_gates() -> Vec<String> {
    PROMPT_PACKET_REQUIRED_GATES
        .iter()
        .map(|gate| (*gate).to_string())
        .collect()
}

fn shell_prompt_packet_gate_policy() -> ShellPromptPacketGatePolicy {
    ShellPromptPacketGatePolicy {
        schema_version: "forge.shell.prompt_packet_gate_policy.v1".to_string(),
        context_source: "forge_context_packet".to_string(),
        required_gates: prompt_packet_required_gates(),
        policy: "verify_prompt_packet_required_gates_before_brain_launch".to_string(),
        reason:
            "Forge-first brain shells must receive bounded prompt packets with organization, personality and company-work decisions before execution."
                .to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct BrainSessionLifecycleOptions<'a> {
    pub session_id: &'a str,
    pub state: &'a str,
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub origin: &'a str,
    pub note: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellSessionReceipt {
    pub schema_version: String,
    pub status: String,
    pub source: String,
    pub source_id: String,
    pub global_event_id: i64,
    pub kind: String,
    pub origin: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub executor_filter: Option<String>,
    pub launch_plan: ShellLaunchPlanReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionLifecycleReceipt {
    pub schema_version: String,
    pub status: String,
    pub source: String,
    pub source_id: String,
    pub global_event_id: i64,
    pub kind: String,
    pub origin: String,
    pub session_id: String,
    pub provider_id: String,
    pub previous_state: String,
    pub state: String,
    pub lifecycle_sequence: usize,
    pub transition: BrainSessionLifecycleTransition,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub note: Option<String>,
    pub event_recorded: bool,
    pub execution: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionsReport {
    pub schema_version: String,
    pub status: String,
    pub controller: String,
    pub selected_provider_id: Option<String>,
    pub filter: BrainSessionsFilterReport,
    pub provider_count: usize,
    pub session_count: usize,
    pub ready_session_count: usize,
    pub planned_event_count: usize,
    pub lifecycle_event_count: usize,
    pub providers: Vec<BrainProviderSessionSummary>,
    pub sessions: Vec<BrainSessionState>,
    pub recent_events: Vec<BrainSessionEventSummary>,
    pub safety_gates: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BrainSessionsReportOptions {
    pub provider_id: Option<String>,
    pub lifecycle_state: Option<String>,
    pub readiness: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionsFilterReport {
    pub provider_id: Option<String>,
    pub lifecycle_state: Option<String>,
    pub readiness: Option<String>,
    pub matched_provider_count: usize,
    pub matched_session_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainProviderSessionSummary {
    pub provider_id: String,
    pub display_name: String,
    pub provider_kind: String,
    pub status: String,
    pub selected: bool,
    pub installed: bool,
    pub configured: bool,
    pub allowed: bool,
    pub non_interactive_ready: bool,
    pub forge_first_ready: bool,
    pub session_count: usize,
    pub ready_session_count: usize,
    pub recorded_plan_count: usize,
    pub session_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionState {
    pub session_id: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub readiness: String,
    pub entry_command: Vec<String>,
    pub attachable: bool,
    pub launch_mode: String,
    pub forge_first_ready: bool,
    pub state_boundary: String,
    pub lifecycle_policy: BrainSessionLifecyclePolicy,
    pub recorded_plan_count: usize,
    pub lifecycle_state: String,
    pub lifecycle_event_count: usize,
    pub last_planned_at: Option<String>,
    pub last_origin: Option<String>,
    pub last_workflow_id: Option<String>,
    pub last_task_id: Option<String>,
    pub last_run_id: Option<String>,
    pub last_lifecycle_at: Option<String>,
    pub last_lifecycle_origin: Option<String>,
    pub last_lifecycle_note: Option<String>,
    pub operation_plan: BrainSessionOperationPlan,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionOperationPlan {
    pub schema_version: String,
    pub status: String,
    pub session_id: String,
    pub provider_id: String,
    pub lifecycle_state: String,
    pub readiness: String,
    pub recommended_action: String,
    pub lineage_complete: bool,
    pub requires_context: bool,
    pub requires_handoff: bool,
    pub requires_heartbeat: bool,
    pub commands: BrainSessionOperationCommands,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionOperationCommands {
    pub history: Vec<String>,
    pub launch_plan: Vec<String>,
    pub record_plan: Vec<String>,
    pub open: Option<Vec<String>>,
    pub attach: Option<Vec<String>>,
    pub detach: Option<Vec<String>>,
    pub close: Option<Vec<String>>,
    pub context: Option<Vec<String>>,
    pub handoff: Option<Vec<String>>,
    pub heartbeat: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionEventSummary {
    pub global_event_id: i64,
    pub kind: String,
    pub origin: String,
    pub status: String,
    pub provider_id: Option<String>,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub executor_filter: Option<String>,
    pub session_ids: Vec<String>,
    pub previous_state: Option<String>,
    pub lifecycle_state: Option<String>,
    pub lifecycle_sequence: Option<usize>,
    pub transition_kind: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionHistoryReport {
    pub schema_version: String,
    pub status: String,
    pub controller: String,
    pub session_id: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub readiness: String,
    pub current_state: String,
    pub lifecycle_policy: BrainSessionLifecyclePolicy,
    pub planned_event_count: usize,
    pub lifecycle_event_count: usize,
    pub event_count: usize,
    pub planned_events: Vec<BrainSessionEventSummary>,
    pub lifecycle_events: Vec<BrainSessionEventSummary>,
    pub events: Vec<BrainSessionEventSummary>,
    pub safety_gates: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionLifecycleTransition {
    pub schema_version: String,
    pub previous_state: String,
    pub next_state: String,
    pub transition_kind: String,
    pub allowed: bool,
    pub reason: String,
    pub allowed_next_states: Vec<String>,
    pub next_lifecycle_commands: Vec<Vec<String>>,
    pub policy: String,
    pub execution: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainSessionLifecyclePolicy {
    pub schema_version: String,
    pub current_state: String,
    pub allowed_next_states: Vec<String>,
    pub next_lifecycle_commands: Vec<Vec<String>>,
    pub policy: String,
    pub execution: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorQuotaPolicyReport {
    pub schema_version: String,
    pub selection_principle: String,
    pub decision_factors: Vec<String>,
    pub workload_routes: Vec<ExecutorQuotaWorkloadRoute>,
    pub observed_quota_evidence: Vec<ExecutorQuotaObservation>,
    pub candidates: Vec<ExecutorQuotaPolicyCandidate>,
    pub selection_trace: Vec<ExecutorSelectionTrace>,
    pub skipped_to_preserve_quota: Vec<String>,
    pub repair_goals: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorSelectionTrace {
    pub schema_version: String,
    pub executor: String,
    pub provider: String,
    pub model: Option<String>,
    pub local_vs_non_local: String,
    pub selection_tier: u32,
    pub selection_status: String,
    pub decision: String,
    pub reason: String,
    pub next_fallback_reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorQuotaWorkloadRoute {
    pub workload_class: String,
    pub default_policy: String,
    pub preferred_candidate: String,
    pub quota_spend_rule: String,
    pub quota_preservation_rule: String,
    pub business_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorQuotaObservation {
    pub executor: String,
    pub provider: String,
    pub model: Option<String>,
    pub local_vs_non_local: String,
    pub free_vs_paid_if_known: String,
    pub remaining_quota: String,
    pub rate_limit_risk: String,
    pub monetary_or_token_cost: String,
    pub latency: String,
    pub expected_quality: String,
    pub suitability: String,
    pub source: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorQuotaPolicyCandidate {
    pub executor: String,
    pub provider: String,
    pub model: Option<String>,
    pub local_vs_non_local: String,
    pub free_vs_paid_if_known: String,
    pub quota_model: String,
    pub remaining_quota: String,
    pub rate_limit_risk: String,
    pub cost_model: String,
    pub latency: String,
    pub expected_quality: String,
    pub product_business_suitability: String,
    pub fallback_risk: String,
    pub selection_tier: u32,
    pub selection_status: String,
    pub reason: String,
    pub evidence: Vec<String>,
}

#[derive(Debug)]
struct ProbeOutput {
    status: Option<ExitStatus>,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone)]
struct ExecutorDefinition {
    id: &'static str,
    display_name: &'static str,
    command: &'static str,
}

const EXECUTORS: &[ExecutorDefinition] = &[
    ExecutorDefinition {
        id: "codex",
        display_name: "Codex CLI",
        command: "codex",
    },
    ExecutorDefinition {
        id: "opencode",
        display_name: "OpenCode CLI",
        command: "opencode",
    },
    ExecutorDefinition {
        id: "gemini",
        display_name: "Gemini CLI",
        command: "gemini",
    },
    ExecutorDefinition {
        id: "claude",
        display_name: "Claude Code",
        command: "claude",
    },
    ExecutorDefinition {
        id: "ollama",
        display_name: "Ollama",
        command: "ollama",
    },
];

pub fn sync_executors(
    store: &ForgeStore,
    options: ExecutorSyncOptions,
) -> Result<ExecutorSyncReport> {
    let previous = load_previous_states(store)?;
    let allow = normalize_set(&options.allow);
    let deny = normalize_set(&options.deny);
    let mut executors = Vec::new();

    for definition in EXECUTORS {
        let mut state = probe_executor(
            definition,
            &options.home,
            &options.executor_paths,
            &options.shim_dirs,
        );
        apply_decision(&mut state, &previous, &allow, &deny, options.prompt)?;
        store.save_executor_state(&state.id, &serde_json::to_value(&state)?)?;
        executors.push(state);
    }

    let report = build_report("synced", &options.home, executors, store);
    store.record_event(
        "_system",
        "executors_synced",
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

pub fn load_executors(store: &ForgeStore) -> Result<ExecutorSyncReport> {
    let states = store
        .load_executor_states()?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<ExecutorState>, _>>()?;
    Ok(build_report("loaded", &store.base_dir(), states, store))
}

fn build_report(
    status: &str,
    home: &Path,
    mut executors: Vec<ExecutorState>,
    store: &ForgeStore,
) -> ExecutorSyncReport {
    executors.sort_by(|left, right| left.id.cmp(&right.id));
    let usable = executors
        .iter()
        .filter(|executor| {
            executor.allowed
                && executor.installed
                && executor.configured
                && executor.non_interactive_ready
        })
        .map(|executor| executor.id.clone())
        .collect::<Vec<_>>();
    let needs_human_approval = executors.iter().any(|executor| {
        executor.installed
            && executor.configured
            && !executor.allowed
            && executor.decision_source == "pending_human_approval"
    });
    let integrations = build_integrations(&executors);
    let quota_policy = build_quota_policy(&executors, store);
    let brain_router = build_brain_router(&executors, &quota_policy);

    ExecutorSyncReport {
        status: status.to_string(),
        home: home.display().to_string(),
        needs_human_approval,
        usable,
        executors,
        integrations,
        brain_router,
        quota_policy,
    }
}

fn probe_executor(
    definition: &ExecutorDefinition,
    home: &Path,
    executor_paths: &[PathBuf],
    shim_dirs: &[PathBuf],
) -> ExecutorState {
    let command_path = find_executable(definition.command, executor_paths);
    let config_evidence = config_evidence(definition.id, home);
    let configured = !config_evidence.is_empty();

    let mut non_interactive_ready = false;
    let mut probe_evidence = Vec::new();

    if let Some(ref path) = command_path {
        let (ready, evidence) = probe_non_interactive(definition.id, path);
        non_interactive_ready = ready;
        probe_evidence = evidence;
    }
    let harness_status = probe_executor_harness(definition.id, home, shim_dirs);
    let forge_first_ready = harness_status
        .as_ref()
        .is_some_and(|status| status.status == "shim_status_ready" && !status.would_recurse);
    let forge_first_entrypoint = harness_status
        .as_ref()
        .filter(|_| forge_first_ready)
        .map(|status| vec![status.shim_path.clone()]);
    if let Some(status) = &harness_status {
        probe_evidence.extend(status.evidence.clone());
    }

    ExecutorState {
        id: definition.id.to_string(),
        display_name: definition.display_name.to_string(),
        command: definition.command.to_string(),
        installed: command_path.is_some(),
        configured,
        command_path: command_path.map(|path| path.display().to_string()),
        config_evidence,
        non_interactive_ready,
        probe_evidence,
        forge_first_ready,
        forge_first_entrypoint,
        harness_status,
        allowed: false,
        decision_source: "unavailable".to_string(),
        synced_at: Utc::now().to_rfc3339(),
    }
}

fn probe_executor_harness(
    executor: &str,
    home: &Path,
    explicit_shim_dirs: &[PathBuf],
) -> Option<ExecutorHarnessStatus> {
    let mut candidates = Vec::new();
    for dir in explicit_shim_dirs {
        candidates.push((dir.clone(), true));
    }
    let default_dir = home.join(".forge/bin");
    if !candidates
        .iter()
        .any(|(dir, _)| same_path_or_display(dir, &default_dir))
    {
        candidates.push((default_dir, false));
    }

    let mut fallback = None;
    for (shim_dir, explicit) in candidates {
        let Ok(report) = inspect_cli_harness_shim_status(CliShimStatusOptions {
            shim_dir: &shim_dir,
            executor,
        }) else {
            continue;
        };
        if !report.shim_exists && !explicit {
            continue;
        }
        let harness = ExecutorHarnessStatus {
            schema_version: "forge.executor_harness_status.v1".to_string(),
            status: report.status.clone(),
            shim_dir: report.shim_dir.clone(),
            shim_path: report.shim_path.clone(),
            path_precedence: report.path_precedence.clone(),
            forge_owned: report.forge_owned,
            executable: report.executable,
            would_recurse: report.would_recurse,
            real_command: report.real_command.clone(),
            store_path: report.store_path.clone(),
            evidence: vec![
                format!("harness_status:{}", report.status),
                format!("path_precedence:{}", report.path_precedence),
                format!("forge_owned:{}", report.forge_owned),
                format!("would_recurse:{}", report.would_recurse),
            ],
        };
        if harness.status == "shim_status_ready" {
            return Some(harness);
        }
        fallback.get_or_insert(harness);
    }
    fallback
}

fn probe_non_interactive(id: &str, path: &Path) -> (bool, Vec<String>) {
    let mut evidence = Vec::new();
    let mut ready = false;

    // Probe 1: Version/Help check (smoke test)
    let args = match id {
        "gemini" | "opencode" | "codex" | "claude" | "ollama" => vec!["--version"],
        _ => vec!["--help"],
    };

    let start = Instant::now();
    let output = run_probe_command(path, &args, Duration::from_secs(2));

    match output {
        Ok(output) if output.status.is_some_and(|status| status.success()) => {
            evidence.push(format!(
                "non-interactive smoke test `{}` passed in {}ms",
                args.join(" "),
                start.elapsed().as_millis()
            ));
            ready = true;
        }
        Ok(output) if output.timed_out => {
            evidence.push(format!(
                "non-interactive smoke test `{}` timed out after {}ms",
                args.join(" "),
                start.elapsed().as_millis()
            ));
        }
        Ok(output) => {
            evidence.push(format!(
                "non-interactive smoke test `{}` failed with exit code {:?}",
                args.join(" "),
                output.status.and_then(|status| status.code())
            ));
            if !output.stderr.trim().is_empty() {
                evidence.push(format!("probe stderr: {}", output.stderr.trim()));
            }
        }
        Err(e) => {
            evidence.push(format!("failed to run non-interactive smoke test: {}", e));
        }
    }

    // Probe 2: Model/Provider check for AI executors
    if ready && (id == "gemini" || id == "opencode") {
        let (model_ready, model_evidence) = probe_model_availability(id, path);
        ready = model_ready;
        evidence.extend(model_evidence);
    }

    (ready, evidence)
}

fn probe_model_availability(id: &str, path: &Path) -> (bool, Vec<String>) {
    let mut evidence = Vec::new();
    match id {
        "gemini" => {
            // Check if GEMINI_API_KEY is set first as it's the strongest indicator of auth
            if env::var_os("GEMINI_API_KEY").is_some() || env::var_os("GOOGLE_API_KEY").is_some() {
                evidence.push("Gemini auth detected in environment".to_string());
                if let Ok(model) = std::env::var("GEMINI_MODEL") {
                    evidence.push(format!("GEMINI_MODEL is set to {} in environment", model));
                } else {
                    evidence.push("GEMINI_MODEL not set; using gemini-2.0-flash-exp for low-latency non-interactive validation".to_string());
                }
                (true, evidence)
            } else {
                evidence.push("Gemini auth (GEMINI_API_KEY) not found in environment; marking as not non-interactive ready".to_string());
                (false, evidence)
            }
        }
        "opencode" => {
            let output = run_probe_command(path, &["models"], Duration::from_secs(3));
            match output {
                Ok(output) if output.status.is_some_and(|status| status.success()) => {
                    evidence.push("opencode models listed successfully".to_string());
                    let has_non_local = output.stdout.contains("google/")
                        || output.stdout.contains("openai/")
                        || output.stdout.contains("anthropic/");
                    let has_local = output.stdout.contains("ollama/");

                    if has_non_local {
                        evidence.push("non-local models detected in opencode".to_string());
                    }
                    if has_local {
                        evidence.push("local models (ollama) detected in opencode".to_string());
                    }
                    if !has_non_local && !has_local {
                        evidence.push("no models detected in opencode output".to_string());
                        (false, evidence)
                    } else {
                        (true, evidence)
                    }
                }
                Ok(output) if output.timed_out => {
                    evidence.push(
                        "opencode models probe timed out; non-interactive provider/model readiness is not validated"
                            .to_string(),
                    );
                    (false, evidence)
                }
                Ok(output) => {
                    evidence.push(format!(
                        "failed to list opencode models with exit code {:?}; non-interactive provider/model readiness is not validated",
                        output.status.and_then(|status| status.code())
                    ));
                    if !output.stderr.trim().is_empty() {
                        evidence.push(format!("probe stderr: {}", output.stderr.trim()));
                    }
                    (false, evidence)
                }
                Err(error) => {
                    evidence.push(format!(
                        "failed to run opencode models probe: {error}; non-interactive provider/model readiness is not validated"
                    ));
                    (false, evidence)
                }
            }
        }
        _ => (true, evidence),
    }
}

fn run_probe_command(path: &Path, args: &[&str], timeout: Duration) -> Result<ProbeOutput> {
    let mut child = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    let mut timed_out = false;

    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        thread::sleep(Duration::from_millis(25));
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }

    Ok(ProbeOutput {
        status,
        timed_out,
        stdout,
        stderr,
    })
}

fn apply_decision(
    state: &mut ExecutorState,
    previous: &BTreeMap<String, ExecutorState>,
    allow: &BTreeSet<String>,
    deny: &BTreeSet<String>,
    prompt: bool,
) -> Result<()> {
    if !state.installed || !state.configured {
        state.allowed = false;
        state.decision_source = "unavailable".to_string();
        return Ok(());
    }

    if deny.contains(&state.id) {
        state.allowed = false;
        state.decision_source = "human_deny".to_string();
        return Ok(());
    }

    if allow.contains(&state.id) {
        state.allowed = true;
        state.decision_source = "human_allow".to_string();
        return Ok(());
    }

    if let Some(previous_state) = previous.get(&state.id) {
        if matches!(
            previous_state.decision_source.as_str(),
            "human_allow" | "human_deny"
        ) {
            state.allowed = previous_state.allowed;
            state.decision_source = previous_state.decision_source.clone();
            return Ok(());
        }
    }

    if prompt && io::stdin().is_terminal() {
        if prompt_for_executor(state)? {
            state.allowed = true;
            state.decision_source = "human_allow".to_string();
        } else {
            state.allowed = false;
            state.decision_source = "human_deny".to_string();
        }
        return Ok(());
    }

    state.allowed = false;
    state.decision_source = "pending_human_approval".to_string();
    Ok(())
}

fn prompt_for_executor(state: &ExecutorState) -> Result<bool> {
    print!(
        "Allow Forge to use {} ({}) as an execution engine on this machine? [y/N] ",
        state.display_name, state.command
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let normalized = answer.trim().to_lowercase();
    Ok(matches!(normalized.as_str(), "y" | "yes" | "s" | "sim"))
}

fn load_previous_states(store: &ForgeStore) -> Result<BTreeMap<String, ExecutorState>> {
    let mut previous = BTreeMap::new();
    for value in store.load_executor_states()? {
        let state: ExecutorState = serde_json::from_value(value)?;
        previous.insert(state.id.clone(), state);
    }
    Ok(previous)
}

fn normalize_set(values: &[String]) -> BTreeSet<String> {
    values.iter().map(|value| value.to_lowercase()).collect()
}

fn find_executable(command: &str, executor_paths: &[PathBuf]) -> Option<PathBuf> {
    candidate_dirs(executor_paths)
        .into_iter()
        .map(|directory| directory.join(command))
        .find(|path| is_executable(path))
}

fn candidate_dirs(executor_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = executor_paths.to_vec();
    if let Some(paths) = env::var_os("PATH") {
        dirs.extend(env::split_paths(&paths));
    }
    dirs
}

fn same_path_or_display(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn config_evidence(id: &str, home: &Path) -> Vec<String> {
    let mut evidence = Vec::new();
    for path in config_candidates(id, home) {
        if path.exists() {
            evidence.push(path.display().to_string());
        }
    }

    match id {
        "gemini" => {
            if env::var_os("GEMINI_API_KEY").is_some() {
                evidence.push("env:GEMINI_API_KEY".to_string());
            }
            if env::var_os("GOOGLE_API_KEY").is_some() {
                evidence.push("env:GOOGLE_API_KEY".to_string());
            }
        }
        "claude" => {
            if env::var_os("ANTHROPIC_API_KEY").is_some() {
                evidence.push("env:ANTHROPIC_API_KEY".to_string());
            }
        }
        "ollama" => {
            if env::var_os("OLLAMA_HOST").is_some() {
                evidence.push("env:OLLAMA_HOST".to_string());
            }
        }
        _ => {}
    }

    evidence
}

fn config_candidates(id: &str, home: &Path) -> Vec<PathBuf> {
    match id {
        "codex" => vec![home.join(".codex/config.toml"), home.join(".codex")],
        "opencode" => vec![
            home.join(".config/opencode"),
            home.join(".opencode"),
            home.join(".agents/skills/forge-core/SKILL.md"),
        ],
        "gemini" => vec![
            home.join(".gemini/settings.json"),
            home.join(".gemini"),
            home.join(".config/gemini"),
        ],
        "claude" => vec![home.join(".claude"), home.join(".config/claude")],
        "ollama" => vec![home.join(".ollama")],
        _ => Vec::new(),
    }
}

fn build_integrations(executors: &[ExecutorState]) -> Vec<ExecutorIntegration> {
    let codex_allowed = executor_is_allowed(executors, "codex");
    let opencode_allowed = executor_is_allowed(executors, "opencode");
    let enabled = codex_allowed && opencode_allowed;
    vec![ExecutorIntegration {
        id: "opencode_codex_bridge".to_string(),
        from: "opencode".to_string(),
        to: "codex".to_string(),
        kind: "delegated_cli_executor".to_string(),
        enabled,
        reason: if enabled {
            "opencode and codex are both authorized; Forge may route bounded tasks through either executor and record the bridge in workflow policy"
                .to_string()
        } else {
            "requires both opencode and codex to be installed, configured and human-authorized"
                .to_string()
        },
    }]
}

fn build_brain_router(
    executors: &[ExecutorState],
    quota_policy: &ExecutorQuotaPolicyReport,
) -> BrainRouterReport {
    let selected_brain = quota_policy
        .selection_trace
        .iter()
        .find(|candidate| candidate.decision == "select")
        .map(|candidate| candidate.executor.clone());
    let mut brains = executors
        .iter()
        .map(brain_candidate)
        .collect::<Vec<BrainCandidate>>();
    brains.sort_by(|left, right| left.id.cmp(&right.id));
    let mut safety_gates = vec![
        "sync_executors_before_handoff".to_string(),
        "human_authorization_for_external_cli_use".to_string(),
        "forge_context_packet_required_before_ai_handoff".to_string(),
    ];
    safety_gates.extend(prompt_packet_required_gates());
    safety_gates.extend([
        "credential_vault_secrets_never_printed".to_string(),
        "validation_or_final_audit_required_before_claiming_completion".to_string(),
    ]);

    BrainRouterReport {
        schema_version: "forge.brain_router.v1".to_string(),
        controller: "forge".to_string(),
        controller_role: "orchestration_control_plane".to_string(),
        orchestrator_brain: "forge".to_string(),
        brain_role: "replaceable_execution_brain".to_string(),
        node_brain_role: "per_node_agentic_execution_brain".to_string(),
        routing_principle:
            "Forge owns memory, skills, MCP routing, context, workflow state, shell/session lifecycle, permissions, cost policy and validation; external CLIs only execute bounded brain work."
                .to_string(),
        node_brain_routing_policy:
            "Each AI or mixed workflow node may declare its own Forge-owned node_brain_routing contract with one or more agent slots, different brains per slot, and multiple agents on the same brain."
                .to_string(),
        parallel_agent_policy:
            "Forge may lease and run independent AI node agent slots in parallel when dependencies, context budgets, quota and validation gates allow it."
                .to_string(),
        hot_swap_policy:
            "A workflow run can switch the active execution brain through forge request switch-executor without changing run id, workflow id, checkpoints or user directives. One AI/mixed workflow node can mutate its own node_brain_routing through forge workflow update-node-brain while the workflow remains active."
                .to_string(),
        selected_brain,
        forge_controlled_surfaces: vec![
            "workflow_graph".to_string(),
            "memory".to_string(),
            "skills".to_string(),
            "mcp_servers_and_tools".to_string(),
            "credential_vault_references".to_string(),
            "context_packets".to_string(),
            "artifact_lineage".to_string(),
            "shell_session_lifecycle".to_string(),
            "permissions".to_string(),
            "cost_and_quota_policy".to_string(),
            "validation_gates".to_string(),
            "self_improvement_decisions".to_string(),
        ],
        brain_owned_surfaces: vec![
            "reasoning_for_assigned_task".to_string(),
            "bounded_code_or_text_proposals".to_string(),
            "child_process_execution_when_authorized_by_forge".to_string(),
        ],
        shell_sessions: brain_shell_sessions(&brains),
        safety_gates,
        brains,
    }
}

fn brain_candidate(executor: &ExecutorState) -> BrainCandidate {
    let status = if executor.allowed && executor.installed && executor.configured {
        if executor.non_interactive_ready {
            "ready"
        } else {
            "interactive_or_auth_blocked"
        }
    } else if !executor.installed {
        "not_installed"
    } else if !executor.configured {
        "not_configured"
    } else if !executor.allowed {
        "not_authorized"
    } else {
        "unknown"
    };
    let reason = match status {
        "ready" => "installed, configured, human-authorized and non-interactive smoke test passed",
        "interactive_or_auth_blocked" => {
            "installed/configured/authorized, but Forge has not validated safe non-interactive execution"
        }
        "not_installed" => "command is not available on PATH or configured executor paths",
        "not_configured" => "Forge did not find CLI configuration or required environment evidence",
        "not_authorized" => "human authorization is required before Forge may use this brain adapter",
        _ => "brain adapter state is unknown",
    };

    BrainCandidate {
        id: executor.id.clone(),
        display_name: executor.display_name.clone(),
        command: executor.command.clone(),
        status: status.to_string(),
        execution_mode: brain_execution_mode(&executor.id).to_string(),
        session_role: "execution_brain_adapter".to_string(),
        persistent_state_owner: "forge".to_string(),
        context_source: "forge_context_packet".to_string(),
        memory_source: "forge_memory_router".to_string(),
        skills_source: "forge_skill_router".to_string(),
        mcp_source: "forge_mcp_router".to_string(),
        installed: executor.installed,
        configured: executor.configured,
        allowed: executor.allowed,
        non_interactive_ready: executor.non_interactive_ready,
        forge_first_ready: executor.forge_first_ready,
        forge_first_entrypoint: executor.forge_first_entrypoint.clone(),
        harness_status: executor.harness_status.clone(),
        shell_entrypoints: brain_shell_entrypoints(executor),
        reason: reason.to_string(),
    }
}

fn brain_execution_mode(id: &str) -> &'static str {
    match id {
        "codex" | "opencode" | "gemini" | "claude" => "external_cli_brain",
        "ollama" => "local_model_runtime",
        _ => "custom_execution_brain",
    }
}

fn brain_shell_entrypoints(executor: &ExecutorState) -> Vec<Vec<String>> {
    let mut entrypoints = Vec::new();
    if let Some(entrypoint) = executor
        .forge_first_entrypoint
        .as_ref()
        .filter(|_| executor.forge_first_ready)
    {
        entrypoints.push(entrypoint.clone());
    }
    entrypoints.extend(match executor.id.as_str() {
        "opencode" => vec![
            vec!["opencode".to_string()],
            vec![
                "opencode".to_string(),
                "attach".to_string(),
                "<url>".to_string(),
            ],
        ],
        "gemini" => vec![
            vec!["gemini".to_string()],
            vec![
                "gemini".to_string(),
                "-p".to_string(),
                "<prompt>".to_string(),
            ],
        ],
        "claude" => vec![
            vec!["claude".to_string()],
            vec![
                "claude".to_string(),
                "-p".to_string(),
                "<prompt>".to_string(),
            ],
        ],
        "codex" => vec![vec!["codex".to_string()]],
        "ollama" => vec![vec![
            "ollama".to_string(),
            "run".to_string(),
            "<model>".to_string(),
        ]],
        _ => vec![vec![executor.command.clone()]],
    });
    entrypoints
}

fn brain_shell_sessions(brains: &[BrainCandidate]) -> Vec<BrainShellSessionSpec> {
    let mut sessions = vec![BrainShellSessionSpec {
        id: "forge-tui".to_string(),
        brain_id: "forge".to_string(),
        entry_command: vec!["forge".to_string()],
        attachable: true,
        launch_mode: "forge_control_tui".to_string(),
        forge_first_ready: true,
        forge_first_entrypoint: Some(vec!["forge".to_string()]),
        role: "primary_control_tui".to_string(),
        state_boundary: "Forge owns workflow state, memory, skills, MCP routing and shell lifecycle."
            .to_string(),
        safety_note:
            "Use this as the default human operation surface; external brains should be launched from Forge-controlled handoffs."
                .to_string(),
    }];

    for brain in brains {
        if let Some(entry_command) = brain.shell_entrypoints.first() {
            sessions.push(BrainShellSessionSpec {
                id: format!("{}-shell", brain.id),
                brain_id: brain.id.clone(),
                entry_command: entry_command.clone(),
                attachable: brain.installed,
                launch_mode: if brain.forge_first_ready {
                    "forge_first_harness"
                } else {
                    "native_cli"
                }
                .to_string(),
                forge_first_ready: brain.forge_first_ready,
                forge_first_entrypoint: brain.forge_first_entrypoint.clone(),
                role: "execution_brain_shell".to_string(),
                state_boundary:
                    "External CLI session is an execution surface only; Forge remains the source of truth for memory, skills, MCPs, context and workflow lineage."
                        .to_string(),
                safety_note:
                    "Open directly only for inspection/debugging; production handoff should go through Forge permissions, context packets and validation gates."
                        .to_string(),
            });
        }
    }

    sessions
}

pub fn build_shell_launch_plan(
    router: &BrainRouterReport,
    options: ShellLaunchPlanOptions,
) -> ShellLaunchPlanReport {
    let normalized_filter = options
        .executor_filter
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let context_budget = options.context_budget.unwrap_or(1200);
    let ttl_seconds = options.ttl_seconds.unwrap_or(900);
    let launch_plans = router
        .shell_sessions
        .iter()
        .filter(|session| match normalized_filter.as_deref() {
            Some(filter) => {
                session.brain_id.eq_ignore_ascii_case(filter)
                    || session.id.eq_ignore_ascii_case(filter)
                    || session
                        .id
                        .strip_suffix("-shell")
                        .is_some_and(|id| id.eq_ignore_ascii_case(filter))
            }
            None => true,
        })
        .map(|session| {
            let brain = router
                .brains
                .iter()
                .find(|candidate| candidate.id == session.brain_id);
            let harness_status = brain.and_then(|candidate| candidate.harness_status.clone());
            let context_command = shell_context_command(&options, context_budget);
            let handoff_command =
                shell_handoff_command(session, &options, context_budget, ttl_seconds);
            let heartbeat_command = shell_heartbeat_command(session, &options, ttl_seconds);
            ShellLaunchPlan {
                session_id: session.id.clone(),
                brain_id: session.brain_id.clone(),
                readiness: shell_session_readiness(session, brain).to_string(),
                entry_command: session.entry_command.clone(),
                attachable: session.attachable,
                launch_mode: session.launch_mode.clone(),
                forge_first_ready: session.forge_first_ready,
                forge_first_entrypoint: session.forge_first_entrypoint.clone(),
                harness_status: harness_status.clone(),
                prompt_packet_gate_policy: shell_prompt_packet_gate_policy(),
                dry_run: true,
                execution_boundary: "plan_only_no_child_process_started".to_string(),
                context_command: context_command.clone(),
                handoff_command: handoff_command.clone(),
                heartbeat_command: heartbeat_command.clone(),
                preflight_commands: shell_preflight_commands(
                    session,
                    harness_status.as_ref(),
                    context_command.as_ref(),
                    handoff_command.as_ref(),
                ),
                state_boundary: session.state_boundary.clone(),
                safety_note: session.safety_note.clone(),
                next_actions: shell_next_actions(session),
            }
        })
        .collect::<Vec<ShellLaunchPlan>>();
    let status = if launch_plans.is_empty() {
        "not_found"
    } else if launch_plans.iter().any(|plan| plan.readiness == "ready") {
        "ready"
    } else {
        "needs_attention"
    };

    ShellLaunchPlanReport {
        schema_version: "forge.shell_launch_plan.v1".to_string(),
        status: status.to_string(),
        controller: router.controller.clone(),
        executor_filter: normalized_filter,
        workflow_id: options.workflow_id,
        task_id: options.task_id,
        run_id: options.run_id,
        context_budget,
        ttl_seconds,
        execution: "plan_only".to_string(),
        launch_plans,
        safety_gates: router.safety_gates.clone(),
        next_actions: vec![
            "Sync executors after PATH, credential or shim changes before trusting a shell plan."
                .to_string(),
            "Acquire a Forge task handoff before using an external brain for production work."
                .to_string(),
            "Record validation or final-audit evidence before claiming task completion."
                .to_string(),
        ],
    }
}

pub fn record_shell_session_plan(
    store: &ForgeStore,
    router: &BrainRouterReport,
    options: ShellLaunchPlanOptions,
    origin: &str,
) -> Result<ShellSessionReceipt> {
    let launch_plan = build_shell_launch_plan(router, options);
    let data = serde_json::json!({
        "schema_version": "forge.shell_session_event.v1",
        "status": "shell_session_plan_recorded",
        "run_id": launch_plan.run_id,
        "task_id": launch_plan.task_id,
        "executor_filter": launch_plan.executor_filter,
        "context_budget": launch_plan.context_budget,
        "ttl_seconds": launch_plan.ttl_seconds,
        "launch_plan": launch_plan,
    });
    let source_id = format!(
        "shell_{}",
        &hex_sha256(serde_json::to_string(&data)?.as_bytes())[..16]
    );
    let tenant_context = shell_session_tenant_context(store, launch_plan.workflow_id.as_deref())?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "forge_shell",
        source_id: &source_id,
        workflow_id: launch_plan.workflow_id.as_deref(),
        kind: "shell_launch_planned",
        origin,
        status: "planned",
        data: &data,
        tenant_context: &tenant_context,
    })?;

    Ok(ShellSessionReceipt {
        schema_version: "forge.shell_session_receipt.v1".to_string(),
        status: "shell_session_plan_recorded".to_string(),
        source: "forge_shell".to_string(),
        source_id,
        global_event_id,
        kind: "shell_launch_planned".to_string(),
        origin: origin.to_string(),
        workflow_id: launch_plan.workflow_id.clone(),
        task_id: launch_plan.task_id.clone(),
        run_id: launch_plan.run_id.clone(),
        executor_filter: launch_plan.executor_filter.clone(),
        launch_plan,
    })
}

pub fn record_brain_session_lifecycle(
    store: &ForgeStore,
    router: &BrainRouterReport,
    options: BrainSessionLifecycleOptions<'_>,
) -> Result<BrainSessionLifecycleReceipt> {
    let normalized_state = normalize_brain_session_lifecycle_state(options.state)?;
    let session = router
        .shell_sessions
        .iter()
        .find(|session| session.id == options.session_id)
        .ok_or_else(|| anyhow::anyhow!("unknown Forge shell session: {}", options.session_id))?;
    let previous_events = brain_session_lifecycle_events(store, &session.id)?;
    let previous_state = previous_events
        .first()
        .and_then(|event| event.lifecycle_state.clone())
        .unwrap_or_else(|| "untracked".to_string());
    let transition =
        brain_session_lifecycle_transition(&session.id, &previous_state, normalized_state);
    if !transition.allowed {
        anyhow::bail!(
            "invalid brain session lifecycle transition for {}: {} -> {}; allowed next states: {}",
            session.id,
            previous_state,
            normalized_state,
            transition.allowed_next_states.join(", ")
        );
    }
    let lifecycle_sequence = previous_events.len() + 1;
    let data = serde_json::json!({
        "schema_version": "forge.brain_session_lifecycle_event.v1",
        "status": "brain_session_lifecycle_recorded",
        "session_id": session.id,
        "provider_id": session.brain_id,
        "previous_state": previous_state,
        "state": normalized_state,
        "lifecycle_sequence": lifecycle_sequence,
        "transition": transition,
        "workflow_id": options.workflow_id,
        "task_id": options.task_id,
        "run_id": options.run_id,
        "origin": options.origin,
        "note": options.note,
        "execution": "audit_only",
    });
    let source_id = format!(
        "session_{}",
        &hex_sha256(serde_json::to_string(&data)?.as_bytes())[..16]
    );
    let tenant_context = shell_session_tenant_context(store, options.workflow_id)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "forge_session",
        source_id: &source_id,
        workflow_id: options.workflow_id,
        kind: "brain_session_lifecycle",
        origin: options.origin,
        status: normalized_state,
        data: &data,
        tenant_context: &tenant_context,
    })?;

    Ok(BrainSessionLifecycleReceipt {
        schema_version: "forge.brain_session_lifecycle.v1".to_string(),
        status: "brain_session_lifecycle_recorded".to_string(),
        source: "forge_session".to_string(),
        source_id,
        global_event_id,
        kind: "brain_session_lifecycle".to_string(),
        origin: options.origin.to_string(),
        session_id: session.id.clone(),
        provider_id: session.brain_id.clone(),
        previous_state,
        state: normalized_state.to_string(),
        lifecycle_sequence,
        transition,
        workflow_id: options.workflow_id.map(str::to_string),
        task_id: options.task_id.map(str::to_string),
        run_id: options.run_id.map(str::to_string),
        note: options.note.map(str::to_string),
        event_recorded: true,
        execution: "audit_only".to_string(),
    })
}

pub fn build_brain_sessions_report(
    store: &ForgeStore,
    router: &BrainRouterReport,
) -> Result<BrainSessionsReport> {
    build_brain_sessions_report_with_options(store, router, BrainSessionsReportOptions::default())
}

pub fn build_brain_sessions_report_with_options(
    store: &ForgeStore,
    router: &BrainRouterReport,
    options: BrainSessionsReportOptions,
) -> Result<BrainSessionsReport> {
    let filter_provider_id = normalize_optional_filter(options.provider_id.as_deref());
    let filter_lifecycle_state = normalize_brain_session_filter_state(options.lifecycle_state)?;
    let filter_readiness = normalize_optional_filter(options.readiness.as_deref());
    let mut events = store
        .load_global_events()?
        .into_iter()
        .filter(|event| {
            (event.source == "forge_shell" && event.kind == "shell_launch_planned")
                || (event.source == "forge_session" && event.kind == "brain_session_lifecycle")
        })
        .map(brain_session_event_summary)
        .collect::<Vec<_>>();
    events.sort_by(|left, right| right.global_event_id.cmp(&left.global_event_id));

    let mut sessions = router
        .shell_sessions
        .iter()
        .map(|session| {
            let brain = router
                .brains
                .iter()
                .find(|candidate| candidate.id == session.brain_id);
            brain_session_state(session, brain, &events)
        })
        .collect::<Vec<_>>();
    sessions.retain(|session| {
        filter_provider_id
            .as_deref()
            .is_none_or(|provider_id| session.provider_id == provider_id)
            && filter_lifecycle_state
                .as_deref()
                .is_none_or(|state| session.lifecycle_state == state)
            && filter_readiness
                .as_deref()
                .is_none_or(|readiness| session.readiness == readiness)
    });
    let visible_session_ids = sessions
        .iter()
        .map(|session| session.session_id.clone())
        .collect::<BTreeSet<_>>();
    let visible_events = events
        .into_iter()
        .filter(|event| {
            event
                .session_ids
                .iter()
                .any(|session_id| visible_session_ids.contains(session_id))
        })
        .collect::<Vec<_>>();
    let planned_event_count = visible_events
        .iter()
        .filter(|event| event.kind == "shell_launch_planned")
        .count();
    let lifecycle_event_count = visible_events
        .iter()
        .filter(|event| event.kind == "brain_session_lifecycle")
        .count();
    let filter_active = filter_provider_id.is_some()
        || filter_lifecycle_state.is_some()
        || filter_readiness.is_some();
    let mut providers = brain_provider_session_summaries(router, &sessions);
    if filter_active {
        providers.retain(|provider| provider.session_count > 0);
    }
    let ready_session_count = sessions
        .iter()
        .filter(|session| session.readiness == "ready")
        .count();
    let status = if sessions.is_empty() {
        "empty"
    } else {
        "loaded"
    };

    Ok(BrainSessionsReport {
        schema_version: "forge.brain_sessions.v1".to_string(),
        status: status.to_string(),
        controller: router.controller.clone(),
        selected_provider_id: router.selected_brain.clone(),
        filter: BrainSessionsFilterReport {
            provider_id: filter_provider_id,
            lifecycle_state: filter_lifecycle_state,
            readiness: filter_readiness,
            matched_provider_count: providers.len(),
            matched_session_count: sessions.len(),
        },
        provider_count: providers.len(),
        session_count: sessions.len(),
        ready_session_count,
        planned_event_count,
        lifecycle_event_count,
        providers,
        sessions,
        recent_events: visible_events.into_iter().take(20).collect(),
        safety_gates: router.safety_gates.clone(),
        next_actions: vec![
            "Use forge shells --record-session before handing a shell to a human or external brain."
                .to_string(),
            "Use forge sessions lifecycle --session <id> --state opened|attached|closed to keep shell lifecycle auditable."
                .to_string(),
            "Use forge request switch-executor to hot-swap an active run provider without losing workflow lineage."
                .to_string(),
            "Use forge workflow update-node-brain to change provider routing for one AI or mixed node while the workflow remains active."
                .to_string(),
        ],
    })
}

pub fn build_brain_session_history_report(
    store: &ForgeStore,
    router: &BrainRouterReport,
    session_id: &str,
) -> Result<BrainSessionHistoryReport> {
    let session = router
        .shell_sessions
        .iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| anyhow::anyhow!("unknown Forge shell session: {session_id}"))?;
    let brain = router
        .brains
        .iter()
        .find(|candidate| candidate.id == session.brain_id);
    let mut events = store
        .load_global_events()?
        .into_iter()
        .filter(|event| {
            (event.source == "forge_shell" && event.kind == "shell_launch_planned")
                || (event.source == "forge_session" && event.kind == "brain_session_lifecycle")
        })
        .map(brain_session_event_summary)
        .filter(|event| event.session_ids.iter().any(|id| id == session_id))
        .collect::<Vec<_>>();
    events.sort_by(|left, right| left.global_event_id.cmp(&right.global_event_id));
    let planned_events = events
        .iter()
        .filter(|event| event.kind == "shell_launch_planned")
        .cloned()
        .collect::<Vec<_>>();
    let lifecycle_events = events
        .iter()
        .filter(|event| event.kind == "brain_session_lifecycle")
        .cloned()
        .collect::<Vec<_>>();
    let current_state = lifecycle_events
        .last()
        .and_then(|event| event.lifecycle_state.clone())
        .unwrap_or_else(|| "untracked".to_string());
    let status = if events.is_empty() { "empty" } else { "loaded" };

    Ok(BrainSessionHistoryReport {
        schema_version: "forge.brain_session_history.v1".to_string(),
        status: status.to_string(),
        controller: router.controller.clone(),
        session_id: session.id.clone(),
        provider_id: session.brain_id.clone(),
        provider_kind: brain
            .map(|candidate| candidate.execution_mode.clone())
            .unwrap_or_else(|| "forge_control_plane".to_string()),
        readiness: shell_session_readiness(session, brain).to_string(),
        current_state: current_state.clone(),
        lifecycle_policy: brain_session_lifecycle_policy(&session.id, &current_state),
        planned_event_count: planned_events.len(),
        lifecycle_event_count: lifecycle_events.len(),
        event_count: events.len(),
        planned_events,
        lifecycle_events,
        events,
        safety_gates: router.safety_gates.clone(),
        next_actions: vec![
            "Use lifecycle_policy.next_lifecycle_commands for the next audited state change."
                .to_string(),
            "Use forge sessions --provider <id> --state <state> when an operator needs a lane view."
                .to_string(),
            "Use forge shells --record-session before handing a fresh shell to an external brain."
                .to_string(),
        ],
    })
}

fn brain_session_event_summary(
    event: crate::storage::StoredGlobalEventRecord,
) -> BrainSessionEventSummary {
    let launch_plan = &event.data["launch_plan"];
    let session_ids = if event.kind == "shell_launch_planned" {
        launch_plan["launch_plans"]
            .as_array()
            .map(|plans| {
                plans
                    .iter()
                    .filter_map(|plan| plan["session_id"].as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        event
            .data
            .get("session_id")
            .and_then(|value| value.as_str())
            .map(|value| vec![value.to_string()])
            .unwrap_or_default()
    };

    BrainSessionEventSummary {
        global_event_id: event.id,
        kind: event.kind,
        origin: event.origin,
        status: event.status,
        provider_id: event.data["provider_id"].as_str().map(str::to_string),
        workflow_id: event.workflow_id,
        task_id: event.data["task_id"].as_str().map(str::to_string),
        run_id: event.data["run_id"].as_str().map(str::to_string),
        executor_filter: event.data["executor_filter"].as_str().map(str::to_string),
        session_ids,
        previous_state: event.data["previous_state"].as_str().map(str::to_string),
        lifecycle_state: event.data["state"].as_str().map(str::to_string),
        lifecycle_sequence: event
            .data
            .get("lifecycle_sequence")
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok()),
        transition_kind: event.data["transition"]["transition_kind"]
            .as_str()
            .map(str::to_string),
        note: event.data["note"].as_str().map(str::to_string),
        created_at: event.created_at,
    }
}

fn normalize_brain_session_lifecycle_state(state: &str) -> Result<&'static str> {
    match state.trim().to_ascii_lowercase().as_str() {
        "opened" | "open" => Ok("opened"),
        "attached" | "attach" => Ok("attached"),
        "detached" | "detach" => Ok("detached"),
        "closed" | "close" => Ok("closed"),
        "failed" | "failure" => Ok("failed"),
        "abandoned" | "abandon" => Ok("abandoned"),
        other => Err(anyhow::anyhow!(
            "unsupported brain session lifecycle state '{other}'; use opened, attached, detached, closed, failed or abandoned"
        )),
    }
}

fn normalize_optional_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("all"))
        .map(|value| value.to_ascii_lowercase())
}

fn normalize_brain_session_filter_state(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = normalize_optional_filter(value.as_deref()) else {
        return Ok(None);
    };
    let normalized = value.replace('_', "-");
    if normalized == "untracked" {
        return Ok(Some(normalized));
    }
    Ok(Some(
        normalize_brain_session_lifecycle_state(&normalized)?.to_string(),
    ))
}

fn brain_session_lifecycle_events(
    store: &ForgeStore,
    session_id: &str,
) -> Result<Vec<BrainSessionEventSummary>> {
    let mut events = store
        .load_global_events()?
        .into_iter()
        .filter(|event| event.source == "forge_session" && event.kind == "brain_session_lifecycle")
        .map(brain_session_event_summary)
        .filter(|event| event.session_ids.iter().any(|id| id == session_id))
        .collect::<Vec<_>>();
    events.sort_by(|left, right| right.global_event_id.cmp(&left.global_event_id));
    Ok(events)
}

fn brain_session_lifecycle_transition(
    session_id: &str,
    previous_state: &str,
    next_state: &str,
) -> BrainSessionLifecycleTransition {
    let allowed_next_states = allowed_brain_session_next_states(previous_state)
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let idempotent = previous_state == next_state;
    let allowed = idempotent || allowed_next_states.iter().any(|state| state == next_state);
    let transition_kind = if idempotent {
        "idempotent"
    } else if previous_state == "untracked" {
        "bootstrap"
    } else {
        "state_change"
    };
    let reason = if allowed {
        if idempotent {
            format!("session {session_id} already reports lifecycle state {next_state}")
        } else {
            format!("session {session_id} can transition from {previous_state} to {next_state}")
        }
    } else {
        format!("session {session_id} cannot transition from {previous_state} to {next_state}")
    };

    BrainSessionLifecycleTransition {
        schema_version: "forge.brain_session_transition_policy.v1".to_string(),
        previous_state: previous_state.to_string(),
        next_state: next_state.to_string(),
        transition_kind: transition_kind.to_string(),
        allowed,
        reason,
        next_lifecycle_commands: lifecycle_transition_commands(session_id, &allowed_next_states),
        allowed_next_states,
        policy: "forge_owned_ordered_shell_lifecycle".to_string(),
        execution: "audit_only_no_child_process".to_string(),
    }
}

fn brain_session_lifecycle_policy(
    session_id: &str,
    current_state: &str,
) -> BrainSessionLifecyclePolicy {
    let allowed_next_states = allowed_brain_session_next_states(current_state)
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    BrainSessionLifecyclePolicy {
        schema_version: "forge.brain_session_transition_policy.v1".to_string(),
        current_state: current_state.to_string(),
        next_lifecycle_commands: lifecycle_transition_commands(session_id, &allowed_next_states),
        allowed_next_states,
        policy: "forge_owned_ordered_shell_lifecycle".to_string(),
        execution: "audit_only_no_child_process".to_string(),
    }
}

fn allowed_brain_session_next_states(current_state: &str) -> Vec<&'static str> {
    match current_state {
        "untracked" => vec!["opened", "attached", "failed", "abandoned"],
        "opened" => vec!["attached", "closed", "failed", "abandoned"],
        "attached" => vec!["detached", "closed", "failed", "abandoned"],
        "detached" => vec!["attached", "closed", "failed", "abandoned"],
        "closed" => vec!["opened"],
        "failed" => vec!["opened", "abandoned"],
        "abandoned" => vec!["opened"],
        _ => vec!["opened"],
    }
}

fn lifecycle_transition_commands(session_id: &str, states: &[String]) -> Vec<Vec<String>> {
    states
        .iter()
        .map(|state| {
            vec![
                "forge".to_string(),
                "sessions".to_string(),
                "lifecycle".to_string(),
                "--session".to_string(),
                session_id.to_string(),
                "--state".to_string(),
                state.clone(),
                "--output".to_string(),
                "json".to_string(),
            ]
        })
        .collect()
}

fn brain_session_state(
    session: &BrainShellSessionSpec,
    brain: Option<&BrainCandidate>,
    events: &[BrainSessionEventSummary],
) -> BrainSessionState {
    let planned_events = events
        .iter()
        .filter(|event| {
            event.kind == "shell_launch_planned"
                && event.session_ids.iter().any(|id| id == &session.id)
        })
        .collect::<Vec<_>>();
    let lifecycle_events = events
        .iter()
        .filter(|event| {
            event.kind == "brain_session_lifecycle"
                && event.session_ids.iter().any(|id| id == &session.id)
        })
        .collect::<Vec<_>>();
    let last_event = planned_events.first().copied();
    let last_lifecycle_event = lifecycle_events.first().copied();
    let lifecycle_state = last_lifecycle_event
        .and_then(|event| event.lifecycle_state.clone())
        .unwrap_or_else(|| "untracked".to_string());
    let readiness = shell_session_readiness(session, brain).to_string();
    let operation_plan =
        brain_session_operation_plan(session, &readiness, &lifecycle_state, last_event);
    BrainSessionState {
        session_id: session.id.clone(),
        provider_id: session.brain_id.clone(),
        provider_kind: brain
            .map(|candidate| candidate.execution_mode.clone())
            .unwrap_or_else(|| "forge_control_plane".to_string()),
        readiness,
        entry_command: session.entry_command.clone(),
        attachable: session.attachable,
        launch_mode: session.launch_mode.clone(),
        forge_first_ready: session.forge_first_ready,
        state_boundary: session.state_boundary.clone(),
        lifecycle_policy: brain_session_lifecycle_policy(&session.id, &lifecycle_state),
        recorded_plan_count: planned_events.len(),
        lifecycle_state,
        lifecycle_event_count: lifecycle_events.len(),
        last_planned_at: last_event.map(|event| event.created_at.clone()),
        last_origin: last_event.map(|event| event.origin.clone()),
        last_workflow_id: last_event.and_then(|event| event.workflow_id.clone()),
        last_task_id: last_event.and_then(|event| event.task_id.clone()),
        last_run_id: last_event.and_then(|event| event.run_id.clone()),
        last_lifecycle_at: last_lifecycle_event.map(|event| event.created_at.clone()),
        last_lifecycle_origin: last_lifecycle_event.map(|event| event.origin.clone()),
        last_lifecycle_note: last_lifecycle_event.and_then(|event| event.note.clone()),
        operation_plan,
        next_actions: shell_next_actions(session),
    }
}

fn brain_provider_session_summaries(
    router: &BrainRouterReport,
    sessions: &[BrainSessionState],
) -> Vec<BrainProviderSessionSummary> {
    let mut providers = Vec::new();
    providers.push(forge_provider_session_summary(router, sessions));
    providers.extend(router.brains.iter().map(|brain| {
        let provider_sessions = sessions
            .iter()
            .filter(|session| session.provider_id == brain.id)
            .collect::<Vec<_>>();
        BrainProviderSessionSummary {
            provider_id: brain.id.clone(),
            display_name: brain.display_name.clone(),
            provider_kind: brain.execution_mode.clone(),
            status: brain.status.clone(),
            selected: router.selected_brain.as_deref() == Some(brain.id.as_str()),
            installed: brain.installed,
            configured: brain.configured,
            allowed: brain.allowed,
            non_interactive_ready: brain.non_interactive_ready,
            forge_first_ready: brain.forge_first_ready,
            session_count: provider_sessions.len(),
            ready_session_count: provider_sessions
                .iter()
                .filter(|session| session.readiness == "ready")
                .count(),
            recorded_plan_count: provider_sessions
                .iter()
                .map(|session| session.recorded_plan_count)
                .sum(),
            session_ids: provider_sessions
                .iter()
                .map(|session| session.session_id.clone())
                .collect(),
            reason: brain.reason.clone(),
        }
    }));
    providers
}

fn forge_provider_session_summary(
    router: &BrainRouterReport,
    sessions: &[BrainSessionState],
) -> BrainProviderSessionSummary {
    let provider_sessions = sessions
        .iter()
        .filter(|session| session.provider_id == "forge")
        .collect::<Vec<_>>();
    BrainProviderSessionSummary {
        provider_id: "forge".to_string(),
        display_name: "Forge Control Plane".to_string(),
        provider_kind: "forge_control_plane".to_string(),
        status: "ready".to_string(),
        selected: router.selected_brain.as_deref() == Some("forge"),
        installed: true,
        configured: true,
        allowed: true,
        non_interactive_ready: true,
        forge_first_ready: true,
        session_count: provider_sessions.len(),
        ready_session_count: provider_sessions
            .iter()
            .filter(|session| session.readiness == "ready")
            .count(),
        recorded_plan_count: provider_sessions
            .iter()
            .map(|session| session.recorded_plan_count)
            .sum(),
        session_ids: provider_sessions
            .iter()
            .map(|session| session.session_id.clone())
            .collect(),
        reason: "Forge owns orchestration, workflow state, context, memory, skills, MCP routing and shell lifecycle.".to_string(),
    }
}

fn shell_session_tenant_context(
    store: &ForgeStore,
    workflow_id: Option<&str>,
) -> Result<serde_json::Value> {
    if let Some(workflow_id) = workflow_id {
        if let Ok(workflow) = store.load_workflow(workflow_id) {
            return Ok(serde_json::to_value(&workflow.intent.operating_context)?);
        }
    }
    Ok(serde_json::to_value(OperatingContextSpec::default())?)
}

fn shell_session_readiness(
    session: &BrainShellSessionSpec,
    brain: Option<&BrainCandidate>,
) -> &'static str {
    if session.brain_id == "forge" {
        return "ready";
    }
    if !session.attachable {
        return "not_installed";
    }
    if session.forge_first_ready {
        return "ready";
    }
    if brain
        .is_some_and(|candidate| candidate.allowed && candidate.installed && candidate.configured)
    {
        return "native_cli_available";
    }
    "needs_sync_or_authorization"
}

fn shell_preflight_commands(
    session: &BrainShellSessionSpec,
    harness_status: Option<&ExecutorHarnessStatus>,
    context_command: Option<&Vec<String>>,
    handoff_command: Option<&Vec<String>>,
) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    if let Some(harness_status) = harness_status {
        commands.push(vec![
            "forge".to_string(),
            "harness".to_string(),
            "shim-status".to_string(),
            "--shim-dir".to_string(),
            harness_status.shim_dir.clone(),
            "--executor".to_string(),
            session.brain_id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ]);
    }
    commands.push(vec![
        "forge".to_string(),
        "brains".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]);
    if let Some(context_command) = context_command {
        commands.push(context_command.clone());
    }
    if let Some(handoff_command) = handoff_command {
        commands.push(handoff_command.clone());
    } else if session.brain_id != "forge" {
        commands.push(vec![
            "forge".to_string(),
            "task".to_string(),
            "handoff".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--executor".to_string(),
            session.brain_id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ]);
    }
    commands
}

fn shell_context_command(
    options: &ShellLaunchPlanOptions,
    context_budget: usize,
) -> Option<Vec<String>> {
    let workflow_id = options.workflow_id.as_ref()?;
    let task_id = options.task_id.as_ref()?;
    Some(vec![
        "forge".to_string(),
        "context".to_string(),
        "--workflow".to_string(),
        workflow_id.clone(),
        "--task".to_string(),
        task_id.clone(),
        "--budget".to_string(),
        context_budget.to_string(),
        "--strict".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ])
}

fn shell_handoff_command(
    session: &BrainShellSessionSpec,
    options: &ShellLaunchPlanOptions,
    context_budget: usize,
    ttl_seconds: u64,
) -> Option<Vec<String>> {
    if session.brain_id == "forge" {
        return None;
    }
    let workflow_id = options.workflow_id.as_ref()?;
    let task_id = options.task_id.as_ref()?;
    Some(vec![
        "forge".to_string(),
        "task".to_string(),
        "handoff".to_string(),
        "--workflow".to_string(),
        workflow_id.clone(),
        "--task".to_string(),
        task_id.clone(),
        "--executor".to_string(),
        session.brain_id.clone(),
        "--budget".to_string(),
        context_budget.to_string(),
        "--ttl-seconds".to_string(),
        ttl_seconds.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ])
}

fn shell_heartbeat_command(
    session: &BrainShellSessionSpec,
    options: &ShellLaunchPlanOptions,
    ttl_seconds: u64,
) -> Option<Vec<String>> {
    if session.brain_id == "forge" {
        return None;
    }
    let run_id = options.run_id.as_ref()?;
    Some(vec![
        "forge".to_string(),
        "request".to_string(),
        "heartbeat".to_string(),
        "--run".to_string(),
        run_id.clone(),
        "--executor".to_string(),
        session.brain_id.clone(),
        "--summary".to_string(),
        "shell session active".to_string(),
        "--ttl-seconds".to_string(),
        ttl_seconds.to_string(),
        "--origin".to_string(),
        "forge_shell".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ])
}

fn brain_session_operation_plan(
    session: &BrainShellSessionSpec,
    readiness: &str,
    lifecycle_state: &str,
    last_plan_event: Option<&BrainSessionEventSummary>,
) -> BrainSessionOperationPlan {
    let external_brain = session.brain_id != "forge";
    let context = brain_session_context_command(last_plan_event, 1200);
    let handoff = brain_session_handoff_command(session, last_plan_event, 1200, 900);
    let heartbeat = brain_session_heartbeat_command(session, last_plan_event, 900);
    let lineage_complete = last_plan_event.is_some_and(|event| {
        event.workflow_id.is_some() && event.task_id.is_some() && event.run_id.is_some()
    });
    let requires_context = external_brain
        && context.is_some()
        && matches!(
            lifecycle_state,
            "untracked" | "opened" | "attached" | "detached"
        );
    let requires_handoff = external_brain
        && handoff.is_some()
        && matches!(
            lifecycle_state,
            "untracked" | "opened" | "attached" | "detached"
        );
    let requires_heartbeat = external_brain
        && heartbeat.is_some()
        && matches!(lifecycle_state, "opened" | "attached" | "detached");
    let recommended_action =
        brain_session_recommended_action(session, readiness, lifecycle_state, last_plan_event);
    let status = if matches!(readiness, "needs_sync_or_authorization" | "not_installed") {
        "session_operation_needs_attention"
    } else if matches!(lifecycle_state, "closed" | "abandoned") {
        "session_operation_idle"
    } else {
        "session_operation_ready"
    };
    let mut warnings = Vec::new();
    if external_brain && !lineage_complete {
        warnings.push(
            "Session does not yet have complete workflow/task/run lineage; record a shell plan before production handoff."
                .to_string(),
        );
    }
    if matches!(readiness, "needs_sync_or_authorization" | "not_installed") {
        warnings.push(
            "Provider is not ready for Forge-controlled shell operation; sync executors and resolve authorization or installation first."
                .to_string(),
        );
    }

    BrainSessionOperationPlan {
        schema_version: "forge.brain_session_operation_plan.v1".to_string(),
        status: status.to_string(),
        session_id: session.id.clone(),
        provider_id: session.brain_id.clone(),
        lifecycle_state: lifecycle_state.to_string(),
        readiness: readiness.to_string(),
        recommended_action,
        lineage_complete,
        requires_context,
        requires_handoff,
        requires_heartbeat,
        commands: BrainSessionOperationCommands {
            history: brain_session_history_command(&session.id),
            launch_plan: brain_session_launch_plan_command(session, last_plan_event),
            record_plan: brain_session_record_plan_command(session, last_plan_event),
            open: brain_session_lifecycle_command(&session.id, "opened", last_plan_event),
            attach: brain_session_lifecycle_command(&session.id, "attached", last_plan_event),
            detach: brain_session_lifecycle_command(&session.id, "detached", last_plan_event),
            close: brain_session_lifecycle_command(&session.id, "closed", last_plan_event),
            context,
            handoff,
            heartbeat,
        },
        warnings,
        notes: vec![
            "Operation plan is read-only and gives TUI/MCP clients the next safe session controls without launching child CLIs."
                .to_string(),
            "External brains remain execution resources; Forge owns context, handoff, heartbeat, lifecycle and validation gates."
                .to_string(),
        ],
    }
}

fn brain_session_recommended_action(
    session: &BrainShellSessionSpec,
    readiness: &str,
    lifecycle_state: &str,
    last_plan_event: Option<&BrainSessionEventSummary>,
) -> String {
    if session.brain_id == "forge" {
        return "open_forge_control_surface".to_string();
    }
    if matches!(readiness, "needs_sync_or_authorization" | "not_installed") {
        return "sync_or_authorize_provider".to_string();
    }
    match lifecycle_state {
        "untracked" if last_plan_event.is_some() => "record_opened_lifecycle".to_string(),
        "untracked" => "record_shell_launch_plan".to_string(),
        "opened" => "attach_session_before_handoff".to_string(),
        "attached" => "heartbeat_or_close_session".to_string(),
        "detached" => "reattach_or_close_session".to_string(),
        "closed" => "open_session_when_needed".to_string(),
        "failed" => "inspect_history_before_reopen".to_string(),
        "abandoned" => "recover_or_archive_session".to_string(),
        _ => "inspect_session_history".to_string(),
    }
}

fn brain_session_history_command(session_id: &str) -> Vec<String> {
    vec![
        "forge".to_string(),
        "sessions".to_string(),
        "history".to_string(),
        "--session".to_string(),
        session_id.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

fn brain_session_launch_plan_command(
    session: &BrainShellSessionSpec,
    last_plan_event: Option<&BrainSessionEventSummary>,
) -> Vec<String> {
    let mut command = vec![
        "forge".to_string(),
        "shells".to_string(),
        "--executor".to_string(),
        session.brain_id.clone(),
    ];
    append_shell_lineage_args(&mut command, last_plan_event, false);
    command.extend(["--output".to_string(), "json".to_string()]);
    command
}

fn brain_session_record_plan_command(
    session: &BrainShellSessionSpec,
    last_plan_event: Option<&BrainSessionEventSummary>,
) -> Vec<String> {
    let mut command = vec![
        "forge".to_string(),
        "shells".to_string(),
        "--executor".to_string(),
        session.brain_id.clone(),
    ];
    append_shell_lineage_args(&mut command, last_plan_event, false);
    command.extend([
        "--record-session".to_string(),
        "--origin".to_string(),
        "forge_cli".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]);
    command
}

fn brain_session_lifecycle_command(
    session_id: &str,
    state: &str,
    last_plan_event: Option<&BrainSessionEventSummary>,
) -> Option<Vec<String>> {
    let mut command = vec![
        "forge".to_string(),
        "sessions".to_string(),
        "lifecycle".to_string(),
        "--session".to_string(),
        session_id.to_string(),
        "--state".to_string(),
        state.to_string(),
    ];
    append_shell_lineage_args(&mut command, last_plan_event, true);
    command.extend([
        "--origin".to_string(),
        "forge_cli".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]);
    Some(command)
}

fn brain_session_context_command(
    last_plan_event: Option<&BrainSessionEventSummary>,
    context_budget: usize,
) -> Option<Vec<String>> {
    let event = last_plan_event?;
    let workflow_id = event.workflow_id.as_ref()?;
    let task_id = event.task_id.as_ref()?;
    Some(vec![
        "forge".to_string(),
        "context".to_string(),
        "--workflow".to_string(),
        workflow_id.clone(),
        "--task".to_string(),
        task_id.clone(),
        "--budget".to_string(),
        context_budget.to_string(),
        "--strict".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ])
}

fn brain_session_handoff_command(
    session: &BrainShellSessionSpec,
    last_plan_event: Option<&BrainSessionEventSummary>,
    context_budget: usize,
    ttl_seconds: u64,
) -> Option<Vec<String>> {
    if session.brain_id == "forge" {
        return None;
    }
    let event = last_plan_event?;
    let workflow_id = event.workflow_id.as_ref()?;
    let task_id = event.task_id.as_ref()?;
    Some(vec![
        "forge".to_string(),
        "task".to_string(),
        "handoff".to_string(),
        "--workflow".to_string(),
        workflow_id.clone(),
        "--task".to_string(),
        task_id.clone(),
        "--executor".to_string(),
        session.brain_id.clone(),
        "--budget".to_string(),
        context_budget.to_string(),
        "--ttl-seconds".to_string(),
        ttl_seconds.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ])
}

fn brain_session_heartbeat_command(
    session: &BrainShellSessionSpec,
    last_plan_event: Option<&BrainSessionEventSummary>,
    ttl_seconds: u64,
) -> Option<Vec<String>> {
    if session.brain_id == "forge" {
        return None;
    }
    let event = last_plan_event?;
    let run_id = event.run_id.as_ref()?;
    Some(vec![
        "forge".to_string(),
        "request".to_string(),
        "heartbeat".to_string(),
        "--run".to_string(),
        run_id.clone(),
        "--executor".to_string(),
        session.brain_id.clone(),
        "--summary".to_string(),
        "shell session active".to_string(),
        "--ttl-seconds".to_string(),
        ttl_seconds.to_string(),
        "--origin".to_string(),
        "forge_shell".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ])
}

fn append_shell_lineage_args(
    command: &mut Vec<String>,
    last_plan_event: Option<&BrainSessionEventSummary>,
    include_run: bool,
) {
    if let Some(event) = last_plan_event {
        if let Some(workflow_id) = &event.workflow_id {
            command.extend(["--workflow".to_string(), workflow_id.clone()]);
        }
        if let Some(task_id) = &event.task_id {
            command.extend(["--task".to_string(), task_id.clone()]);
        }
        if include_run {
            if let Some(run_id) = &event.run_id {
                command.extend(["--run".to_string(), run_id.clone()]);
            }
        } else if let Some(run_id) = &event.run_id {
            command.extend(["--run".to_string(), run_id.clone()]);
        }
    }
}

fn shell_next_actions(session: &BrainShellSessionSpec) -> Vec<String> {
    if session.brain_id == "forge" {
        return vec![
            "Start the Forge TUI when the operator needs the primary control surface.".to_string(),
        ];
    }
    vec![
        "Run the entry_command only after Forge has prepared a workflow/task context packet and recorded the handoff lease."
            .to_string(),
        "Prefer Forge harness execution receipts for non-interactive commands that need audit lineage."
            .to_string(),
    ]
}

fn build_quota_policy(
    executors: &[ExecutorState],
    store: &ForgeStore,
) -> ExecutorQuotaPolicyReport {
    let observations = load_quota_observations(store);
    let opencode_free_model = env::var("OPENCODE_FREE_MODEL").ok();
    let opencode_model = env::var("OPENCODE_MODEL").ok();

    let mut candidates = vec![
        quota_candidate(
            executors,
            &observations,
            "opencode",
            "configured_cli",
            opencode_free_model.clone().or_else(|| Some("google/gemini-2.5-pro".to_string())),
            "non_local",
            "unknown_or_configured_non_local_quota_bound",
            "quota_or_rate_limit_bound",
            "unknown",
            "medium",
            "provider_config_dependent",
            "medium",
            "medium_high",
            "good_for_product_and_code_when_configured_quota_or_cost_is_available",
            "medium",
            10,
            "OpenCode non-local configured provider path is the first choice when expected value justifies quota or cost.",
        ),
        quota_candidate(
            executors,
            &observations,
            "gemini",
            "google",
            None,
            "non_local",
            "quota_bound",
            "quota_bound",
            "unknown",
            "medium_high",
            "quota_or_paid",
            "medium",
            "high",
            "strong_for_product_business_reasoning_when_non_interactive",
            "medium_high",
            20,
            "Gemini is a non-local quota-bound capability; use it for high-value reasoning when non-interactive auth and model selection are validated.",
        ),
        quota_candidate(
            executors,
            &observations,
            "codex",
            "openai",
            None,
            "non_local",
            "quota_bound",
            "quota_bound",
            "unknown",
            "medium",
            "quota_or_paid",
            "medium",
            "high",
            "strong_for_product_business_reasoning_and_code_when_quota_value_is_justified",
            "low",
            30,
            "Codex is an authorized non-local quota-bound fallback when expected value justifies consuming quota.",
        ),
        quota_candidate(
            executors,
            &observations,
            "opencode",
            "configured_cli",
            opencode_model.or(opencode_free_model),
            "non_local",
            "unknown_or_paid",
            "quota_or_rate_limit_bound",
            "unknown",
            "medium",
            "provider_config_dependent",
            "medium",
            "medium_high",
            "good_for_product_and_code_when_configured",
            "medium",
            35,
            "OpenCode non-local (potentially paid) provider path is used when free options are exhausted or unsuitable.",
        ),
        quota_candidate(
            executors,
            &observations,
            "opencode",
            "ollama",
            Some("ollama/qwen3:14b".to_string()),
            "local",
            "local_resource_cost",
            "local_capacity_bound",
            "local_capacity",
            "low",
            "local_compute",
            "low",
            "medium",
            "efficient_for_repetitive_low_value_or_privacy_sensitive_work",
            "medium",
            40,
            "OpenCode local/Ollama models are efficient when quota should be preserved, work is repetitive, privacy matters or expected value does not justify non-local quota.",
        ),
    ];
    candidates.sort_by_key(|candidate| candidate.selection_tier);
    let repair_goals = quota_policy_repair_goals(&candidates);
    let selection_trace = executor_selection_trace(&candidates);

    ExecutorQuotaPolicyReport {
        schema_version: "forge.executor_quota_policy.v1".to_string(),
        selection_principle:
            "maximize useful progress under expected value, quota, cost, latency, quality and fallback risk constraints"
                .to_string(),
        decision_factors: vec![
            "provider".to_string(),
            "model".to_string(),
            "local_vs_non_local".to_string(),
            "free_vs_paid_if_known".to_string(),
            "remaining_quota_if_available".to_string(),
            "rate_limit_risk".to_string(),
            "monetary_or_token_cost".to_string(),
            "latency".to_string(),
            "expected_quality".to_string(),
            "product_business_suitability".to_string(),
            "fallback_risk".to_string(),
        ],
        workload_routes: quota_workload_routes(),
        observed_quota_evidence: observations,
        candidates,
        selection_trace,
        skipped_to_preserve_quota: vec![
            "Use deterministic command nodes for repeated validation, file inspection and low-value mechanical work before spending non-local quota.".to_string(),
            "Use local models when quota is low, privacy/locality matters or the task value does not justify Gemini/Codex/OpenCode non-local capacity.".to_string(),
        ],
        repair_goals,
    }
}

fn executor_selection_trace(
    candidates: &[ExecutorQuotaPolicyCandidate],
) -> Vec<ExecutorSelectionTrace> {
    let selected_index = candidates
        .iter()
        .position(|candidate| candidate.selection_status == "eligible");

    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let decision = if Some(index) == selected_index {
                "select"
            } else {
                "skip"
            };
            let next_fallback_reason = if Some(index) == selected_index {
                "selected as first quota-aware candidate that is installed, configured, authorized and non-interactive ready".to_string()
            } else if candidate.selection_status == "eligible" {
                "not selected because an earlier quota-aware candidate is eligible".to_string()
            } else {
                format!(
                    "{}; try next quota-aware candidate if this executor is needed for the workflow",
                    candidate.selection_status
                )
            };

            ExecutorSelectionTrace {
                schema_version: "forge.executor_selection_trace.v1".to_string(),
                executor: candidate.executor.clone(),
                provider: candidate.provider.clone(),
                model: candidate.model.clone(),
                local_vs_non_local: candidate.local_vs_non_local.clone(),
                selection_tier: candidate.selection_tier,
                selection_status: candidate.selection_status.clone(),
                decision: decision.to_string(),
                reason: candidate.reason.clone(),
                next_fallback_reason,
            }
        })
        .collect()
}

fn quota_policy_repair_goals(candidates: &[ExecutorQuotaPolicyCandidate]) -> Vec<String> {
    let mut goals = vec![
        "Detect Gemini non-interactive auth/model/approval readiness before handoff and mark interactive waits as executor configuration failures.".to_string(),
        "Record OpenCode provider/model availability, including non-local provider options and local Ollama fallback, before selection.".to_string(),
        "Persist observed quota, rate-limit and cost evidence when executors report it so future selection can move from estimates to measurements.".to_string(),
    ];
    let mut reported_executors = BTreeSet::new();

    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.selection_status == "skipped_interactive_hang_risk")
    {
        if !reported_executors.insert(candidate.executor.clone()) {
            continue;
        }
        goals.push(format!(
            "Repair {} non-interactive readiness before executor selection; current evidence: {}.",
            executor_display_name(&candidate.executor),
            if candidate.evidence.is_empty() {
                "no probe evidence recorded".to_string()
            } else {
                candidate.evidence.join(" | ")
            }
        ));
    }

    goals
}

fn executor_display_name(executor: &str) -> &str {
    match executor {
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "gemini" => "Gemini",
        "claude" => "Claude",
        "ollama" => "Ollama",
        _ => executor,
    }
}

fn quota_workload_routes() -> Vec<ExecutorQuotaWorkloadRoute> {
    vec![
        ExecutorQuotaWorkloadRoute {
            workload_class: "high_value_pm_business_creative_reasoning".to_string(),
            default_policy: "prefer_best_authorized_non_local_when_quota_value_is_justified"
                .to_string(),
            preferred_candidate: "opencode_non_local_then_gemini_then_codex".to_string(),
            quota_spend_rule:
                "spend non-local quota when decision quality materially changes product or business outcome"
                    .to_string(),
            quota_preservation_rule:
                "fall back when quota is low, rate-limit risk is high or provider readiness is unvalidated"
                    .to_string(),
            business_reason:
                "stronger reasoning is worth scarce quota for product direction, trade-off analysis and creative leverage"
                    .to_string(),
        },
        ExecutorQuotaWorkloadRoute {
            workload_class: "deterministic_validation_file_inspection_reporting".to_string(),
            default_policy: "prefer_no_ai_command_or_local_execution".to_string(),
            preferred_candidate: "command_node_or_opencode_local".to_string(),
            quota_spend_rule:
                "avoid non-local model calls unless failures require high-value diagnosis".to_string(),
            quota_preservation_rule:
                "preserve Gemini/Codex/OpenCode non-local quota for reasoning that cannot be checked deterministically"
                    .to_string(),
            business_reason:
                "keeps recurring validation cheap and repeatable while reserving quota for decisions with user value"
                    .to_string(),
        },
        ExecutorQuotaWorkloadRoute {
            workload_class: "privacy_sensitive_or_low_value_repetitive_work".to_string(),
            default_policy: "prefer_local_model_when_quality_is_sufficient".to_string(),
            preferred_candidate: "opencode_local_ollama".to_string(),
            quota_spend_rule:
                "use non-local quota only when local output quality blocks validated progress".to_string(),
            quota_preservation_rule:
                "local capacity is acceptable when latency, privacy or low expected value outweighs model quality"
                    .to_string(),
            business_reason:
                "reduces operating cost and quota burn without blocking routine workflow progress".to_string(),
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn quota_candidate(
    executors: &[ExecutorState],
    observations: &[ExecutorQuotaObservation],
    executor: &str,
    provider: &str,
    model: Option<String>,
    local_vs_non_local: &str,
    free_vs_paid_if_known: &str,
    quota_model: &str,
    remaining_quota: &str,
    rate_limit_risk: &str,
    cost_model: &str,
    latency: &str,
    expected_quality: &str,
    product_business_suitability: &str,
    fallback_risk: &str,
    selection_tier: u32,
    reason: &str,
) -> ExecutorQuotaPolicyCandidate {
    let state = executors.iter().find(|state| state.id == executor);
    let selection_status = match state {
        Some(state)
            if state.allowed
                && state.installed
                && state.configured
                && state.non_interactive_ready =>
        {
            "eligible"
        }
        Some(state)
            if state.allowed
                && state.installed
                && state.configured
                && !state.non_interactive_ready =>
        {
            "skipped_interactive_hang_risk"
        }
        Some(state) if !state.installed => "skipped_not_installed",
        Some(state) if !state.configured => "skipped_not_configured",
        Some(_) => "skipped_not_allowed",
        None => "skipped_unknown_executor",
    };
    let observation =
        matching_quota_observation(observations, executor, provider, model.as_deref());
    let mut evidence = state
        .map(|state| {
            let mut ev = state.config_evidence.clone();
            ev.extend(state.probe_evidence.clone());
            ev
        })
        .unwrap_or_default();
    if let Some(observation) = observation {
        evidence.push(format!(
            "quota_observation:{}:{}:{}:{}",
            observation.source,
            observation.remaining_quota,
            observation.rate_limit_risk,
            observation.observed_at
        ));
    }

    ExecutorQuotaPolicyCandidate {
        executor: executor.to_string(),
        provider: observation
            .map(|observation| observation.provider.clone())
            .unwrap_or_else(|| provider.to_string()),
        model: observation
            .and_then(|observation| observation.model.clone())
            .or(model),
        local_vs_non_local: observation
            .map(|observation| observation.local_vs_non_local.clone())
            .unwrap_or_else(|| local_vs_non_local.to_string()),
        free_vs_paid_if_known: observation
            .map(|observation| observation.free_vs_paid_if_known.clone())
            .unwrap_or_else(|| free_vs_paid_if_known.to_string()),
        quota_model: quota_model.to_string(),
        remaining_quota: observation
            .map(|observation| observation.remaining_quota.clone())
            .unwrap_or_else(|| remaining_quota.to_string()),
        rate_limit_risk: observation
            .map(|observation| observation.rate_limit_risk.clone())
            .unwrap_or_else(|| rate_limit_risk.to_string()),
        cost_model: observation
            .map(|observation| observation.monetary_or_token_cost.clone())
            .unwrap_or_else(|| cost_model.to_string()),
        latency: observation
            .map(|observation| observation.latency.clone())
            .unwrap_or_else(|| latency.to_string()),
        expected_quality: observation
            .map(|observation| observation.expected_quality.clone())
            .unwrap_or_else(|| expected_quality.to_string()),
        product_business_suitability: observation
            .map(|observation| observation.suitability.clone())
            .unwrap_or_else(|| product_business_suitability.to_string()),
        fallback_risk: fallback_risk.to_string(),
        selection_tier,
        selection_status: selection_status.to_string(),
        reason: reason.to_string(),
        evidence,
    }
}

fn load_quota_observations(store: &ForgeStore) -> Vec<ExecutorQuotaObservation> {
    store
        .load_executor_quotas()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect()
}

fn matching_quota_observation<'a>(
    observations: &'a [ExecutorQuotaObservation],
    executor: &str,
    provider: &str,
    model: Option<&str>,
) -> Option<&'a ExecutorQuotaObservation> {
    observations.iter().find(|observation| {
        observation.executor == executor
            && (observation.provider == provider || provider == "configured_cli")
            && model
                .map(|model| observation.model.as_deref() == Some(model))
                .unwrap_or(true)
    })
}

fn executor_is_allowed(executors: &[ExecutorState], id: &str) -> bool {
    executors
        .iter()
        .find(|executor| executor.id == id)
        .map(|executor| {
            executor.allowed
                && executor.installed
                && executor.configured
                && executor.non_interactive_ready
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn executor_report_surfaces_persisted_quota_observations() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();

        let codex_state = ExecutorState {
            id: "codex".to_string(),
            display_name: "Codex CLI".to_string(),
            command: "codex".to_string(),
            installed: true,
            configured: true,
            command_path: Some("/tmp/codex".to_string()),
            config_evidence: vec!["test-config".to_string()],
            non_interactive_ready: true,
            probe_evidence: vec!["smoke test passed".to_string()],
            forge_first_ready: false,
            forge_first_entrypoint: None,
            harness_status: None,
            allowed: true,
            decision_source: "human_allow".to_string(),
            synced_at: "2026-06-02T00:00:00Z".to_string(),
        };
        store
            .save_executor_state("codex", &serde_json::to_value(codex_state).unwrap())
            .unwrap();
        store
            .save_executor_quota(
                "codex",
                "openai",
                "gpt-5.5",
                &json!({
                    "executor": "codex",
                    "provider": "openai",
                    "model": "gpt-5.5",
                    "local_vs_non_local": "non_local",
                    "free_vs_paid_if_known": "not_free_quota_bound",
                    "remaining_quota": "preserve_for_high_value_pm_reasoning",
                    "rate_limit_risk": "medium_high",
                    "monetary_or_token_cost": "quota_or_paid_usage",
                    "latency": "medium",
                    "expected_quality": "high",
                    "suitability": "high_for_product_business_decisions",
                    "source": "self_evolution_cycle_5",
                    "observed_at": "2026-06-02T00:00:00Z"
                }),
            )
            .unwrap();

        let report = load_executors(&store).unwrap();

        assert_eq!(report.quota_policy.observed_quota_evidence.len(), 1);
        let codex_candidate = report
            .quota_policy
            .candidates
            .iter()
            .find(|candidate| candidate.executor == "codex")
            .unwrap();
        assert_eq!(codex_candidate.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(
            codex_candidate.remaining_quota,
            "preserve_for_high_value_pm_reasoning"
        );
        assert_eq!(codex_candidate.rate_limit_risk, "medium_high");
        assert!(codex_candidate
            .evidence
            .iter()
            .any(|evidence| evidence.contains("quota_observation:self_evolution_cycle_5")));
    }

    #[test]
    fn executor_report_excludes_interactive_hang_risk_from_usable() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();

        let gemini_state = ExecutorState {
            id: "gemini".to_string(),
            display_name: "Gemini CLI".to_string(),
            command: "gemini".to_string(),
            installed: true,
            configured: true,
            command_path: Some("/tmp/gemini".to_string()),
            config_evidence: vec!["env:GEMINI_API_KEY".to_string()],
            non_interactive_ready: false,
            probe_evidence: vec![
                "non-interactive smoke test `--version` timed out after 2000ms".to_string(),
            ],
            forge_first_ready: false,
            forge_first_entrypoint: None,
            harness_status: None,
            allowed: true,
            decision_source: "human_allow".to_string(),
            synced_at: "2026-06-02T00:00:00Z".to_string(),
        };
        store
            .save_executor_state("gemini", &serde_json::to_value(gemini_state).unwrap())
            .unwrap();

        let report = load_executors(&store).unwrap();

        assert!(!report.usable.iter().any(|executor| executor == "gemini"));
        let gemini_candidate = report
            .quota_policy
            .candidates
            .iter()
            .find(|candidate| candidate.executor == "gemini")
            .unwrap();
        assert_eq!(
            gemini_candidate.selection_status,
            "skipped_interactive_hang_risk"
        );
        assert!(gemini_candidate
            .evidence
            .iter()
            .any(|evidence| evidence.contains("timed out")));
        assert!(report.quota_policy.repair_goals.iter().any(|goal| {
            goal.contains("Repair Gemini non-interactive readiness")
                && goal.contains("non-interactive smoke test `--version` timed out")
        }));
    }

    #[test]
    fn executor_report_deduplicates_repair_goals_for_one_executor() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();

        let opencode_state = ExecutorState {
            id: "opencode".to_string(),
            display_name: "OpenCode CLI".to_string(),
            command: "opencode".to_string(),
            installed: true,
            configured: true,
            command_path: Some("/tmp/opencode".to_string()),
            config_evidence: vec!["test opencode config".to_string()],
            non_interactive_ready: false,
            probe_evidence: vec![
                "failed to list opencode models; non-interactive provider/model readiness is not validated"
                    .to_string(),
            ],
            forge_first_ready: false,
            forge_first_entrypoint: None,
            harness_status: None,
            allowed: true,
            decision_source: "human_allow".to_string(),
            synced_at: "2026-06-02T00:00:00Z".to_string(),
        };
        store
            .save_executor_state("opencode", &serde_json::to_value(opencode_state).unwrap())
            .unwrap();

        let report = load_executors(&store).unwrap();

        let dynamic_opencode_goals = report
            .quota_policy
            .repair_goals
            .iter()
            .filter(|goal| goal.contains("Repair OpenCode non-interactive readiness"))
            .count();
        assert_eq!(dynamic_opencode_goals, 1);
    }

    #[test]
    fn executor_state_loads_legacy_records_without_probe_fields() {
        let legacy = json!({
            "id": "codex",
            "display_name": "Codex CLI",
            "command": "codex",
            "installed": true,
            "configured": true,
            "command_path": "/tmp/codex",
            "config_evidence": ["test-config"],
            "allowed": true,
            "decision_source": "human_allow",
            "synced_at": "2026-06-02T00:00:00Z"
        });

        let state: ExecutorState = serde_json::from_value(legacy).unwrap();

        assert!(!state.non_interactive_ready);
        assert!(state.probe_evidence.is_empty());
    }
}
