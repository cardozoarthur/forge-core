use crate::storage::ForgeStore;
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
    pub allowed: bool,
    pub decision_source: String,
    pub synced_at: String,
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
    pub quota_policy: ExecutorQuotaPolicyReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorQuotaPolicyReport {
    pub schema_version: String,
    pub selection_principle: String,
    pub decision_factors: Vec<String>,
    pub workload_routes: Vec<ExecutorQuotaWorkloadRoute>,
    pub observed_quota_evidence: Vec<ExecutorQuotaObservation>,
    pub candidates: Vec<ExecutorQuotaPolicyCandidate>,
    pub skipped_to_preserve_quota: Vec<String>,
    pub repair_goals: Vec<String>,
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
        let mut state = probe_executor(definition, &options.home, &options.executor_paths);
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

    ExecutorSyncReport {
        status: status.to_string(),
        home: home.display().to_string(),
        needs_human_approval,
        usable,
        executors,
        integrations,
        quota_policy,
    }
}

fn probe_executor(
    definition: &ExecutorDefinition,
    home: &Path,
    executor_paths: &[PathBuf],
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
        allowed: false,
        decision_source: "unavailable".to_string(),
        synced_at: Utc::now().to_rfc3339(),
    }
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
        skipped_to_preserve_quota: vec![
            "Use deterministic command nodes for repeated validation, file inspection and low-value mechanical work before spending non-local quota.".to_string(),
            "Use local models when quota is low, privacy/locality matters or the task value does not justify Gemini/Codex/OpenCode non-local capacity.".to_string(),
        ],
        repair_goals,
    }
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
