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

    ExecutorSyncReport {
        status: status.to_string(),
        home: home.display().to_string(),
        needs_human_approval,
        usable,
        executors,
        integrations,
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

fn executor_is_allowed(executors: &[ExecutorState], id: &str) -> bool {
    executors
        .iter()
        .find(|executor| executor.id == id)
        .map(|executor| executor.allowed && executor.installed && executor.configured)
        .unwrap_or(false)
}
