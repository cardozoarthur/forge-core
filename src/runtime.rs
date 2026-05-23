use crate::storage::ForgeStore;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimeSyncOptions {
    pub home: PathBuf,
    pub runtime_paths: Vec<PathBuf>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub prompt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub installed: bool,
    pub configured: bool,
    pub command_path: Option<String>,
    pub config_evidence: Vec<String>,
    pub allowed: bool,
    pub decision_source: String,
    pub async_capable: bool,
    pub ownership_policy: String,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInstallSuggestion {
    pub id: String,
    pub display_name: String,
    pub reason: String,
    pub requires_human_approval: bool,
    pub suggested_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSyncReport {
    pub status: String,
    pub home: String,
    pub needs_human_approval: bool,
    pub usable: Vec<String>,
    pub runtimes: Vec<RuntimeState>,
    pub install_suggestions: Vec<RuntimeInstallSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeGuardReport {
    pub allowed: bool,
    pub requires_human_approval: bool,
    pub decision: String,
    pub substrate: String,
    pub resource: String,
    pub namespace: String,
    pub action: String,
    pub owner: String,
    pub scope_rule: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeGuardRequest {
    pub substrate: String,
    pub resource: String,
    pub namespace: String,
    pub action: String,
    pub owner: String,
    pub allow_external: bool,
}

#[derive(Debug, Clone)]
struct RuntimeDefinition {
    id: &'static str,
    display_name: &'static str,
    command: &'static str,
}

const RUNTIMES: &[RuntimeDefinition] = &[
    RuntimeDefinition {
        id: "docker",
        display_name: "Docker",
        command: "docker",
    },
    RuntimeDefinition {
        id: "kubernetes",
        display_name: "Kubernetes",
        command: "kubectl",
    },
    RuntimeDefinition {
        id: "knative",
        display_name: "Knative",
        command: "kn",
    },
];

pub fn sync_runtimes(store: &ForgeStore, options: RuntimeSyncOptions) -> Result<RuntimeSyncReport> {
    let previous = load_previous_states(store)?;
    let allow = normalize_set(&options.allow);
    let deny = normalize_set(&options.deny);
    let mut runtimes = Vec::new();

    for definition in RUNTIMES {
        let mut state = probe_runtime(definition, &options.home, &options.runtime_paths);
        apply_decision(&mut state, &previous, &allow, &deny, options.prompt)?;
        store.save_runtime_state(&state.id, &serde_json::to_value(&state)?)?;
        runtimes.push(state);
    }

    let report = build_report("synced", &options.home, runtimes);
    store.record_event(
        "_system",
        "runtimes_synced",
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

pub fn load_runtimes(store: &ForgeStore) -> Result<RuntimeSyncReport> {
    let states = store
        .load_runtime_states()?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<RuntimeState>, _>>()?;
    Ok(build_report("loaded", &store.base_dir(), states))
}

pub fn guard_runtime_scope(
    store: &ForgeStore,
    request: RuntimeGuardRequest,
) -> Result<RuntimeGuardReport> {
    let report = evaluate_runtime_guard(request);
    store.record_event(
        "_system",
        "runtime_scope_guard",
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

fn evaluate_runtime_guard(request: RuntimeGuardRequest) -> RuntimeGuardReport {
    let mutating = matches!(
        request.action.as_str(),
        "create" | "update" | "delete" | "patch" | "apply"
    );
    let forge_owned = request.owner == "forge";
    let external = !forge_owned;

    let (allowed, requires_human_approval, decision) = if forge_owned {
        (true, false, "forge_owned_resource".to_string())
    } else if mutating && request.allow_external {
        (true, false, "human_allow_external_resource".to_string())
    } else if mutating && external {
        (false, true, "blocked_external_resource".to_string())
    } else {
        (true, false, "read_only_external_resource".to_string())
    };

    RuntimeGuardReport {
        allowed,
        requires_human_approval,
        decision,
        substrate: request.substrate,
        resource: request.resource,
        namespace: request.namespace,
        action: request.action,
        owner: request.owner,
        scope_rule:
            "Forge may mutate resources it created; external resources require explicit human authorization"
                .to_string(),
    }
}

fn build_report(status: &str, home: &Path, mut runtimes: Vec<RuntimeState>) -> RuntimeSyncReport {
    runtimes.sort_by(|left, right| left.id.cmp(&right.id));
    let usable = runtimes
        .iter()
        .filter(|runtime| runtime.allowed && runtime.installed && runtime.configured)
        .map(|runtime| runtime.id.clone())
        .collect::<Vec<_>>();
    let needs_human_approval = runtimes.iter().any(|runtime| {
        runtime.installed
            && runtime.configured
            && !runtime.allowed
            && runtime.decision_source == "pending_human_approval"
    });
    let install_suggestions = build_install_suggestions(&runtimes);

    RuntimeSyncReport {
        status: status.to_string(),
        home: home.display().to_string(),
        needs_human_approval,
        usable,
        runtimes,
        install_suggestions,
    }
}

fn probe_runtime(
    definition: &RuntimeDefinition,
    home: &Path,
    runtime_paths: &[PathBuf],
) -> RuntimeState {
    let command_path = find_executable(definition.command, runtime_paths);
    let config_evidence = config_evidence(definition.id, home, command_path.is_some());
    let configured = !config_evidence.is_empty();

    RuntimeState {
        id: definition.id.to_string(),
        display_name: definition.display_name.to_string(),
        command: definition.command.to_string(),
        installed: command_path.is_some(),
        configured,
        command_path: command_path.map(|path| path.display().to_string()),
        config_evidence,
        allowed: false,
        decision_source: "unavailable".to_string(),
        async_capable: true,
        ownership_policy: "forge_owned_resources_only_unless_human_authorizes_external_mutation"
            .to_string(),
        synced_at: Utc::now().to_rfc3339(),
    }
}

fn apply_decision(
    state: &mut RuntimeState,
    previous: &BTreeMap<String, RuntimeState>,
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
        if prompt_for_runtime(state)? {
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

fn prompt_for_runtime(state: &RuntimeState) -> Result<bool> {
    print!(
        "Allow Forge to use {} ({}) as an async workflow runtime on this machine? [y/N] ",
        state.display_name, state.command
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let normalized = answer.trim().to_lowercase();
    Ok(matches!(normalized.as_str(), "y" | "yes" | "s" | "sim"))
}

fn load_previous_states(store: &ForgeStore) -> Result<BTreeMap<String, RuntimeState>> {
    let mut previous = BTreeMap::new();
    for value in store.load_runtime_states()? {
        let state: RuntimeState = serde_json::from_value(value)?;
        previous.insert(state.id.clone(), state);
    }
    Ok(previous)
}

fn normalize_set(values: &[String]) -> BTreeSet<String> {
    values.iter().map(|value| value.to_lowercase()).collect()
}

fn find_executable(command: &str, runtime_paths: &[PathBuf]) -> Option<PathBuf> {
    candidate_dirs(runtime_paths)
        .into_iter()
        .map(|directory| directory.join(command))
        .find(|path| is_executable(path))
}

fn candidate_dirs(runtime_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = runtime_paths.to_vec();
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

fn config_evidence(id: &str, home: &Path, installed: bool) -> Vec<String> {
    let mut evidence = Vec::new();
    match id {
        "docker" => {
            if installed {
                evidence.push("command:docker".to_string());
            }
        }
        "kubernetes" | "knative" => {
            for path in kube_config_candidates(home) {
                if path.exists() {
                    evidence.push(path.display().to_string());
                }
            }
            if env::var_os("KUBECONFIG").is_some() {
                evidence.push("env:KUBECONFIG".to_string());
            }
        }
        _ => {}
    }
    evidence
}

fn kube_config_candidates(home: &Path) -> Vec<PathBuf> {
    vec![home.join(".kube/config")]
}

fn build_install_suggestions(runtimes: &[RuntimeState]) -> Vec<RuntimeInstallSuggestion> {
    let docker_ready = runtime_ready(runtimes, "docker");
    let kubernetes_ready = runtime_ready(runtimes, "kubernetes");
    let knative_ready = runtimes
        .iter()
        .find(|runtime| runtime.id == "knative")
        .map(|runtime| runtime.installed && runtime.configured)
        .unwrap_or(false);

    if docker_ready && kubernetes_ready && !knative_ready {
        return vec![RuntimeInstallSuggestion {
            id: "knative".to_string(),
            display_name: "Knative".to_string(),
            reason:
                "Docker and Kubernetes are available; Knative can provide async service nodes for Forge workflows"
                    .to_string(),
            requires_human_approval: true,
            suggested_commands: vec![
                "install kn CLI".to_string(),
                "install Knative Serving into the selected Kubernetes cluster".to_string(),
                "label Forge-owned Knative resources with app.kubernetes.io/managed-by=forge-core"
                    .to_string(),
            ],
        }];
    }

    Vec::new()
}

fn runtime_ready(runtimes: &[RuntimeState], id: &str) -> bool {
    runtimes
        .iter()
        .find(|runtime| runtime.id == id)
        .map(|runtime| runtime.installed && runtime.configured && runtime.allowed)
        .unwrap_or(false)
}
