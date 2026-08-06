use crate::executor::{canonical_executor_id, load_executors};
use crate::storage::FoundryStore;
use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const AGENT_SCHEMA_VERSION: &str = "foundry.agent.v2";
const AGENT_STATE_PREFIX: &str = "agent:";
const GLOBAL_ASSISTANT_ID: &str = "agent_foundry_assistant";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectAgentSpec {
    pub schema_version: String,
    pub id: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub name: String,
    pub kind: String,
    pub system: bool,
    pub role: String,
    pub executor: String,
    pub model: Option<String>,
    pub main_prompt: String,
    pub skills: Vec<String>,
    pub autonomy: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ProjectAgentUpsertInput<'a> {
    pub id: Option<&'a str>,
    pub project_id: &'a str,
    pub project_root: &'a str,
    pub name: &'a str,
    pub role: &'a str,
    pub executor: &'a str,
    pub model: Option<&'a str>,
    pub main_prompt: &'a str,
    pub skills: Vec<String>,
    pub autonomy: &'a str,
    pub enabled: bool,
}

pub fn list_project_agents(store: &FoundryStore) -> Result<Vec<ProjectAgentSpec>> {
    let mut agents = store
        .load_runtime_states()?
        .into_iter()
        .filter(|value| {
            value.get("schema_version").and_then(|item| item.as_str()) == Some(AGENT_SCHEMA_VERSION)
        })
        .map(serde_json::from_value)
        .collect::<Result<Vec<ProjectAgentSpec>, _>>()?;
    agents.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then_with(|| left.project_id.cmp(&right.project_id))
            .then_with(|| agent_kind_order(&left.kind).cmp(&agent_kind_order(&right.kind)))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(agents)
}

pub fn ensure_required_agents(
    store: &FoundryStore,
    projects: &[(String, String)],
) -> Result<Vec<ProjectAgentSpec>> {
    let existing = list_project_agents(store)?;
    if !existing.iter().any(|agent| agent.id == GLOBAL_ASSISTANT_ID) {
        save_system_agent(store, global_assistant())?;
    }
    for (project_id, project_root) in projects {
        if !existing.iter().any(|agent| {
            agent.project_id.as_deref() == Some(project_id) && agent.kind == "orchestrator"
        }) {
            save_system_agent(store, project_orchestrator(project_id, project_root))?;
        }
        if !existing
            .iter()
            .any(|agent| agent.project_id.as_deref() == Some(project_id) && agent.kind == "router")
        {
            save_system_agent(store, project_router(project_id, project_root))?;
        }
    }
    list_project_agents(store)
}

pub fn upsert_project_agent(
    store: &FoundryStore,
    input: ProjectAgentUpsertInput<'_>,
) -> Result<ProjectAgentSpec> {
    let project_id = required(input.project_id, "project_id")?;
    let project_root = required(input.project_root, "project_root")?;
    let name = required(input.name, "name")?;
    let role = required(input.role, "role")?;
    let executor = canonical_executor_id(&required(input.executor, "executor")?);
    let main_prompt = required(input.main_prompt, "main_prompt")?;
    let autonomy = required(input.autonomy, "autonomy")?;
    if !matches!(
        autonomy.as_str(),
        "supervised" | "approval_required" | "autonomous"
    ) {
        bail!("invalid project agent autonomy `{autonomy}`");
    }
    if input.enabled {
        let report = load_executors(store)?;
        let state = report.executors.iter().find(|state| state.id == executor);
        if !state.is_some_and(|state| {
            state.installed && state.configured && state.non_interactive_ready && state.allowed
        }) {
            bail!("executor `{executor}` is not usable by Foundry policy");
        }
    }

    let now = Utc::now().to_rfc3339();
    let id = input
        .id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("agent_{}", Uuid::new_v4().simple()));
    let existing = list_project_agents(store)?;
    let previous = existing.iter().find(|agent| agent.id == id);
    if previous.is_some_and(|agent| agent.system) {
        bail!("system agent `{id}` cannot be replaced through the specialist endpoint");
    }
    if previous.is_some_and(|agent| agent.project_id.as_deref() != Some(project_id.as_str())) {
        bail!("project agent `{id}` belongs to another project");
    }
    let agent = ProjectAgentSpec {
        schema_version: AGENT_SCHEMA_VERSION.to_string(),
        id,
        scope: "project".to_string(),
        project_id: Some(project_id),
        project_root: Some(project_root),
        name,
        kind: "specialist".to_string(),
        system: false,
        role,
        executor,
        model: input.model.and_then(clean_optional),
        main_prompt,
        skills: clean_unique(input.skills),
        autonomy,
        enabled: input.enabled,
        created_at: previous
            .map(|agent| agent.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    store.save_runtime_state(
        &format!("{AGENT_STATE_PREFIX}{}", agent.id),
        &serde_json::to_value(&agent)?,
    )?;
    store.record_event(
        agent.project_id.as_deref().unwrap_or("_system"),
        "project_agent_upserted",
        &serde_json::to_value(&agent)?,
    )?;
    Ok(agent)
}

pub fn remove_project_agent(
    store: &FoundryStore,
    project_id: &str,
    agent_id: &str,
) -> Result<ProjectAgentSpec> {
    let project_id = required(project_id, "project_id")?;
    let agent_id = required(agent_id, "agent_id")?;
    let agent = list_project_agents(store)?
        .into_iter()
        .find(|agent| agent.id == agent_id && agent.project_id.as_deref() == Some(&project_id))
        .ok_or_else(|| {
            anyhow::anyhow!("project agent `{agent_id}` was not found in project `{project_id}`")
        })?;
    if agent.system {
        bail!("required project agent `{agent_id}` cannot be removed");
    }
    store.delete_runtime_state(&format!("{AGENT_STATE_PREFIX}{}", agent.id))?;
    store.record_event(
        &project_id,
        "project_agent_removed",
        &serde_json::json!({
            "project_id": project_id,
            "agent_id": agent.id,
            "origin": "foundry_ops",
        }),
    )?;
    Ok(agent)
}

pub fn project_agent_routing_context(store: &FoundryStore, project_id: &str) -> Result<String> {
    let agents = list_project_agents(store)?
        .into_iter()
        .filter(|agent| {
            agent.enabled
                && (agent.scope == "foundry" || agent.project_id.as_deref() == Some(project_id))
        })
        .map(|agent| {
            format!(
                "{} [{}] role={} executor={} model={} skills={} primary_prompt={}",
                agent.name,
                agent.kind,
                agent.role,
                agent.executor,
                agent.model.as_deref().unwrap_or("auto"),
                agent.skills.join(","),
                agent.main_prompt,
            )
        })
        .collect::<Vec<_>>();
    Ok(format!("Available project agents: {}", agents.join(" | ")))
}

pub fn get_agent(store: &FoundryStore, agent_id: &str) -> Result<ProjectAgentSpec> {
    list_project_agents(store)?
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("agent `{agent_id}` was not found"))
}

fn global_assistant() -> ProjectAgentSpec {
    system_agent(
        GLOBAL_ASSISTANT_ID,
        "foundry",
        None,
        None,
        "Foundry Assistant",
        "assistant",
        "Cross-project assistant and orchestrator for authorized Foundry context.",
        "foundry",
        "Help the user find projects, summarize progress, identify blockers and coordinate authorized work across projects without leaking tenant context.",
        &["project-discovery", "progress-summary", "cross-project-orchestration", "search", "governance"],
    )
}

fn project_orchestrator(project_id: &str, project_root: &str) -> ProjectAgentSpec {
    system_agent(
        &format!("agent_orchestrator_{project_id}"),
        "project",
        Some(project_id),
        Some(project_root),
        "Project Orchestrator",
        "orchestrator",
        "Owns decomposition, context, agent lifecycle, validation and completion for this project.",
        "foundry",
        "Orchestrate this project safely. Open bounded agents when useful, preserve Foundry as source of truth, query ai-limits before parallel handoffs, and validate outcomes before completion.",
        &["agent-orchestration", "task-decomposition", "context-routing", "ai-limits", "validation"],
    )
}

fn project_router(project_id: &str, project_root: &str) -> ProjectAgentSpec {
    system_agent(
        &format!("agent_router_{project_id}"),
        "project",
        Some(project_id),
        Some(project_root),
        "Project Router",
        "router",
        "Selects the best available agent, executor and model for each task.",
        "auto",
        "Route each task using the primary prompt, task objective, required skills, available project agents, machine readiness, executor policy, cost and fresh quota evidence.",
        &["executor-routing", "skill-matching", "prompt-analysis", "capacity-awareness", "quota-routing"],
    )
}

#[allow(clippy::too_many_arguments)]
fn system_agent(
    id: &str,
    scope: &str,
    project_id: Option<&str>,
    project_root: Option<&str>,
    name: &str,
    kind: &str,
    role: &str,
    executor: &str,
    main_prompt: &str,
    skills: &[&str],
) -> ProjectAgentSpec {
    let now = Utc::now().to_rfc3339();
    ProjectAgentSpec {
        schema_version: AGENT_SCHEMA_VERSION.to_string(),
        id: id.to_string(),
        scope: scope.to_string(),
        project_id: project_id.map(ToString::to_string),
        project_root: project_root.map(ToString::to_string),
        name: name.to_string(),
        kind: kind.to_string(),
        system: true,
        role: role.to_string(),
        executor: executor.to_string(),
        model: None,
        main_prompt: main_prompt.to_string(),
        skills: skills.iter().map(|skill| (*skill).to_string()).collect(),
        autonomy: "autonomous".to_string(),
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn save_system_agent(store: &FoundryStore, agent: ProjectAgentSpec) -> Result<()> {
    store.save_runtime_state(
        &format!("{AGENT_STATE_PREFIX}{}", agent.id),
        &serde_json::to_value(agent)?,
    )
}

fn required(value: &str, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("missing required field `{field}`");
    }
    Ok(value.to_string())
}

fn clean_optional(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn clean_unique(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn agent_kind_order(kind: &str) -> usize {
    match kind {
        "assistant" => 0,
        "orchestrator" => 1,
        "router" => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::ExecutorState;
    use tempfile::tempdir;

    fn usable_executor(store: &FoundryStore, id: &str) {
        let state = ExecutorState {
            id: id.to_string(),
            display_name: id.to_string(),
            command: id.to_string(),
            installed: true,
            configured: true,
            command_path: Some(id.to_string()),
            config_evidence: vec![],
            non_interactive_ready: true,
            probe_evidence: vec![],
            foundry_first_ready: false,
            foundry_first_entrypoint: None,
            harness_status: None,
            allowed: true,
            decision_source: "human_allow".to_string(),
            synced_at: Utc::now().to_rfc3339(),
        };
        store
            .save_executor_state(id, &serde_json::to_value(state).unwrap())
            .unwrap();
    }

    #[test]
    fn required_agents_and_specialists_are_scoped_and_persistent() {
        let temp = tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        usable_executor(&store, "codex");
        ensure_required_agents(
            &store,
            &[
                ("project-a".to_string(), "C:/a".to_string()),
                ("project-b".to_string(), "C:/b".to_string()),
            ],
        )
        .unwrap();
        let specialist = upsert_project_agent(
            &store,
            ProjectAgentUpsertInput {
                id: None,
                project_id: "project-a",
                project_root: "C:/a",
                name: "Builder",
                role: "Implementation",
                executor: "codex",
                model: None,
                main_prompt: "Implement the task and verify it.",
                skills: vec!["coding".to_string(), "tests".to_string()],
                autonomy: "supervised",
                enabled: true,
            },
        )
        .unwrap();
        let agents = list_project_agents(&store).unwrap();
        assert_eq!(
            agents
                .iter()
                .filter(|agent| agent.kind == "assistant")
                .count(),
            1
        );
        assert_eq!(
            agents
                .iter()
                .filter(|agent| agent.kind == "orchestrator")
                .count(),
            2
        );
        assert_eq!(
            agents.iter().filter(|agent| agent.kind == "router").count(),
            2
        );
        assert_eq!(
            agents
                .iter()
                .filter(|agent| agent.kind == "specialist")
                .count(),
            1
        );
        assert!(remove_project_agent(&store, "project-a", "agent_orchestrator_project-a").is_err());
        remove_project_agent(&store, "project-a", &specialist.id).unwrap();
        assert_eq!(list_project_agents(&store).unwrap().len(), 5);
    }
}
