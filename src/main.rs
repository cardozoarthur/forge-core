use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use forge_core::adapter::validate_executor_response_file;
use forge_core::artifact::list_workflow_artifacts;
use forge_core::checkpoint::{
    load_latest_task_checkpoint, record_task_checkpoint, TaskCheckpointRequest,
};
use forge_core::cluster::{
    build_cluster_task_handoff, list_cluster_node_leases, list_cluster_nodes,
    place_task_on_cluster, register_cluster_node, ClusterNodeInput,
};
use forge_core::context::build_context_package_with_checkpoint;
use forge_core::execution::run_simulated;
use forge_core::executor::{load_executors, sync_executors, ExecutorSyncOptions};
use forge_core::graph::create_workflow;
use forge_core::handoff::build_task_handoff;
use forge_core::improve::generate_improvement;
use forge_core::inspection::inspect_workflow_with_focus;
use forge_core::intent::parse_intent;
use forge_core::interaction::{
    answer_human_interaction, create_choice_interaction, create_form_interaction,
    expire_human_interaction, list_human_interactions, summarize_human_interactions,
    CreateChoiceInteractionRequest,
};
use forge_core::interactive::{
    build_interactive_home, render_interactive_home, route_interactive_input, slash_command_catalog,
};
use forge_core::ir::{CreativeArtifact, TokenCollection};
use forge_core::lease::{acquire_task_lease, release_task_lease};
use forge_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use forge_core::milestone::{
    build_milestone_manifest, build_milestone_research, build_milestone_status,
};
use forge_core::registry::{
    attach_reuse_candidates_as_child_subflows, context_action_catalog, find_reuse_candidates,
    list_workflows_with_filters, quality_action_catalog, WorkflowLifecycleFilter,
    WorkflowRegistryFilters,
};
use forge_core::request::{
    cancel_request, list_requests, load_request_status, resume_async_request, start_async_request,
};
use forge_core::runtime::{
    guard_runtime_scope, load_runtimes, sync_runtimes, RuntimeGuardRequest, RuntimeSyncOptions,
};
use forge_core::schedule::{
    aggregate_summary, build_schedule_worker_status, create_daily_goal_research_workflow,
    run_daily_goal_research_smoke, run_due_workflow, scan_due_workflows,
    scan_due_workflows_parallel, update_loop_state, update_workflow_schedule,
    ScheduleUpdateOptions,
};
use forge_core::self_evolve::{run_self_evolution, SelfRunOptions};
use forge_core::skill::install_skill;
use forge_core::storage::ForgeStore;
use forge_core::validation::validate_workflow;
use forge_core::workflow::{
    attach_creative_artifact, attach_workflow_artifact, get_workflow_token_collection,
    inspect_creative_artifact, inspect_creative_collaboration, list_creative_artifacts,
    patch_workflow_token, record_creative_collaboration_event, resolve_workflow_tokens,
    set_workflow_token_collection, update_workflow_goal, validate_child_subflow_binding,
    CreativeCollaborationEventRequest,
};
use serde::Serialize;
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "forge", version, about = "Forge Core workflow runtime")]
struct Cli {
    #[arg(long, default_value = ".forge/forge.sqlite")]
    store: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Plan {
        #[arg(long)]
        goal: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long, value_enum, default_value_t = WorkflowLifecycleArg::All)]
        lifecycle: WorkflowLifecycleArg,
        #[arg(long = "context-action")]
        context_action: Option<String>,
        #[arg(long = "context-actions")]
        context_actions: bool,
        #[arg(long = "quality-action")]
        quality_action: Option<String>,
        #[arg(long = "quality-actions")]
        quality_actions: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inspect {
        workflow: String,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        verbose: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Status {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Context {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value_t = 1200)]
        budget: usize,
        #[arg(long)]
        strict: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Run {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        simulate: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Validate {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Improve {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        target_version: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Artifacts {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },
    Sync {
        #[command(subcommand)]
        command: SyncCommands,
    },
    Executors {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Runtimes {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommands,
    },
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommands,
    },
    Cluster {
        #[command(subcommand)]
        command: ClusterCommands,
    },
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommands,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommands,
    },
    Request {
        #[command(subcommand)]
        command: RequestCommands,
    },
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
    Interactive {
        #[command(subcommand)]
        command: InteractiveCommands,
    },
    Interaction {
        #[command(subcommand)]
        command: InteractionCommands,
    },
    Milestone {
        #[command(subcommand)]
        command: MilestoneCommands,
    },
    #[command(name = "self")]
    SelfRun {
        #[command(subcommand)]
        command: SelfCommands,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    Install {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long = "runtime-path")]
        runtime_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SyncCommands {
    Executors {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Runtimes {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long = "runtime-path")]
        runtime_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    All {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long = "runtime-path")]
        runtime_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum RuntimeCommands {
    Guard {
        #[arg(long)]
        substrate: String,
        #[arg(long)]
        resource: String,
        #[arg(long)]
        namespace: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        allow_external: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum ScheduleCommands {
    CreateDailyGoalResearch {
        #[arg(long = "goal")]
        goals: Vec<String>,
        #[arg(long, default_value = "America/Sao_Paulo")]
        timezone: String,
        #[arg(long, default_value = "0 8 * * *")]
        cron: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inspect {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Update {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        cron: Option<String>,
        #[arg(long)]
        timezone: Option<String>,
        #[arg(long = "missed-run-policy")]
        missed_run_policy: Option<String>,
        #[arg(long = "next-run-at")]
        next_run_at: Option<String>,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Pause {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Resume {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Stop {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RunDue {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ScanDue {
        #[arg(long, default_value = "forge-scheduler")]
        executor: String,
        #[arg(long = "max-workers", default_value_t = 1)]
        max_workers: usize,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Summary {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    LoopSummary {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    WorkerStatus {
        #[arg(long, default_value = "forge-scheduler")]
        executor: String,
        #[arg(long = "max-workers", default_value_t = 1)]
        max_workers: usize,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
enum ClusterCommands {
    Register {
        #[arg(long = "node-id")]
        node_id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long = "os")]
        os: String,
        #[arg(long)]
        arch: String,
        #[arg(long = "cpu-cores")]
        cpu_cores: u16,
        #[arg(long = "memory-gb")]
        memory_gb: u32,
        #[arg(long = "gpu")]
        gpus: Vec<String>,
        #[arg(long = "software")]
        installed_software: Vec<String>,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
        #[arg(long = "python")]
        python_available: bool,
        #[arg(long = "node")]
        node_available: bool,
        #[arg(long = "docker")]
        docker_available: bool,
        #[arg(long = "gpu-available")]
        gpu_available: bool,
        #[arg(long = "network-reachable")]
        network_reachable: bool,
        #[arg(long)]
        status: String,
        #[arg(long = "trust")]
        trust_level: String,
        #[arg(long = "sandbox")]
        sandbox_permissions: Vec<String>,
        #[arg(long = "cost-per-hour-usd", default_value_t = 0.0)]
        cost_per_hour_usd: f64,
        #[arg(long = "latency-ms", default_value_t = 0)]
        latency_ms: u32,
        #[arg(long, default_value_t = 1.0)]
        reliability: f64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Leases {
        #[arg(long = "node-id")]
        node_id: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Place {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Handoff {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value_t = 1200)]
        budget: usize,
        #[arg(long, default_value_t = 900)]
        ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum WorkflowCommands {
    UpdateGoal {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        goal: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AttachArtifact {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ValidateSubflow {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "child-workflow")]
        child_workflow: String,
        #[arg(long = "child-task")]
        child_task: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AttachCreative {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ListCreative {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    InspectCreative {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        artifact: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CollaborationEvent {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        artifact: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        summary: String,
        #[arg(long, default_value = "")]
        target: String,
        #[arg(long = "selection")]
        selections: Vec<String>,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CollaborationStatus {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        artifact: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    SetTokens {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    GetTokens {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ResolveTokens {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    PatchToken {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        value: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum TaskCommands {
    Handoff {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        executor: String,
        #[arg(long, default_value_t = 1200)]
        budget: usize,
        #[arg(long, default_value_t = 900)]
        ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Acquire {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        executor: String,
        #[arg(long, default_value_t = 900)]
        ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Release {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        lease: String,
        #[arg(long)]
        executor: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Checkpoint {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        executor: String,
        #[arg(long)]
        state: String,
        #[arg(long)]
        summary: String,
        #[arg(long = "context-sha256")]
        context_sha256: String,
        #[arg(long = "context-routing-cache-key")]
        context_routing_cache_key: Option<String>,
        #[arg(long = "workflow-revision")]
        workflow_revision: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ValidateResponse {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        response: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum RequestCommands {
    Start {
        #[arg(long)]
        goal: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Status {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Cancel {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Resume {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum McpCommands {
    Tools {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Call {
        tool: String,
        #[arg(long)]
        input: Option<String>,
        #[arg(long = "input-file")]
        input_file: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum InteractiveCommands {
    Home {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    SlashCommands {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Route {
        #[arg(long)]
        input: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum InteractionCommands {
    CreateChoice {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "single_choice")]
        kind: String,
        #[arg(long)]
        prompt: String,
        #[arg(long = "choice")]
        choices: Vec<String>,
        #[arg(long = "timeout-seconds")]
        timeout_seconds: Option<u64>,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CreateForm {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        prompt: String,
        #[arg(long = "field")]
        fields: Vec<String>,
        #[arg(long = "timeout-seconds")]
        timeout_seconds: Option<u64>,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Answer {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "selected")]
        selected_options: Vec<String>,
        #[arg(long = "field")]
        field_values: Vec<String>,
        #[arg(long)]
        rationale: Option<String>,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Expire {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum MilestoneCommands {
    Status {
        #[arg(long, default_value = "0.5")]
        version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Manifest {
        #[arg(long, default_value = "0.5")]
        version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Research {
        #[arg(long, default_value = "0.5")]
        version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SelfCommands {
    Run {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        until: String,
        #[arg(long, default_value_t = 1)]
        max_cycles: u32,
        #[arg(long, default_value_t = 1800)]
        sleep_seconds: u64,
        #[arg(long = "executor")]
        executors: Vec<String>,
        #[arg(long, default_value = "balanced")]
        mode: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        push: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum WorkflowLifecycleArg {
    All,
    Running,
    NonRunning,
}

impl From<WorkflowLifecycleArg> for WorkflowLifecycleFilter {
    fn from(value: WorkflowLifecycleArg) -> Self {
        match value {
            WorkflowLifecycleArg::All => WorkflowLifecycleFilter::All,
            WorkflowLifecycleArg::Running => WorkflowLifecycleFilter::Running,
            WorkflowLifecycleArg::NonRunning => WorkflowLifecycleFilter::NonRunning,
        }
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error:?}");
            std::process::exit(1);
        }
    }
}

fn show_dashboard(store_path: PathBuf) -> Result<i32> {
    if !std::io::stdin().is_terminal() {
        println!("Forge Core workflow runtime -- use `forge --help` for available commands");
        return Ok(0);
    }

    let store = ForgeStore::open(store_path)?;
    let report = build_interactive_home(&store)?;
    println!("{}", render_interactive_home(&report));

    Ok(0)
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        return show_dashboard(cli.store);
    };
    match command {
        Commands::Plan { goal, output } => {
            let store = ForgeStore::open(cli.store)?;
            let intent = parse_intent(&goal);
            let mut workflow = create_workflow(intent);
            let reuse_candidates = find_reuse_candidates(&store, &workflow)?;
            let attached_subflows =
                attach_reuse_candidates_as_child_subflows(&mut workflow, &reuse_candidates);
            store.save_workflow(&workflow)?;
            store.record_event(
                &workflow.id,
                "workflow_planned",
                &serde_json::to_value(&workflow)?,
            )?;
            let response = serde_json::json!({
                "status": "planned",
                "workflow_id": workflow.id,
                "goal": workflow.goal,
                "tasks": workflow.tasks,
                "intent": workflow.intent,
                "reuse_candidates": reuse_candidates,
                "attached_subflows": attached_subflows,
            });
            print_response(output, &response)?;
            Ok(0)
        }
        Commands::List {
            lifecycle,
            context_action,
            context_actions,
            quality_action,
            quality_actions,
            output,
        } => {
            if context_actions {
                let catalog = context_action_catalog();
                print_response(output, &catalog)?;
                return Ok(0);
            }

            if quality_actions {
                let catalog = quality_action_catalog();
                print_response(output, &catalog)?;
                return Ok(0);
            }

            let store = ForgeStore::open(cli.store)?;
            let quality_action = quality_action
                .map(|action| action.trim().to_string())
                .filter(|action| !action.is_empty());
            let context_action = context_action
                .map(|action| action.trim().to_string())
                .filter(|action| !action.is_empty());
            let filters = WorkflowRegistryFilters::new(lifecycle.into())
                .with_context_action(context_action)
                .with_quality_action(quality_action);
            let report = list_workflows_with_filters(&store, filters)?;
            print_response(output, &report)?;
            Ok(0)
        }
        Commands::Inspect {
            workflow,
            task,
            verbose,
            output,
        } => {
            let store = ForgeStore::open(cli.store)?;
            let report = inspect_workflow_with_focus(&store, &workflow, verbose, task.as_deref())?;
            match output {
                OutputFormat::Json => print_response(output, &report)?,
                OutputFormat::Human => println!("{}", report.diagram),
            }
            Ok(0)
        }
        Commands::Status { workflow, output } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let creative_summaries: Vec<serde_json::Value> = workflow
                .creative_artifacts
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "id": a.id,
                        "title": a.title,
                        "kind": format!("{:?}", a.kind),
                        "created_at": a.created_at,
                        "collaboration_summary": a.collaboration.summary(),
                    })
                })
                .collect();
            let token_summary = workflow.token_collection.as_ref().map(|tokens| {
                serde_json::json!({
                    "schema_version": "forge.tokens.workflow_summary.v1",
                    "collection_name": tokens.name,
                    "token_count": tokens.tokens.len(),
                    "semantic_alias_count": tokens.semantic_aliases.len(),
                    "mode_count": tokens.modes.len(),
                    "resolution_schema_version": "forge.tokens.resolution.v1",
                })
            });
            let response = serde_json::json!({
                "workflow_id": workflow.id,
                "status": workflow.status,
                "goal": workflow.goal,
                "tasks": workflow.tasks,
                "artifacts": workflow.artifacts,
                "creative_artifacts": creative_summaries,
                "has_token_collection": workflow.token_collection.is_some(),
                "token_summary": token_summary,
                "revisions": workflow.revisions,
                "human_interaction_summary": summarize_human_interactions(&workflow.tasks),
            });
            print_response(output, &response)?;
            Ok(0)
        }
        Commands::Context {
            workflow,
            task,
            budget,
            strict,
            output,
        } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let latest_checkpoint = load_latest_task_checkpoint(&store, &workflow.id, &task)?;
            let context =
                build_context_package_with_checkpoint(&workflow, &task, budget, latest_checkpoint)?;
            print_response(output, &context)?;
            Ok(if strict && !context.handoff_ready {
                1
            } else {
                0
            })
        }
        Commands::Run {
            workflow,
            simulate,
            output,
        } => {
            if !simulate {
                anyhow::bail!("v0 execution requires --simulate; real provider execution is intentionally not enabled");
            }
            let store = ForgeStore::open(cli.store)?;
            let mut workflow = store.load_workflow(&workflow)?;
            let mut report = run_simulated(&mut workflow);
            let completed = report.status == "completed";
            if completed {
                if let Some(smoke) = run_daily_goal_research_smoke(&store, &mut workflow)? {
                    report.daily_goal_research = Some(serde_json::to_value(smoke)?);
                }
            }
            store.save_workflow(&workflow)?;
            store.record_event(
                &workflow.id,
                "workflow_simulated",
                &serde_json::to_value(&report)?,
            )?;
            print_response(output, &report)?;
            Ok(if completed { 0 } else { 1 })
        }
        Commands::Validate { workflow, output } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let report = validate_workflow(&workflow);
            let exit_code = if report.promotable { 0 } else { 1 };
            print_response(output, &report)?;
            Ok(exit_code)
        }
        Commands::Improve {
            workflow,
            target_version,
            output,
        } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let proposal = generate_improvement(&store, &workflow, target_version)?;
            print_response(output, &proposal)?;
            Ok(0)
        }
        Commands::Artifacts { workflow, output } => {
            let store = ForgeStore::open(cli.store)?;
            let _workflow = store.load_workflow(&workflow)?;
            let artifacts = list_workflow_artifacts(&store.base_dir(), &workflow)?;
            let response = serde_json::json!({
                "workflow_id": workflow,
                "artifacts": artifacts,
            });
            print_response(output, &response)?;
            Ok(0)
        }
        Commands::Skill { command } => match command {
            SkillCommands::Install {
                home,
                target,
                executor_paths,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = install_skill(&home, &target)?;
                let executor_sync = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home: home.clone(),
                        executor_paths,
                        allow: allow.clone(),
                        deny: deny.clone(),
                        prompt: !no_prompt,
                    },
                )?;
                let runtime_sync = sync_runtimes(
                    &store,
                    RuntimeSyncOptions {
                        home: home.clone(),
                        runtime_paths,
                        allow: allow.clone(),
                        deny: deny.clone(),
                        prompt: !no_prompt,
                    },
                )?;
                let response = serde_json::json!({
                    "skill": report.skill,
                    "installed": report.installed,
                    "executor_sync": executor_sync,
                    "runtime_sync": runtime_sync,
                });
                print_response(output, &response)?;
                Ok(0)
            }
        },
        Commands::Sync { command } => match command {
            SyncCommands::Executors {
                home,
                executor_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home,
                        executor_paths,
                        allow,
                        deny,
                        prompt: !no_prompt,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            SyncCommands::Runtimes {
                home,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = sync_runtimes(
                    &store,
                    RuntimeSyncOptions {
                        home,
                        runtime_paths,
                        allow,
                        deny,
                        prompt: !no_prompt,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            SyncCommands::All {
                home,
                executor_paths,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let executor_sync = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home: home.clone(),
                        executor_paths,
                        allow: allow.clone(),
                        deny: deny.clone(),
                        prompt: !no_prompt,
                    },
                )?;
                let runtime_sync = sync_runtimes(
                    &store,
                    RuntimeSyncOptions {
                        home,
                        runtime_paths,
                        allow,
                        deny,
                        prompt: !no_prompt,
                    },
                )?;
                let response = serde_json::json!({
                    "status": "synced",
                    "executor_sync": executor_sync,
                    "runtime_sync": runtime_sync,
                });
                print_response(output, &response)?;
                Ok(0)
            }
        },
        Commands::Executors { output } => {
            let store = ForgeStore::open(cli.store)?;
            let report = load_executors(&store)?;
            print_response(output, &report)?;
            Ok(0)
        }
        Commands::Runtimes { output } => {
            let store = ForgeStore::open(cli.store)?;
            let report = load_runtimes(&store)?;
            print_response(output, &report)?;
            Ok(0)
        }
        Commands::Runtime { command } => match command {
            RuntimeCommands::Guard {
                substrate,
                resource,
                namespace,
                action,
                owner,
                allow_external,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = guard_runtime_scope(
                    &store,
                    RuntimeGuardRequest {
                        substrate,
                        resource,
                        namespace,
                        action,
                        owner,
                        allow_external,
                    },
                )?;
                let exit_code = if report.allowed { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
        },
        Commands::Schedule { command } => match command {
            ScheduleCommands::CreateDailyGoalResearch {
                goals,
                timezone,
                cron,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report =
                    create_daily_goal_research_workflow(&store, goals, &timezone, &cron, &origin)?;
                let workflow = report.workflow.clone();
                let response = serde_json::json!({
                    "status": report.status,
                    "workflow_id": report.workflow_id,
                    "origin": report.origin,
                    "goals": report.goals,
                    "workflow": workflow.clone(),
                    "tasks": workflow.tasks,
                    "schedule_summary": report.schedule_summary,
                    "loop_summary": report.loop_summary,
                    "attached_subflows": report.attached_subflows,
                });
                print_response(output, &response)?;
                Ok(0)
            }
            ScheduleCommands::List { output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = list_workflows_with_filters(
                    &store,
                    WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All)
                        .only_scheduled_or_looping(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Inspect { workflow, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = inspect_workflow_with_focus(&store, &workflow, true, None)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Update {
                workflow,
                task,
                cron,
                timezone,
                missed_run_policy,
                next_run_at,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = update_workflow_schedule(
                    &store,
                    &workflow,
                    &task,
                    ScheduleUpdateOptions {
                        cron: cron.as_deref(),
                        timezone: timezone.as_deref(),
                        missed_run_policy: missed_run_policy.as_deref(),
                        next_run_at: next_run_at.as_deref(),
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Pause {
                workflow,
                task,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = update_loop_state(&store, &workflow, &task, "paused", &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Resume {
                workflow,
                task,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = update_loop_state(&store, &workflow, &task, "active", &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Stop {
                workflow,
                task,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = update_loop_state(&store, &workflow, &task, "stopped", &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::RunDue { workflow, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = run_due_workflow(&store, &workflow)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::ScanDue {
                executor,
                max_workers,
                ttl_seconds,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = if max_workers > 1 {
                    scan_due_workflows_parallel(&store, &executor, max_workers, ttl_seconds)?
                } else {
                    scan_due_workflows(&store, &executor, ttl_seconds)?
                };
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Summary { output } => {
                let store = ForgeStore::open(cli.store)?;
                let workflows = store.load_workflows()?;
                let task_slices: Vec<&[forge_core::graph::AtomicTask]> =
                    workflows.iter().map(|wf| wf.tasks.as_slice()).collect();
                let report = aggregate_summary(&task_slices);
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::LoopSummary { output } => {
                let store = ForgeStore::open(cli.store)?;
                let workflows = store.load_workflows()?;
                let task_slices: Vec<&[forge_core::graph::AtomicTask]> =
                    workflows.iter().map(|wf| wf.tasks.as_slice()).collect();
                let report = aggregate_summary(&task_slices);
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::WorkerStatus {
                executor,
                max_workers,
                ttl_seconds,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report =
                    build_schedule_worker_status(&store, &executor, max_workers, ttl_seconds)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Cluster { command } => match command {
            ClusterCommands::Register {
                node_id,
                name,
                endpoint,
                os,
                arch,
                cpu_cores,
                memory_gb,
                gpus,
                installed_software,
                capabilities,
                python_available,
                node_available,
                docker_available,
                gpu_available,
                network_reachable,
                status,
                trust_level,
                sandbox_permissions,
                cost_per_hour_usd,
                latency_ms,
                reliability,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = register_cluster_node(
                    &store,
                    ClusterNodeInput {
                        node_id,
                        name,
                        endpoint,
                        os,
                        arch,
                        cpu_cores,
                        memory_gb,
                        gpus,
                        installed_software,
                        capabilities,
                        python_available,
                        node_available,
                        docker_available,
                        gpu_available,
                        network_reachable,
                        status,
                        trust_level,
                        sandbox_permissions,
                        cost_per_hour_usd,
                        latency_ms,
                        reliability,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            ClusterCommands::List { output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = list_cluster_nodes(&store)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ClusterCommands::Leases { node_id, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = list_cluster_node_leases(&store, node_id.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            ClusterCommands::Place {
                workflow,
                task,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = place_task_on_cluster(&store, &workflow, &task)?;
                let exit_code = if report.selected_node.is_some() { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            ClusterCommands::Handoff {
                workflow,
                task,
                budget,
                ttl_seconds,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report =
                    build_cluster_task_handoff(&store, &workflow, &task, budget, ttl_seconds)?;
                let exit_code = if report.allowed { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
        },
        Commands::Workflow { command } => match command {
            WorkflowCommands::UpdateGoal {
                workflow,
                goal,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = update_workflow_goal(&store, &workflow, &goal, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::AttachArtifact {
                workflow,
                path,
                kind,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = attach_workflow_artifact(&store, &workflow, &path, &kind, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::ValidateSubflow {
                workflow,
                task,
                child_workflow,
                child_task,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = validate_child_subflow_binding(
                    &store,
                    &workflow,
                    &task,
                    &child_workflow,
                    &child_task,
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::AttachCreative {
                workflow,
                title,
                kind,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let artifact = match kind.as_str() {
                    "screen" => CreativeArtifact::new_screen(
                        &title,
                        forge_core::ir::ScreenSpec {
                            schema_version: forge_core::ir::ir_schema_version(),
                            width_px: 1440,
                            height_px: 900,
                            background: "#ffffff".to_string(),
                            breakpoints: Vec::new(),
                            elements: Vec::new(),
                            interactions: Vec::new(),
                        },
                    ),
                    "whiteboard" => CreativeArtifact::new_whiteboard(
                        &title,
                        forge_core::ir::WhiteboardSpec {
                            schema_version: forge_core::ir::ir_schema_version(),
                            width_px: 1920,
                            height_px: 1080,
                            background: "#ffffff".to_string(),
                            layers: Vec::new(),
                            sticky_notes: Vec::new(),
                            drawings: Vec::new(),
                            text_blocks: Vec::new(),
                            images: Vec::new(),
                        },
                    ),
                    "document" => CreativeArtifact::new_document(
                        &title,
                        forge_core::ir::DocumentSpec {
                            schema_version: forge_core::ir::ir_schema_version(),
                            title: title.clone(),
                            author: origin.clone(),
                            front_matter: std::collections::BTreeMap::new(),
                            sections: Vec::new(),
                        },
                    ),
                    "slide_deck" => CreativeArtifact::new_slide_deck(
                        &title,
                        forge_core::ir::SlideDeckSpec {
                            schema_version: forge_core::ir::ir_schema_version(),
                            title: title.clone(),
                            theme: "default".to_string(),
                            slides: Vec::new(),
                        },
                    ),
                    "component" => CreativeArtifact::new_component(
                        &title,
                        forge_core::ir::ComponentSpec {
                            schema_version: forge_core::ir::ir_schema_version(),
                            name: title.clone(),
                            description: String::new(),
                            props: Vec::new(),
                            variants: Vec::new(),
                            states: Vec::new(),
                            slots: Vec::new(),
                            token_dependencies: Vec::new(),
                            code_template: None,
                        },
                    ),
                    other => anyhow::bail!(
                        "unknown creative artifact kind: {other}; expected one of: screen, whiteboard, document, slide_deck, component"
                    ),
                };
                let report = attach_creative_artifact(&store, &workflow, artifact, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::ListCreative { workflow, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = list_creative_artifacts(&store, &workflow)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::InspectCreative {
                workflow,
                artifact,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = inspect_creative_artifact(&store, &workflow, &artifact)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::CollaborationEvent {
                workflow,
                artifact,
                kind,
                actor,
                summary,
                target,
                selections,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = record_creative_collaboration_event(
                    &store,
                    CreativeCollaborationEventRequest {
                        workflow_id: workflow,
                        artifact_id: artifact,
                        event_kind: kind,
                        actor,
                        summary,
                        target,
                        selections,
                        origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::CollaborationStatus {
                workflow,
                artifact,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = inspect_creative_collaboration(&store, &workflow, &artifact)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::SetTokens {
                workflow,
                name,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let token_collection = TokenCollection {
                    schema_version: forge_core::ir::ir_schema_version(),
                    description: format!("Design tokens for {name}"),
                    tokens: vec![
                        forge_core::ir::DesignToken {
                            name: "color.primary".to_string(),
                            value: "#3B82F6".to_string(),
                            token_type: forge_core::ir::TokenType::Color,
                            description: "Primary brand color".to_string(),
                            group: "color".to_string(),
                            extensions: std::collections::BTreeMap::new(),
                        },
                        forge_core::ir::DesignToken {
                            name: "spacing.md".to_string(),
                            value: "16px".to_string(),
                            token_type: forge_core::ir::TokenType::Spacing,
                            description: "Medium spacing".to_string(),
                            group: "spacing".to_string(),
                            extensions: std::collections::BTreeMap::new(),
                        },
                    ],
                    semantic_aliases: vec![forge_core::ir::SemanticAlias {
                        name: format!("semantic.{name}"),
                        resolves_to: "color.primary".to_string(),
                        description: format!("Semantic alias for {name}"),
                    }],
                    name,
                    modes: Vec::new(),
                };
                let report =
                    set_workflow_token_collection(&store, &workflow, token_collection, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::GetTokens { workflow, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = get_workflow_token_collection(&store, &workflow)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::ResolveTokens {
                workflow,
                mode,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = resolve_workflow_tokens(&store, &workflow, mode.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::PatchToken {
                workflow,
                token,
                value,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = patch_workflow_token(&store, &workflow, &token, &value, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Task { command } => match command {
            TaskCommands::Handoff {
                workflow,
                task,
                executor,
                budget,
                ttl_seconds,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report =
                    build_task_handoff(&store, &workflow, &task, &executor, budget, ttl_seconds)?;
                let exit_code = if report.allowed { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            TaskCommands::Acquire {
                workflow,
                task,
                executor,
                ttl_seconds,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = acquire_task_lease(&store, &workflow, &task, &executor, ttl_seconds)?;
                let exit_code = if report.allowed { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            TaskCommands::Release {
                workflow,
                task,
                lease,
                executor,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = release_task_lease(&store, &workflow, &task, &lease, &executor)?;
                let exit_code = if report.released { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            TaskCommands::Checkpoint {
                workflow,
                task,
                executor,
                state,
                summary,
                context_sha256,
                context_routing_cache_key,
                workflow_revision,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = record_task_checkpoint(
                    &store,
                    TaskCheckpointRequest {
                        workflow_id: &workflow,
                        task_id: &task,
                        executor: &executor,
                        state: &state,
                        summary: &summary,
                        context_sha256: &context_sha256,
                        context_routing_cache_key: context_routing_cache_key.as_deref(),
                        workflow_revision,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            TaskCommands::ValidateResponse {
                workflow,
                task,
                response,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = validate_executor_response_file(&store, &workflow, &task, &response)?;
                let exit_code = if report.accepted { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
        },
        Commands::Request { command } => match command {
            RequestCommands::Start {
                goal,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = start_async_request(&store, &goal, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Status { run_id, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = load_request_status(&store, &run_id)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::List { status, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = list_requests(&store, status.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Cancel {
                run_id,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = cancel_request(&store, &run_id, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Resume {
                run_id,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = resume_async_request(&store, &run_id, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Mcp { command } => match command {
            McpCommands::Tools { output } => {
                let manifest = mcp_tools_manifest();
                print_response(output, &manifest)?;
                Ok(0)
            }
            McpCommands::Call {
                tool,
                input,
                input_file,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let input = read_mcp_input(input, input_file)?;
                let report = call_mcp_tool(&store, &tool, input)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Interactive { command } => match command {
            InteractiveCommands::Home { output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = build_interactive_home(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_home(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::SlashCommands { output } => {
                let report = slash_command_catalog();
                print_response(output, &report)?;
                Ok(0)
            }
            InteractiveCommands::Route {
                input,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = route_interactive_input(&store, &input, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Interaction { command } => match command {
            InteractionCommands::CreateChoice {
                workflow,
                task,
                kind,
                prompt,
                choices,
                timeout_seconds,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = create_choice_interaction(
                    &store,
                    CreateChoiceInteractionRequest {
                        workflow_id: &workflow,
                        task_id: &task,
                        kind: &kind,
                        prompt: &prompt,
                        choices: &choices,
                        timeout_seconds,
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            InteractionCommands::CreateForm {
                workflow,
                task,
                prompt,
                fields,
                timeout_seconds,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = create_form_interaction(
                    &store,
                    &workflow,
                    &task,
                    &prompt,
                    &fields,
                    timeout_seconds,
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            InteractionCommands::Answer {
                workflow,
                task,
                selected_options,
                field_values,
                rationale,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = answer_human_interaction(
                    &store,
                    &workflow,
                    &task,
                    &selected_options,
                    &field_values,
                    rationale.as_deref(),
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            InteractionCommands::Expire {
                workflow,
                task,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = expire_human_interaction(&store, &workflow, &task, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            InteractionCommands::List { output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = list_human_interactions(&store)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Milestone { command } => match command {
            MilestoneCommands::Status { version, output } => {
                let report = build_milestone_status(&version)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::Manifest { version, output } => {
                let report = build_milestone_manifest(&version)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::Research { version, output } => {
                let report = build_milestone_research(&version)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::SelfRun { command } => match command {
            SelfCommands::Run {
                repo,
                until,
                max_cycles,
                sleep_seconds,
                executors,
                mode,
                dry_run,
                push,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = run_self_evolution(
                    &store,
                    SelfRunOptions {
                        repo,
                        until,
                        max_cycles,
                        sleep_seconds,
                        executors,
                        mode,
                        dry_run,
                        push,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
    }
}

fn print_response<T: Serialize>(format: OutputFormat, value: &T) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
        OutputFormat::Human => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}

fn read_mcp_input(input: Option<String>, input_file: Option<PathBuf>) -> Result<serde_json::Value> {
    match (input, input_file) {
        (Some(_), Some(_)) => anyhow::bail!("use either --input or --input-file, not both"),
        (Some(input), None) => Ok(serde_json::from_str(&input)?),
        (None, Some(path)) => {
            let bytes = std::fs::read(&path)
                .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", path.display()))?;
            Ok(serde_json::from_slice(&bytes)?)
        }
        (None, None) => Ok(serde_json::json!({})),
    }
}
