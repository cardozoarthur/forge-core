use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
    WorkflowRegistryReport,
};
use crate::request::{
    complete_ready_task, drive_request, step_request, RequestTaskCompletionInput,
};
use crate::storage::ForgeStore;
use crate::workflow::{update_workflow_goal, update_workflow_task, WorkflowTaskUpdateInput};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use uuid::Uuid;

const OPS_SNAPSHOT_SCHEMA_VERSION: &str = "forge.ops.snapshot.v1";
const OPS_ACTION_SCHEMA_VERSION: &str = "forge.ops.action.v1";
const OPS_MODIFIER_LANE_SCHEMA_VERSION: &str = "forge.ops.modifier_lane.v1";
const OPS_MODIFIER_PROPOSAL_SCHEMA_VERSION: &str = "forge.ops.modifier_proposal.v1";
const OPS_MODIFIER_PROPOSAL_CREATED_EVENT: &str = "ops_modifier_proposal_created";
const OPS_MODIFIER_PROPOSAL_APPLIED_EVENT: &str = "ops_modifier_proposal_applied";
const MAX_HTTP_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct OpsSnapshot {
    pub status: String,
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub mode: OpsMode,
    pub registry: WorkflowRegistryReport,
    pub modifier_lane: OpsModifierLane,
    pub actions: Vec<OpsActionSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsMode {
    pub operational: bool,
    pub strategic: bool,
    pub realtime_mutation: bool,
    pub assisted_operations: bool,
    pub local_only_by_default: bool,
    pub ai_modifier_lane: String,
    pub human_access: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsActionSpec {
    pub id: String,
    pub method: String,
    pub path: String,
    pub description: String,
    pub mutates_workflow: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsModifierLane {
    pub schema_version: String,
    pub purpose: String,
    pub pending_count: usize,
    pub applied_count: usize,
    pub proposals: Vec<OpsModifierProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsModifierProposal {
    pub schema_version: String,
    pub proposal_id: String,
    pub workflow_id: String,
    pub target_kind: String,
    pub task_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub proposed_goal: Option<String>,
    pub proposed_title: Option<String>,
    pub proposed_expected_output: Option<String>,
    pub author: String,
    pub status: String,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub applied_revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct OpsModifierProposalInput<'a> {
    pub workflow_id: &'a str,
    pub target_kind: &'a str,
    pub task_id: Option<&'a str>,
    pub title: &'a str,
    pub summary: &'a str,
    pub rationale: &'a str,
    pub proposed_goal: Option<&'a str>,
    pub proposed_title: Option<&'a str>,
    pub proposed_expected_output: Option<&'a str>,
    pub author: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsModifierProposalReport {
    pub status: String,
    pub proposal: OpsModifierProposal,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsModifierApplyReport {
    pub status: String,
    pub proposal_id: String,
    pub workflow_id: String,
    pub target_kind: String,
    pub task_id: Option<String>,
    pub origin: String,
    pub applied_at: String,
    pub revision: u64,
    pub mutation: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsServeReport {
    pub status: String,
    pub schema_version: String,
    pub bind_addr: String,
    pub url: String,
    pub local_only: bool,
    pub routes: Vec<OpsActionSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsActionReport {
    pub status: String,
    pub schema_version: String,
    pub action: String,
    pub result: Value,
}

#[derive(Debug)]
pub struct OpsHttpResponse {
    pub status_code: u16,
    pub reason: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

pub fn build_ops_snapshot(store: &ForgeStore) -> Result<OpsSnapshot> {
    let registry = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    let modifier_lane = load_modifier_lane(store)?;
    Ok(OpsSnapshot {
        status: "ok".to_string(),
        schema_version: OPS_SNAPSHOT_SCHEMA_VERSION.to_string(),
        generated_at: Utc::now(),
        mode: OpsMode {
            operational: true,
            strategic: true,
            realtime_mutation: true,
            assisted_operations: true,
            local_only_by_default: true,
            ai_modifier_lane:
                "separate_orchestrator_can_update_goals_nodes_and_subflows_via_forge_apis"
                    .to_string(),
            human_access: "full_local_workflow_visibility_and_runtime_mutation_controls"
                .to_string(),
        },
        registry,
        modifier_lane,
        actions: ops_actions(),
    })
}

pub fn load_modifier_lane(store: &ForgeStore) -> Result<OpsModifierLane> {
    let mut proposals = Vec::new();
    let mut applied = BTreeMap::new();

    for workflow in store.load_workflows()? {
        for event in store.load_workflow_events(&workflow.id)? {
            match event.kind.as_str() {
                OPS_MODIFIER_PROPOSAL_CREATED_EVENT => {
                    let mut proposal: OpsModifierProposal =
                        serde_json::from_value(event.data.clone()).with_context(|| {
                            format!("invalid modifier proposal event for {}", workflow.id)
                        })?;
                    if proposal.created_at.trim().is_empty() {
                        proposal.created_at = event.created_at;
                    }
                    proposals.push(proposal);
                }
                OPS_MODIFIER_PROPOSAL_APPLIED_EVENT => {
                    if let Some(proposal_id) = event.data.get("proposal_id").and_then(Value::as_str)
                    {
                        let applied_at = event
                            .data
                            .get("applied_at")
                            .and_then(Value::as_str)
                            .unwrap_or(&event.created_at)
                            .to_string();
                        let revision = event.data.get("revision").and_then(Value::as_u64);
                        applied.insert(proposal_id.to_string(), (applied_at, revision));
                    }
                }
                _ => {}
            }
        }
    }

    for proposal in &mut proposals {
        if let Some((applied_at, revision)) = applied.get(&proposal.proposal_id) {
            proposal.status = "applied".to_string();
            proposal.applied_at = Some(applied_at.clone());
            proposal.applied_revision = *revision;
        }
    }

    proposals.sort_by(|left, right| {
        left.status
            .cmp(&right.status)
            .then(left.created_at.cmp(&right.created_at))
            .then(left.proposal_id.cmp(&right.proposal_id))
    });
    let pending_count = proposals
        .iter()
        .filter(|proposal| proposal.status == "pending")
        .count();
    let applied_count = proposals
        .iter()
        .filter(|proposal| proposal.status == "applied")
        .count();

    Ok(OpsModifierLane {
        schema_version: OPS_MODIFIER_LANE_SCHEMA_VERSION.to_string(),
        purpose: "separate_ai_or_human_modifier_lane_for_live_strategy_and_node_mutation"
            .to_string(),
        pending_count,
        applied_count,
        proposals,
    })
}

pub fn create_modifier_proposal(
    store: &ForgeStore,
    input: OpsModifierProposalInput<'_>,
) -> Result<OpsModifierProposalReport> {
    let workflow = store.load_workflow(input.workflow_id)?;
    let task_id = clean_optional(input.task_id);

    match input.target_kind {
        "workflow_goal" => {
            if clean_optional(input.proposed_goal).is_none() {
                bail!("workflow goal modifier proposals require proposed_goal");
            }
        }
        "task_node" => {
            let Some(task_id) = task_id.as_deref() else {
                bail!("task node modifier proposals require task_id");
            };
            if !workflow.tasks.iter().any(|task| task.id == task_id) {
                bail!("task {task_id} not found in workflow {}", input.workflow_id);
            }
            if clean_optional(input.proposed_title).is_none()
                && clean_optional(input.proposed_goal).is_none()
                && clean_optional(input.proposed_expected_output).is_none()
            {
                bail!(
                    "task node modifier proposals require title, goal or expected_output mutation"
                );
            }
        }
        other => bail!("unsupported modifier target_kind `{other}`"),
    }

    let proposal = OpsModifierProposal {
        schema_version: OPS_MODIFIER_PROPOSAL_SCHEMA_VERSION.to_string(),
        proposal_id: format!("ops_prop_{}", Uuid::new_v4().to_string().replace('-', "")),
        workflow_id: input.workflow_id.to_string(),
        target_kind: input.target_kind.to_string(),
        task_id,
        title: clean_required(input.title, "title")?,
        summary: clean_required(input.summary, "summary")?,
        rationale: clean_required(input.rationale, "rationale")?,
        proposed_goal: clean_optional(input.proposed_goal),
        proposed_title: clean_optional(input.proposed_title),
        proposed_expected_output: clean_optional(input.proposed_expected_output),
        author: clean_optional(Some(input.author)).unwrap_or_else(|| "ops-web".to_string()),
        status: "pending".to_string(),
        created_at: Utc::now().to_rfc3339(),
        applied_at: None,
        applied_revision: None,
    };
    store.record_event(
        &proposal.workflow_id,
        OPS_MODIFIER_PROPOSAL_CREATED_EVENT,
        &serde_json::to_value(&proposal)?,
    )?;

    Ok(OpsModifierProposalReport {
        status: "modifier_proposal_created".to_string(),
        proposal,
    })
}

pub fn apply_modifier_proposal(
    store: &ForgeStore,
    proposal_id: &str,
    origin: &str,
) -> Result<OpsModifierApplyReport> {
    let lane = load_modifier_lane(store)?;
    let proposal = lane
        .proposals
        .into_iter()
        .find(|proposal| proposal.proposal_id == proposal_id)
        .with_context(|| format!("modifier proposal not found: {proposal_id}"))?;
    if proposal.status != "pending" {
        bail!(
            "modifier proposal {} is not pending; current status is {}",
            proposal.proposal_id,
            proposal.status
        );
    }

    let applied_at = Utc::now().to_rfc3339();
    let (revision, mutation) = match proposal.target_kind.as_str() {
        "workflow_goal" => {
            let goal = proposal
                .proposed_goal
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .context("workflow goal proposal has no proposed_goal")?;
            let report = update_workflow_goal(store, &proposal.workflow_id, goal, origin)?;
            (report.revision, serde_json::to_value(report)?)
        }
        "task_node" => {
            let task_id = proposal
                .task_id
                .as_deref()
                .context("task node proposal has no task_id")?;
            let report = update_workflow_task(
                store,
                &proposal.workflow_id,
                WorkflowTaskUpdateInput {
                    task_id,
                    title: proposal.proposed_title.as_deref(),
                    goal: proposal.proposed_goal.as_deref(),
                    expected_output: proposal.proposed_expected_output.as_deref(),
                    origin,
                },
            )?;
            (report.revision, serde_json::to_value(report)?)
        }
        other => bail!("unsupported modifier target_kind `{other}`"),
    };

    store.record_event(
        &proposal.workflow_id,
        OPS_MODIFIER_PROPOSAL_APPLIED_EVENT,
        &serde_json::json!({
            "proposal_id": proposal.proposal_id,
            "workflow_id": proposal.workflow_id,
            "target_kind": proposal.target_kind,
            "task_id": proposal.task_id,
            "origin": origin,
            "applied_at": applied_at,
            "revision": revision,
            "mutation": mutation
        }),
    )?;

    Ok(OpsModifierApplyReport {
        status: "modifier_proposal_applied".to_string(),
        proposal_id: proposal.proposal_id,
        workflow_id: proposal.workflow_id,
        target_kind: proposal.target_kind,
        task_id: proposal.task_id,
        origin: origin.to_string(),
        applied_at,
        revision,
        mutation,
    })
}

pub fn serve_ops_console(store_path: PathBuf, host: &str, port: u16) -> Result<OpsServeReport> {
    let listener = TcpListener::bind((host, port))
        .with_context(|| format!("failed to bind Forge ops server on {host}:{port}"))?;
    let addr = listener.local_addr()?;
    let report = OpsServeReport {
        status: "listening".to_string(),
        schema_version: "forge.ops.serve.v1".to_string(),
        bind_addr: addr.to_string(),
        url: format!("http://{addr}/"),
        local_only: addr.ip().is_loopback(),
        routes: ops_actions(),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle_stream(&store_path, &mut stream) {
                    let response = error_response(500, "Internal Server Error", &error.to_string());
                    let _ = stream.write_all(&response.to_http_bytes());
                }
            }
            Err(error) => eprintln!("forge ops server connection error: {error}"),
        }
    }

    Ok(report)
}

pub fn handle_ops_http_request(store: &ForgeStore, request: &str) -> OpsHttpResponse {
    match route_ops_http_request(store, request) {
        Ok(response) => response,
        Err(error) => error_response(400, "Bad Request", &error.to_string()),
    }
}

fn handle_stream(store_path: &PathBuf, stream: &mut TcpStream) -> Result<()> {
    let mut buffer = vec![0; MAX_HTTP_REQUEST_BYTES];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
    let store = ForgeStore::open(store_path)?;
    let response = handle_ops_http_request(&store, &request);
    stream.write_all(&response.to_http_bytes())?;
    Ok(())
}

fn route_ops_http_request(store: &ForgeStore, request: &str) -> Result<OpsHttpResponse> {
    let parsed = ParsedRequest::parse(request)?;
    match (parsed.method.as_str(), parsed.path.as_str()) {
        ("GET", "/") => {
            let snapshot = build_ops_snapshot(store)?;
            Ok(html_response(render_ops_html(&snapshot)))
        }
        ("GET", "/api/snapshot") => json_response(&build_ops_snapshot(store)?),
        ("POST", "/api/run/drive") => {
            let run_id = parsed.required("run_id")?;
            let executor = parsed
                .params
                .get("executor")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = drive_request(store, run_id, executor, 300, "ops-web")?;
            action_response("drive_run", &report)
        }
        ("POST", "/api/run/step") => {
            let run_id = parsed.required("run_id")?;
            let executor = parsed
                .params
                .get("executor")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = step_request(store, run_id, executor, 300, "ops-web")?;
            action_response("step_run", &report)
        }
        ("POST", "/api/run/complete-task") => {
            let run_id = parsed.required("run_id")?;
            let task_id = parsed.required("task_id")?;
            let summary = parsed.required("summary")?;
            let executor = parsed
                .params
                .get("executor")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let evidence_command = parsed.params.get("evidence_command").map(String::as_str);
            let report = complete_ready_task(
                store,
                run_id,
                RequestTaskCompletionInput {
                    task_id,
                    executor,
                    summary,
                    artifact_paths: &[],
                    evidence_command,
                    evidence_summary: Some(summary),
                    estimated_usd: 0.0,
                    tokens_in: 0,
                    tokens_out: 0,
                    ttl_seconds: 300,
                    origin: "ops-web",
                },
            )?;
            action_response("complete_task", &report)
        }
        ("POST", "/api/workflow/update-goal") => {
            let workflow_id = parsed.required("workflow_id")?;
            let goal = parsed.required("goal")?;
            let report = update_workflow_goal(store, workflow_id, goal, "ops-web")?;
            action_response("update_goal", &report)
        }
        ("POST", "/api/workflow/update-task") => {
            let workflow_id = parsed.required("workflow_id")?;
            let task_id = parsed.required("task_id")?;
            let title = parsed.params.get("title").map(String::as_str);
            let goal = parsed.params.get("goal").map(String::as_str);
            let expected_output = parsed.params.get("expected_output").map(String::as_str);
            let report = update_workflow_task(
                store,
                workflow_id,
                WorkflowTaskUpdateInput {
                    task_id,
                    title,
                    goal,
                    expected_output,
                    origin: "ops-web",
                },
            )?;
            action_response("update_task", &report)
        }
        ("POST", "/api/modifier/propose-goal") => {
            let workflow_id = parsed.required("workflow_id")?;
            let goal = parsed.required("goal")?;
            let title = parsed
                .params
                .get("title")
                .map(String::as_str)
                .unwrap_or("Proposta de objetivo");
            let summary = parsed
                .params
                .get("summary")
                .map(String::as_str)
                .unwrap_or(goal);
            let rationale = parsed
                .params
                .get("rationale")
                .map(String::as_str)
                .unwrap_or("Proposta criada pela lane modificadora");
            let author = parsed
                .params
                .get("author")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = create_modifier_proposal(
                store,
                OpsModifierProposalInput {
                    workflow_id,
                    target_kind: "workflow_goal",
                    task_id: None,
                    title,
                    summary,
                    rationale,
                    proposed_goal: Some(goal),
                    proposed_title: None,
                    proposed_expected_output: None,
                    author,
                },
            )?;
            action_response("modifier_propose_goal", &report)
        }
        ("POST", "/api/modifier/propose-task") => {
            let workflow_id = parsed.required("workflow_id")?;
            let task_id = parsed.required("task_id")?;
            let title = parsed
                .params
                .get("proposal_title")
                .map(String::as_str)
                .unwrap_or("Proposta de atualização de node");
            let summary = parsed
                .params
                .get("summary")
                .map(String::as_str)
                .unwrap_or("Atualizar node durante a operação");
            let rationale = parsed
                .params
                .get("rationale")
                .map(String::as_str)
                .unwrap_or("Proposta criada pela lane modificadora");
            let author = parsed
                .params
                .get("author")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = create_modifier_proposal(
                store,
                OpsModifierProposalInput {
                    workflow_id,
                    target_kind: "task_node",
                    task_id: Some(task_id),
                    title,
                    summary,
                    rationale,
                    proposed_goal: parsed.params.get("goal").map(String::as_str),
                    proposed_title: parsed.params.get("node_title").map(String::as_str),
                    proposed_expected_output: parsed
                        .params
                        .get("expected_output")
                        .map(String::as_str),
                    author,
                },
            )?;
            action_response("modifier_propose_task", &report)
        }
        ("POST", "/api/modifier/apply") => {
            let proposal_id = parsed.required("proposal_id")?;
            let origin = parsed
                .params
                .get("origin")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = apply_modifier_proposal(store, proposal_id, origin)?;
            action_response("modifier_apply", &report)
        }
        _ => Ok(error_response(404, "Not Found", "unknown Forge ops route")),
    }
}

fn action_response<T: Serialize>(action: &str, result: &T) -> Result<OpsHttpResponse> {
    json_response(&OpsActionReport {
        status: "ok".to_string(),
        schema_version: OPS_ACTION_SCHEMA_VERSION.to_string(),
        action: action.to_string(),
        result: serde_json::to_value(result)?,
    })
}

pub fn render_ops_html(snapshot: &OpsSnapshot) -> String {
    let mut rows = String::new();
    for workflow in &snapshot.registry.workflows {
        let run_ids = if workflow.run_ids.is_empty() {
            "none".to_string()
        } else {
            workflow.run_ids.join(", ")
        };
        rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}/{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&workflow.workflow_id),
            escape_html(&workflow.workflow_status),
            escape_html(&workflow.lifecycle_state),
            workflow.task_summary.completed,
            workflow.task_summary.total,
            workflow.active_run_count,
            escape_html(&run_ids),
            escape_html(&truncate(&workflow.current_goal, 120)),
        ));
    }
    let mut proposal_rows = String::new();
    for proposal in &snapshot.modifier_lane.proposals {
        let target = proposal
            .task_id
            .as_ref()
            .map(|task_id| format!("{} / {}", proposal.target_kind, task_id))
            .unwrap_or_else(|| proposal.target_kind.clone());
        proposal_rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&proposal.proposal_id),
            escape_html(&proposal.status),
            escape_html(&proposal.workflow_id),
            escape_html(&target),
            escape_html(&proposal.title),
            escape_html(&truncate(&proposal.summary, 120)),
        ));
    }
    if proposal_rows.is_empty() {
        proposal_rows.push_str(
            "<tr><td colspan=\"6\">Nenhuma proposta da lane modificadora registrada.</td></tr>",
        );
    }

    format!(
        r#"<!doctype html>
<html lang="pt-BR">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Forge Ops</title>
  <style>
    body {{ font-family: system-ui, -apple-system, Segoe UI, sans-serif; margin: 24px; color: #18212f; background: #f7f8fb; }}
    h1 {{ margin: 0 0 8px; font-size: 28px; }}
    h2 {{ margin-top: 28px; font-size: 18px; }}
    table {{ width: 100%; border-collapse: collapse; background: white; border: 1px solid #d9deea; }}
    th, td {{ padding: 10px 12px; border-bottom: 1px solid #e8ecf4; text-align: left; vertical-align: top; font-size: 14px; }}
    th {{ background: #eef2f7; }}
    code {{ font-size: 12px; }}
    form {{ display: grid; gap: 8px; max-width: 760px; margin: 12px 0; }}
    input, textarea, button {{ font: inherit; padding: 8px 10px; border: 1px solid #cbd3df; border-radius: 6px; }}
    button {{ width: fit-content; background: #1f6feb; color: white; border-color: #1f6feb; cursor: pointer; }}
    .summary {{ display: flex; gap: 12px; flex-wrap: wrap; margin: 16px 0; }}
    .pill {{ background: white; border: 1px solid #d9deea; border-radius: 999px; padding: 8px 12px; }}
    .section-note {{ max-width: 900px; color: #4b5563; }}
  </style>
</head>
<body>
  <h1>Forge Ops</h1>
  <p>Operação assistida local: humano e IA podem observar workflows, dirigir runs e alterar objetivos em tempo real.</p>
  <div class="summary">
    <span class="pill">workflows: {}</span>
    <span class="pill">running: {}</span>
    <span class="pill">generated: {}</span>
    <span class="pill">local-only: {}</span>
    <span class="pill">modifier pending: {}</span>
  </div>
  <h2>Workflows</h2>
  <table>
    <thead><tr><th>Workflow</th><th>Status</th><th>Lifecycle</th><th>Tasks</th><th>Active runs</th><th>Runs</th><th>Goal</th></tr></thead>
    <tbody>{}</tbody>
  </table>
  <h2>Lane modificadora</h2>
  <p class="section-note">Trilha separada para uma IA estratégica ou operador humano propor mudanças de objetivo e nodes sem interromper a operação.</p>
  <table>
    <thead><tr><th>Proposta</th><th>Status</th><th>Workflow</th><th>Alvo</th><th>Título</th><th>Resumo</th></tr></thead>
    <tbody>{}</tbody>
  </table>
  <form method="post" action="/api/modifier/propose-goal">
    <input name="workflow_id" placeholder="workflow_id">
    <input name="title" value="Ajuste estratégico de objetivo">
    <textarea name="goal" placeholder="Objetivo proposto"></textarea>
    <textarea name="summary" placeholder="Resumo da proposta"></textarea>
    <textarea name="rationale" placeholder="Racional estratégico"></textarea>
    <input name="author" value="ops-web">
    <button type="submit">Propor objetivo</button>
  </form>
  <form method="post" action="/api/modifier/propose-task">
    <input name="workflow_id" placeholder="workflow_id">
    <input name="task_id" placeholder="task_id">
    <input name="proposal_title" value="Ajuste estratégico de node">
    <input name="node_title" placeholder="Novo título opcional">
    <textarea name="goal" placeholder="Novo objetivo do node opcional"></textarea>
    <input name="expected_output" placeholder="Novo output esperado opcional">
    <textarea name="summary" placeholder="Resumo da proposta"></textarea>
    <textarea name="rationale" placeholder="Racional estratégico"></textarea>
    <input name="author" value="ops-web">
    <button type="submit">Propor node</button>
  </form>
  <form method="post" action="/api/modifier/apply">
    <input name="proposal_id" placeholder="proposal_id">
    <input name="origin" value="ops-web">
    <button type="submit">Aplicar proposta</button>
  </form>
  <h2>Operar run</h2>
  <form method="post" action="/api/run/drive">
    <input name="run_id" placeholder="run_id">
    <input name="executor" value="ops-web">
    <button type="submit">Drive</button>
  </form>
  <form method="post" action="/api/run/step">
    <input name="run_id" placeholder="run_id">
    <input name="executor" value="ops-web">
    <button type="submit">Step determinístico</button>
  </form>
  <form method="post" action="/api/run/complete-task">
    <input name="run_id" placeholder="run_id">
    <input name="task_id" placeholder="task_id">
    <input name="executor" value="ops-web">
    <textarea name="summary" placeholder="Resumo/evidência do executor"></textarea>
    <input name="evidence_command" placeholder="comando ou gate de evidência">
    <button type="submit">Completar task com evidência</button>
  </form>
  <h2>Atualizar objetivo em tempo real</h2>
  <form method="post" action="/api/workflow/update-goal">
    <input name="workflow_id" placeholder="workflow_id">
    <textarea name="goal" placeholder="Novo objetivo"></textarea>
    <button type="submit">Atualizar objetivo</button>
  </form>
  <h2>Atualizar node em tempo real</h2>
  <form method="post" action="/api/workflow/update-task">
    <input name="workflow_id" placeholder="workflow_id">
    <input name="task_id" placeholder="task_id">
    <input name="title" placeholder="Novo título opcional">
    <textarea name="goal" placeholder="Novo objetivo do node opcional"></textarea>
    <input name="expected_output" placeholder="Novo output esperado opcional">
    <button type="submit">Atualizar node</button>
  </form>
</body>
</html>"#,
        snapshot.registry.summary.total,
        snapshot.registry.summary.running,
        snapshot.generated_at,
        snapshot.mode.local_only_by_default,
        snapshot.modifier_lane.pending_count,
        rows,
        proposal_rows
    )
}

fn json_response<T: Serialize>(value: &T) -> Result<OpsHttpResponse> {
    Ok(OpsHttpResponse {
        status_code: 200,
        reason: "OK".to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec_pretty(value)?,
    })
}

fn html_response(html: String) -> OpsHttpResponse {
    OpsHttpResponse {
        status_code: 200,
        reason: "OK".to_string(),
        content_type: "text/html; charset=utf-8".to_string(),
        body: html.into_bytes(),
    }
}

fn error_response(status_code: u16, reason: &str, message: &str) -> OpsHttpResponse {
    OpsHttpResponse {
        status_code,
        reason: reason.to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::json!({
            "status": "error",
            "schema_version": "forge.ops.error.v1",
            "message": message
        })
        .to_string()
        .into_bytes(),
    }
}

impl OpsHttpResponse {
    fn to_http_bytes(&self) -> Vec<u8> {
        let header = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.status_code,
            self.reason,
            self.content_type,
            self.body.len()
        );
        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

#[derive(Debug)]
struct ParsedRequest {
    method: String,
    path: String,
    params: BTreeMap<String, String>,
}

impl ParsedRequest {
    fn parse(request: &str) -> Result<Self> {
        let (head, body) = request.split_once("\r\n\r\n").unwrap_or((request, ""));
        let request_line = head.lines().next().context("missing HTTP request line")?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().context("missing HTTP method")?.to_string();
        let raw_target = parts.next().context("missing HTTP target")?;
        let (path, query) = raw_target.split_once('?').unwrap_or((raw_target, ""));
        let mut params = parse_form(query);
        if method == "POST" {
            params.extend(parse_form(body));
        }
        Ok(Self {
            method,
            path: path.to_string(),
            params,
        })
    }

    fn required(&self, key: &str) -> Result<&str> {
        self.params
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .with_context(|| format!("missing required parameter `{key}`"))
    }
}

fn parse_form(input: &str) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    for pair in input.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(percent_decode(key), percent_decode(value));
    }
    params
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                output.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                    if let Ok(value) = u8::from_str_radix(hex, 16) {
                        output.push(value);
                        i += 3;
                        continue;
                    }
                }
                output.push(bytes[i]);
                i += 1;
            }
            byte => {
                output.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).to_string()
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn truncate(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let mut value = input.chars().take(max_chars).collect::<String>();
    value.push_str("...");
    value
}

fn clean_required(input: &str, field: &str) -> Result<String> {
    let value = input.trim();
    if value.is_empty() {
        bail!("missing required modifier proposal field `{field}`");
    }
    Ok(value.to_string())
}

fn clean_optional(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn ops_actions() -> Vec<OpsActionSpec> {
    vec![
        action(
            "snapshot",
            "GET",
            "/api/snapshot",
            "Read the current operational snapshot.",
            false,
        ),
        action(
            "drive_run",
            "POST",
            "/api/run/drive",
            "Drive a run to its next safe action.",
            true,
        ),
        action(
            "step_run",
            "POST",
            "/api/run/step",
            "Auto-promote one ready deterministic task when safe.",
            true,
        ),
        action(
            "complete_task",
            "POST",
            "/api/run/complete-task",
            "Complete a ready task with executor evidence.",
            true,
        ),
        action(
            "update_goal",
            "POST",
            "/api/workflow/update-goal",
            "Mutate a workflow objective while processing is live.",
            true,
        ),
        action(
            "update_task",
            "POST",
            "/api/workflow/update-task",
            "Mutate a workflow task/node title, goal or expected output while processing is live.",
            true,
        ),
        action(
            "modifier_propose_goal",
            "POST",
            "/api/modifier/propose-goal",
            "Create a pending strategic modifier proposal for a workflow objective.",
            true,
        ),
        action(
            "modifier_propose_task",
            "POST",
            "/api/modifier/propose-task",
            "Create a pending strategic modifier proposal for a workflow task/node.",
            true,
        ),
        action(
            "modifier_apply",
            "POST",
            "/api/modifier/apply",
            "Apply a pending modifier proposal as a live workflow mutation.",
            true,
        ),
    ]
}

fn action(
    id: &str,
    method: &str,
    path: &str,
    description: &str,
    mutates_workflow: bool,
) -> OpsActionSpec {
    OpsActionSpec {
        id: id.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        description: description.to_string(),
        mutates_workflow,
    }
}
