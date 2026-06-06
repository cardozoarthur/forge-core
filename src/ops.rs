use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
    WorkflowRegistryReport,
};
use crate::request::{
    complete_ready_task, drive_request, step_request, RequestTaskCompletionInput,
};
use crate::storage::ForgeStore;
use crate::workflow::{update_workflow_goal, update_workflow_task, WorkflowTaskUpdateInput};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

const OPS_SNAPSHOT_SCHEMA_VERSION: &str = "forge.ops.snapshot.v1";
const OPS_ACTION_SCHEMA_VERSION: &str = "forge.ops.action.v1";
const MAX_HTTP_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct OpsSnapshot {
    pub status: String,
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub mode: OpsMode,
    pub registry: WorkflowRegistryReport,
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
        actions: ops_actions(),
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
  </div>
  <h2>Workflows</h2>
  <table>
    <thead><tr><th>Workflow</th><th>Status</th><th>Lifecycle</th><th>Tasks</th><th>Active runs</th><th>Runs</th><th>Goal</th></tr></thead>
    <tbody>{}</tbody>
  </table>
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
        rows
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
