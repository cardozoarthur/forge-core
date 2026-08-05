use crate::executor::canonical_executor_id;
use crate::graph::{
    create_workflow, default_execution_policy, AtomicTask, CoreParallelLaneSpec,
    CoreParallelTeamSpec, ExecutorKind, NodeBrainAgentSlotSpec, TaskStatus, ValidationRule,
    Workflow, WorkflowRevision, CORE_PARALLEL_TEAM_SCHEMA_VERSION,
};
use crate::intent::parse_intent;
use crate::request::{create_run_record, save_run_record};
use crate::storage::{open_configured_connection, FoundryStore};
use crate::worktree::{
    bind_worktree, bound_worktree_mutation_claim, create_worktree, discover_worktrees,
    list_registered_worktrees, WorktreeCreateOptions, WorktreeMutationClaim, WorktreeRecord,
};
use anyhow::{anyhow, Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamworkResponse {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub run_id: Option<String>,
    pub goal: String,
    pub detached: bool,
    pub strategy: TeamworkStrategy,
    pub roster: TeamworkRoster,
    #[serde(default)]
    pub workspace_isolation: TeamworkWorkspaceIsolation,
    #[serde(default)]
    pub planning_evidence: TeamworkPlanningEvidence,
    pub tasks: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmarks: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TeamworkPlanningEvidence {
    pub schema_version: String,
    pub status: String,
    pub completed_task_ids: Vec<String>,
    pub evidence: Vec<String>,
    pub reason: String,
}

impl Default for TeamworkPlanningEvidence {
    fn default() -> Self {
        Self {
            schema_version: "foundry.teamwork.planning_evidence.v1".to_string(),
            status: "not_materialized".to_string(),
            completed_task_ids: Vec::new(),
            evidence: Vec::new(),
            reason: "teamwork planning evidence has not been materialized".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TeamworkWorktreePrepareOptions {
    pub workflow_id: String,
    pub repository: PathBuf,
    pub worktree_root: PathBuf,
    pub branch_prefix: String,
    pub origin: String,
    pub allow_repository_mutation: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TeamworkWorktreePreparationEntry {
    pub task_id: String,
    pub task_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brain: Option<String>,
    pub parallel_branch: bool,
    pub path: String,
    pub branch: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<WorktreeMutationClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TeamworkWorktreePreparationReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub repository: String,
    pub worktree_root: String,
    pub branch_prefix: String,
    pub origin: String,
    pub mutation_authorized: bool,
    pub planned_worktrees: usize,
    pub parallel_branch_worktrees: usize,
    pub supporting_agent_worktrees: usize,
    pub created_worktrees: usize,
    pub bound_existing_worktrees: usize,
    pub reused_worktrees: usize,
    pub commands: Vec<Vec<String>>,
    pub entries: Vec<TeamworkWorktreePreparationEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TeamworkWorkspaceIsolation {
    pub schema_version: String,
    pub status: String,
    pub mutation_claim: String,
    pub task_scoped_worktree_required: bool,
    pub shared_checkout_parallel_mutation_allowed: bool,
    pub claim_frozen_in_lease_and_receipt: bool,
    pub branch_task_ids: Vec<String>,
    pub reason: String,
}

impl Default for TeamworkWorkspaceIsolation {
    fn default() -> Self {
        Self {
            schema_version: "foundry.teamwork.workspace_isolation.v1".to_string(),
            status: "task_worktree_bindings_required".to_string(),
            mutation_claim: "exclusive_worktree_mutation".to_string(),
            task_scoped_worktree_required: true,
            shared_checkout_parallel_mutation_allowed: false,
            claim_frozen_in_lease_and_receipt: true,
            branch_task_ids: Vec::new(),
            reason: "Logical independence does not make concurrent writes to one checkout safe; every mutating branch needs its own task-bound Git worktree.".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamworkRoster {
    pub agent_count: usize,
    pub max_parallel_agents: usize,
    pub policy: String,
    pub roles: Vec<TeamworkRole>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamworkRole {
    pub slot_id: String,
    pub role: String,
    pub brain: String,
    pub parallel_group: String,
    pub responsibility: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamworkStrategy {
    pub schema_version: String,
    pub mode: String,
    pub source_evidence: Vec<String>,
    pub phases: Vec<TeamworkPhase>,
    pub recommended_agent_count: usize,
    pub max_parallel_agents: usize,
    #[serde(default)]
    pub parallelism: TeamworkParallelConfig,
    pub primary_brains: Vec<String>,
    pub legacy_brains_invalidated: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamworkPhase {
    pub phase: String,
    pub owner_role: String,
    pub execution_model: String,
    pub exit_gate: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TeamworkParallelConfig {
    #[serde(default = "default_teamwork_lanes")]
    pub lanes: Vec<TeamworkLaneConfig>,
    #[serde(default = "default_teamwork_max_parallel_agents")]
    pub max_parallel_agents: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TeamworkLaneConfig {
    pub id: String,
    pub brain: String,
    pub agent_count: usize,
    pub parallel_group: String,
    pub responsibility: String,
}

impl From<&CoreParallelTeamSpec> for TeamworkParallelConfig {
    fn from(spec: &CoreParallelTeamSpec) -> Self {
        Self {
            lanes: spec
                .lanes
                .iter()
                .map(|lane| TeamworkLaneConfig {
                    id: lane.id.clone(),
                    brain: lane.executor_id.clone(),
                    agent_count: lane.agent_count,
                    parallel_group: lane.parallel_group.clone(),
                    responsibility: lane.responsibility.clone(),
                })
                .collect(),
            max_parallel_agents: spec.max_parallel_agents,
        }
    }
}

impl Default for TeamworkParallelConfig {
    fn default() -> Self {
        Self {
            lanes: default_teamwork_lanes(),
            max_parallel_agents: default_teamwork_max_parallel_agents(),
        }
    }
}

fn default_teamwork_lanes() -> Vec<TeamworkLaneConfig> {
    vec![TeamworkLaneConfig {
        id: "worker".to_string(),
        brain: "auto".to_string(),
        agent_count: 2,
        parallel_group: TEAMWORK_IMPLEMENTATION_WAVE.to_string(),
        responsibility: "Execute independent bounded delivery slices.".to_string(),
    }]
}

fn default_teamwork_max_parallel_agents() -> usize {
    2
}

const TEAMWORK_IMPLEMENTATION_WAVE: &str = "implementation-wave-001";
const TEAMWORK_DOMAIN_JOIN_WAVE: &str = "domain-join-wave-002";
const TEAMWORK_FINAL_AUDIT_WAVE: &str = "final-audit-wave-003";
pub const TEAMWORK_GIT_FAN_IN_VALIDATION_KIND: &str = "git_dependency_fan_in";
const MAX_TEAMWORK_BRANCHES: usize = 64;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FetchBenchmarkEval {
    lmsys_chatbot_arena: Option<i64>,
    mmlu: Option<f64>,
    human_eval: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FetchBenchmarkItem {
    brain: String,
    mismatch_penalty: Option<f64>,
    evals: FetchBenchmarkEval,
}

#[derive(Debug, Clone)]
struct CachedBrain {
    brain_id: String,
    lmsys_score: i64,
    mmlu_score: f64,
    human_eval_score: f64,
    updated_at: String,
}

pub fn plan_teamwork_workflow(
    store: &FoundryStore,
    goal: &str,
    detached: bool,
    bypass_cache: bool,
) -> Result<TeamworkResponse> {
    plan_teamwork_workflow_with_config(
        store,
        goal,
        detached,
        bypass_cache,
        TeamworkParallelConfig::default(),
    )
}

pub fn plan_teamwork_workflow_with_config(
    store: &FoundryStore,
    goal: &str,
    detached: bool,
    bypass_cache: bool,
    parallelism: TeamworkParallelConfig,
) -> Result<TeamworkResponse> {
    if goal.trim().is_empty() {
        return Err(anyhow!("Goal cannot be empty"));
    }
    validate_teamwork_parallel_config(&parallelism)?;

    let intent = parse_intent(goal);
    let mut workflow = create_workflow(intent);

    // Roster and Heuristics Logic
    let conn = open_configured_connection(store.path())?;

    // 1. Query disallowed brains from executor policy
    let mut disallowed_brains = HashSet::new();
    disallowed_brains.insert("gemini".to_string());
    let mut policy_decisions = BTreeMap::<String, (bool, bool)>::new();
    let policy_table_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='executor_policy')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if policy_table_exists {
        if let Ok(mut stmt) = conn.prepare("SELECT id, data_json FROM executor_policy") {
            if let Ok(mut rows) = stmt.query([]) {
                while let Ok(Some(row)) = rows.next() {
                    let id: String = row.get(0)?;
                    let data_json_str: String = row.get(1)?;
                    if let Ok(data_json) = serde_json::from_str::<serde_json::Value>(&data_json_str)
                    {
                        if let Some(allowed) = data_json["allowed"].as_bool() {
                            let canonical_id = canonical_executor_id(&id);
                            let is_canonical_id = id.trim().to_ascii_lowercase() == canonical_id;
                            let replace = match policy_decisions.get(&canonical_id) {
                                Some((current_is_canonical, current_allowed)) => {
                                    (is_canonical_id && !current_is_canonical)
                                        || (is_canonical_id == *current_is_canonical
                                            && !allowed
                                            && *current_allowed)
                                }
                                None => true,
                            };
                            if replace {
                                policy_decisions.insert(canonical_id, (is_canonical_id, allowed));
                            }
                        }
                    }
                }
            }
        }
    }

    for (brain, (_, allowed)) in policy_decisions {
        if !allowed {
            disallowed_brains.insert(brain);
        }
    }

    // 2. Fetch and Cache Benchmarks if FOUNDRY_BENCHMARK_URL is configured
    let benchmark_url = crate::brand::env_var("FOUNDRY_BENCHMARK_URL").ok();
    let mut benchmark_scores = Vec::new();
    let mut benchmarks_json = None;

    let cache_table_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='web_benchmark_cache')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    let mut hit_cache = false;
    if cache_table_exists && !bypass_cache {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT brain_id, lmsys_score, mmlu_score, human_eval_score, updated_at FROM web_benchmark_cache"
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok(CachedBrain {
                    brain_id: row.get(0)?,
                    lmsys_score: row.get(1)?,
                    mmlu_score: row.get(2)?,
                    human_eval_score: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            }) {
                let mut cached_list = Vec::new();
                for cached in rows.flatten() {
                    cached_list.push(cached);
                }

                // Check if cache contains unexpired records (e.g. within 1 hour)
                if !cached_list.is_empty() {
                    let mut all_unexpired = true;
                    let now = chrono::Utc::now();
                    for item in &cached_list {
                        if let Ok(parsed_time) = chrono::DateTime::parse_from_rfc3339(&item.updated_at) {
                            if now.signed_duration_since(parsed_time).num_seconds() > 86400 {
                                all_unexpired = false;
                                break;
                            }
                        } else {
                            all_unexpired = false;
                            break;
                        }
                    }

                    if all_unexpired {
                        hit_cache = true;
                        benchmark_scores = cached_list;
                    }
                }
            }
        }
    }

    if !hit_cache {
        if let Some(ref url) = benchmark_url {
            // Fetch from URL
            if let Ok(fetched) = fetch_benchmarks_from_url(url) {
                if let Ok(items) =
                    serde_json::from_value::<Vec<FetchBenchmarkItem>>(fetched.clone())
                {
                    if !cache_table_exists {
                        let _ = conn.execute(
                            "CREATE TABLE IF NOT EXISTS web_benchmark_cache (
                                brain_id TEXT PRIMARY KEY,
                                lmsys_score INTEGER NOT NULL,
                                mmlu_score REAL NOT NULL,
                                human_eval_score REAL NOT NULL,
                                updated_at TEXT NOT NULL
                            );",
                            [],
                        );
                    }
                    for item in &items {
                        let lmsys = item.evals.lmsys_chatbot_arena.unwrap_or(0);
                        let mmlu = item.evals.mmlu.unwrap_or(0.0);
                        let human = item.evals.human_eval.unwrap_or(0.0);
                        let now_str = chrono::Utc::now().to_rfc3339();
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO web_benchmark_cache (brain_id, lmsys_score, mmlu_score, human_eval_score, updated_at) VALUES (?, ?, ?, ?, ?)",
                            params![item.brain, lmsys, mmlu, human, now_str],
                        );
                        benchmark_scores.push(CachedBrain {
                            brain_id: item.brain.clone(),
                            lmsys_score: lmsys,
                            mmlu_score: mmlu,
                            human_eval_score: human,
                            updated_at: now_str,
                        });
                    }
                    benchmarks_json = Some(serde_json::json!({
                        "scores": items
                    }));
                }
            }
        }
    } else {
        // Reconstruct benchmarks JSON from cache
        let scores: Vec<serde_json::Value> = benchmark_scores
            .iter()
            .map(|item| {
                serde_json::json!({
                    "brain": item.brain_id,
                    "mismatch_penalty": 0.0,
                    "evals": {
                        "lmsys_chatbot_arena": item.lmsys_score,
                        "mmlu": item.mmlu_score,
                        "human_eval": item.human_eval_score
                    }
                })
            })
            .collect();
        benchmarks_json = Some(serde_json::json!({
            "scores": scores
        }));
    }

    // 3. Select Worker Brain based on Heuristics
    let lower_goal = goal.to_lowercase();
    let preferred_list = if lower_goal.contains("visual")
        || lower_goal.contains("css")
        || lower_goal.contains("html")
        || lower_goal.contains("layout")
        || lower_goal.contains("ui")
        || lower_goal.contains("dashboard")
        || lower_goal.contains("page")
    {
        vec!["agy", "codex", "opencode"]
    } else {
        vec!["codex", "agy", "opencode"]
    };

    // Find first allowed brain in the preference list
    let mut selected_worker_brain = None;
    for brain in &preferred_list {
        if !disallowed_brains.contains(*brain) {
            selected_worker_brain = Some((*brain).to_string());
            break;
        }
    }

    if selected_worker_brain.is_none() {
        for brain in &["codex", "agy", "opencode"] {
            if !disallowed_brains.contains(*brain) {
                selected_worker_brain = Some((*brain).to_string());
                break;
            }
        }
    }

    // If benchmarks override is available, check if the benchmarked brain is allowed and has a high score
    if !benchmark_scores.is_empty() {
        let mut best_benchmarked: Option<&CachedBrain> = None;
        for score in &benchmark_scores {
            if !disallowed_brains.contains(&score.brain_id) {
                if let Some(best) = best_benchmarked {
                    if score.human_eval_score > best.human_eval_score {
                        best_benchmarked = Some(score);
                    }
                } else {
                    best_benchmarked = Some(score);
                }
            }
        }
        if let Some(best) = best_benchmarked {
            selected_worker_brain = Some(best.brain_id.clone());
        }
    }

    let advisory_worker_brain = match selected_worker_brain {
        Some(brain) => brain,
        None => {
            return Err(anyhow!(
                "No allowed modern brain found in executor policy for role Worker; legacy Gemini is invalidated"
            ))
        }
    };

    let mut selected_orchestrator_brain = None;
    for brain in &["codex", "agy", "opencode"] {
        if !disallowed_brains.contains(*brain) {
            selected_orchestrator_brain = Some((*brain).to_string());
            break;
        }
    }
    let orchestrator_brain = match selected_orchestrator_brain {
        Some(brain) => brain,
        None => {
            return Err(anyhow!(
                "No allowed modern brain found in executor policy for role Orchestrator; legacy Gemini is invalidated"
            ))
        }
    };

    let mut selected_auditor_brain = None;
    for brain in &["opencode", "codex", "agy"] {
        if !disallowed_brains.contains(*brain) {
            selected_auditor_brain = Some((*brain).to_string());
            break;
        }
    }
    let auditor_brain = match selected_auditor_brain {
        Some(brain) => brain,
        None => {
            return Err(anyhow!(
                "No allowed modern brain found in executor policy for role Auditor; legacy Gemini is invalidated"
            ))
        }
    };

    let parallelism =
        resolve_teamwork_parallel_config(parallelism, &advisory_worker_brain, &disallowed_brains)?;
    let auditor_brain = ["codex", "agy"]
        .into_iter()
        .find(|candidate| {
            parallelism
                .lanes
                .iter()
                .any(|lane| canonical_executor_id(&lane.brain) == *candidate)
        })
        .map(str::to_string)
        .unwrap_or(auditor_brain);
    let lane_summary = parallelism
        .lanes
        .iter()
        .map(|lane| format!("{}={}x{}", lane.id, lane.agent_count, lane.brain))
        .collect::<Vec<_>>()
        .join(", ");

    // 4. Assemble the Roster
    let roles = teamwork_roles_for_config(&parallelism, orchestrator_brain, auditor_brain);
    let strategy = TeamworkStrategy {
        schema_version: "foundry.teamwork.strategy.v1".to_string(),
        mode: if detached {
            "detached_teamwork_run".to_string()
        } else {
            "planned_teamwork_run".to_string()
        },
        source_evidence: vec![
            "Antigravity agy exposes /teamwork-preview as an internal slash-command pattern, not as a public CLI subcommand.".to_string(),
            "Observed flow: prompt draft, user approval, delegated teamwork_preview subagents, execution approvals, and artifact-bound handoff.".to_string(),
            "Foundry adaptation keeps workflow state, context routing, validation gates, artifacts, and executor policy inside Foundry.".to_string(),
            format!("Goal and benchmark heuristics recommended {advisory_worker_brain}; Foundry resolved the configured lanes as {lane_summary} without transferring workflow authority to an executor."),
        ],
        phases: vec![
            TeamworkPhase {
                phase: "prompt_and_goal_review".to_string(),
                owner_role: "Orchestrator".to_string(),
                execution_model: "draft approved objective and decompose into graph".to_string(),
                exit_gate: "goal, constraints, and acceptance criteria are explicit".to_string(),
            },
            TeamworkPhase {
                phase: "parallel_execution_wave".to_string(),
                owner_role: "ConfiguredLaneWorkers".to_string(),
                execution_model: "run every configured lane as independent bounded branches under its declared brain and parallel group".to_string(),
                exit_gate: "every configured lane branch has persisted semantic output and validation evidence".to_string(),
            },
            TeamworkPhase {
                phase: "domain_convergence_wave".to_string(),
                owner_role: "ConfiguredLaneIntegrators".to_string(),
                execution_model: "join each lane independently before the final cross-lane audit".to_string(),
                exit_gate: "every configured lane join is definitively ready".to_string(),
            },
            TeamworkPhase {
                phase: "audit_and_promotion".to_string(),
                owner_role: "Auditor".to_string(),
                execution_model: "review, validate, request rework, and promote only with evidence".to_string(),
                exit_gate: "validation rules pass and no unresolved impediment remains".to_string(),
            },
        ],
        recommended_agent_count: roles.len(),
        max_parallel_agents: parallelism.max_parallel_agents,
        parallelism: parallelism.clone(),
        primary_brains: vec![
            "codex".to_string(),
            "agy".to_string(),
            "opencode".to_string(),
        ],
        legacy_brains_invalidated: vec!["gemini".to_string()],
    };

    apply_teamwork_parallel_topology(&mut workflow, &roles, &parallelism)?;
    let planning_evidence = complete_materialized_teamwork_planning_nodes(&mut workflow)?;
    let run = create_run_record(&workflow, "foundry_cli", "accepted");
    let run_id = if detached {
        Some(run.run_id.clone())
    } else {
        None
    };
    let tasks_json = workflow
        .tasks
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let response = TeamworkResponse {
        schema_version: "foundry.teamwork.plan.v1".to_string(),
        status: "planned".to_string(),
        workflow_id: workflow.id.clone(),
        run_id,
        goal: goal.to_string(),
        detached,
        strategy,
        roster: TeamworkRoster {
            agent_count: roles.len(),
            max_parallel_agents: parallelism.max_parallel_agents,
            policy: "foundry_authority_configured_parallel_lanes".to_string(),
            roles,
        },
        workspace_isolation: TeamworkWorkspaceIsolation {
            branch_task_ids: parallelism
                .lanes
                .iter()
                .flat_map(|lane| {
                    (1..=lane.agent_count).map(|index| format!("task-005-{}-{index:03}", lane.id))
                })
                .collect(),
            ..TeamworkWorkspaceIsolation::default()
        },
        planning_evidence,
        tasks: tasks_json,
        benchmarks: benchmarks_json,
    };
    let event_data = serde_json::to_value(&response)?;
    store.with_transaction(|| {
        store.save_workflow(&workflow)?;
        save_run_record(store, &run)?;
        store.record_event(&workflow.id, "teamwork_planned", &event_data)?;
        Ok(())
    })?;
    Ok(response)
}

fn complete_materialized_teamwork_planning_nodes(
    workflow: &mut Workflow,
) -> Result<TeamworkPlanningEvidence> {
    let required = ["task-001", "task-002", "task-003", "task-004"];
    let evidence = vec![
        "task-001: parse_intent produced the persisted IntentSpec used to create this workflow"
            .to_string(),
        "task-002: create_workflow materialized requirements, deliverables, risks and validation contracts"
            .to_string(),
        "task-003: apply_teamwork_parallel_topology materialized configured lanes, roles, dependencies and brain routing"
            .to_string(),
        "task-004: the task-local context requirements and routing contracts were persisted before dispatch"
            .to_string(),
    ];
    for task_id in required {
        let task = workflow
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .with_context(|| {
                format!(
                    "teamwork planning node {task_id} is missing from the materialized workflow"
                )
            })?;
        task.status = TaskStatus::Completed;
        task.active_impediments.clear();
        task.work_item.backlog_state = "done".to_string();
        task.work_item.impediments.clear();
        task.work_item.goal_validation.definitively_ready = true;
        for subtask in &mut task.work_item.subtasks {
            subtask.status = TaskStatus::Completed;
        }
        task.version = task.version.saturating_add(1);
    }
    normalize_teamwork_dependency_versions(workflow);
    let revision = workflow
        .revisions
        .last()
        .map_or(1, |item| item.revision.saturating_add(1));
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: "teamwork.core".to_string(),
        change_type: "teamwork_planning_nodes_materialized".to_string(),
        summary: format!(
            "completed deterministic planning nodes {} because the teamwork plan already persisted their outputs; branches and joins remain pending",
            required.join(", ")
        ),
        created_at: chrono::Utc::now(),
    });
    Ok(TeamworkPlanningEvidence {
        schema_version: "foundry.teamwork.planning_evidence.v1".to_string(),
        status: "materialized".to_string(),
        completed_task_ids: required.into_iter().map(str::to_string).collect(),
        evidence,
        reason: "Foundry completed only the deterministic meta-work already performed by plan_teamwork_workflow; executor branches, joins, documentation and promotion remain pending validation."
            .to_string(),
    })
}

#[derive(Debug, Clone)]
enum TeamworkWorktreeAction {
    Create,
    BindExisting(Box<WorktreeRecord>),
    Reuse(Box<(WorktreeRecord, WorktreeMutationClaim)>),
}

#[derive(Debug, Clone)]
struct PlannedTeamworkWorktree {
    task_id: String,
    task_title: String,
    brain: Option<String>,
    parallel_branch: bool,
    path: PathBuf,
    branch: String,
    action: TeamworkWorktreeAction,
    commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Copy)]
struct TeamworkWorktreeReportContext<'a> {
    workflow_id: &'a str,
    repository: &'a Path,
    worktree_root: &'a Path,
    branch_prefix: &'a str,
    origin: &'a str,
    mutation_authorized: bool,
}

pub fn prepare_teamwork_worktrees(
    store: &FoundryStore,
    options: TeamworkWorktreePrepareOptions,
) -> Result<TeamworkWorktreePreparationReport> {
    let workflow_id = required_teamwork_text(&options.workflow_id, "workflow id")?;
    let origin = required_teamwork_text(&options.origin, "worktree preparation origin")?;
    let branch_prefix = normalize_teamwork_branch_prefix(&options.branch_prefix)?;
    let workflow = store.load_workflow(workflow_id)?;
    let discovery = discover_worktrees(&options.repository)?;
    let repository = process_compatible_path(
        &fs::canonicalize(&discovery.repository_root).with_context(|| {
            format!(
                "failed to resolve teamwork repository {}",
                discovery.repository_root
            )
        })?,
    );
    let worktree_root = normalize_teamwork_worktree_root(&options.worktree_root, &repository)?;
    let registered = list_registered_worktrees(store, Some(&repository), None)?.worktrees;
    let workflow_bound = list_registered_worktrees(store, None, Some(workflow_id))?.worktrees;
    let mut tasks = Vec::new();
    for task in workflow
        .tasks
        .iter()
        .filter(|task| task.status != TaskStatus::Completed)
    {
        let brain = teamwork_external_task_brain(task)?;
        if brain.is_some() || teamwork_task_requires_git_fan_in_worktree(task) {
            tasks.push((task, brain));
        }
    }
    tasks.sort_by(|(left, _), (right, _)| left.id.cmp(&right.id));

    let mut planned_paths = BTreeSet::new();
    let mut planned_branches = BTreeSet::new();
    let mut plans = Vec::with_capacity(tasks.len());
    for (task, brain) in tasks {
        validate_teamwork_task_path_component(&task.id)?;
        let path = worktree_root.join(&task.id);
        if !path.starts_with(&worktree_root) || !planned_paths.insert(path.clone()) {
            return Err(anyhow!(
                "teamwork worktree path collision for task {} at {}",
                task.id,
                path.display()
            ));
        }
        let branch = format!("{branch_prefix}/{}", task.id);
        validate_teamwork_git_branch(&repository, &branch)?;
        if !planned_branches.insert(branch.clone()) {
            return Err(anyhow!(
                "teamwork worktree branch collision for task {} at {}",
                task.id,
                branch
            ));
        }

        if let Some(bound_elsewhere) = workflow_bound.iter().find(|record| {
            !process_paths_equal(Path::new(&record.worktree_root), &path)
                && record.bindings.iter().any(|binding| {
                    binding.workflow_id == workflow_id
                        && binding.task_id.as_deref() == Some(task.id.as_str())
                })
        }) {
            return Err(anyhow!(
                "teamwork task {} is already bound to worktree {}; refusing to move its execution root to {}",
                task.id,
                bound_elsewhere.worktree_root,
                path.display()
            ));
        }

        let existing = registered
            .iter()
            .find(|record| process_paths_equal(Path::new(&record.worktree_root), &path))
            .cloned();
        let mut commands = Vec::new();
        let action = if let Some(record) = existing {
            validate_existing_teamwork_worktree(&record, workflow_id, task.id.as_str(), &branch)?;
            if let Some(claim) = bound_worktree_mutation_claim(store, workflow_id, &task.id)? {
                if claim.worktree_id != record.id || claim.binding_scope != "task" {
                    return Err(anyhow!(
                        "teamwork task {} resolved a conflicting worktree claim {} with scope {}",
                        task.id,
                        claim.worktree_id,
                        claim.binding_scope
                    ));
                }
                TeamworkWorktreeAction::Reuse(Box::new((record, claim)))
            } else {
                commands.push(teamwork_worktree_bind_command(
                    store,
                    &path,
                    workflow_id,
                    &task.id,
                    origin,
                ));
                TeamworkWorktreeAction::BindExisting(Box::new(record))
            }
        } else {
            if path.exists() {
                return Err(anyhow!(
                    "teamwork worktree destination already exists but is not registered: {}",
                    path.display()
                ));
            }
            if let Some(conflict) = discovery.worktrees.iter().find(|worktree| {
                Path::new(&worktree.path) == path
                    || worktree.branch.as_deref() == Some(branch.as_str())
            }) {
                return Err(anyhow!(
                    "teamwork worktree plan for task {} collides with Git worktree {} branch {}",
                    task.id,
                    conflict.path,
                    conflict.branch.as_deref().unwrap_or("detached")
                ));
            }
            if teamwork_git_branch_exists(&repository, &branch)? {
                return Err(anyhow!(
                    "teamwork worktree branch already exists without the expected registered task binding: {branch}"
                ));
            }
            commands.push(teamwork_worktree_create_command(
                store,
                &repository,
                &path,
                &branch,
            ));
            commands.push(teamwork_worktree_bind_command(
                store,
                &path,
                workflow_id,
                &task.id,
                origin,
            ));
            TeamworkWorktreeAction::Create
        };
        plans.push(PlannedTeamworkWorktree {
            task_id: task.id.clone(),
            task_title: task.title.clone(),
            brain,
            parallel_branch: task
                .node_brain_routing
                .agent_slots
                .iter()
                .any(|slot| slot.parallel_group == TEAMWORK_IMPLEMENTATION_WAVE),
            path,
            branch,
            action,
            commands,
        });
    }

    if !options.allow_repository_mutation {
        return Ok(teamwork_worktree_report(
            TeamworkWorktreeReportContext {
                workflow_id,
                repository: &repository,
                worktree_root: &worktree_root,
                branch_prefix: &branch_prefix,
                origin,
                mutation_authorized: false,
            },
            0,
            0,
            plans
                .into_iter()
                .map(teamwork_worktree_planned_entry)
                .collect(),
        ));
    }

    if plans
        .iter()
        .any(|plan| matches!(&plan.action, TeamworkWorktreeAction::Create))
    {
        fs::create_dir_all(&worktree_root).with_context(|| {
            format!(
                "failed to create teamwork worktree root {}",
                worktree_root.display()
            )
        })?;
    }
    let mut created = 0usize;
    let mut bound_existing = 0usize;
    let mut entries = Vec::with_capacity(plans.len());
    for plan in plans {
        let (status, record, claim) = match &plan.action {
            TeamworkWorktreeAction::Create => {
                let created_report = create_worktree(
                    store,
                    WorktreeCreateOptions {
                        repository: repository.clone(),
                        path: plan.path.clone(),
                        branch: plan.branch.clone(),
                        start_point: Some("HEAD".to_string()),
                        allow_repository_mutation: true,
                        origin: origin.to_string(),
                    },
                )?;
                let bound = bind_worktree(
                    store,
                    &created_report.worktree.id,
                    workflow_id,
                    Some(&plan.task_id),
                    origin,
                )?;
                let claim = bound_worktree_mutation_claim(store, workflow_id, &plan.task_id)?
                    .with_context(|| {
                        format!(
                            "created teamwork worktree {} has no task-scoped mutation claim",
                            bound.worktree.id
                        )
                    })?;
                created = created.saturating_add(1);
                ("created_and_bound", bound.worktree, claim)
            }
            TeamworkWorktreeAction::BindExisting(record) => {
                let bound =
                    bind_worktree(store, &record.id, workflow_id, Some(&plan.task_id), origin)?;
                let claim = bound_worktree_mutation_claim(store, workflow_id, &plan.task_id)?
                    .with_context(|| {
                        format!(
                            "bound teamwork worktree {} has no task-scoped mutation claim",
                            bound.worktree.id
                        )
                    })?;
                bound_existing = bound_existing.saturating_add(1);
                ("bound_existing", bound.worktree, claim)
            }
            TeamworkWorktreeAction::Reuse(reuse) => {
                let (record, claim) = reuse.as_ref();
                ("reused", record.clone(), claim.clone())
            }
        };
        if claim.binding_scope != "task"
            || claim.worktree_id != record.id
            || !process_paths_equal(Path::new(&claim.worktree_root), &plan.path)
        {
            return Err(anyhow!(
                "teamwork worktree claim verification failed for task {}",
                plan.task_id
            ));
        }
        entries.push(TeamworkWorktreePreparationEntry {
            task_id: plan.task_id,
            task_title: plan.task_title,
            brain: plan.brain,
            parallel_branch: plan.parallel_branch,
            path: plan.path.display().to_string(),
            branch: plan.branch,
            status: status.to_string(),
            worktree_id: Some(record.id),
            claim: Some(claim),
            commands: plan.commands,
        });
    }
    let report = teamwork_worktree_report(
        TeamworkWorktreeReportContext {
            workflow_id,
            repository: &repository,
            worktree_root: &worktree_root,
            branch_prefix: &branch_prefix,
            origin,
            mutation_authorized: true,
        },
        created,
        bound_existing,
        entries,
    );
    if created > 0 || bound_existing > 0 {
        store.record_event(
            workflow_id,
            "teamwork_worktrees_prepared",
            &serde_json::to_value(&report)?,
        )?;
    }
    Ok(report)
}

fn teamwork_worktree_report(
    context: TeamworkWorktreeReportContext<'_>,
    created_worktrees: usize,
    bound_existing_worktrees: usize,
    entries: Vec<TeamworkWorktreePreparationEntry>,
) -> TeamworkWorktreePreparationReport {
    let planned_worktrees = entries.len();
    let parallel_branch_worktrees = entries.iter().filter(|entry| entry.parallel_branch).count();
    let reused_worktrees = entries
        .iter()
        .filter(|entry| entry.status == "reused" || entry.status == "already_bound")
        .count();
    let commands = entries
        .iter()
        .flat_map(|entry| entry.commands.iter().cloned())
        .collect::<Vec<_>>();
    let status = if !context.mutation_authorized {
        "teamwork_worktrees_planned"
    } else if created_worktrees == 0 && bound_existing_worktrees == 0 {
        "teamwork_worktrees_already_prepared"
    } else {
        "teamwork_worktrees_prepared"
    };
    TeamworkWorktreePreparationReport {
        schema_version: "foundry.teamwork.worktree_preparation.v1".to_string(),
        status: status.to_string(),
        workflow_id: context.workflow_id.to_string(),
        repository: context.repository.display().to_string(),
        worktree_root: context.worktree_root.display().to_string(),
        branch_prefix: context.branch_prefix.to_string(),
        origin: context.origin.to_string(),
        mutation_authorized: context.mutation_authorized,
        planned_worktrees,
        parallel_branch_worktrees,
        supporting_agent_worktrees: planned_worktrees.saturating_sub(parallel_branch_worktrees),
        created_worktrees,
        bound_existing_worktrees,
        reused_worktrees,
        commands,
        entries,
    }
}

fn teamwork_worktree_planned_entry(
    plan: PlannedTeamworkWorktree,
) -> TeamworkWorktreePreparationEntry {
    let (status, worktree_id, claim) = match plan.action {
        TeamworkWorktreeAction::Create => ("planned_create_and_bind", None, None),
        TeamworkWorktreeAction::BindExisting(record) => {
            ("planned_bind_existing", Some(record.id), None)
        }
        TeamworkWorktreeAction::Reuse(reuse) => {
            let (record, claim) = *reuse;
            ("already_bound", Some(record.id), Some(claim))
        }
    };
    TeamworkWorktreePreparationEntry {
        task_id: plan.task_id,
        task_title: plan.task_title,
        brain: plan.brain,
        parallel_branch: plan.parallel_branch,
        path: plan.path.display().to_string(),
        branch: plan.branch,
        status: status.to_string(),
        worktree_id,
        claim,
        commands: plan.commands,
    }
}

fn required_teamwork_text<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        return Err(anyhow!("{label} cannot be empty"));
    }
    Ok(value)
}

fn normalize_teamwork_branch_prefix(prefix: &str) -> Result<String> {
    let prefix = required_teamwork_text(prefix, "teamwork branch prefix")?.trim_matches('/');
    if prefix.is_empty()
        || prefix.starts_with('-')
        || prefix.ends_with(".lock")
        || prefix.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
    {
        return Err(anyhow!(
            "teamwork branch prefix `{prefix}` must be a safe Git ref prefix"
        ));
    }
    Ok(prefix.to_string())
}

fn normalize_teamwork_worktree_root(path: &Path, repository: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(anyhow!(
            "teamwork worktree root must be absolute: {}",
            path.display()
        ));
    }
    for component in path.components() {
        if matches!(component, Component::CurDir | Component::ParentDir) {
            return Err(anyhow!(
                "teamwork worktree root cannot contain relative traversal components: {}",
                path.display()
            ));
        }
    }
    for ancestor in path.ancestors().filter(|ancestor| ancestor.exists()) {
        if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
            return Err(anyhow!(
                "teamwork worktree root must be symlink-free: {}",
                ancestor.display()
            ));
        }
    }
    let resolved = process_compatible_path(&if path.exists() {
        if !path.is_dir() {
            return Err(anyhow!(
                "teamwork worktree root is not a directory: {}",
                path.display()
            ));
        }
        fs::canonicalize(path)?
    } else {
        let parent = path.parent().with_context(|| {
            format!(
                "teamwork worktree root has no bounded parent: {}",
                path.display()
            )
        })?;
        if !parent.is_dir() {
            return Err(anyhow!(
                "teamwork worktree root parent must already exist: {}",
                parent.display()
            ));
        }
        let name = path.file_name().with_context(|| {
            format!(
                "teamwork worktree root must name a bounded directory: {}",
                path.display()
            )
        })?;
        fs::canonicalize(parent)?.join(name)
    });
    if resolved.parent().is_none()
        || resolved == repository
        || repository.starts_with(&resolved)
        || resolved.starts_with(repository)
    {
        return Err(anyhow!(
            "teamwork worktree root {} is too broad, equals the repository, contains it, or is contained by it; choose a disjoint bounded directory",
            resolved.display()
        ));
    }
    Ok(resolved)
}

#[cfg(windows)]
fn process_compatible_path(path: &Path) -> PathBuf {
    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if let Some(rest) = wide.strip_prefix(VERBATIM_UNC_PREFIX) {
        let mut normalized = vec![b'\\' as u16, b'\\' as u16];
        normalized.extend_from_slice(rest);
        return PathBuf::from(OsString::from_wide(&normalized));
    }
    if let Some(rest) = wide.strip_prefix(VERBATIM_PREFIX) {
        return PathBuf::from(OsString::from_wide(rest));
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn process_compatible_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn process_paths_equal(left: &Path, right: &Path) -> bool {
    process_compatible_path(left) == process_compatible_path(right)
}

fn teamwork_external_task_brain(task: &AtomicTask) -> Result<Option<String>> {
    let mut explicit = BTreeSet::new();
    if let Some(brain) = task.node_brain_routing.default_brain.as_deref() {
        let brain = canonical_executor_id(brain);
        if !brain.is_empty() && !matches!(brain.as_str(), "foundry" | "auto") {
            explicit.insert(brain);
        }
    }
    for slot in &task.node_brain_routing.agent_slots {
        if let Some(brain) = slot.brain_id.as_deref() {
            let brain = canonical_executor_id(brain);
            if !brain.is_empty() && !matches!(brain.as_str(), "foundry" | "auto") {
                explicit.insert(brain);
            }
        }
    }
    if explicit.len() > 1 {
        return Err(anyhow!(
            "teamwork task {} binds multiple external brains ({}); split them into independent tasks before provisioning worktrees",
            task.id,
            explicit.into_iter().collect::<Vec<_>>().join(",")
        ));
    }
    if let Some(brain) = explicit.into_iter().next() {
        return Ok(Some(brain));
    }
    let deterministic_without_external_brain = matches!(
        task.executor,
        ExecutorKind::Command | ExecutorKind::Wait | ExecutorKind::Notification
    );
    if task.node_brain_routing.scope == "agentic_ai_node" && !deterministic_without_external_brain {
        return Ok(Some("auto".to_string()));
    }
    Ok(None)
}

fn teamwork_task_requires_git_fan_in_worktree(task: &AtomicTask) -> bool {
    task.validation_rules
        .iter()
        .any(|rule| rule.kind == TEAMWORK_GIT_FAN_IN_VALIDATION_KIND)
}

fn validate_teamwork_task_path_component(task_id: &str) -> Result<()> {
    if task_id.is_empty()
        || task_id == "."
        || task_id == ".."
        || task_id.len() > 160
        || !task_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(anyhow!(
            "teamwork task id `{task_id}` is not safe as a worktree path component"
        ));
    }
    Ok(())
}

fn validate_teamwork_git_branch(repository: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["check-ref-format", "--branch", branch])
        .output()
        .context("failed to invoke git check-ref-format for teamwork worktree")?;
    if !output.status.success() {
        return Err(anyhow!(
            "invalid teamwork worktree branch `{branch}`: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn teamwork_git_branch_exists(repository: &Path, branch: &str) -> Result<bool> {
    let reference = format!("refs/heads/{branch}");
    let status = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["show-ref", "--verify", "--quiet", &reference])
        .status()
        .context("failed to inspect teamwork Git branch collision")?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(anyhow!(
            "git show-ref failed while checking teamwork branch `{branch}`"
        )),
    }
}

fn validate_existing_teamwork_worktree(
    record: &WorktreeRecord,
    workflow_id: &str,
    task_id: &str,
    branch: &str,
) -> Result<()> {
    if record.branch.as_deref() != Some(branch) {
        return Err(anyhow!(
            "registered teamwork destination {} uses branch {}, expected {}",
            record.worktree_root,
            record.branch.as_deref().unwrap_or("detached"),
            branch
        ));
    }
    if let Some(conflict) = record.bindings.iter().find(|binding| {
        binding.workflow_id != workflow_id || binding.task_id.as_deref() != Some(task_id)
    }) {
        return Err(anyhow!(
            "registered teamwork destination {} is already bound to workflow {} task {}; refusing task collision with {}",
            record.worktree_root,
            conflict.workflow_id,
            conflict.task_id.as_deref().unwrap_or("<workflow-scope>"),
            task_id
        ));
    }
    Ok(())
}

fn teamwork_worktree_create_command(
    store: &FoundryStore,
    repository: &Path,
    path: &Path,
    branch: &str,
) -> Vec<String> {
    vec![
        "foundry".to_string(),
        "--store".to_string(),
        store.path().display().to_string(),
        "worktree".to_string(),
        "create".to_string(),
        "--repository".to_string(),
        repository.display().to_string(),
        "--path".to_string(),
        path.display().to_string(),
        "--branch".to_string(),
        branch.to_string(),
        "--start-point".to_string(),
        "HEAD".to_string(),
        "--allow-repository-mutation".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

fn teamwork_worktree_bind_command(
    store: &FoundryStore,
    path: &Path,
    workflow_id: &str,
    task_id: &str,
    origin: &str,
) -> Vec<String> {
    vec![
        "foundry".to_string(),
        "--store".to_string(),
        store.path().display().to_string(),
        "worktree".to_string(),
        "bind".to_string(),
        "--worktree".to_string(),
        path.display().to_string(),
        "--workflow".to_string(),
        workflow_id.to_string(),
        "--task".to_string(),
        task_id.to_string(),
        "--origin".to_string(),
        origin.to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

pub fn core_parallel_team_from_teamwork(
    config: &TeamworkParallelConfig,
    source: impl Into<String>,
) -> Result<CoreParallelTeamSpec> {
    normalize_explicit_parallel_team(CoreParallelTeamSpec::explicit(
        source,
        config
            .lanes
            .iter()
            .map(|lane| CoreParallelLaneSpec {
                id: lane.id.clone(),
                executor_id: lane.brain.clone(),
                agent_count: lane.agent_count,
                parallel_group: lane.parallel_group.clone(),
                responsibility: lane.responsibility.clone(),
            })
            .collect(),
        config.max_parallel_agents,
    ))
}

pub fn normalize_explicit_parallel_team(
    mut spec: CoreParallelTeamSpec,
) -> Result<CoreParallelTeamSpec> {
    if !crate::brand::identifier_matches(&spec.schema_version, CORE_PARALLEL_TEAM_SCHEMA_VERSION) {
        return Err(anyhow!(
            "unsupported Core parallel team schema_version `{}`; expected {CORE_PARALLEL_TEAM_SCHEMA_VERSION}",
            spec.schema_version
        ));
    }
    spec.source = spec.source.trim().to_string();
    if spec.source.is_empty() {
        return Err(anyhow!(
            "Core parallel team declaration requires a non-empty source"
        ));
    }
    if spec.lanes.is_empty() {
        return Err(anyhow!(
            "Core parallel team declaration requires at least one explicit lane"
        ));
    }

    let mut lane_ids = HashSet::new();
    let mut branch_count = 0usize;
    for lane in &mut spec.lanes {
        lane.id = lane.id.trim().to_string();
        validate_parallel_slug(&lane.id, "Core parallel lane id")?;
        if !lane_ids.insert(lane.id.clone()) {
            return Err(anyhow!("duplicate Core parallel lane id: {}", lane.id));
        }

        lane.executor_id = canonical_executor_id(&lane.executor_id);
        if lane.executor_id == "auto" {
            return Err(anyhow!(
                "Core parallel lane {} requires an explicit executor_id; `auto` is not allowed",
                lane.id
            ));
        }
        validate_parallel_executor_id(
            &lane.executor_id,
            &format!("Core parallel lane {} executor_id", lane.id),
        )?;

        lane.parallel_group = lane.parallel_group.trim().to_string();
        validate_parallel_slug(
            &lane.parallel_group,
            &format!("Core parallel lane {} parallel_group", lane.id),
        )?;

        lane.responsibility = lane.responsibility.trim().to_string();
        if lane.responsibility.is_empty() {
            return Err(anyhow!(
                "Core parallel lane {} requires a non-empty responsibility",
                lane.id
            ));
        }
        if lane.agent_count == 0 {
            return Err(anyhow!(
                "Core parallel lane {} requires at least one agent",
                lane.id
            ));
        }
        branch_count = branch_count
            .checked_add(lane.agent_count)
            .context("Core parallel team branch count overflow")?;
    }

    if branch_count > MAX_TEAMWORK_BRANCHES {
        return Err(anyhow!(
            "Core parallel team supports at most {MAX_TEAMWORK_BRANCHES} total branches; requested {branch_count}"
        ));
    }
    if !(1..=MAX_TEAMWORK_BRANCHES).contains(&spec.max_parallel_agents) {
        return Err(anyhow!(
            "Core parallel team max_parallel_agents must be between 1 and {MAX_TEAMWORK_BRANCHES}"
        ));
    }
    if spec.max_parallel_agents > branch_count {
        return Err(anyhow!(
            "Core parallel team max_parallel_agents cannot exceed the {branch_count} configured independent agent slots"
        ));
    }
    Ok(spec)
}

fn validate_parallel_slug(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(anyhow!("{label} `{value}` must be a lowercase ASCII slug"));
    }
    Ok(())
}

fn validate_parallel_executor_id(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.starts_with('_')
        || value.ends_with('-')
        || value.ends_with('_')
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        return Err(anyhow!(
            "{label} `{value}` must be a lowercase ASCII executor id"
        ));
    }
    Ok(())
}

fn validate_teamwork_parallel_config(config: &TeamworkParallelConfig) -> Result<()> {
    if config.lanes.is_empty() {
        return Err(anyhow!("teamwork parallelism requires at least one lane"));
    }
    let mut lane_ids = HashSet::new();
    let mut branch_count = 0usize;
    for lane in &config.lanes {
        let lane_id = lane.id.trim();
        validate_parallel_slug(lane_id, "teamwork lane id")?;
        if !lane_ids.insert(lane_id) {
            return Err(anyhow!("duplicate teamwork lane id: {}", lane.id));
        }
        let brain = canonical_executor_id(&lane.brain);
        if brain.is_empty() || lane.responsibility.trim().is_empty() {
            return Err(anyhow!(
                "teamwork lane {} requires brain, parallel_group and responsibility",
                lane.id
            ));
        }
        validate_parallel_executor_id(&brain, &format!("teamwork lane {lane_id} brain"))?;
        validate_parallel_slug(
            lane.parallel_group.trim(),
            &format!("teamwork lane {lane_id} parallel_group"),
        )?;
        if lane.agent_count == 0 {
            return Err(anyhow!(
                "teamwork lane {} requires at least one agent",
                lane.id
            ));
        }
        branch_count = branch_count
            .checked_add(lane.agent_count)
            .context("teamwork branch count overflow")?;
    }
    if branch_count > MAX_TEAMWORK_BRANCHES {
        return Err(anyhow!(
            "teamwork parallelism supports at most {MAX_TEAMWORK_BRANCHES} total branches; requested {branch_count}"
        ));
    }
    if !(1..=MAX_TEAMWORK_BRANCHES).contains(&config.max_parallel_agents) {
        return Err(anyhow!(
            "teamwork max_parallel_agents must be between 1 and {MAX_TEAMWORK_BRANCHES}"
        ));
    }
    if config.max_parallel_agents > branch_count {
        return Err(anyhow!(
            "teamwork max_parallel_agents cannot exceed the {branch_count} configured independent agent slots"
        ));
    }
    Ok(())
}

fn resolve_teamwork_parallel_config(
    mut config: TeamworkParallelConfig,
    advisory_brain: &str,
    disallowed_brains: &HashSet<String>,
) -> Result<TeamworkParallelConfig> {
    for lane in &mut config.lanes {
        if lane.brain == "auto" {
            lane.brain = advisory_brain.to_string();
        }
        lane.brain = canonical_executor_id(&lane.brain);
        if disallowed_brains.contains(&lane.brain) {
            return Err(anyhow!(
                "teamwork lane {} requires brain {}, but Foundry executor policy marks it disallowed",
                lane.id,
                lane.brain
            ));
        }
    }
    let normalized = core_parallel_team_from_teamwork(&config, "foundry_teamwork_command")?;
    Ok(TeamworkParallelConfig::from(&normalized))
}

fn lane_display_name(lane_id: &str) -> String {
    lane_id
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
            })
        })
        .collect::<Vec<_>>()
        .join("")
}

fn lane_worker_role(lane_id: &str) -> String {
    if lane_id == "worker" {
        "Worker".to_string()
    } else {
        format!("{}Worker", lane_display_name(lane_id))
    }
}

fn lane_integrator_role(lane_id: &str) -> String {
    format!("{}Integrator", lane_display_name(lane_id))
}

fn teamwork_lane_roles_for_config(config: &TeamworkParallelConfig) -> Vec<TeamworkRole> {
    let branch_count = config
        .lanes
        .iter()
        .map(|lane| lane.agent_count)
        .sum::<usize>();
    let mut roles = Vec::with_capacity(branch_count + config.lanes.len());
    for lane in &config.lanes {
        for branch in 1..=lane.agent_count {
            roles.push(TeamworkRole {
                slot_id: format!("teamwork-{}-{branch:03}", lane.id),
                role: lane_worker_role(&lane.id),
                brain: lane.brain.clone(),
                parallel_group: lane.parallel_group.clone(),
                responsibility: format!(
                    "{} Slice {branch}/{} is independent within lane {}.",
                    lane.responsibility, lane.agent_count, lane.id
                ),
            });
        }
        roles.push(TeamworkRole {
            slot_id: format!("teamwork-{}-integrator-001", lane.id),
            role: lane_integrator_role(&lane.id),
            brain: lane.brain.clone(),
            parallel_group: TEAMWORK_DOMAIN_JOIN_WAVE.to_string(),
            responsibility: format!(
                "Join every {} lane slice into one validated domain delivery.",
                lane.id
            ),
        });
    }
    roles
}

fn teamwork_roles_for_config(
    config: &TeamworkParallelConfig,
    orchestrator_brain: String,
    auditor_brain: String,
) -> Vec<TeamworkRole> {
    let branch_count = config
        .lanes
        .iter()
        .map(|lane| lane.agent_count)
        .sum::<usize>();
    let mut roles = Vec::with_capacity(branch_count + config.lanes.len() + 2);
    roles.push(TeamworkRole {
        slot_id: "teamwork-orchestrator-001".to_string(),
        role: "Orchestrator".to_string(),
        brain: orchestrator_brain,
        parallel_group: "control".to_string(),
        responsibility: "Turn the approved goal into an auditable graph while Foundry remains the workflow authority.".to_string(),
    });
    roles.extend(teamwork_lane_roles_for_config(config));
    roles.push(TeamworkRole {
        slot_id: "teamwork-auditor-001".to_string(),
        role: "Auditor".to_string(),
        brain: auditor_brain,
        parallel_group: TEAMWORK_FINAL_AUDIT_WAVE.to_string(),
        responsibility:
            "Audit all configured lane joins and block final promotion until their evidence converges."
                .to_string(),
    });
    roles
}

fn teamwork_role_by_slot<'a>(roles: &'a [TeamworkRole], slot_id: &str) -> Result<&'a TeamworkRole> {
    roles
        .iter()
        .find(|candidate| candidate.slot_id == slot_id)
        .with_context(|| format!("teamwork roster is missing required slot {slot_id}"))
}

fn ensure_teamwork_roster_slots_unique(roles: &[TeamworkRole]) -> Result<()> {
    let mut slot_ids = HashSet::new();
    for role in roles {
        if role.slot_id.trim().is_empty() {
            return Err(anyhow!("teamwork roster contains an empty slot id"));
        }
        if !slot_ids.insert(role.slot_id.as_str()) {
            return Err(anyhow!(
                "teamwork roster contains duplicate slot id: {}",
                role.slot_id
            ));
        }
    }
    Ok(())
}

fn bind_teamwork_role(task: &mut AtomicTask, role: &TeamworkRole) {
    task.executor = ExecutorKind::Mixed;
    task.execution_policy = default_execution_policy(&task.executor);
    let routing = &mut task.node_brain_routing;
    routing.scope = "agentic_ai_node".to_string();
    routing.default_brain = Some(role.brain.clone());
    if !routing
        .allowed_brains
        .iter()
        .any(|brain| brain == &role.brain)
    {
        routing.allowed_brains.push(role.brain.clone());
    }
    routing.agent_slots = vec![NodeBrainAgentSlotSpec {
        slot_id: role.slot_id.clone(),
        brain_id: Some(role.brain.clone()),
        role: role.role.clone(),
        parallel_group: role.parallel_group.clone(),
        state_owner: "foundry".to_string(),
    }];
    routing.max_parallel_agents = 1;
    routing.supports_parallel_agent_brains = true;
    routing.supports_multiple_agents_per_brain = true;
    routing.hot_swappable = true;
    task.work_item.owner_role = role.role.clone();
    task.version = task.version.saturating_add(1);
}

fn retarget_teamwork_subtasks(task: &mut AtomicTask, scope: &str, evidence: &str) {
    let actions = ["Prepare", "Deliver", "Validate"];
    for (subtask_index, subtask) in task.work_item.subtasks.iter_mut().enumerate() {
        let action = actions.get(subtask_index).copied().unwrap_or("Complete");
        subtask.id = format!("{}-subtask-{:03}", task.id, subtask_index + 1);
        subtask.title = format!("{action} {scope}");
        subtask.goal = format!("{action} {scope} with evidence: {evidence}");
        subtask.definition_of_done = vec![
            format!("{scope} scope remains isolated and traceable"),
            format!("Evidence is attached for {evidence}"),
        ];
    }
}

fn retarget_teamwork_branch(
    task: &mut AtomicTask,
    lane: &TeamworkLaneConfig,
    branch_number: usize,
) {
    let lane_scope = format!("{} lane slice {branch_number}", lane.id);
    task.id = format!("task-005-{}-{branch_number:03}", lane.id);
    task.title = format!("Deliver {lane_scope}");
    task.expected_output = format!("{} delivery evidence", lane_scope);
    task.goal = format!(
        "Deliver {lane_scope} independently: {}",
        lane.responsibility
    );
    task.context_requirements = vec![
        "bounded context package".to_string(),
        format!("{} lane contract", lane.id),
        lane.responsibility.clone(),
    ];
    task.validation_rules = vec![ValidationRule {
        kind: format!("{}_lane_slice", lane.id.replace('-', "_")),
        command: None,
        expected: format!(
            "{} produces independent, traceable evidence for {}",
            lane_scope, lane.responsibility
        ),
    }];
    task.work_item.item_type = "execution_story".to_string();
    task.work_item.acceptance_criteria = vec![format!(
        "{} is complete without depending on a sibling lane slice",
        lane_scope
    )];
    task.work_item.goal_validation.goal = format!(
        "{lane_scope} is definitively ready under the {} brain binding",
        lane.brain
    );
    task.work_item.goal_validation.evidence_required = vec![
        format!("{} output", lane_scope),
        format!("{} validation", lane.id),
    ];
    retarget_teamwork_subtasks(task, &lane_scope, &lane.responsibility);
}

fn retarget_teamwork_lane_join(
    task: &mut AtomicTask,
    lane: &TeamworkLaneConfig,
    dependencies: Vec<String>,
) {
    let join_scope = format!("{} lane join", lane.id);
    task.id = format!("task-005-{}-join", lane.id);
    task.title = format!("Join {} lane deliveries", lane.id);
    task.goal = format!(
        "Join all {} lane slices only after every declared dependency is definitively ready",
        lane.id
    );
    task.dependencies = dependencies;
    task.expected_output = format!("Consolidated {} lane delivery", lane.id);
    task.context_requirements = vec![
        format!("all {} lane slice outputs", lane.id),
        format!("{} lane validation evidence", lane.id),
    ];
    task.validation_rules = vec![
        ValidationRule {
            kind: format!("{}_lane_join", lane.id.replace('-', "_")),
            command: None,
            expected: format!(
                "every {} lane dependency is complete, validated and represented in the consolidated output",
                lane.id
            ),
        },
        ValidationRule {
            kind: TEAMWORK_GIT_FAN_IN_VALIDATION_KIND.to_string(),
            command: None,
            expected: "all dependency Git heads are converged into the task-scoped join worktree under an immutable Foundry receipt before executor dispatch".to_string(),
        },
    ];
    task.work_item.item_type = "validation_story".to_string();
    task.work_item.parent_id = task.dependencies.first().cloned();
    task.work_item.acceptance_criteria = vec![format!(
        "every {} lane slice is included exactly once",
        lane.id
    )];
    task.work_item.goal_validation.goal = format!("{join_scope} is definitively ready");
    task.work_item.goal_validation.evidence_required =
        vec![format!("consolidated {} lane evidence", lane.id)];
    retarget_teamwork_subtasks(task, &join_scope, &lane.responsibility);
}

fn retarget_teamwork_final_join(task: &mut AtomicTask, dependencies: Vec<String>) {
    task.title = "Audit and join configured teamwork lanes".to_string();
    task.goal = "Audit every lane join and produce one promotion decision under Foundry authority"
        .to_string();
    task.dependencies = dependencies;
    task.expected_output = "Cross-lane audit and promotion evidence".to_string();
    task.context_requirements = vec![
        "all configured lane joins".to_string(),
        "cross-lane acceptance contract".to_string(),
    ];
    task.validation_rules = vec![
        ValidationRule {
            kind: "configured_lane_final_join".to_string(),
            command: None,
            expected: "every lane join is complete and the auditor records a single evidence-bound promotion decision".to_string(),
        },
        ValidationRule {
            kind: TEAMWORK_GIT_FAN_IN_VALIDATION_KIND.to_string(),
            command: None,
            expected: "all lane-join Git heads are converged into the final auditor worktree under an immutable Foundry receipt before executor dispatch".to_string(),
        },
    ];
    task.work_item.item_type = "validation_story".to_string();
    task.work_item.parent_id = task.dependencies.first().cloned();
    task.work_item.acceptance_criteria = vec![
        "all configured lane joins are represented".to_string(),
        "Foundry remains the promotion authority".to_string(),
    ];
    task.work_item.goal_validation.goal =
        "Configured teamwork lanes are definitively ready as one delivery".to_string();
    task.work_item.goal_validation.evidence_required = vec![
        "lane join evidence".to_string(),
        "auditor promotion decision".to_string(),
    ];
    retarget_teamwork_subtasks(
        task,
        "configured lane final audit",
        "all domain joins and promotion gates",
    );
}

fn normalize_teamwork_dependency_versions(workflow: &mut Workflow) {
    loop {
        let versions = workflow
            .tasks
            .iter()
            .map(|task| (task.id.clone(), task.version))
            .collect::<BTreeMap<_, _>>();
        let mut changed = false;
        for task in &mut workflow.tasks {
            let minimum_version = task
                .dependencies
                .iter()
                .filter_map(|dependency| versions.get(dependency))
                .copied()
                .max()
                .unwrap_or(task.version);
            if task.version < minimum_version {
                task.version = minimum_version;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

fn ensure_teamwork_task_and_subtask_ids_unique(workflow: &Workflow) -> Result<()> {
    let mut task_ids = HashSet::new();
    let mut subtask_ids = HashSet::new();
    for task in &workflow.tasks {
        if !task_ids.insert(task.id.as_str()) {
            return Err(anyhow!(
                "teamwork topology produced duplicate task id: {}",
                task.id
            ));
        }
        for subtask in &task.work_item.subtasks {
            if !subtask_ids.insert(subtask.id.as_str()) {
                return Err(anyhow!(
                    "teamwork topology produced duplicate subtask id: {}",
                    subtask.id
                ));
            }
        }
    }
    Ok(())
}

pub fn materialize_explicit_parallel_team(
    workflow: &mut Workflow,
    spec: CoreParallelTeamSpec,
) -> Result<()> {
    let normalized = normalize_explicit_parallel_team(spec)?;
    let config = TeamworkParallelConfig::from(&normalized);
    let roles = teamwork_lane_roles_for_config(&config);
    let mut candidate = workflow.clone();
    apply_teamwork_parallel_topology_inner(&mut candidate, &roles, &config, false)?;
    candidate.core_orchestration.parallel_team = Some(normalized);
    *workflow = candidate;
    Ok(())
}

fn apply_teamwork_parallel_topology(
    workflow: &mut Workflow,
    roles: &[TeamworkRole],
    config: &TeamworkParallelConfig,
) -> Result<()> {
    let normalized = core_parallel_team_from_teamwork(config, "foundry_teamwork_command")?;
    let normalized_config = TeamworkParallelConfig::from(&normalized);
    let mut normalized_roles = roles.to_vec();
    for role in &mut normalized_roles {
        role.brain = canonical_executor_id(&role.brain);
    }
    let mut candidate = workflow.clone();
    apply_teamwork_parallel_topology_inner(
        &mut candidate,
        &normalized_roles,
        &normalized_config,
        true,
    )?;
    candidate.core_orchestration.parallel_team = Some(normalized);
    *workflow = candidate;
    Ok(())
}

fn apply_teamwork_parallel_topology_inner(
    workflow: &mut Workflow,
    roles: &[TeamworkRole],
    config: &TeamworkParallelConfig,
    bind_control_roles: bool,
) -> Result<()> {
    validate_teamwork_parallel_config(config)?;
    ensure_teamwork_roster_slots_unique(roles)?;
    workflow.core_orchestration.max_parallel_tasks = config.max_parallel_agents;
    if bind_control_roles {
        let orchestrator = teamwork_role_by_slot(roles, "teamwork-orchestrator-001")?;
        let planner_index = workflow
            .tasks
            .iter()
            .position(|task| task.id == "task-003")
            .context("teamwork planner task task-003 is missing")?;
        bind_teamwork_role(&mut workflow.tasks[planner_index], orchestrator);
    }

    let worker_index = workflow
        .tasks
        .iter()
        .position(|task| task.id == "task-005")
        .context("teamwork worker task task-005 is missing")?;
    let base_branch = workflow.tasks[worker_index].clone();
    let base_join = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-006")
        .cloned()
        .context("teamwork auditor join task task-006 is missing")?;
    workflow.tasks.remove(worker_index);

    let mut materialized_tasks = Vec::new();
    let mut lane_join_tasks = Vec::new();
    let mut lane_join_ids = Vec::with_capacity(config.lanes.len());
    for lane in &config.lanes {
        let mut branch_ids = Vec::with_capacity(lane.agent_count);
        for branch_number in 1..=lane.agent_count {
            let mut branch = base_branch.clone();
            retarget_teamwork_branch(&mut branch, lane, branch_number);
            let slot_id = format!("teamwork-{}-{branch_number:03}", lane.id);
            let role = teamwork_role_by_slot(roles, &slot_id)?;
            bind_teamwork_role(&mut branch, role);
            branch_ids.push(branch.id.clone());
            materialized_tasks.push(branch);
        }
        let mut lane_join = base_join.clone();
        retarget_teamwork_lane_join(&mut lane_join, lane, branch_ids);
        let integrator_slot = format!("teamwork-{}-integrator-001", lane.id);
        let integrator = teamwork_role_by_slot(roles, &integrator_slot)?;
        bind_teamwork_role(&mut lane_join, integrator);
        lane_join_ids.push(lane_join.id.clone());
        lane_join_tasks.push(lane_join);
    }
    materialized_tasks.extend(lane_join_tasks);
    for (offset, task) in materialized_tasks.into_iter().enumerate() {
        workflow.tasks.insert(worker_index + offset, task);
    }

    let join_index = workflow
        .tasks
        .iter()
        .position(|task| task.id == "task-006")
        .context("teamwork auditor join task task-006 is missing")?;
    retarget_teamwork_final_join(&mut workflow.tasks[join_index], lane_join_ids.clone());
    if bind_control_roles {
        let auditor = teamwork_role_by_slot(roles, "teamwork-auditor-001")?;
        bind_teamwork_role(&mut workflow.tasks[join_index], auditor);
    }
    normalize_teamwork_dependency_versions(workflow);
    ensure_teamwork_task_and_subtask_ids_unique(workflow)?;

    let revision = workflow
        .revisions
        .last()
        .map_or(1, |item| item.revision.saturating_add(1));
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: "teamwork.core".to_string(),
        change_type: "teamwork_parallel_topology_materialized".to_string(),
        summary: format!(
            "materialized {} configured lanes with {} branches, lane joins {:?}, and final auditor join",
            config.lanes.len(),
            config.lanes.iter().map(|lane| lane.agent_count).sum::<usize>(),
            lane_join_ids
        ),
        created_at: chrono::Utc::now(),
    });
    Ok(())
}

fn fetch_benchmarks_from_url(url_str: &str) -> Result<serde_json::Value> {
    let host_port = url_str.strip_prefix("http://").unwrap_or(url_str);
    let mut parts = host_port.splitn(2, '/');
    let host_port_part = parts.next().unwrap();
    let path = format!("/{}", parts.next().unwrap_or(""));

    let mut stream = TcpStream::connect(host_port_part)
        .with_context(|| format!("Failed to connect to {}", host_port_part))?;

    // Set a short read/write timeout to prevent blocking tests indefinitely
    let timeout = std::time::Duration::from_secs(5);
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host_port_part
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;

    let response_str = String::from_utf8_lossy(&response);
    let mut parts = response_str.splitn(2, "\r\n\r\n");
    let _headers = parts
        .next()
        .ok_or_else(|| anyhow!("Invalid HTTP response"))?;
    let body = parts.next().ok_or_else(|| anyhow!("No HTTP body found"))?;

    let val: serde_json::Value = serde_json::from_str(body)
        .with_context(|| format!("Failed to parse response body as JSON: {}", body))?;
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::validate_workflow_structure;
    use std::collections::HashSet;
    use tempfile::tempdir;

    fn frontend_backend_config() -> TeamworkParallelConfig {
        TeamworkParallelConfig {
            lanes: vec![
                TeamworkLaneConfig {
                    id: "frontend".to_string(),
                    brain: "agy".to_string(),
                    agent_count: 3,
                    parallel_group: TEAMWORK_IMPLEMENTATION_WAVE.to_string(),
                    responsibility:
                        "Implement isolated UI slices with browser and design-system evidence."
                            .to_string(),
                },
                TeamworkLaneConfig {
                    id: "backend".to_string(),
                    brain: "codex".to_string(),
                    agent_count: 5,
                    parallel_group: TEAMWORK_IMPLEMENTATION_WAVE.to_string(),
                    responsibility:
                        "Implement isolated API and data slices with service-contract evidence."
                            .to_string(),
                },
            ],
            max_parallel_agents: 8,
        }
    }

    fn frontend_backend_core_spec() -> CoreParallelTeamSpec {
        core_parallel_team_from_teamwork(&frontend_backend_config(), "test_explicit_declaration")
            .unwrap()
    }

    fn task<'a>(workflow: &'a Workflow, id: &str) -> &'a AtomicTask {
        workflow.tasks.iter().find(|task| task.id == id).unwrap()
    }

    fn assert_external_role_execution_contract(task: &AtomicTask) {
        assert_eq!(task.executor, ExecutorKind::Mixed);
        assert_eq!(task.execution_policy.mode, "bounded_mixed_executor");
        assert!(task.execution_policy.ai_allowed);
        assert!(!task.execution_policy.deterministic);
        assert_eq!(
            task.execution_policy.validation_gate,
            "task_validation_rules"
        );
    }

    #[test]
    fn explicit_core_parallel_team_normalizes_aliases_for_stable_persistence() {
        let mut spec = frontend_backend_core_spec();
        spec.source = "  request_start  ".to_string();
        spec.lanes[0].executor_id = " Antigravity-CLI ".to_string();
        spec.lanes[0].responsibility = "  Implement isolated frontend slices.  ".to_string();

        let normalized = normalize_explicit_parallel_team(spec).unwrap();

        assert_eq!(normalized.source, "request_start");
        assert_eq!(normalized.lanes[0].executor_id, "agy");
        assert_eq!(
            normalized.lanes[0].responsibility,
            "Implement isolated frontend slices."
        );
        assert_eq!(normalized.schema_version, CORE_PARALLEL_TEAM_SCHEMA_VERSION);
    }

    #[test]
    fn explicit_and_teamwork_lane_validators_share_the_strict_slug_grammar() {
        for lane_id in ["frontend", "front-end", "lane2", " frontend "] {
            let teamwork = TeamworkParallelConfig {
                lanes: vec![TeamworkLaneConfig {
                    id: lane_id.to_string(),
                    brain: "codex".to_string(),
                    agent_count: 1,
                    parallel_group: TEAMWORK_IMPLEMENTATION_WAVE.to_string(),
                    responsibility: "Deliver one independent slice.".to_string(),
                }],
                max_parallel_agents: 1,
            };
            let explicit = CoreParallelTeamSpec::explicit(
                "slug_test",
                vec![CoreParallelLaneSpec {
                    id: lane_id.to_string(),
                    executor_id: "codex".to_string(),
                    agent_count: 1,
                    parallel_group: TEAMWORK_IMPLEMENTATION_WAVE.to_string(),
                    responsibility: "Deliver one independent slice.".to_string(),
                }],
                1,
            );
            assert!(validate_teamwork_parallel_config(&teamwork).is_ok());
            assert!(normalize_explicit_parallel_team(explicit).is_ok());
        }

        for lane_id in [
            "front_end",
            "front.end",
            "Frontend",
            "-frontend",
            "frontend-",
            " ",
        ] {
            let teamwork = TeamworkParallelConfig {
                lanes: vec![TeamworkLaneConfig {
                    id: lane_id.to_string(),
                    brain: "codex".to_string(),
                    agent_count: 1,
                    parallel_group: TEAMWORK_IMPLEMENTATION_WAVE.to_string(),
                    responsibility: "Deliver one independent slice.".to_string(),
                }],
                max_parallel_agents: 1,
            };
            let explicit = CoreParallelTeamSpec::explicit(
                "slug_test",
                vec![CoreParallelLaneSpec {
                    id: lane_id.to_string(),
                    executor_id: "codex".to_string(),
                    agent_count: 1,
                    parallel_group: TEAMWORK_IMPLEMENTATION_WAVE.to_string(),
                    responsibility: "Deliver one independent slice.".to_string(),
                }],
                1,
            );
            assert!(validate_teamwork_parallel_config(&teamwork).is_err());
            assert!(normalize_explicit_parallel_team(explicit).is_err());
        }
    }

    #[test]
    fn explicit_core_materializer_persists_canonical_lanes_and_keeps_controls_generic() {
        let mut workflow = create_workflow(parse_intent("Build with explicit parallel lanes"));
        let spec = frontend_backend_core_spec();

        materialize_explicit_parallel_team(&mut workflow, spec.clone()).unwrap();

        assert_eq!(
            workflow.core_orchestration.parallel_team.as_ref(),
            Some(&spec)
        );
        assert_eq!(workflow.core_orchestration.max_parallel_tasks, 8);
        assert_eq!(task(&workflow, "task-003").executor, ExecutorKind::Command);
        assert!(task(&workflow, "task-003")
            .node_brain_routing
            .agent_slots
            .is_empty());
        assert_eq!(task(&workflow, "task-006").executor, ExecutorKind::Command);
        assert!(task(&workflow, "task-006")
            .node_brain_routing
            .agent_slots
            .is_empty());
        assert_eq!(
            task(&workflow, "task-005-frontend-001")
                .node_brain_routing
                .default_brain
                .as_deref(),
            Some("agy")
        );
        assert_eq!(
            task(&workflow, "task-005-backend-005")
                .node_brain_routing
                .default_brain
                .as_deref(),
            Some("codex")
        );
        assert!(
            validate_workflow_structure(&workflow).is_empty(),
            "{:?}",
            validate_workflow_structure(&workflow)
        );
    }

    #[test]
    fn explicit_core_materializer_rejects_auto_without_partial_mutation() {
        let mut workflow = create_workflow(parse_intent("Reject ambiguous executor routing"));
        let before = serde_json::to_value(&workflow).unwrap();
        let mut spec = frontend_backend_core_spec();
        spec.lanes[0].executor_id = "auto".to_string();

        let error = materialize_explicit_parallel_team(&mut workflow, spec).unwrap_err();

        assert!(error.to_string().contains("`auto` is not allowed"));
        assert_eq!(serde_json::to_value(&workflow).unwrap(), before);
    }

    #[test]
    fn explicit_core_normalizer_rejects_duplicate_lanes_and_invalid_maximum() {
        let mut duplicate = frontend_backend_core_spec();
        duplicate.lanes[1].id = duplicate.lanes[0].id.clone();
        let duplicate_error = normalize_explicit_parallel_team(duplicate).unwrap_err();
        assert!(duplicate_error
            .to_string()
            .contains("duplicate Core parallel lane id"));

        let mut invalid_maximum = frontend_backend_core_spec();
        invalid_maximum.max_parallel_agents = 9;
        let maximum_error = normalize_explicit_parallel_team(invalid_maximum).unwrap_err();
        assert!(maximum_error
            .to_string()
            .contains("cannot exceed the 8 configured independent agent slots"));
    }

    #[test]
    fn legacy_core_orchestration_json_defaults_parallel_team_to_none() {
        let mut workflow = create_workflow(parse_intent("Read old workflow JSON"));
        workflow.core_orchestration.parallel_team = Some(frontend_backend_core_spec());
        let mut value = serde_json::to_value(&workflow).unwrap();
        value["core_orchestration"]
            .as_object_mut()
            .unwrap()
            .remove("parallel_team");

        let restored: Workflow = serde_json::from_value(value).unwrap();

        assert!(restored.core_orchestration.parallel_team.is_none());
    }

    #[test]
    fn configurable_lanes_materialize_three_agy_frontend_five_codex_backend_and_joins() {
        let config = frontend_backend_config();
        let roles = teamwork_roles_for_config(&config, "codex".to_string(), "opencode".to_string());
        let mut workflow = create_workflow(parse_intent("Build with parallel workers"));
        apply_teamwork_parallel_topology(&mut workflow, &roles, &config).unwrap();
        assert_eq!(workflow.core_orchestration.max_parallel_tasks, 8);
        assert_eq!(
            workflow
                .core_orchestration
                .parallel_team
                .as_ref()
                .unwrap()
                .lanes
                .len(),
            2
        );

        let frontend_ids = (1..=3)
            .map(|index| format!("task-005-frontend-{index:03}"))
            .collect::<Vec<_>>();
        let backend_ids = (1..=5)
            .map(|index| format!("task-005-backend-{index:03}"))
            .collect::<Vec<_>>();
        let mut bound_slots = HashSet::new();
        for (lane, ids, expected_role, expected_brain) in [
            ("frontend", &frontend_ids, "FrontendWorker", "agy"),
            ("backend", &backend_ids, "BackendWorker", "codex"),
        ] {
            for id in ids {
                let branch = task(&workflow, id);
                assert_external_role_execution_contract(branch);
                assert_eq!(branch.dependencies, vec!["task-004".to_string()]);
                assert_eq!(branch.work_item.owner_role, expected_role);
                assert!(branch.title.contains(lane));
                assert!(branch.goal.contains(lane));
                assert!(branch.expected_output.contains(lane));
                assert!(branch
                    .context_requirements
                    .iter()
                    .any(|requirement| requirement.contains(lane)));
                assert!(branch.validation_rules[0].kind.contains(lane));
                let slot = &branch.node_brain_routing.agent_slots[0];
                assert_eq!(slot.brain_id.as_deref(), Some(expected_brain));
                assert_eq!(slot.role, expected_role);
                assert_eq!(slot.parallel_group, TEAMWORK_IMPLEMENTATION_WAVE);
                assert!(bound_slots.insert(slot.slot_id.clone()));
            }
        }

        let frontend_join = task(&workflow, "task-005-frontend-join");
        let backend_join = task(&workflow, "task-005-backend-join");
        assert_external_role_execution_contract(frontend_join);
        assert_external_role_execution_contract(backend_join);
        assert_eq!(frontend_join.dependencies, frontend_ids);
        assert_eq!(backend_join.dependencies, backend_ids);
        assert!(frontend_join
            .validation_rules
            .iter()
            .any(|rule| rule.kind == TEAMWORK_GIT_FAN_IN_VALIDATION_KIND));
        assert!(backend_join
            .validation_rules
            .iter()
            .any(|rule| rule.kind == TEAMWORK_GIT_FAN_IN_VALIDATION_KIND));
        assert!(frontend_join
            .dependencies
            .iter()
            .all(|dependency| !backend_join.dependencies.contains(dependency)));
        assert_eq!(frontend_join.work_item.owner_role, "FrontendIntegrator");
        assert_eq!(backend_join.work_item.owner_role, "BackendIntegrator");
        assert_eq!(
            frontend_join.node_brain_routing.default_brain.as_deref(),
            Some("agy")
        );
        assert_eq!(
            backend_join.node_brain_routing.default_brain.as_deref(),
            Some("codex")
        );
        assert_eq!(
            frontend_join.node_brain_routing.agent_slots[0].parallel_group,
            TEAMWORK_DOMAIN_JOIN_WAVE
        );
        assert_eq!(
            backend_join.node_brain_routing.agent_slots[0].parallel_group,
            TEAMWORK_DOMAIN_JOIN_WAVE
        );
        assert!(bound_slots.insert(
            frontend_join.node_brain_routing.agent_slots[0]
                .slot_id
                .clone()
        ));
        assert!(bound_slots.insert(
            backend_join.node_brain_routing.agent_slots[0]
                .slot_id
                .clone()
        ));

        let final_join = task(&workflow, "task-006");
        assert_external_role_execution_contract(final_join);
        assert_eq!(
            final_join.dependencies,
            vec![
                "task-005-frontend-join".to_string(),
                "task-005-backend-join".to_string()
            ]
        );
        assert_eq!(final_join.work_item.owner_role, "Auditor");
        assert!(final_join
            .validation_rules
            .iter()
            .any(|rule| rule.kind == TEAMWORK_GIT_FAN_IN_VALIDATION_KIND));
        assert_eq!(
            final_join.node_brain_routing.agent_slots[0].parallel_group,
            TEAMWORK_FINAL_AUDIT_WAVE
        );
        assert!(bound_slots.insert(final_join.node_brain_routing.agent_slots[0].slot_id.clone()));
        let planner = task(&workflow, "task-003");
        assert_external_role_execution_contract(planner);
        assert!(bound_slots.insert(planner.node_brain_routing.agent_slots[0].slot_id.clone()));
        assert_eq!(bound_slots.len(), roles.len());

        let frontend = task(&workflow, "task-005-frontend-001");
        let backend = task(&workflow, "task-005-backend-001");
        assert_ne!(frontend.title, backend.title);
        assert_ne!(frontend.goal, backend.goal);
        assert_ne!(frontend.expected_output, backend.expected_output);
        assert_ne!(frontend.context_requirements, backend.context_requirements);
        assert_ne!(
            frontend.validation_rules[0].kind,
            backend.validation_rules[0].kind
        );

        let subtask_ids = workflow
            .tasks
            .iter()
            .flat_map(|task| task.work_item.subtasks.iter())
            .map(|subtask| subtask.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            subtask_ids.len(),
            subtask_ids.iter().copied().collect::<HashSet<_>>().len()
        );
        for task in &workflow.tasks {
            for dependency in &task.dependencies {
                let dependency_version = workflow
                    .tasks
                    .iter()
                    .find(|candidate| candidate.id == *dependency)
                    .unwrap()
                    .version;
                assert!(
                    task.version >= dependency_version,
                    "{} v{} is behind {} v{}",
                    task.id,
                    task.version,
                    dependency,
                    dependency_version
                );
            }
        }
        assert!(
            validate_workflow_structure(&workflow).is_empty(),
            "{:?}",
            validate_workflow_structure(&workflow)
        );
        assert_eq!(
            workflow.revisions.last().unwrap().change_type,
            "teamwork_parallel_topology_materialized"
        );
    }

    #[test]
    fn default_plan_preserves_two_generic_workers() {
        let temp = tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();

        let response =
            plan_teamwork_workflow(&store, "Deliver two independent branches", false, false)
                .unwrap();
        let workflow = store.load_workflow(&response.workflow_id).unwrap();
        assert!(workflow
            .tasks
            .iter()
            .any(|task| task.id == "task-005-worker-001"));
        assert!(workflow
            .tasks
            .iter()
            .any(|task| task.id == "task-005-worker-002"));
        assert!(workflow
            .tasks
            .iter()
            .any(|task| task.id == "task-005-worker-join"));
        assert_eq!(response.strategy.parallelism.lanes.len(), 1);
        assert_eq!(response.strategy.parallelism.lanes[0].agent_count, 2);
        assert_ne!(response.strategy.parallelism.lanes[0].brain, "auto");
        assert_eq!(response.roster.agent_count, 5);
    }

    #[test]
    fn configured_plan_persists_lanes_roster_strategy_workflow_run_and_event() {
        let temp = tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        let config = frontend_backend_config();
        let response = plan_teamwork_workflow_with_config(
            &store,
            "Deliver semantic frontend and backend lanes",
            false,
            false,
            config.clone(),
        )
        .unwrap();
        let workflow = store.load_workflow(&response.workflow_id).unwrap();
        assert!(workflow
            .tasks
            .iter()
            .any(|task| task.id == "task-005-frontend-003"));
        assert!(workflow
            .tasks
            .iter()
            .any(|task| task.id == "task-005-backend-005"));
        let persisted_core_team = workflow.core_orchestration.parallel_team.as_ref().unwrap();
        assert_eq!(persisted_core_team.source, "foundry_teamwork_command");
        assert_eq!(persisted_core_team.lanes[0].executor_id, "agy");
        assert_eq!(persisted_core_team.lanes[1].executor_id, "codex");

        let events = store.load_workflow_events(&response.workflow_id).unwrap();
        let event = events
            .iter()
            .find(|event| event.kind == "teamwork_planned")
            .unwrap();
        let persisted: TeamworkResponse = serde_json::from_value(event.data.clone()).unwrap();
        assert_eq!(persisted.workflow_id, response.workflow_id);
        assert_eq!(persisted.roster.agent_count, 12);
        assert_eq!(persisted.roster.roles.len(), 12);
        assert_eq!(persisted.strategy.parallelism, config);
        assert_eq!(
            persisted.strategy.schema_version,
            "foundry.teamwork.strategy.v1"
        );
        assert_eq!(
            persisted.strategy.phases[1].execution_model,
            response.strategy.phases[1].execution_model
        );
        assert_eq!(store.load_runs().unwrap().len(), 1);
    }

    #[test]
    fn configured_lane_brain_cannot_bypass_executor_policy() {
        let temp = tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        store
            .save_executor_state("agy", &serde_json::json!({ "allowed": false }))
            .unwrap();
        let error = plan_teamwork_workflow_with_config(
            &store,
            "Reject a disallowed configured lane brain",
            false,
            false,
            frontend_backend_config(),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("lane frontend requires brain agy"));
        assert!(store.load_workflows().unwrap().is_empty());
    }

    #[test]
    fn configured_parallel_ceiling_cannot_invent_agents_without_independent_slots() {
        let temp = tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        let mut config = frontend_backend_config();
        config.max_parallel_agents = 9;

        let error = plan_teamwork_workflow_with_config(
            &store,
            "Do not exceed the declared independent frontend and backend slots",
            false,
            false,
            config,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("cannot exceed the 8 configured independent agent slots"));
        assert!(store.load_workflows().unwrap().is_empty());
    }

    #[test]
    fn teamwork_plan_rolls_back_workflow_run_and_event_as_one_unit() {
        let temp = tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        let connection = open_configured_connection(store.path()).unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_teamwork_plan_event
                BEFORE INSERT ON events
                WHEN NEW.kind = 'teamwork_planned'
                BEGIN
                    SELECT RAISE(ABORT, 'reject teamwork event for atomicity proof');
                END;
                "#,
            )
            .unwrap();
        drop(connection);

        let error =
            plan_teamwork_workflow(&store, "Prove atomic teamwork persistence", true, false)
                .unwrap_err();
        assert!(error
            .to_string()
            .contains("reject teamwork event for atomicity proof"));
        assert!(store.load_workflows().unwrap().is_empty());
        assert!(store.load_runs().unwrap().is_empty());
        assert!(open_configured_connection(store.path())
            .unwrap()
            .query_row("SELECT COUNT(*) FROM events", [], |row| row
                .get::<_, i64>(0))
            .is_ok_and(|count| count == 0));
    }
}
