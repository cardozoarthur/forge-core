use crate::graph::create_workflow;
use crate::intent::parse_intent;
use crate::request::{create_run_record, save_run_record};
use crate::storage::{open_configured_connection, ForgeStore};
use anyhow::{anyhow, Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;

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
    pub tasks: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmarks: Option<serde_json::Value>,
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
    store: &ForgeStore,
    goal: &str,
    detached: bool,
    bypass_cache: bool,
) -> Result<TeamworkResponse> {
    if goal.trim().is_empty() {
        return Err(anyhow!("Goal cannot be empty"));
    }

    let intent = parse_intent(goal);
    let workflow = create_workflow(intent);

    store.save_workflow(&workflow)?;
    store.record_event(
        &workflow.id,
        "teamwork_planned",
        &serde_json::to_value(&workflow)?,
    )?;

    let run = create_run_record(&workflow, "forge_cli", "accepted");
    save_run_record(store, &run)?;
    let run_id = if detached { Some(run.run_id) } else { None };

    let tasks_json = workflow
        .tasks
        .iter()
        .map(|task| serde_json::to_value(task).unwrap())
        .collect();

    // Roster and Heuristics Logic
    let conn = open_configured_connection(store.path())?;

    // 1. Query disallowed brains from executor policy
    let mut disallowed_brains = HashSet::new();
    disallowed_brains.insert("gemini".to_string());
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
                        if data_json["allowed"].as_bool() == Some(false) {
                            disallowed_brains.insert(id);
                        }
                    }
                }
            }
        }
    }

    // 2. Fetch and Cache Benchmarks if FORGE_BENCHMARK_URL is configured
    let benchmark_url = std::env::var("FORGE_BENCHMARK_URL").ok();
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

    let worker_brain = match selected_worker_brain {
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

    // 4. Assemble the Roster
    let roles = vec![
        TeamworkRole {
            slot_id: "agent-001".to_string(),
            role: "Orchestrator".to_string(),
            brain: orchestrator_brain,
            parallel_group: "control".to_string(),
            responsibility: "Turn the approved goal into an auditable task graph, assign work, and keep Forge workflow state authoritative.".to_string(),
        },
        TeamworkRole {
            slot_id: "agent-002".to_string(),
            role: "Worker".to_string(),
            brain: worker_brain,
            parallel_group: "implementation".to_string(),
            responsibility: "Execute the main implementation or analysis tasks using the selected Codex or Antigravity-compatible brain.".to_string(),
        },
        TeamworkRole {
            slot_id: "agent-003".to_string(),
            role: "Auditor".to_string(),
            brain: auditor_brain,
            parallel_group: "validation".to_string(),
            responsibility: "Challenge the result, verify acceptance criteria, and block promotion until evidence is attached.".to_string(),
        },
    ];
    let strategy = TeamworkStrategy {
        schema_version: "forge.teamwork.strategy.v1".to_string(),
        mode: if detached {
            "detached_teamwork_run".to_string()
        } else {
            "planned_teamwork_run".to_string()
        },
        source_evidence: vec![
            "Antigravity agy exposes /teamwork-preview as an internal slash-command pattern, not as a public CLI subcommand.".to_string(),
            "Observed flow: prompt draft, user approval, delegated teamwork_preview subagents, execution approvals, and artifact-bound handoff.".to_string(),
            "Forge adaptation keeps workflow state, context routing, validation gates, artifacts, and executor policy inside Forge.".to_string(),
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
                owner_role: "Worker".to_string(),
                execution_model: "run the implementation wave with bounded Codex or Antigravity/agy agent slots".to_string(),
                exit_gate: "task outputs are persisted or attached to the workflow".to_string(),
            },
            TeamworkPhase {
                phase: "audit_and_promotion".to_string(),
                owner_role: "Auditor".to_string(),
                execution_model: "review, validate, request rework, and promote only with evidence".to_string(),
                exit_gate: "validation rules pass and no unresolved impediment remains".to_string(),
            },
        ],
        recommended_agent_count: roles.len(),
        max_parallel_agents: 3,
        primary_brains: vec![
            "codex".to_string(),
            "agy".to_string(),
            "opencode".to_string(),
        ],
        legacy_brains_invalidated: vec!["gemini".to_string()],
    };

    Ok(TeamworkResponse {
        schema_version: "forge.teamwork.plan.v1".to_string(),
        status: "planned".to_string(),
        workflow_id: workflow.id,
        run_id,
        goal: goal.to_string(),
        detached,
        strategy,
        roster: TeamworkRoster {
            agent_count: roles.len(),
            max_parallel_agents: 3,
            policy: "codex_or_agy_first_legacy_gemini_invalidated".to_string(),
            roles,
        },
        tasks: tasks_json,
        benchmarks: benchmarks_json,
    })
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
