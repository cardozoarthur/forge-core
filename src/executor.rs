use crate::storage::ForgeStore;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

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
    pub candidates: Vec<ExecutorQuotaPolicyCandidate>,
    pub skipped_to_preserve_quota: Vec<String>,
    pub repair_goals: Vec<String>,
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

    let report = build_report("synced", &options.home, executors);
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
    Ok(build_report("loaded", &store.base_dir(), states))
}

fn build_report(
    status: &str,
    home: &Path,
    mut executors: Vec<ExecutorState>,
) -> ExecutorSyncReport {
    executors.sort_by(|left, right| left.id.cmp(&right.id));
    let usable = executors
        .iter()
        .filter(|executor| executor.allowed && executor.installed && executor.configured)
        .map(|executor| executor.id.clone())
        .collect::<Vec<_>>();
    let needs_human_approval = executors.iter().any(|executor| {
        executor.installed
            && executor.configured
            && !executor.allowed
            && executor.decision_source == "pending_human_approval"
    });
    let integrations = build_integrations(&executors);
    let quota_policy = build_quota_policy(&executors);

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

    ExecutorState {
        id: definition.id.to_string(),
        display_name: definition.display_name.to_string(),
        command: definition.command.to_string(),
        installed: command_path.is_some(),
        configured,
        command_path: command_path.map(|path| path.display().to_string()),
        config_evidence,
        allowed: false,
        decision_source: "unavailable".to_string(),
        synced_at: Utc::now().to_rfc3339(),
    }
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

fn build_quota_policy(executors: &[ExecutorState]) -> ExecutorQuotaPolicyReport {
    let mut candidates = vec![
        quota_candidate(
            executors,
            "opencode",
            "configured_cli",
            None,
            "non_local",
            "unknown",
            "quota_or_provider_bound",
            "unknown",
            "medium",
            "provider_config_dependent",
            "medium",
            "medium_high",
            "good_for_product_and_code_when_configured",
            "medium",
            10,
            "OpenCode non-local provider path is preferred when authorized and expected value justifies provider quota or configured no-cost capacity.",
        ),
        quota_candidate(
            executors,
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
            "ollama",
            "local_runtime",
            Some("configured_local_model".to_string()),
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
            "Local Ollama models are efficient when quota should be preserved, work is repetitive, privacy matters or expected value does not justify non-local quota.",
        ),
    ];
    candidates.sort_by_key(|candidate| candidate.selection_tier);

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
        candidates,
        skipped_to_preserve_quota: vec![
            "Use deterministic command nodes for repeated validation, file inspection and low-value mechanical work before spending non-local quota.".to_string(),
            "Use local models when quota is low, privacy/locality matters or the task value does not justify Gemini/Codex/OpenCode non-local capacity.".to_string(),
        ],
        repair_goals: vec![
            "Detect Gemini non-interactive auth/model/approval readiness before handoff and mark interactive waits as executor configuration failures.".to_string(),
            "Record OpenCode provider/model availability, including non-local provider options and local Ollama fallback, before selection.".to_string(),
            "Persist observed quota, rate-limit and cost evidence when executors report it so future selection can move from estimates to measurements.".to_string(),
        ],
    }
}

#[allow(clippy::too_many_arguments)]
fn quota_candidate(
    executors: &[ExecutorState],
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
        Some(state) if state.allowed && state.installed && state.configured => "eligible",
        Some(state) if !state.installed => "skipped_not_installed",
        Some(state) if !state.configured => "skipped_not_configured",
        Some(_) => "skipped_not_allowed",
        None => "skipped_unknown_executor",
    };
    let evidence = state
        .map(|state| state.config_evidence.clone())
        .unwrap_or_default();

    ExecutorQuotaPolicyCandidate {
        executor: executor.to_string(),
        provider: provider.to_string(),
        model,
        local_vs_non_local: local_vs_non_local.to_string(),
        free_vs_paid_if_known: free_vs_paid_if_known.to_string(),
        quota_model: quota_model.to_string(),
        remaining_quota: remaining_quota.to_string(),
        rate_limit_risk: rate_limit_risk.to_string(),
        cost_model: cost_model.to_string(),
        latency: latency.to_string(),
        expected_quality: expected_quality.to_string(),
        product_business_suitability: product_business_suitability.to_string(),
        fallback_risk: fallback_risk.to_string(),
        selection_tier,
        selection_status: selection_status.to_string(),
        reason: reason.to_string(),
        evidence,
    }
}

fn executor_is_allowed(executors: &[ExecutorState], id: &str) -> bool {
    executors
        .iter()
        .find(|executor| executor.id == id)
        .map(|executor| executor.allowed && executor.installed && executor.configured)
        .unwrap_or(false)
}
