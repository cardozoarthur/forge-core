use crate::artifact::{hex_sha256, list_workflow_artifacts, ListedArtifact};
use crate::checkpoint::load_latest_task_checkpoint;
use crate::context::{build_context_package_with_checkpoint, DEFAULT_CONTEXT_BUDGET};
use crate::handoff::build_task_handoff;
use crate::inspection::inspect_workflow_with_focus;
use crate::interaction::{
    answer_human_interaction, create_choice_interaction, create_form_interaction,
    expire_human_interaction, list_human_interactions, CreateChoiceInteractionRequest,
};
use crate::ir::{CreativeArtifact, TokenCollection};
use crate::milestone::{
    build_milestone_export_demo, build_milestone_manifest, build_milestone_research,
    build_milestone_status, build_replacement_cli_demo,
};
use crate::multimodal::{
    build_multimodal_benchmark_template, build_multimodal_demo_plan, build_multimodal_install_plan,
    build_multimodal_status, evaluate_multimodal_guard,
};
use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
};
use crate::request::{
    cancel_request, heartbeat_request, list_requests, load_request_status, recover_stale_request,
    resume_async_request, start_async_request,
};
use crate::schedule::{
    aggregate_summary, build_schedule_worker_status, create_daily_goal_research_workflow,
    run_due_workflow, scan_due_workflows, scan_due_workflows_parallel, update_loop_state,
    update_workflow_schedule, ScheduleUpdateOptions,
};
use crate::storage::ForgeStore;
use crate::validation::{validate_workflow, ValidationReport};
use crate::workflow::{
    attach_creative_artifact, attach_workflow_artifact, get_workflow_token_collection,
    inspect_creative_artifact, inspect_creative_collaboration, list_creative_artifacts,
    patch_workflow_token, record_creative_collaboration_event, resolve_workflow_tokens,
    set_workflow_token_collection, update_workflow_goal, CreativeCollaborationEventRequest,
};
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
struct DailyGoalResearchInput {
    goals: Vec<String>,
    timezone: Option<String>,
    cron: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScheduleUpdateInput {
    workflow_id: String,
    task_id: String,
    cron: Option<String>,
    timezone: Option<String>,
    missed_run_policy: Option<String>,
    next_run_at: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoopInspectInput {
    workflow_id: String,
}

#[derive(Debug, Deserialize)]
struct LoopStateInput {
    workflow_id: String,
    task_id: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunDueInput {
    workflow_id: String,
}

#[derive(Debug, Deserialize)]
struct ScanDueInput {
    executor: Option<String>,
    max_workers: Option<usize>,
    ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkerStatusInput {
    executor: Option<String>,
    max_workers: Option<usize>,
    ttl_seconds: Option<u64>,
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
struct RunHeartbeatInput {
    run_id: String,
    executor: Option<String>,
    summary: Option<String>,
    ttl_seconds: Option<u64>,
    pid: Option<u32>,
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
struct InteractionCreateChoiceInput {
    workflow_id: String,
    task_id: String,
    kind: Option<String>,
    prompt: String,
    choices: Vec<String>,
    timeout_seconds: Option<u64>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InteractionCreateFormInput {
    workflow_id: String,
    task_id: String,
    prompt: String,
    fields: Vec<String>,
    timeout_seconds: Option<u64>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InteractionAnswerInput {
    workflow_id: String,
    task_id: String,
    #[serde(default)]
    selected_options: Vec<String>,
    #[serde(default)]
    field_values: Vec<String>,
    rationale: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InteractionExpireInput {
    workflow_id: String,
    task_id: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContextRequestInput {
    workflow_id: String,
    task_id: String,
    budget: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TaskHandoffInput {
    workflow_id: String,
    task_id: String,
    executor: String,
    budget: Option<usize>,
    ttl_seconds: Option<u64>,
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

#[derive(Debug, Deserialize)]
struct MilestoneStatusInput {
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MilestoneCliDemoInput {
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MultimodalStatusInput {
    enable_experimental: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultimodalInstallPlanInput {
    capability: Option<String>,
    capability_id: Option<String>,
    enable_experimental: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultimodalBenchmarkTemplateInput {
    capability: Option<String>,
    capability_id: Option<String>,
    enable_experimental: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultimodalDemoPlanInput {
    demo: Option<String>,
    demo_id: Option<String>,
    enable_experimental: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultimodalGuardInput {
    capability: String,
    action: String,
    enable_experimental: Option<bool>,
    allow: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreativeListInput {
    workflow_id: String,
}

#[derive(Debug, Deserialize)]
struct CreativeInspectInput {
    workflow_id: String,
    artifact_id: String,
}

#[derive(Debug, Deserialize)]
struct CreativeAttachInput {
    workflow_id: String,
    title: String,
    kind: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreativeCollaborationEventInput {
    workflow_id: String,
    artifact_id: String,
    kind: String,
    actor: String,
    summary: String,
    target: Option<String>,
    #[serde(default)]
    selections: Vec<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreativeCollaborationStatusInput {
    workflow_id: String,
    artifact_id: String,
}

#[derive(Debug, Deserialize)]
struct TokensGetInput {
    workflow_id: String,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokensSetInput {
    workflow_id: String,
    name: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokensPatchInput {
    workflow_id: String,
    token_name: String,
    value: String,
    origin: Option<String>,
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
                "forge.schedule.create_daily_goal_research",
                "Create Daily Goal Research Schedule",
                "Create a native Forge scheduled/looping daily Goal research workflow with per-Goal report subflows.",
                object_schema(&[
                    ("goals", "array", "configured Goal names, for example hackathon"),
                    ("timezone", "string", "IANA timezone"),
                    ("cron", "string", "five-field cron expression"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["goals"]),
                "forge.daily_goal_research_plan.v1",
                &["forge", "schedule", "create-daily-goal-research", "--goal", "<goal>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.schedule.update",
                "Update Schedule Node",
                "Mutate a Forge-owned scheduled node with revision tracking.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "scheduled task id"),
                    ("cron", "string", "optional five-field cron expression"),
                    ("timezone", "string", "optional IANA timezone"),
                    ("missed_run_policy", "string", "optional missed-run policy"),
                    ("next_run_at", "string", "optional RFC3339 next due timestamp"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id"]),
                "forge.schedule_update.v1",
                &["forge", "schedule", "update", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.schedule.list",
                "List Scheduled Workflows",
                "List workflows with schedule and loop summaries for async scheduled work visibility.",
                object_schema(&[("lifecycle", "string", "all|running|non-running")], &[]),
                "forge.registry.workflow_list.v1",
                &["forge", "schedule", "list", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.schedule.summary",
                "Summarize Scheduled Workflows",
                "Aggregate cron/schedule state across all Forge-owned workflows for agent runtime visibility.",
                object_schema(&[], &[]),
                "forge.schedule.aggregate_summary.v1",
                &["forge", "schedule", "summary", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.schedule.loop_summary",
                "Summarize Loop Nodes",
                "Aggregate explicit loop node state across all Forge-owned workflows for agent runtime visibility.",
                object_schema(&[], &[]),
                "forge.schedule.aggregate_summary.v1",
                &["forge", "schedule", "loop-summary", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.loop.inspect",
                "Inspect Loop Nodes",
                "Inspect loop primitives and the workflow nodes they trigger.",
                object_schema(&[("workflow_id", "string", "workflow id")], &["workflow_id"]),
                "forge.inspection.v1",
                &["forge", "schedule", "inspect", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.schedule.pause",
                "Pause Loop Node",
                "Pause a loop node in a scheduled workflow. Loop iterations will not advance while paused.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "loop task id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id"]),
                "forge.loop_state_update.v1",
                &["forge", "schedule", "pause", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.schedule.resume",
                "Resume Loop Node",
                "Resume a paused loop node in a scheduled workflow.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "loop task id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id"]),
                "forge.loop_state_update.v1",
                &["forge", "schedule", "resume", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.schedule.stop",
                "Stop Loop Node",
                "Stop a loop node permanently. The loop will not execute again.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "loop task id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id"]),
                "forge.loop_state_update.v1",
                &["forge", "schedule", "stop", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.schedule.run_due",
                "Run Due Schedule",
                "Execute a scheduled workflow that has due cron nodes (next_run_at <= now).",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                ], &["workflow_id"]),
                "forge.schedule_run_due.v1",
                &["forge", "schedule", "run-due", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.schedule.scan_due",
                "Scan Due Schedules",
                "Scan Forge-owned scheduled workflows, lease due schedule nodes locally, run due work and report idle scale-to-zero decisions. Supports bounded parallel dispatch with max_workers and returns WorkerPool evidence when parallel.",
                object_schema(&[
                    ("executor", "string", "scheduler executor id for local leases"),
                    ("max_workers", "integer", "bounded concurrent worker count (1=sequential, >1=parallel WorkerPool dispatch)"),
                    ("ttl_seconds", "integer", "local schedule-task lease TTL"),
                ], &[]),
                "forge.schedule.scan_due.v1",
                &["forge", "schedule", "scan-due", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.schedule.worker_status",
                "Inspect Scheduler Worker Status",
                "Inspect Forge-owned scheduler worker readiness, next wakeup, bounded worker-pool capacity, cancellation safe points and backpressure without executing due work.",
                object_schema(&[
                    ("executor", "string", "scheduler executor id for local leases"),
                    ("max_workers", "integer", "bounded local worker-pool size"),
                    ("ttl_seconds", "integer", "local schedule-task lease TTL"),
                ], &[]),
                "forge.schedule.worker_status.v1",
                &["forge", "schedule", "worker-status", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.schedule.scan_due_parallel",
                "Scan Due Schedules (Parallel)",
                "Scan Forge-owned scheduled workflows with bounded concurrent WorkerPool dispatch. Idle workflows are reconciled into scale-to-zero state, while each due workflow acquires its own lease, runs due work, and releases the lease in a worker thread.",
                object_schema(&[
                    ("executor", "string", "scheduler executor id for local leases"),
                    ("max_workers", "integer", "bounded concurrent worker count"),
                    ("ttl_seconds", "integer", "local schedule-task lease TTL"),
                ], &[]),
                "forge.schedule.scan_due.v1",
                &["forge", "schedule", "scan-due", "--max-workers", "<n>", "--output", "json"],
                ToolFlags::new(true, true),
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
                "forge.run.heartbeat",
                "Heartbeat Async Forge Run",
                "Mark an async run as running, refresh its executor heartbeat TTL and keep active handoffs visible in request status, list and inspect.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("executor", "string", "codex|opencode|skill|mcp|custom executor id"),
                    ("summary", "string", "short progress summary without secrets"),
                    ("ttl_seconds", "integer", "heartbeat freshness TTL"),
                    ("pid", "integer", "optional executor process id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id"]),
                "forge.request_heartbeat.v1",
                &["forge", "request", "heartbeat", "--run", "<run-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.recover_stale",
                "Recover Stale Async Run",
                "Transition a stale running async handoff to needs_attention so humans or executors can resume, cancel or inspect without losing lineage.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id"]),
                "forge.request_stale_recovery.v1",
                &["forge", "request", "recover-stale", "--run", "<run-id>", "--output", "json"],
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
                "forge.interaction.create_choice",
                "Create Human Choice Interaction",
                "Pause a workflow task on a Forge-owned human choice gate that can be answered from CLI, web or agent surfaces.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("kind", "string", "single_choice|multi_choice|ranked_choice|approve_reject_refine_combine|yes_no|risk_acknowledgement"),
                    ("prompt", "string", "human-facing prompt"),
                    ("choices", "array", "choice specs as id=Label|Description|Effect"),
                    ("timeout_seconds", "integer", "optional timeout in seconds"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id", "prompt", "choices"]),
                "forge.human_interaction.v1",
                &["forge", "interaction", "create-choice", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.interaction.create_form",
                "Create Human Form Interaction",
                "Pause a workflow task on a Forge-owned structured form with validation and durable decision state.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("prompt", "string", "human-facing form prompt"),
                    ("fields", "array", "field specs as id:type:required|optional[:default]"),
                    ("timeout_seconds", "integer", "optional timeout in seconds"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id", "prompt", "fields"]),
                "forge.human_interaction.v1",
                &["forge", "interaction", "create-form", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.interaction.answer",
                "Answer Human Interaction",
                "Record a human decision or form answer and resume the blocked workflow task through Forge state.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("selected_options", "array", "choice option ids"),
                    ("field_values", "array", "form values as id=value"),
                    ("rationale", "string", "optional human rationale"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id"]),
                "forge.human_interaction.v1",
                &["forge", "interaction", "answer", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.interaction.expire",
                "Expire Human Interaction",
                "Mark a timed-out human interaction blocked without letting the workflow skip the decision.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id"]),
                "forge.human_interaction.v1",
                &["forge", "interaction", "expire", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.interaction.list",
                "List Human Interactions",
                "List pending, answered and timed-out human interactions across Forge workflows for agent approval bridges.",
                object_schema(&[], &[]),
                "forge.human_interaction.list.v1",
                &["forge", "interaction", "list", "--output", "json"],
                ToolFlags::new(true, false),
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
                "forge.task.handoff",
                "Acquire Task Handoff",
                "Acquire a bounded executor handoff packet for an authorized task executor.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("executor", "string", "selected executor id"),
                    ("budget", "integer", "context byte budget"),
                    ("ttl_seconds", "integer", "lease TTL in seconds"),
                ], &["workflow_id", "task_id", "executor"]),
                "forge.executor_handoff.v8",
                &["forge", "task", "handoff", "--workflow", "<workflow-id>", "--task", "<task-id>", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, true),
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
            tool(
                "forge.milestone.status",
                "Inspect Forge Milestone Status",
                "Inspect the Forge 0.5 milestone boundary, capability statuses and promotion gate.",
                object_schema(&[("version", "string", "milestone version, currently 0.5")], &[]),
                "forge.milestone.status.v1",
                &[
                    "forge",
                    "milestone",
                    "status",
                    "--version",
                    "0.5",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.milestone.manifest",
                "Generate Forge Milestone Manifest",
                "Generate the Forge 0.5 promotion manifest with requirements, completed and missing capabilities, validation evidence, demos, gaps and decision.",
                object_schema(&[("version", "string", "milestone version, currently 0.5")], &[]),
                "forge.milestone.manifest.v1",
                &[
                    "forge",
                    "milestone",
                    "manifest",
                    "--version",
                    "0.5",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.milestone.research",
                "Inspect Forge Milestone Research",
                "Inspect the source-grounded Forge 0.5 creative-runtime research baseline, validation gates and workflow templates.",
                object_schema(&[("version", "string", "milestone version, currently 0.5")], &[]),
                "forge.milestone.research.v1",
                &[
                    "forge",
                    "milestone",
                    "research",
                    "--version",
                    "0.5",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.milestone.export_demo",
                "Generate Milestone Export Demo",
                "Generate a self-contained export/demo workflow with screen and document creative artifacts, design token collection, and full lineage evidence for the Forge 0.5 export/demo baseline.",
                object_schema(&[], &[]),
                "forge.milestone.export_demo.v1",
                &[
                    "forge",
                    "milestone",
                    "export-demo",
                    "--origin",
                    "mcp",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.milestone.cli_demo",
                "Generate Replacement CLI Demo",
                "Generate deterministic Forge-first replacement-grade CLI demo evidence for coding, research/artifact and long-running async workflows without mutating external resources.",
                object_schema(&[("origin", "string", "codex|opencode|skill|mcp")], &[]),
                "forge.milestone.cli_demo.v1",
                &[
                    "forge",
                    "milestone",
                    "cli-demo",
                    "--origin",
                    "mcp",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.multimodal.status",
                "Inspect Experimental Multimodal Status",
                "List Forge-owned experimental multimodal capabilities, missing model/runtime gaps, disabled-by-default feature flag state and runtime guard requirements without accessing devices or installing models.",
                object_schema(&[
                    ("enable_experimental", "boolean", "optional explicit experimental flag for planning output only"),
                ], &[]),
                "forge.multimodal.status.v1",
                &[
                    "forge",
                    "multimodal",
                    "status",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.multimodal.install_plan",
                "Generate Multimodal Install Plan",
                "Generate a plan-only install and benchmark manifest for one multimodal capability. This tool never downloads models or mutates local devices.",
                object_schema(&[
                    ("capability_id", "string", "capability id from forge.multimodal.status"),
                    ("enable_experimental", "boolean", "optional explicit experimental flag for planning output only"),
                ], &["capability_id"]),
                "forge.multimodal.install_plan.v1",
                &[
                    "forge",
                    "multimodal",
                    "install-plan",
                    "--capability",
                    "<capability-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.multimodal.benchmark_template",
                "Generate Multimodal Benchmark Template",
                "Generate a plan-only benchmark/report template for one multimodal capability. This tool performs no installs, model execution, device access or automation.",
                object_schema(&[
                    ("capability_id", "string", "capability id from forge.multimodal.status"),
                    ("enable_experimental", "boolean", "optional explicit experimental flag for planning output only"),
                ], &["capability_id"]),
                "forge.multimodal.benchmark_template.v1",
                &[
                    "forge",
                    "multimodal",
                    "benchmark-template",
                    "--capability",
                    "<capability-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.multimodal.demo_plan",
                "Generate Multimodal Demo Plan",
                "Generate a guarded demo plan for local image recognition, audio transcription/synthesis or Blender/avatar preparation. This tool performs no installs, model execution, device access or automation.",
                object_schema(&[
                    ("demo_id", "string", "local_image_recognition|audio_transcription_synthesis|blender_avatar_preparation"),
                    ("enable_experimental", "boolean", "optional explicit experimental flag for planning output only"),
                ], &["demo_id"]),
                "forge.multimodal.demo_plan.v1",
                &[
                    "forge",
                    "multimodal",
                    "demo-plan",
                    "--demo",
                    "<demo-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.multimodal.guard",
                "Evaluate Multimodal Runtime Guard",
                "Evaluate whether a camera, microphone, screen, input, peripheral, model or filesystem multimodal action is allowed under Forge's experimental opt-in policy.",
                object_schema(&[
                    ("capability", "string", "capability id or permission scope"),
                    ("action", "string", "requested action such as access, capture, transcribe or automate"),
                    ("enable_experimental", "boolean", "experimental feature flag"),
                    ("allow", "boolean", "explicit human/runtime allow for this action"),
                ], &["capability", "action"]),
                "forge.multimodal.guard.v1",
                &[
                    "forge",
                    "multimodal",
                    "guard",
                    "--capability",
                    "<capability-or-scope>",
                    "--action",
                    "<action>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.creative.list",
                "List Creative Artifacts",
                "List creative artifacts (screens, whiteboards, documents, slide decks, components) attached to a workflow.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                ], &["workflow_id"]),
                "forge.creative.list.v1",
                &["forge", "workflow", "list-creative", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.creative.inspect",
                "Inspect Creative Artifact",
                "Inspect a specific creative artifact with full spec content.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("artifact_id", "string", "creative artifact id"),
                ], &["workflow_id", "artifact_id"]),
                "forge.creative.inspect.v1",
                &["forge", "workflow", "inspect-creative", "--workflow", "<workflow-id>", "--artifact", "<artifact-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.creative.attach",
                "Attach Creative Artifact",
                "Attach a new creative artifact (screen, whiteboard, document, slide_deck, component) to a workflow.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("title", "string", "artifact title"),
                    ("kind", "string", "screen|whiteboard|document|slide_deck|component"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "title", "kind"]),
                "forge.creative.attach.v1",
                &["forge", "workflow", "attach-creative", "--workflow", "<workflow-id>", "--title", "<title>", "--kind", "<kind>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.creative.collaboration_event",
                "Record Creative Collaboration Event",
                "Record presence, comment, patch, conflict or rollback state on a creative artifact with workflow revision and audit history.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("artifact_id", "string", "creative artifact id"),
                    ("kind", "string", "presence|comment|patch|conflict|rollback"),
                    ("actor", "string", "human or AI actor id"),
                    ("summary", "string", "event body, patch instruction or rollback reason"),
                    ("target", "string", "cursor, selected object, path or rollback event id"),
                    ("selections", "array", "optional selected object ids"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "artifact_id", "kind", "actor", "summary"]),
                "forge.creative_collaboration.event.v1",
                &["forge", "workflow", "collaboration-event", "--workflow", "<workflow-id>", "--artifact", "<artifact-id>", "--kind", "<kind>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.creative.collaboration_status",
                "Inspect Creative Collaboration Status",
                "Inspect presence, comments, patch stream, conflicts, rollbacks and audit history for a creative artifact.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("artifact_id", "string", "creative artifact id"),
                ], &["workflow_id", "artifact_id"]),
                "forge.creative_collaboration.status.v1",
                &["forge", "workflow", "collaboration-status", "--workflow", "<workflow-id>", "--artifact", "<artifact-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.tokens.get",
                "Get Design Tokens",
                "Get the design token collection (colors, typography, spacing, etc.) attached to a workflow.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                ], &["workflow_id"]),
                "forge.tokens.get.v1",
                &["forge", "workflow", "get-tokens", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.tokens.resolve",
                "Resolve Design Tokens",
                "Resolve raw tokens, semantic aliases and optional mode overrides, then return impact references across creative artifacts.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("mode", "string", "optional token mode, for example dark"),
                ], &["workflow_id"]),
                "forge.tokens.resolve.v1",
                &["forge", "workflow", "resolve-tokens", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.tokens.set",
                "Set Design Tokens",
                "Set or replace the design token collection on a workflow with a minimal token set.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("name", "string", "token collection name"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "name"]),
                "forge.tokens.set.v1",
                &["forge", "workflow", "set-tokens", "--workflow", "<workflow-id>", "--name", "<name>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.tokens.patch",
                "Patch Design Token",
                "Apply a targeted patch-by-intent to a single design token while preserving creative artifact content and token references.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("token_name", "string", "token name to patch"),
                    ("value", "string", "new token value"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "token_name", "value"]),
                "forge.tokens.patch.v1",
                &["forge", "workflow", "patch-token", "--workflow", "<workflow-id>", "--token", "<token-name>", "--value", "<value>", "--output", "json"],
                ToolFlags::new(true, true),
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
        "forge.schedule.create_daily_goal_research" => {
            let input: DailyGoalResearchInput = parse_input(input)?;
            let timezone = input
                .timezone
                .unwrap_or_else(|| "America/Sao_Paulo".to_string());
            let cron = input.cron.unwrap_or_else(|| "0 8 * * *".to_string());
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(create_daily_goal_research_workflow(
                store,
                input.goals,
                &timezone,
                &cron,
                &origin,
            )?)?
        }
        "forge.schedule.update" => {
            let input: ScheduleUpdateInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(update_workflow_schedule(
                store,
                &input.workflow_id,
                &input.task_id,
                ScheduleUpdateOptions {
                    cron: input.cron.as_deref(),
                    timezone: input.timezone.as_deref(),
                    missed_run_policy: input.missed_run_policy.as_deref(),
                    next_run_at: input.next_run_at.as_deref(),
                    origin: &origin,
                },
            )?)?
        }
        "forge.schedule.pause" => {
            let input: LoopStateInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(update_loop_state(
                store,
                &input.workflow_id,
                &input.task_id,
                "paused",
                &origin,
            )?)?
        }
        "forge.schedule.resume" => {
            let input: LoopStateInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(update_loop_state(
                store,
                &input.workflow_id,
                &input.task_id,
                "active",
                &origin,
            )?)?
        }
        "forge.schedule.stop" => {
            let input: LoopStateInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(update_loop_state(
                store,
                &input.workflow_id,
                &input.task_id,
                "stopped",
                &origin,
            )?)?
        }
        "forge.schedule.run_due" => {
            let input: RunDueInput = parse_input(input)?;
            serde_json::to_value(run_due_workflow(store, &input.workflow_id)?)?
        }
        "forge.schedule.scan_due" => {
            let input: ScanDueInput = parse_input(input)?;
            let executor = input
                .executor
                .unwrap_or_else(|| "mcp-scheduler".to_string());
            let max_workers = input.max_workers.unwrap_or(1);
            let ttl_seconds = input.ttl_seconds.unwrap_or(300);
            serde_json::to_value(if max_workers > 1 {
                scan_due_workflows_parallel(store, &executor, max_workers, ttl_seconds)?
            } else {
                scan_due_workflows(store, &executor, ttl_seconds)?
            })?
        }
        "forge.schedule.worker_status" => {
            let input: WorkerStatusInput = parse_input(input)?;
            let executor = input
                .executor
                .unwrap_or_else(|| "mcp-scheduler".to_string());
            let max_workers = input.max_workers.unwrap_or(1);
            let ttl_seconds = input.ttl_seconds.unwrap_or(300);
            serde_json::to_value(build_schedule_worker_status(
                store,
                &executor,
                max_workers,
                ttl_seconds,
            )?)?
        }
        "forge.schedule.list" => {
            let input: WorkflowListInput = parse_input(input)?;
            let filters =
                WorkflowRegistryFilters::new(parse_lifecycle(input.lifecycle.as_deref())?)
                    .with_context_action(clean_optional(input.context_action))
                    .with_quality_action(clean_optional(input.quality_action))
                    .only_scheduled_or_looping();
            serde_json::to_value(list_workflows_with_filters(store, filters)?)?
        }
        "forge.schedule.summary" | "forge.schedule.loop_summary" => {
            let workflows = store.load_workflows()?;
            let task_slices: Vec<&[crate::graph::AtomicTask]> =
                workflows.iter().map(|wf| wf.tasks.as_slice()).collect();
            serde_json::to_value(aggregate_summary(&task_slices))?
        }
        "forge.loop.inspect" => {
            let input: LoopInspectInput = parse_input(input)?;
            serde_json::to_value(inspect_workflow_with_focus(
                store,
                &input.workflow_id,
                true,
                None,
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
        "forge.run.heartbeat" => {
            let input: RunHeartbeatInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(heartbeat_request(
                store,
                &input.run_id,
                input.executor.as_deref().unwrap_or("mcp"),
                input.summary.as_deref().unwrap_or("executor heartbeat"),
                input.ttl_seconds.unwrap_or(300),
                input.pid,
                &origin,
            )?)?
        }
        "forge.run.recover_stale" => {
            let input: RunIdInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(recover_stale_request(store, &input.run_id, &origin)?)?
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
        "forge.interaction.create_choice" => {
            let input: InteractionCreateChoiceInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let kind = input.kind.unwrap_or_else(|| "single_choice".to_string());
            serde_json::to_value(create_choice_interaction(
                store,
                CreateChoiceInteractionRequest {
                    workflow_id: &input.workflow_id,
                    task_id: &input.task_id,
                    kind: &kind,
                    prompt: &input.prompt,
                    choices: &input.choices,
                    timeout_seconds: input.timeout_seconds,
                    origin: &origin,
                },
            )?)?
        }
        "forge.interaction.create_form" => {
            let input: InteractionCreateFormInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(create_form_interaction(
                store,
                &input.workflow_id,
                &input.task_id,
                &input.prompt,
                &input.fields,
                input.timeout_seconds,
                &origin,
            )?)?
        }
        "forge.interaction.answer" => {
            let input: InteractionAnswerInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(answer_human_interaction(
                store,
                &input.workflow_id,
                &input.task_id,
                &input.selected_options,
                &input.field_values,
                input.rationale.as_deref(),
                &origin,
            )?)?
        }
        "forge.interaction.expire" => {
            let input: InteractionExpireInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(expire_human_interaction(
                store,
                &input.workflow_id,
                &input.task_id,
                &origin,
            )?)?
        }
        "forge.interaction.list" => serde_json::to_value(list_human_interactions(store)?)?,
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
        "forge.task.handoff" => {
            let input: TaskHandoffInput = parse_input(input)?;
            serde_json::to_value(build_task_handoff(
                store,
                &input.workflow_id,
                &input.task_id,
                &input.executor,
                input.budget.unwrap_or(DEFAULT_CONTEXT_BUDGET),
                input.ttl_seconds.unwrap_or(900),
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
        "forge.milestone.status" => {
            let input: MilestoneStatusInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            serde_json::to_value(build_milestone_status(&version)?)?
        }
        "forge.milestone.manifest" => {
            let input: MilestoneStatusInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            serde_json::to_value(build_milestone_manifest(&version)?)?
        }
        "forge.milestone.research" => {
            let input: MilestoneStatusInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            serde_json::to_value(build_milestone_research(&version)?)?
        }
        "forge.milestone.export_demo" => {
            serde_json::to_value(build_milestone_export_demo(store, "mcp")?)?
        }
        "forge.milestone.cli_demo" => {
            let input: MilestoneCliDemoInput = parse_input(input)?;
            serde_json::to_value(build_replacement_cli_demo(
                store,
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.multimodal.status" => {
            let input: MultimodalStatusInput = parse_input(input)?;
            serde_json::to_value(build_multimodal_status(
                input.enable_experimental.unwrap_or(false),
            ))?
        }
        "forge.multimodal.install_plan" => {
            let input: MultimodalInstallPlanInput = parse_input(input)?;
            let capability = input
                .capability_id
                .or(input.capability)
                .ok_or_else(|| anyhow::anyhow!("capability_id is required"))?;
            serde_json::to_value(build_multimodal_install_plan(
                &capability,
                input.enable_experimental.unwrap_or(false),
            )?)?
        }
        "forge.multimodal.benchmark_template" => {
            let input: MultimodalBenchmarkTemplateInput = parse_input(input)?;
            let capability = input
                .capability_id
                .or(input.capability)
                .ok_or_else(|| anyhow::anyhow!("capability_id is required"))?;
            serde_json::to_value(build_multimodal_benchmark_template(
                &capability,
                input.enable_experimental.unwrap_or(false),
            )?)?
        }
        "forge.multimodal.demo_plan" => {
            let input: MultimodalDemoPlanInput = parse_input(input)?;
            let demo = input
                .demo_id
                .or(input.demo)
                .ok_or_else(|| anyhow::anyhow!("demo_id is required"))?;
            serde_json::to_value(build_multimodal_demo_plan(
                &demo,
                input.enable_experimental.unwrap_or(false),
            )?)?
        }
        "forge.multimodal.guard" => {
            let input: MultimodalGuardInput = parse_input(input)?;
            serde_json::to_value(evaluate_multimodal_guard(
                &input.capability,
                &input.action,
                input.enable_experimental.unwrap_or(false),
                input.allow.unwrap_or(false),
            )?)?
        }
        "forge.creative.list" => {
            let input: CreativeListInput = parse_input(input)?;
            serde_json::to_value(list_creative_artifacts(store, &input.workflow_id)?)?
        }
        "forge.creative.inspect" => {
            let input: CreativeInspectInput = parse_input(input)?;
            serde_json::to_value(inspect_creative_artifact(
                store,
                &input.workflow_id,
                &input.artifact_id,
            )?)?
        }
        "forge.creative.attach" => {
            let input: CreativeAttachInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let artifact = build_creative_artifact(&input.title, &input.kind, &origin)?;
            serde_json::to_value(attach_creative_artifact(
                store,
                &input.workflow_id,
                artifact,
                &origin,
            )?)?
        }
        "forge.creative.collaboration_event" => {
            let input: CreativeCollaborationEventInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(record_creative_collaboration_event(
                store,
                CreativeCollaborationEventRequest {
                    workflow_id: input.workflow_id,
                    artifact_id: input.artifact_id,
                    event_kind: input.kind,
                    actor: input.actor,
                    summary: input.summary,
                    target: input.target.unwrap_or_default(),
                    selections: input.selections,
                    origin,
                },
            )?)?
        }
        "forge.creative.collaboration_status" => {
            let input: CreativeCollaborationStatusInput = parse_input(input)?;
            serde_json::to_value(inspect_creative_collaboration(
                store,
                &input.workflow_id,
                &input.artifact_id,
            )?)?
        }
        "forge.tokens.get" => {
            let input: TokensGetInput = parse_input(input)?;
            serde_json::to_value(get_workflow_token_collection(store, &input.workflow_id)?)?
        }
        "forge.tokens.resolve" => {
            let input: TokensGetInput = parse_input(input)?;
            serde_json::to_value(resolve_workflow_tokens(
                store,
                &input.workflow_id,
                input.mode.as_deref(),
            )?)?
        }
        "forge.tokens.set" => {
            let input: TokensSetInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(set_workflow_token_collection(
                store,
                &input.workflow_id,
                make_minimal_token_collection(&input.name),
                &origin,
            )?)?
        }
        "forge.tokens.patch" => {
            let input: TokensPatchInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(patch_workflow_token(
                store,
                &input.workflow_id,
                &input.token_name,
                &input.value,
                &origin,
            )?)?
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

fn build_creative_artifact(title: &str, kind: &str, origin: &str) -> Result<CreativeArtifact> {
    match kind {
        "screen" => Ok(CreativeArtifact::new_screen(
            title,
            crate::ir::ScreenSpec {
                schema_version: crate::ir::ir_schema_version(),
                width_px: 1440,
                height_px: 900,
                background: "#ffffff".to_string(),
                breakpoints: Vec::new(),
                elements: Vec::new(),
                interactions: Vec::new(),
            },
        )),
        "whiteboard" => Ok(CreativeArtifact::new_whiteboard(
            title,
            crate::ir::WhiteboardSpec {
                schema_version: crate::ir::ir_schema_version(),
                width_px: 1920,
                height_px: 1080,
                background: "#ffffff".to_string(),
                layers: Vec::new(),
                sticky_notes: Vec::new(),
                drawings: Vec::new(),
                text_blocks: Vec::new(),
                images: Vec::new(),
            },
        )),
        "document" => Ok(CreativeArtifact::new_document(
            title,
            crate::ir::DocumentSpec {
                schema_version: crate::ir::ir_schema_version(),
                title: title.to_string(),
                author: origin.to_string(),
                front_matter: std::collections::BTreeMap::new(),
                sections: Vec::new(),
            },
        )),
        "slide_deck" => Ok(CreativeArtifact::new_slide_deck(
            title,
            crate::ir::SlideDeckSpec {
                schema_version: crate::ir::ir_schema_version(),
                title: title.to_string(),
                theme: "default".to_string(),
                slides: Vec::new(),
            },
        )),
        "component" => Ok(CreativeArtifact::new_component(
            title,
            crate::ir::ComponentSpec {
                schema_version: crate::ir::ir_schema_version(),
                name: title.to_string(),
                description: String::new(),
                props: Vec::new(),
                variants: Vec::new(),
                states: Vec::new(),
                slots: Vec::new(),
                token_dependencies: Vec::new(),
                code_template: None,
            },
        )),
        other => bail!("unsupported creative artifact kind: {other}. Valid kinds: screen, whiteboard, document, slide_deck, component"),
    }
}

fn make_minimal_token_collection(name: &str) -> TokenCollection {
    TokenCollection {
        name: name.to_string(),
        schema_version: crate::ir::ir_schema_version(),
        description: format!("Design tokens for {name}"),
        tokens: vec![
            crate::ir::DesignToken {
                name: "color.primary".to_string(),
                value: "#3B82F6".to_string(),
                token_type: crate::ir::TokenType::Color,
                description: "Primary brand color".to_string(),
                group: "color".to_string(),
                extensions: std::collections::BTreeMap::new(),
            },
            crate::ir::DesignToken {
                name: "spacing.md".to_string(),
                value: "16px".to_string(),
                token_type: crate::ir::TokenType::Spacing,
                description: "Medium spacing".to_string(),
                group: "spacing".to_string(),
                extensions: std::collections::BTreeMap::new(),
            },
        ],
        semantic_aliases: vec![crate::ir::SemanticAlias {
            name: format!("semantic.{name}"),
            resolves_to: "color.primary".to_string(),
            description: format!("Semantic alias for {name}"),
        }],
        modes: Vec::new(),
    }
}
