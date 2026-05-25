use crate::artifact::{hex_sha256, list_workflow_artifacts, ListedArtifact};
use crate::checkpoint::load_latest_task_checkpoint;
use crate::context::{build_context_package_with_checkpoint, DEFAULT_CONTEXT_BUDGET};
use crate::inspection::inspect_workflow_with_focus;
use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
};
use crate::request::{
    cancel_request, list_requests, load_request_status, resume_async_request, start_async_request,
};
use crate::storage::ForgeStore;
use crate::validation::{validate_workflow, ValidationReport};
use crate::workflow::{attach_workflow_artifact, update_workflow_goal};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const MCP_TOOLS_SCHEMA_VERSION: &str = "forge.mcp.tools.v1";
const MCP_CALL_SCHEMA_VERSION: &str = "forge.mcp.call.v1";
const MCP_VALIDATION_STATUS_SCHEMA_VERSION: &str = "forge.mcp.validation_status.v1";
const MCP_ARTIFACT_FETCH_SCHEMA_VERSION: &str = "forge.mcp.artifact_fetch.v1";
const MAX_ARTIFACT_FETCH_BYTES: usize = 65_536;

#[derive(Debug, Clone, Serialize)]
pub struct McpToolsManifest {
    pub status: String,
    pub schema_version: String,
    pub protocol: String,
    pub tools: Vec<McpToolSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolSpec {
    pub name: String,
    pub title: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: String,
    pub forge_command: Vec<String>,
    pub async_safe: bool,
    pub mutates_workflow: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpCallReport {
    pub schema_version: String,
    pub status: String,
    pub tool_name: String,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize)]
struct McpValidationStatusReport {
    schema_version: String,
    workflow_id: String,
    workflow_revision: u64,
    validation: ValidationReport,
}

#[derive(Debug, Clone, Serialize)]
struct McpArtifactFetchReport {
    schema_version: String,
    workflow_id: String,
    artifacts: Vec<ListedArtifact>,
    artifact: Option<ListedArtifact>,
    artifact_sha256: Option<String>,
    bytes: Option<u64>,
    max_bytes: usize,
    truncated: bool,
    content_sha256: Option<String>,
    content_utf8: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkflowListInput {
    lifecycle: Option<String>,
    context_action: Option<String>,
    quality_action: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkflowInspectInput {
    workflow_id: String,
    task_id: Option<String>,
    verbose: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RunStartInput {
    goal: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunIdInput {
    run_id: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestListInput {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestCancelInput {
    run_id: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkflowUpdateGoalInput {
    workflow_id: String,
    goal: String,
    origin: String,
}

#[derive(Debug, Deserialize)]
struct WorkflowAttachArtifactInput {
    workflow_id: String,
    path: String,
    kind: String,
    origin: String,
}

#[derive(Debug, Deserialize)]
struct ContextRequestInput {
    workflow_id: String,
    task_id: String,
    budget: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct WorkflowIdInput {
    workflow_id: String,
}

#[derive(Debug, Deserialize)]
struct ArtifactFetchInput {
    workflow_id: String,
    path: Option<String>,
    max_bytes: Option<usize>,
}

pub fn mcp_tools_manifest() -> McpToolsManifest {
    McpToolsManifest {
        status: "mcp_tools_loaded".to_string(),
        schema_version: MCP_TOOLS_SCHEMA_VERSION.to_string(),
        protocol: "model_context_protocol".to_string(),
        tools: vec![
            tool(
                "forge.workflow.list",
                "List Forge Workflows",
                "List workflows with lifecycle, context-action and quality-action filters.",
                object_schema(&[
                    ("lifecycle", "string", "all|running|non-running"),
                    ("context_action", "string", "optional registry context action filter"),
                    ("quality_action", "string", "optional registry quality action filter"),
                ], &[]),
                "forge.registry.workflow_list.v1",
                &["forge", "list", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.workflow.inspect",
                "Inspect Forge Workflow",
                "Inspect a workflow graph, terminal DAG nodes, subflows and context routes.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "optional focused task id"),
                    ("verbose", "boolean", "include subtasks and validation rules"),
                ], &["workflow_id"]),
                "forge.inspection.v1",
                &["forge", "inspect", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.run.start",
                "Start Async Forge Run",
                "Start an async workflow request, return a run_id quickly and preserve Forge as source of truth.",
                object_schema(&[
                    ("goal", "string", "human objective"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["goal"]),
                "forge.request_start.v1",
                &["forge", "request", "start", "--goal", "<goal>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.resume",
                "Resume Async Forge Run",
                "Mark an async run as resumed and return the latest status and handoff summary.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id"]),
                "forge.request_resume.v1",
                &["forge", "request", "resume", "--run", "<run-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.status",
                "Poll Async Forge Run",
                "Poll async run status, workflow revision, task summary, validation evidence and artifacts later.",
                object_schema(&[("run_id", "string", "run id")], &["run_id"]),
                "forge.request_status.v1",
                &["forge", "request", "status", "--run", "<run-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.request.list",
                "List Async Forge Requests",
                "List all async requests with optional status filter (accepted|resumed|cancelled).",
                object_schema(&[
                    ("status", "string", "optional filter: accepted|resumed|cancelled"),
                ], &[]),
                "forge.request_list.v1",
                &["forge", "request", "list", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.request.cancel",
                "Cancel Async Forge Request",
                "Mark an async request as cancelled and record the event with origin trace.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id"]),
                "forge.request_cancel.v1",
                &["forge", "request", "cancel", "--run", "<run-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.workflow.update_goal",
                "Update Workflow Goal",
                "Mutate the workflow goal through Forge with revision tracking and origin trace.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("goal", "string", "new goal"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "goal", "origin"]),
                "forge.workflow_goal_update.v1",
                &["forge", "workflow", "update-goal", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.workflow.attach_artifact",
                "Attach Workflow Artifact",
                "Attach an artifact through Forge so the path, hash, origin and revision are persisted.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("path", "string", "local artifact path"),
                    ("kind", "string", "artifact kind"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "path", "kind", "origin"]),
                "forge.artifact_attach.v1",
                &["forge", "workflow", "attach-artifact", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.context.request",
                "Request Bounded Context",
                "Build the minimum correct task-local context package before executor handoff.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("budget", "integer", "context byte budget"),
                ], &["workflow_id", "task_id"]),
                "forge.context.v30",
                &["forge", "context", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.validation.status",
                "Query Validation Status",
                "Run the current validation gate projection without promoting unfinished work.",
                object_schema(&[("workflow_id", "string", "workflow id")], &["workflow_id"]),
                "forge.mcp.validation_status.v1",
                &["forge", "validate", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.artifact.fetch",
                "Fetch Workflow Artifact",
                "List or fetch bounded artifact content from Forge-owned artifact refs asynchronously.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("path", "string", "optional artifact path from Forge artifact listing"),
                    ("max_bytes", "integer", "maximum UTF-8 content bytes to return"),
                ], &["workflow_id"]),
                "forge.mcp.artifact_fetch.v1",
                &["forge", "artifacts", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
        ],
    }
}

pub fn call_mcp_tool(store: &ForgeStore, tool_name: &str, input: Value) -> Result<McpCallReport> {
    let result = match tool_name {
        "forge.workflow.list" => {
            let input: WorkflowListInput = parse_input(input)?;
            let filters =
                WorkflowRegistryFilters::new(parse_lifecycle(input.lifecycle.as_deref())?)
                    .with_context_action(clean_optional(input.context_action))
                    .with_quality_action(clean_optional(input.quality_action));
            serde_json::to_value(list_workflows_with_filters(store, filters)?)?
        }
        "forge.workflow.inspect" => {
            let input: WorkflowInspectInput = parse_input(input)?;
            serde_json::to_value(inspect_workflow_with_focus(
                store,
                &input.workflow_id,
                input.verbose.unwrap_or(false),
                input.task_id.as_deref(),
            )?)?
        }
        "forge.run.start" => {
            let input: RunStartInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(start_async_request(store, &input.goal, &origin)?)?
        }
        "forge.run.resume" => {
            let input: RunIdInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(resume_async_request(store, &input.run_id, &origin)?)?
        }
        "forge.run.status" => {
            let input: RunIdInput = parse_input(input)?;
            serde_json::to_value(load_request_status(store, &input.run_id)?)?
        }
        "forge.request.list" => {
            let input: RequestListInput = parse_input(input)?;
            serde_json::to_value(list_requests(store, input.status.as_deref())?)?
        }
        "forge.request.cancel" => {
            let input: RequestCancelInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(cancel_request(store, &input.run_id, &origin)?)?
        }
        "forge.workflow.update_goal" => {
            let input: WorkflowUpdateGoalInput = parse_input(input)?;
            serde_json::to_value(update_workflow_goal(
                store,
                &input.workflow_id,
                &input.goal,
                &input.origin,
            )?)?
        }
        "forge.workflow.attach_artifact" => {
            let input: WorkflowAttachArtifactInput = parse_input(input)?;
            serde_json::to_value(attach_workflow_artifact(
                store,
                &input.workflow_id,
                &PathBuf::from(input.path),
                &input.kind,
                &input.origin,
            )?)?
        }
        "forge.context.request" => {
            let input: ContextRequestInput = parse_input(input)?;
            let workflow = store.load_workflow(&input.workflow_id)?;
            let latest_checkpoint =
                load_latest_task_checkpoint(store, &input.workflow_id, &input.task_id)?;
            serde_json::to_value(build_context_package_with_checkpoint(
                &workflow,
                &input.task_id,
                input.budget.unwrap_or(DEFAULT_CONTEXT_BUDGET),
                latest_checkpoint,
            )?)?
        }
        "forge.validation.status" => {
            let input: WorkflowIdInput = parse_input(input)?;
            let workflow = store.load_workflow(&input.workflow_id)?;
            let workflow_revision = workflow
                .revisions
                .last()
                .map(|revision| revision.revision)
                .unwrap_or(0);
            let validation = validate_workflow(&workflow);
            serde_json::to_value(McpValidationStatusReport {
                schema_version: MCP_VALIDATION_STATUS_SCHEMA_VERSION.to_string(),
                workflow_id: input.workflow_id,
                workflow_revision,
                validation,
            })?
        }
        "forge.artifact.fetch" => {
            let input: ArtifactFetchInput = parse_input(input)?;
            serde_json::to_value(fetch_artifact(store, input)?)?
        }
        other => bail!("unknown MCP tool: {other}"),
    };

    Ok(McpCallReport {
        schema_version: MCP_CALL_SCHEMA_VERSION.to_string(),
        status: "ok".to_string(),
        tool_name: tool_name.to_string(),
        result,
    })
}

fn fetch_artifact(store: &ForgeStore, input: ArtifactFetchInput) -> Result<McpArtifactFetchReport> {
    let _workflow = store.load_workflow(&input.workflow_id)?;
    let artifacts = list_workflow_artifacts(&store.base_dir(), &input.workflow_id)?;
    let max_bytes = input.max_bytes.unwrap_or(0).min(MAX_ARTIFACT_FETCH_BYTES);

    let Some(path) = input.path else {
        return Ok(McpArtifactFetchReport {
            schema_version: MCP_ARTIFACT_FETCH_SCHEMA_VERSION.to_string(),
            workflow_id: input.workflow_id,
            artifacts,
            artifact: None,
            artifact_sha256: None,
            bytes: None,
            max_bytes,
            truncated: false,
            content_sha256: None,
            content_utf8: None,
        });
    };

    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.path == path)
        .cloned()
        .with_context(|| {
            format!(
                "artifact not found in workflow {}: {path}",
                input.workflow_id
            )
        })?;
    let bytes = fs::read(store.base_dir().join(&artifact.path))
        .with_context(|| format!("failed to read artifact {}", artifact.path))?;
    let truncated = max_bytes > 0 && bytes.len() > max_bytes;
    let content_utf8 = if max_bytes == 0 {
        None
    } else {
        let end = if truncated { max_bytes } else { bytes.len() };
        Some(String::from_utf8_lossy(&bytes[..end]).to_string())
    };

    Ok(McpArtifactFetchReport {
        schema_version: MCP_ARTIFACT_FETCH_SCHEMA_VERSION.to_string(),
        workflow_id: input.workflow_id,
        artifacts,
        artifact_sha256: Some(artifact.sha256.clone()),
        bytes: Some(bytes.len() as u64),
        artifact: Some(artifact),
        max_bytes,
        truncated,
        content_sha256: Some(hex_sha256(&bytes)),
        content_utf8,
    })
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    input_schema: Value,
    output_schema: &str,
    forge_command: &[&str],
    flags: ToolFlags,
) -> McpToolSpec {
    McpToolSpec {
        name: name.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        input_schema,
        output_schema: output_schema.to_string(),
        forge_command: forge_command
            .iter()
            .map(|item| (*item).to_string())
            .collect(),
        async_safe: flags.async_safe,
        mutates_workflow: flags.mutates_workflow,
    }
}

#[derive(Debug, Clone, Copy)]
struct ToolFlags {
    async_safe: bool,
    mutates_workflow: bool,
}

impl ToolFlags {
    const fn new(async_safe: bool, mutates_workflow: bool) -> Self {
        Self {
            async_safe,
            mutates_workflow,
        }
    }
}

fn object_schema(properties: &[(&str, &str, &str)], required: &[&str]) -> Value {
    let mut props = serde_json::Map::new();
    for (name, value_type, description) in properties {
        props.insert(
            (*name).to_string(),
            json!({
                "type": value_type,
                "description": description
            }),
        );
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": props,
        "required": required,
    })
}

fn parse_input<T: for<'de> Deserialize<'de>>(input: Value) -> Result<T> {
    serde_json::from_value(input).context("invalid MCP input payload")
}

fn parse_lifecycle(value: Option<&str>) -> Result<WorkflowLifecycleFilter> {
    let normalized = value
        .unwrap_or("all")
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    match normalized.as_str() {
        "" | "all" => Ok(WorkflowLifecycleFilter::All),
        "running" => Ok(WorkflowLifecycleFilter::Running),
        "non-running" => Ok(WorkflowLifecycleFilter::NonRunning),
        other => bail!("unsupported lifecycle filter for MCP workflow list: {other}"),
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
