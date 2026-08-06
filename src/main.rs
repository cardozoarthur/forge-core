use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use foundry_core::adapter::validate_executor_response_file;
use foundry_core::addon::{
    addon_observability_report, apply_addon_validator_outcome, authorize_addon_permission,
    claim_addon_runtime_contract_dispatch, complete_addon_runtime_contract_dispatch,
    create_addon_migration_workflow, create_addon_package_lock, default_addon_dirs, disable_addon,
    downgrade_addon, enable_addon, enqueue_addon_planner_dispatch,
    enqueue_addon_runtime_contract_dispatch, evaluate_addon_runtime_contract_policy,
    execute_addon_executor, execute_addon_handoff, execute_addon_planning_strategy,
    execute_addon_runtime_contract_dispatch, execute_addon_validator, fetch_addon_package,
    install_addon, install_addon_package, list_addon_capability_index, list_addon_event_adapters,
    list_addon_marketplace, list_addon_permission_authorizations, list_addon_planner_registry,
    list_addon_runtime_contract_dispatches, list_addon_runtime_contracts,
    list_addon_runtime_workers, list_addon_trust_store, list_addon_views, list_installed_addons,
    load_addon_catalog_from_store, package_addon, plan_addon_lifecycle, publish_addon_package,
    register_addon_runtime_worker, resolve_goal_capabilities_with_registry_sync,
    resolve_goal_capabilities_with_store, revoke_addon_permission,
    run_addon_runtime_contract_dispatch, run_addon_runtime_contract_dispatch_worker,
    sync_addon_package_registry, trust_addon_package_key, uninstall_addon, upgrade_addon,
    validate_addon_catalog, AddonExecutorDispatchInput, AddonExecutorExecutionInput,
    AddonHandoffDispatchInput, AddonHandoffExecutionInput, AddonPackageInput,
    AddonPlannerDispatchInput, AddonPlanningStrategyInput, AddonRuntimeContractCompletionInput,
    AddonRuntimeWorkerRegistrationInput, AddonTrustKeyInput, AddonValidatorDispatchInput,
    AddonValidatorExecutionInput, AddonValidatorOutcomeApplyInput, CapabilityRegistrySyncInput,
};
use foundry_core::artifact::list_workflow_artifacts_with_tags;
use foundry_core::aws_ops::{
    run_check as run_aws_ops_check, run_inventory as run_aws_ops_inventory,
    run_raw as run_aws_ops_raw,
};
use foundry_core::checkpoint::{
    load_latest_task_checkpoint, record_task_checkpoint, TaskCheckpointRequest,
};
use foundry_core::cli_factory::{create_cli_factory_plan, CliFactoryCreateInput};
use foundry_core::cli_integration::{
    analyze_token_headroom, build_cli_wrapper_plan, build_harness_activation_profile,
    build_harness_adoption_plan, build_harness_bootstrap_report, build_harness_doctor_report,
    build_harness_headroom_plan, build_harness_mode_report, build_headroom_stats_report,
    inspect_cli_harness_shim_status, install_cli_harness_shim, install_cli_provider_adapter,
    persist_token_headroom_report, resolve_harness_foundry_first_source_for_project,
    resolve_harness_runtime_policy, retrieve_headroom_blob, run_cli_harness_exec,
    CliHarnessExecOptions, CliShimInstallOptions, CliShimStatusOptions, CliWrapperPlanOptions,
    HarnessActivationProfileOptions, HarnessAdoptionPlanOptions, HarnessBootstrapOptions,
    HarnessDoctorOptions, HarnessHeadroomPlanOptions, HarnessModeOptions,
    HarnessRuntimePolicyOptions, HeadroomStatsOptions, ProviderAdapterInstallOptions,
};
use foundry_core::cluster::{
    build_cluster_task_handoff, list_cluster_node_leases, list_cluster_nodes,
    place_task_on_cluster, register_cluster_node, ClusterNodeInput,
};
use foundry_core::context::{
    build_compact_context_view_with_predecessor_plans,
    build_context_package_with_checkpoint_and_project,
    build_context_package_with_checkpoint_project_and_worktree, DEFAULT_CONTEXT_BUDGET,
};
use foundry_core::cost::{
    apply_cost_ledger_retention_for_context, build_cost_ledger_for_context,
    build_cost_ledger_history_for_context, maintain_cost_ledger_for_context,
    materialize_cost_ledger_incremental_for_context, materialize_cost_ledger_index_for_context,
    run_cost_ledger_daemon_for_context,
};
use foundry_core::credential_vault::{
    run_describe as run_credential_vault_describe, run_exec as run_credential_vault_exec,
    run_key_init as run_credential_vault_key_init, run_panel as run_credential_vault_panel,
    run_records as run_credential_vault_records,
};
use foundry_core::event::{
    build_event_improvement_policy_for_context, build_event_observability_history_for_context,
    build_event_observability_index_for_context, build_event_service_plan,
    build_global_event_timeline_for_context, build_workflow_event_stream,
    dispatch_inbound_event_activations, emit_event_egress, ingest_inbound_event_with_context,
    list_event_services, list_inbound_event_inbox_for_context, recover_stale_event_services,
    route_inbound_event, run_event_runtime_daemon, run_event_runtime_reconcile,
    run_event_service_supervisor, run_event_webhook_ingress_server,
    run_event_webhook_ingress_service, run_event_worker_service, run_inbound_event_worker_loop,
    scan_inbound_event_inbox, EventEgressEmitInput, InboundEventIngestInput,
    InboundEventWorkerLoopOptions,
};
use foundry_core::execution::run_simulated;
use foundry_core::executor::{
    build_brain_session_history_report, build_brain_sessions_report_with_options,
    build_shell_launch_plan, decide_executor_model_for_task, import_ai_limits_observations,
    load_executors, record_brain_session_lifecycle, record_shell_session_plan, sync_executors,
    BrainSessionLifecycleOptions, BrainSessionsReportOptions, ExecutorModelDecisionOptions,
    ExecutorQuotaObservation, ExecutorSyncOptions, ShellLaunchPlanOptions,
};
use foundry_core::executor_runtime::{execute_request_executor_wave, RequestExecutorWaveOptions};
use foundry_core::graph::{create_workflow, CoreParallelTeamSpec};
use foundry_core::handoff::{
    build_predecessor_handoff_plans, build_task_handoff_response_with_project, TaskHandoffView,
};
use foundry_core::identity::{
    audit_tenant_index, ensure_operating_context_policy, ensure_workflow_policy,
    evaluate_tenant_policy_for_action, inspect_project_operating_context, link_identity,
    list_identity_links, list_identity_memberships, list_identity_registry, list_tenant_index,
    load_project_operating_context, resolve_identity, sync_project_operating_context,
    unlink_identity, update_identity_membership, IdentityLinkInput, IdentityMembershipUpdateInput,
};
use foundry_core::improve::{
    apply_event_improvement_policy, benchmark_event_improvement_policy, generate_improvement,
    normalize_avoidable_ai_costs, normalize_avoidable_ai_costs_for_candidates,
    promote_event_improvement_policy, rank_improvement_candidates_with_filter,
    ImprovementCandidateFilter,
};
use foundry_core::inspection::inspect_workflow_with_focus;
use foundry_core::intent::{parse_intent_with_catalog_and_context, OperatingContextSpec};
use foundry_core::interaction::{
    answer_human_interaction, create_choice_interaction, create_form_interaction,
    expire_human_interaction, list_human_interactions, summarize_human_interactions,
    CreateChoiceInteractionRequest,
};
use foundry_core::interactive::{
    build_foundry_first_harness_smoke, build_interactive_action_invocation_for_project,
    build_interactive_action_registry_for_project,
    build_interactive_addon_capabilities_for_project, build_interactive_architecture_compass,
    build_interactive_artifacts, build_interactive_autocomplete_for_project,
    build_interactive_command_palette_for_project, build_interactive_context_memory,
    build_interactive_core_boundary_for_project, build_interactive_event_runtime,
    build_interactive_guided_cockpit, build_interactive_harness,
    build_interactive_home_with_options, build_interactive_identity,
    build_interactive_improvement_loop, build_interactive_multimodal_runtime,
    build_interactive_operating_context, build_interactive_operational_cockpit,
    build_interactive_patch_workbench, build_interactive_permissions, build_interactive_readiness,
    build_interactive_release_gates, build_interactive_replacement_cli_with_options,
    build_interactive_schedules, build_interactive_sessions, build_interactive_structured_logs,
    build_interactive_task_board, build_interactive_token_usage, build_interactive_ui_composition,
    build_interactive_workflow_dag, build_interactive_workflow_mutation,
    build_interactive_workflow_sidebar, build_multimodal_runtime_evidence_smoke,
    build_operational_tui_smoke, build_replacement_cli_evidence_smoke,
    dispatch_interactive_action_hooks_for_project, render_foundry_first_harness_smoke,
    render_interactive_action_dispatch, render_interactive_action_invocation,
    render_interactive_action_registry, render_interactive_addon_capabilities,
    render_interactive_architecture_compass, render_interactive_artifacts,
    render_interactive_autocomplete, render_interactive_command_palette,
    render_interactive_context_memory, render_interactive_core_boundary,
    render_interactive_event_runtime, render_interactive_guided_cockpit,
    render_interactive_harness, render_interactive_home, render_interactive_identity,
    render_interactive_improvement_loop, render_interactive_multimodal_runtime,
    render_interactive_operating_context, render_interactive_operational_cockpit,
    render_interactive_patch_workbench, render_interactive_permissions,
    render_interactive_readiness, render_interactive_release_gates,
    render_interactive_replacement_cli, render_interactive_schedules, render_interactive_sessions,
    render_interactive_structured_logs, render_interactive_task_board,
    render_interactive_token_usage, render_interactive_ui_composition,
    render_interactive_workflow_dag, render_interactive_workflow_mutation,
    render_interactive_workflow_sidebar, render_multimodal_runtime_evidence_smoke,
    render_operational_tui_smoke, render_replacement_cli_evidence_smoke, route_interactive_input,
    route_interactive_input_with_context, slash_command_catalog, InteractiveHarnessOptions,
    InteractiveHomeOptions, InteractiveReplacementCliOptions, InteractiveSessionsOptions,
};
use foundry_core::ir::{CreativeArtifact, TokenCollection};
use foundry_core::lease::{acquire_task_lease, release_task_lease};
use foundry_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use foundry_core::mcp_stdio::serve_stdio;
use foundry_core::memory::{
    configure_memory_governance, list_memory_promotions, memory_cleanup_report,
    memory_policy_report_for_project, memory_retention_report, promote_memory, search_memory,
    MemoryCleanupOptions, MemoryGovernanceConfigOptions, MemoryPromotionOptions,
    MemoryRetentionOptions, MemorySearchOptions,
};
use foundry_core::milestone::{
    assemble_production_evidence, attach_milestone_evidence, build_milestone_evidence_plan,
    build_milestone_export_demo, build_milestone_manifest_with_store, build_milestone_research,
    build_milestone_status, build_production_mission_lifecycle_evidence,
    build_production_readiness_plan, build_replacement_cli_demo_with_options,
    collect_milestone_evidence, collect_ready_milestone_evidence, evaluate_production_readiness,
    prepare_milestone_evidence_inputs, write_production_evidence_template,
    MilestoneAttachEvidenceOptions, MilestoneCliDemoOptions, MilestoneCollectEvidenceOptions,
    MilestoneCollectReadyEvidenceOptions, MilestoneEvidencePlanOptions,
    MilestonePrepareEvidenceInputsOptions, ProductionEvidenceAssemblyOptions,
    ProductionEvidenceTemplateOptions, ProductionReadinessOptions,
};
use foundry_core::mission::{
    builtin_squad_catalog, clone_squad, drive_mission, install_builtin_squads, install_squad,
    list_installed_squads, list_missions, load_mission, load_squad, read_squad_manifest,
    resume_mission, simulate_mission_with_worktree, start_mission, submit_mission,
    validate_squad_definition, MissionSubmission,
};
use foundry_core::mission_executor::{
    build_mission_execution_approval, execute_mission_command, inspect_mission_execution_receipt,
    list_mission_execution_receipts, plan_mission_execution, reconcile_mission_execution,
    MissionExecutionReconcileRequest, MissionExecutionRequest,
};
use foundry_core::mission_platform::{
    mission_platform_catalog, simulate_mission_platform_with_store,
};
use foundry_core::multimodal::{
    build_multimodal_benchmark_result, build_multimodal_benchmark_template,
    build_multimodal_demo_plan, build_multimodal_demo_receipt, build_multimodal_install_plan,
    build_multimodal_readiness, build_multimodal_runtime_benchmark,
    build_multimodal_status_with_feature_flag, evaluate_multimodal_guard,
    resolve_multimodal_feature_flag, MultimodalBenchmarkResultOptions,
    MultimodalDemoReceiptOptions, MultimodalReadinessOptions, MultimodalRuntimeBenchmarkOptions,
};
use foundry_core::opencode_tui::{build_foundry_tui, render_foundry_tui, run_foundry_tui};
use foundry_core::ops::{
    build_ops_snapshot_with_addon_dirs_and_project, record_addon_renderer_client_event,
    serve_ops_console_with_addon_dirs_and_project, OpsAddonRendererClientEventInput,
};
use foundry_core::patch::{
    build_patch_apply, build_patch_diff, build_patch_plan, build_patch_restore, build_patch_revert,
    build_patch_review, PatchDiffOptions,
};
use foundry_core::registry::{
    attach_reuse_candidates_as_child_subflows, context_action_catalog, find_reuse_candidates,
    list_workflows_with_filters, quality_action_catalog, WorkflowLifecycleFilter,
    WorkflowRegistryFilters,
};
use foundry_core::request::{
    cancel_request, complete_ready_task, create_final_delivery_package, drive_request,
    ensure_final_audit, heartbeat_request, list_requests, load_request_status,
    recover_stale_request, resume_async_request,
    start_async_request_with_idempotency_and_parallel_team,
    start_async_request_with_project_idempotency_and_parallel_team, step_request,
    switch_request_executor, RequestExecutorSwitchInput, RequestTaskCompletionInput,
};
use foundry_core::request_supervisor::{
    supervise_request_once, supervise_requests_once, RequestSupervisorOptions,
};
use foundry_core::runtime::{
    guard_runtime_scope, load_runtimes, sync_runtimes, RuntimeGuardRequest, RuntimeSyncOptions,
};
use foundry_core::schedule::{
    aggregate_summary, build_schedule_worker_status, create_daily_goal_research_workflow,
    run_daily_goal_research_smoke, run_due_workflow, scan_due_workflows,
    scan_due_workflows_parallel, update_loop_state, update_workflow_schedule,
    ScheduleUpdateOptions,
};
use foundry_core::security::{
    sanitize_prompt_secrets_with_vault, sanitize_workflow_secrets_for_storage,
    SecretSanitizationOptions, SecretVaultPersistOptions,
};
use foundry_core::self_evolve::{run_self_evolution, SelfRunOptions};
use foundry_core::skill::install_skill;
use foundry_core::storage::FoundryStore;
use foundry_core::store_admin::{backup_store, check_store, restore_store};
use foundry_core::validation::validate_workflow;
use foundry_core::workflow::{
    add_workflow_task, add_workflow_task_dependency, attach_creative_artifact,
    attach_workflow_artifact_with_tags, clear_workflow_task_impediment,
    get_workflow_token_collection, inspect_creative_artifact, inspect_creative_collaboration,
    list_creative_artifacts, parse_node_brain_agent_slot, patch_workflow_token,
    record_creative_collaboration_event, remove_workflow_task_dependency, resolve_workflow_tokens,
    set_workflow_task_impediment, set_workflow_task_priority, set_workflow_token_collection,
    update_workflow_goal_with_expected_revision, update_workflow_node_brain_routing,
    update_workflow_task_with_expected_revision, validate_child_subflow_binding,
    CreativeCollaborationEventRequest, ProductDecisionInput, WorkflowNodeBrainRoutingUpdateInput,
    WorkflowTaskAddInput, WorkflowTaskDependencyInput, WorkflowTaskImpedimentClearInput,
    WorkflowTaskImpedimentInput, WorkflowTaskPriorityInput, WorkflowTaskUpdateInput,
};
use foundry_core::worktree::{
    approve_worktree_config, bind_worktree, bound_worktree_context, create_worktree,
    create_worktree_guard_predecessor_task, discover_worktrees,
    evaluate_worktree_modification_guard, initialize_worktree, inspect_registered_worktree,
    inspect_worktree_sandbox_lifecycle, list_registered_worktrees, plan_worktree_sandbox,
    register_worktree, resolve_effective_project_root, resolve_worktree_selector_root,
    run_worktree_sandbox, start_worktree_sandbox, stop_worktree_sandbox,
    supervise_worktree_sandbox, WorktreeCreateOptions, WorktreeModificationGuardRequest,
    WorktreeRegisterOptions, WorktreeSandboxRequest,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const LEGACY_BINARY_NAME: &str = "forge"; // foundry-brand-allow: legacy-compat
const LEGACY_DEFAULT_STORE_PATH: &str = ".forge/forge.sqlite"; // foundry-brand-allow: legacy-compat
const LEGACY_BINARY_WARNING: &str = "warning: `forge` is deprecated and will be removed after the 0.6.x compatibility cycle; use `foundry`"; // foundry-brand-allow: legacy-compat

#[derive(Debug, Parser)]
#[command(name = "foundry", version, about = "Foundry Core workflow runtime")]
struct Cli {
    #[arg(long, default_value = ".foundry/foundry.sqlite")]
    store: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Plan {
        #[arg(long)]
        goal: String,
        #[arg(long)]
        worktree: Option<String>,
        #[arg(
            long = "lane",
            value_name = "ID=EXECUTOR:COUNT",
            help = "Declare an independent parallel lane; repeat for multiple frontend/backend teams"
        )]
        lanes: Vec<String>,
        #[arg(
            long = "max-parallel-agents",
            requires = "lanes",
            help = "Bound total concurrent agents across the declared lanes"
        )]
        max_parallel_agents: Option<usize>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
        #[arg(short = 'd', long = "detached")]
        detached: bool,
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
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, default_value_t = 1200)]
        budget: usize,
        #[arg(long)]
        strict: bool,
        #[arg(long, value_enum, default_value_t = ContextViewArg::Compact)]
        view: ContextViewArg,
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
    Store {
        #[command(subcommand)]
        command: StoreCommands,
    },
    Improve {
        #[command(subcommand)]
        command: Option<ImproveCommands>,
        #[arg(long)]
        workflow: Option<String>,
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
    Events {
        #[command(subcommand)]
        command: EventCommands,
    },
    Identity {
        #[command(subcommand)]
        command: IdentityCommands,
    },
    Addons {
        #[command(subcommand)]
        command: AddonCommands,
    },
    Cli {
        #[command(subcommand)]
        command: CliCommands,
    },
    Harness {
        #[command(subcommand)]
        command: HarnessCommands,
    },
    Cost {
        #[command(subcommand)]
        command: CostCommands,
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
    Brains {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Sessions {
        #[command(subcommand)]
        command: Option<SessionCommands>,
        #[arg(long = "provider")]
        provider_id: Option<String>,
        #[arg(long = "state")]
        lifecycle_state: Option<String>,
        #[arg(long)]
        readiness: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Shells {
        #[arg(long)]
        executor: Option<String>,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        task: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget", default_value_t = DEFAULT_CONTEXT_BUDGET)]
        context_budget: usize,
        #[arg(long = "ttl-seconds", default_value_t = 900)]
        ttl_seconds: u64,
        #[arg(long = "record-session", default_value_t = false)]
        record_session: bool,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ExecutorQuota {
        #[command(subcommand)]
        command: ExecutorQuotaCommands,
    },
    Runtimes {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommands,
    },
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommands,
    },
    Security {
        #[command(subcommand)]
        command: SecurityCommands,
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
    Tui {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Interactive {
        #[command(subcommand)]
        command: InteractiveCommands,
    },
    Interaction {
        #[command(subcommand)]
        command: InteractionCommands,
    },
    Ops {
        #[command(subcommand)]
        command: OpsCommands,
    },
    Milestone {
        #[command(subcommand)]
        command: MilestoneCommands,
    },
    Multimodal {
        #[command(subcommand)]
        command: MultimodalCommands,
    },
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    Patch {
        #[command(subcommand)]
        command: PatchCommands,
    },
    Smoke {
        #[command(subcommand)]
        command: SmokeCommands,
    },
    Aws {
        #[command(subcommand)]
        command: AwsCommands,
    },
    CredentialVault {
        #[command(subcommand)]
        command: CredentialVaultCommands,
    },
    #[command(name = "self")]
    SelfRun {
        #[command(subcommand)]
        command: SelfCommands,
    },
    Teamwork {
        #[arg(long)]
        goal: String,
        #[arg(
            long = "lane",
            value_name = "ID=BRAIN:COUNT",
            help = "Repeatable independent teamwork lane, for example frontend=agy:3"
        )]
        lanes: Vec<String>,
        #[arg(
            long = "max-parallel-agents",
            help = "Optional admission ceiling; defaults to the sum of configured lane agents"
        )]
        max_parallel_agents: Option<usize>,
        #[arg(
            long,
            requires = "worktree_root",
            help = "Git repository used to prepare one task-scoped worktree per external agent task"
        )]
        repository: Option<PathBuf>,
        #[arg(
            long = "worktree-root",
            requires = "repository",
            help = "Dedicated parent directory for the prepared teamwork worktrees"
        )]
        worktree_root: Option<PathBuf>,
        #[arg(long = "branch-prefix", default_value = "foundry/teamwork")]
        branch_prefix: String,
        #[arg(
            long = "allow-repository-mutation",
            help = "Explicitly authorize creation and task-scoped binding of the planned Git worktrees"
        )]
        allow_repository_mutation: bool,
        #[arg(short = 'd', long = "detached")]
        detached: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
        #[arg(long)]
        bypass_cache: bool,
    },
    Squad {
        #[command(subcommand)]
        command: SquadCommands,
    },
    Mission {
        #[command(subcommand)]
        command: MissionCommands,
    },
}

#[derive(Debug, Subcommand)]
enum StoreCommands {
    Check {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Backup {
        #[arg(long)]
        destination: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Restore {
        #[arg(long)]
        source: PathBuf,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long = "confirm-restore", default_value_t = false)]
        confirm_restore: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SquadCommands {
    Catalog {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Capabilities {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    InstallOriginals {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Validate {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Install {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inspect {
        id: String,
        #[arg(long)]
        version: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Clone {
        #[arg(long = "source")]
        source_id: String,
        #[arg(long = "source-version")]
        source_version: Option<String>,
        #[arg(long = "new-id")]
        new_id: String,
        #[arg(long = "new-name")]
        new_name: String,
        #[arg(long = "new-version")]
        new_version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum MissionCommands {
    Simulate {
        #[arg(long)]
        goal: String,
        #[arg(long, default_value = "software-factory")]
        squad: String,
        #[arg(long = "squad-version")]
        squad_version: Option<String>,
        #[arg(long = "without-rework", default_value_t = false)]
        without_rework: bool,
        #[arg(long)]
        worktree: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    SimulatePlatform {
        #[arg(long)]
        goal: String,
        #[arg(long, default_value = "software-factory")]
        squad: String,
        #[arg(long = "squad-version")]
        squad_version: Option<String>,
        #[arg(long)]
        worktree: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Start {
        #[arg(long)]
        goal: String,
        #[arg(long, default_value = "software-factory")]
        squad: String,
        #[arg(long = "squad-version")]
        squad_version: Option<String>,
        #[arg(long)]
        worktree: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Drive {
        id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Execute {
        id: String,
        #[arg(long = "task")]
        task_id: String,
        #[arg(long = "agent")]
        agent_id: String,
        #[arg(long = "idempotency-key")]
        idempotency_key: String,
        #[arg(long)]
        purpose: String,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long = "approval-ttl-seconds", default_value_t = 300)]
        approval_ttl_seconds: u64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "allow-trusted-process-runtime")]
        allow_trusted_process_runtime: bool,
        #[arg(long = "evidence")]
        requested_evidence: Vec<String>,
        #[arg(
            long = "command",
            required = true,
            num_args = 1,
            allow_hyphen_values = true
        )]
        command: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Execution {
        #[command(subcommand)]
        command: MissionExecutionCommands,
    },
    Submit {
        id: String,
        #[arg(long = "task")]
        task_id: String,
        #[arg(long = "agent")]
        agent_id: String,
        #[arg(long = "idempotency-key")]
        idempotency_key: String,
        #[arg(long = "receipt-id", alias = "receipt")]
        receipt_id: String,
        #[arg(long, default_value = "completed")]
        status: String,
        #[arg(long)]
        summary: String,
        #[arg(long = "artifact")]
        artifacts: Vec<String>,
        #[arg(long = "risk")]
        risks: Vec<String>,
        #[arg(long = "followup")]
        followups: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Resume {
        id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inspect {
        id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum MissionExecutionCommands {
    List {
        #[arg(long)]
        mission: Option<String>,
        #[arg(long)]
        task: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inspect {
        receipt_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Reconcile {
        receipt_id: String,
        #[arg(long)]
        outcome: String,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long)]
        reason: String,
        #[arg(long = "confirm-no-effect-retry")]
        confirm_no_effect_retry: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum CliCommands {
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        goal: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long = "command")]
        commands: Vec<String>,
        #[arg(long = "compound-command")]
        compound_commands: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SessionCommands {
    History {
        #[arg(long = "session")]
        session_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Lifecycle {
        #[arg(long = "session")]
        session_id: String,
        #[arg(long)]
        state: String,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum CostCommands {
    Ledger {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Materialize {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long = "source-kind")]
        source_kind: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Incremental {
        #[arg(long = "after-sequence")]
        after_sequence: Option<i64>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long = "source-kind")]
        source_kind: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    History {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long = "source-kind")]
        source_kind: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long, default_value = "day")]
        bucket: String,
        #[arg(long = "group-by", default_value = "none")]
        group_by: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Maintain {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long = "source-kind")]
        source_kind: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long, default_value = "day")]
        bucket: String,
        #[arg(long = "group-by", default_value = "none")]
        group_by: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "retention-days")]
        retention_days: Option<i64>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Daemon {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long = "source-kind")]
        source_kind: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long, default_value = "day")]
        bucket: String,
        #[arg(long = "group-by", default_value = "none")]
        group_by: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "retention-days")]
        retention_days: Option<i64>,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 300)]
        interval_seconds: u64,
        #[arg(long = "idle-exit")]
        idle_exit: bool,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Retention {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long = "source-kind")]
        source_kind: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long = "retention-days")]
        retention_days: Option<i64>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        apply: bool,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        confirm: bool,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    Install {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        target: Vec<String>,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long = "shim-dir")]
        shim_dirs: Vec<PathBuf>,
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
enum HarnessCommands {
    TokenHeadroom {
        #[arg(long)]
        content: String,
        #[arg(long = "kind")]
        content_kind: Option<String>,
        #[arg(long = "budget-tokens", default_value_t = 0)]
        budget_tokens: usize,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long, default_value_t = true)]
        reversible: bool,
        #[arg(long, default_value_t = false)]
        persist: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RetrieveHeadroom {
        #[arg(long = "ref")]
        retrieval_ref: String,
        #[arg(long = "include-content", default_value_t = false)]
        include_content: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    HeadroomStats {
        #[arg(long)]
        source: Option<String>,
        #[arg(long = "kind", alias = "content-kind")]
        content_kind: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Mode {
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only", conflicts_with = "foundry_first")]
        observe_only: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Doctor {
        #[arg(long)]
        executor: String,
        #[arg(long = "shim-dir")]
        shim_dir: PathBuf,
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only", conflicts_with = "foundry_first")]
        observe_only: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    HeadroomPlan {
        #[arg(long)]
        executor: String,
        #[arg(long = "cmd")]
        command: Vec<String>,
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only", conflicts_with = "foundry_first")]
        observe_only: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AdoptionPlan {
        #[arg(long)]
        executor: String,
        #[arg(long = "shim-dir")]
        shim_dir: PathBuf,
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only", conflicts_with = "foundry_first")]
        observe_only: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ActivationProfile {
        #[arg(long = "shim-dir")]
        shim_dir: PathBuf,
        #[arg(long)]
        executor: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "shell-rc")]
        shell_rc: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    WrapPlan {
        #[arg(long)]
        executor: String,
        #[arg(long = "cmd")]
        command: Vec<String>,
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only", conflicts_with = "foundry_first")]
        observe_only: bool,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Bootstrap {
        #[arg(long)]
        executor: String,
        #[arg(long = "shim-dir")]
        shim_dir: PathBuf,
        #[arg(long = "project-root")]
        project_root: PathBuf,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long)]
        apply: bool,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long, default_value_t = false)]
        force: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    InstallShims {
        #[arg(long = "shim-dir")]
        shim_dir: PathBuf,
        #[arg(long)]
        executor: String,
        #[arg(long = "real-cmd")]
        real_cmd: Option<String>,
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only", conflicts_with = "foundry_first")]
        observe_only: bool,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        force: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    InstallProviderAdapter {
        #[arg(long = "shim-dir")]
        shim_dir: PathBuf,
        #[arg(long)]
        executor: String,
        #[arg(long = "real-cmd")]
        real_cmd: Option<String>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long, default_value_t = false)]
        force: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ShimStatus {
        #[arg(long = "shim-dir")]
        shim_dir: PathBuf,
        #[arg(long)]
        executor: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Exec {
        #[arg(long)]
        executor: String,
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only", conflicts_with = "foundry_first")]
        observe_only: bool,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long = "no-token-headroom", conflicts_with = "token_headroom")]
        no_token_headroom: bool,
        #[arg(long = "execute", default_value_t = false)]
        execute: bool,
        #[arg(long = "allow-exec", default_value_t = false)]
        allow_exec: bool,
        #[arg(long = "secret-env")]
        secret_env: Vec<String>,
        #[arg(long = "secret-permission")]
        secret_permissions: Vec<String>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(last = true, num_args = 0.., allow_hyphen_values = true)]
        command: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum AddonCommands {
    Installed {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Capabilities {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        lifecycle: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Observability {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        lifecycle: Option<String>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long = "dispatch-limit", default_value_t = 1000)]
        dispatch_limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Permissions {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        permission: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AuthorizePermission {
        #[arg(long)]
        addon: String,
        #[arg(long)]
        permission: String,
        #[arg(long, default_value = "medium")]
        risk: String,
        #[arg(long = "approved-by", default_value = "human")]
        approved_by: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RevokePermission {
        #[arg(long)]
        addon: String,
        #[arg(long)]
        permission: String,
        #[arg(long = "approved-by", default_value = "human")]
        approved_by: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Catalog {
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Contracts {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long = "type")]
        contract_type: Option<String>,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        lifecycle: Option<String>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Planners {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long = "workflow-extension")]
        workflow_extension: Option<String>,
        #[arg(long)]
        lifecycle: Option<String>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ContractPolicy {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: Option<String>,
        #[arg(long = "type")]
        contract_type: Option<String>,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        lifecycle: Option<String>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Views {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        surface: Option<String>,
        #[arg(long)]
        lifecycle: Option<String>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    DispatchContract {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: String,
        #[arg(long, default_value = "{}")]
        input: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    DispatchPlanner {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: String,
        #[arg(long)]
        goal: String,
        #[arg(long = "constraint")]
        constraints: Vec<String>,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long, default_value = "{}")]
        context: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ExecutePlanner {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        goal: String,
        #[arg(long = "constraint")]
        constraints: Vec<String>,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long, default_value = "{}")]
        context: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ExecuteValidator {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        subject: String,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long, default_value = "{}")]
        input: String,
        #[arg(long, default_value = "{}")]
        context: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ApplyValidatorOutcome {
        #[arg(long = "dispatch")]
        dispatch_id: String,
        #[arg(long = "workflow")]
        workflow_id: String,
        #[arg(long = "task")]
        task_id: String,
        #[arg(long = "expected-revision")]
        expected_revision: u64,
        #[arg(long, default_value = "cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ExecuteExecutor {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: String,
        #[arg(long)]
        worker: String,
        #[arg(long = "task")]
        task_ref: String,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long, default_value = "{}")]
        input: String,
        #[arg(long, default_value = "{}")]
        context: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ExecuteHandoff {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: String,
        #[arg(long)]
        worker: String,
        #[arg(long = "handoff")]
        handoff_ref: String,
        #[arg(long, default_value = "{}")]
        input: String,
        #[arg(long, default_value = "{}")]
        context: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Dispatches {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        contract: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RunDispatch {
        #[arg(long)]
        dispatch: String,
        #[arg(long, default_value = "cli")]
        worker: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    DispatchWorker {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value = "cli")]
        worker: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ExecuteDispatch {
        #[arg(long)]
        dispatch: String,
        #[arg(long)]
        worker: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ClaimDispatch {
        #[arg(long)]
        dispatch: String,
        #[arg(long)]
        worker: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CompleteDispatch {
        #[arg(long)]
        dispatch: String,
        #[arg(long)]
        worker: String,
        #[arg(long, default_value = "completed")]
        status: String,
        #[arg(long, default_value = "{}")]
        result: String,
        #[arg(long)]
        signature: Option<String>,
        #[arg(long, default_value = "{}")]
        attestation: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RegisterWorker {
        #[arg(long)]
        worker: String,
        #[arg(long)]
        runtime: String,
        #[arg(long, default_value = "available")]
        status: String,
        #[arg(long = "trust-level", default_value = "local")]
        trust_level: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long, default_value = "{}")]
        data: String,
        #[arg(long = "rotation-approved-by")]
        rotation_approved_by: Option<String>,
        #[arg(long = "rotation-reason")]
        rotation_reason: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Workers {
        #[arg(long)]
        runtime: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long = "trust-level")]
        trust_level: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Resolve {
        #[arg(long)]
        goal: String,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long = "registry-source")]
        registry_sources: Vec<String>,
        #[arg(long = "registry-cache-dir")]
        registry_cache_dir: Option<PathBuf>,
        #[arg(long = "allow-remote-registry")]
        allow_remote_registry: bool,
        #[arg(long = "registry-max-bytes", default_value_t = 10 * 1024 * 1024)]
        registry_max_bytes: u64,
        #[arg(long = "registry-max-packages", default_value_t = 50)]
        registry_max_packages: usize,
        #[arg(long = "registry-lock")]
        registry_lock_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Validate {
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    LifecyclePlan {
        #[arg(long)]
        action: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long = "package", alias = "package-path")]
        package_path: Option<PathBuf>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Install {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Package {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, default_value = "stable")]
        channel: String,
        #[arg(long)]
        signature: Option<String>,
        #[arg(long = "public-key")]
        public_key: Option<String>,
        #[arg(long = "package-path")]
        package_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    TrustKey {
        #[arg(long)]
        repository: String,
        #[arg(long, default_value = "stable")]
        channel: String,
        #[arg(long = "public-key")]
        public_key: String,
        #[arg(long = "trust-level", default_value = "trusted")]
        trust_level: String,
        #[arg(long = "approved-by", default_value = "human")]
        approved_by: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long, default_value = "{}")]
        data: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    TrustStore {
        #[arg(long)]
        repository: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long = "public-key")]
        public_key: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    PublishPackage {
        #[arg(long = "package")]
        package_path: PathBuf,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    FetchPackage {
        #[arg(long)]
        source: String,
        #[arg(long = "cache-dir")]
        cache_dir: Option<PathBuf>,
        #[arg(long = "expected-sha256")]
        expected_sha256: Option<String>,
        #[arg(long = "lock")]
        lock_path: Option<PathBuf>,
        #[arg(long = "allow-remote")]
        allow_remote: bool,
        #[arg(long = "max-bytes", default_value_t = 10 * 1024 * 1024)]
        max_bytes: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    SyncRegistry {
        #[arg(long)]
        source: String,
        #[arg(long = "cache-dir")]
        cache_dir: Option<PathBuf>,
        #[arg(long = "lock")]
        lock_path: Option<PathBuf>,
        #[arg(long = "allow-remote")]
        allow_remote: bool,
        #[arg(long = "max-bytes", default_value_t = 10 * 1024 * 1024)]
        max_bytes: u64,
        #[arg(long = "max-packages", default_value_t = 50)]
        max_packages: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    PackageLock {
        #[arg(long)]
        repository: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        write: Option<PathBuf>,
        #[arg(long, default_value_t = 200)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Marketplace {
        #[arg(long)]
        repository: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    InstallPackage {
        #[arg(long = "package")]
        package_path: PathBuf,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long = "lock")]
        lock_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    MigrationWorkflow {
        #[arg(long = "from-manifest")]
        from_manifest: PathBuf,
        #[arg(long = "to-manifest")]
        to_manifest: PathBuf,
        #[arg(long, default_value = "upgrade")]
        action: String,
        #[arg(long, default_value = "cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Upgrade {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Downgrade {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Enable {
        id: String,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Disable {
        id: String,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Uninstall {
        id: String,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum EventCommands {
    List {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Timeline {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "after-sequence")]
        after_sequence: Option<i64>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Observability {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long)]
        node: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "after-sequence")]
        after_sequence: Option<i64>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ObservabilityHistory {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long)]
        node: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long, default_value = "day")]
        bucket: String,
        #[arg(long = "group-by", default_value = "none")]
        group_by: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "after-sequence")]
        after_sequence: Option<i64>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ImprovementPolicy {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        organization: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long)]
        node: Option<String>,
        #[arg(long)]
        addon: Option<String>,
        #[arg(long = "min-events")]
        min_events: Option<usize>,
        #[arg(long = "min-duration-ms")]
        min_duration_ms: Option<i64>,
        #[arg(long = "min-retries")]
        min_retries: Option<i64>,
        #[arg(long = "min-context-pressure-bps")]
        min_context_pressure_bps: Option<i64>,
        #[arg(long = "min-wait-seconds")]
        min_wait_seconds: Option<i64>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "after-sequence")]
        after_sequence: Option<i64>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Ingest {
        #[arg(long)]
        origin: String,
        #[arg(long)]
        action: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        input: Option<String>,
        #[arg(long = "input-file")]
        input_file: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inbox {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Scan {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long = "dispatch-activations")]
        dispatch_activations: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Worker {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 300)]
        interval_seconds: u64,
        #[arg(long = "idle-exit")]
        idle_exit: bool,
        #[arg(long = "dispatch-activations")]
        dispatch_activations: bool,
        #[arg(long = "stop-file")]
        stop_file: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ServicePlan {
        #[arg(long = "kind")]
        service_kind: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 300)]
        interval_seconds: u64,
        #[arg(long = "idle-exit")]
        idle_exit: bool,
        #[arg(long = "dispatch-activations")]
        dispatch_activations: bool,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value = "/webhook")]
        path: String,
        #[arg(long)]
        origin: Option<String>,
        #[arg(long)]
        action: Option<String>,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        route: bool,
        #[arg(long = "max-requests", default_value_t = 1)]
        max_requests: usize,
        #[arg(long = "max-body-bytes", default_value_t = 65_536)]
        max_body_bytes: usize,
        #[arg(long = "hmac-secret-env")]
        hmac_secret_env: Option<String>,
        #[arg(long = "signature-header", default_value = "X-Foundry-Signature")]
        signature_header: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long = "heartbeat-seconds", default_value_t = 60)]
        heartbeat_seconds: u64,
        #[arg(long = "backoff-initial-seconds", default_value_t = 5)]
        backoff_initial_seconds: u64,
        #[arg(long = "backoff-max-seconds", default_value_t = 300)]
        backoff_max_seconds: u64,
        #[arg(long = "shutdown-grace-seconds", default_value_t = 30)]
        shutdown_grace_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ServiceRun {
        #[arg(long = "kind", default_value = "worker")]
        service_kind: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 300)]
        interval_seconds: u64,
        #[arg(long = "idle-exit")]
        idle_exit: bool,
        #[arg(long = "dispatch-activations")]
        dispatch_activations: bool,
        #[arg(long = "stop-file")]
        stop_file: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value = "/webhook")]
        path: String,
        #[arg(long)]
        origin: Option<String>,
        #[arg(long)]
        action: Option<String>,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        route: bool,
        #[arg(long = "max-requests", default_value_t = 1)]
        max_requests: usize,
        #[arg(long = "max-body-bytes", default_value_t = 65_536)]
        max_body_bytes: usize,
        #[arg(long = "hmac-secret-env")]
        hmac_secret_env: Option<String>,
        #[arg(long = "signature-header", default_value = "X-Foundry-Signature")]
        signature_header: String,
        #[arg(long = "lease-owner", default_value = "foundry.event_service_manager")]
        lease_owner: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long = "heartbeat-seconds", default_value_t = 60)]
        heartbeat_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ServiceSupervise {
        #[arg(long = "kind", default_value = "worker")]
        service_kind: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 300)]
        interval_seconds: u64,
        #[arg(long = "idle-exit")]
        idle_exit: bool,
        #[arg(long = "dispatch-activations")]
        dispatch_activations: bool,
        #[arg(long = "stop-file")]
        stop_file: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value = "/webhook")]
        path: String,
        #[arg(long)]
        origin: Option<String>,
        #[arg(long)]
        action: Option<String>,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        route: bool,
        #[arg(long = "max-requests", default_value_t = 1)]
        max_requests: usize,
        #[arg(long = "max-body-bytes", default_value_t = 65_536)]
        max_body_bytes: usize,
        #[arg(long = "hmac-secret-env")]
        hmac_secret_env: Option<String>,
        #[arg(long = "signature-header", default_value = "X-Foundry-Signature")]
        signature_header: String,
        #[arg(
            long = "lease-owner",
            default_value = "foundry.event_service_supervisor"
        )]
        lease_owner: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long = "heartbeat-seconds", default_value_t = 60)]
        heartbeat_seconds: u64,
        #[arg(long = "max-runs", default_value_t = 1)]
        max_runs: usize,
        #[arg(long = "backoff-initial-seconds", default_value_t = 5)]
        backoff_initial_seconds: u64,
        #[arg(long = "backoff-max-seconds", default_value_t = 300)]
        backoff_max_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RuntimeReconcile {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "service-limit", default_value_t = 20)]
        service_limit: usize,
        #[arg(long)]
        execute: bool,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 300)]
        interval_seconds: u64,
        #[arg(long = "idle-exit")]
        idle_exit: bool,
        #[arg(long = "dispatch-activations")]
        dispatch_activations: bool,
        #[arg(long = "recover-stale-services")]
        recover_stale_services: bool,
        #[arg(long = "stop-file")]
        stop_file: Option<PathBuf>,
        #[arg(
            long = "lease-owner",
            default_value = "foundry.event_runtime_reconcile"
        )]
        lease_owner: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long = "heartbeat-seconds", default_value_t = 60)]
        heartbeat_seconds: u64,
        #[arg(long = "max-runs", default_value_t = 1)]
        max_runs: usize,
        #[arg(long = "backoff-initial-seconds", default_value_t = 5)]
        backoff_initial_seconds: u64,
        #[arg(long = "backoff-max-seconds", default_value_t = 300)]
        backoff_max_seconds: u64,
        #[arg(long = "scan-schedules")]
        scan_schedules: bool,
        #[arg(
            long = "schedule-executor",
            default_value = "foundry-runtime-scheduler"
        )]
        schedule_executor: String,
        #[arg(long = "schedule-max-workers", default_value_t = 1)]
        schedule_max_workers: usize,
        #[arg(long = "schedule-ttl-seconds", default_value_t = 300)]
        schedule_ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RuntimeDaemon {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "service-limit", default_value_t = 20)]
        service_limit: usize,
        #[arg(long)]
        execute: bool,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 300)]
        interval_seconds: u64,
        #[arg(long = "idle-exit")]
        idle_exit: bool,
        #[arg(long = "dispatch-activations")]
        dispatch_activations: bool,
        #[arg(long = "continuous")]
        continuous: bool,
        #[arg(long = "cycle-retention", default_value_t = 100)]
        cycle_retention: usize,
        #[arg(long = "recover-stale-services")]
        recover_stale_services: bool,
        #[arg(long = "stop-file")]
        stop_file: Option<PathBuf>,
        #[arg(long = "lease-owner", default_value = "foundry.event_runtime_daemon")]
        lease_owner: String,
        #[arg(long = "lease-seconds", default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long = "heartbeat-seconds", default_value_t = 60)]
        heartbeat_seconds: u64,
        #[arg(long = "max-runs", default_value_t = 1)]
        max_runs: usize,
        #[arg(long = "backoff-initial-seconds", default_value_t = 5)]
        backoff_initial_seconds: u64,
        #[arg(long = "backoff-max-seconds", default_value_t = 300)]
        backoff_max_seconds: u64,
        #[arg(long = "scan-schedules")]
        scan_schedules: bool,
        #[arg(
            long = "schedule-executor",
            default_value = "foundry-runtime-scheduler"
        )]
        schedule_executor: String,
        #[arg(long = "schedule-max-workers", default_value_t = 1)]
        schedule_max_workers: usize,
        #[arg(long = "schedule-ttl-seconds", default_value_t = 300)]
        schedule_ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Services {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long = "kind")]
        service_kind: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ServicesRecover {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long = "kind")]
        service_kind: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    WebhookIngress {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value = "/webhook")]
        path: String,
        #[arg(long)]
        origin: String,
        #[arg(long)]
        action: String,
        #[arg(long, default_value = "webhook")]
        transport: String,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        route: bool,
        #[arg(long = "max-requests", default_value_t = 1)]
        max_requests: usize,
        #[arg(long = "max-body-bytes", default_value_t = 65_536)]
        max_body_bytes: usize,
        #[arg(long = "hmac-secret-env")]
        hmac_secret_env: Option<String>,
        #[arg(long = "signature-header", default_value = "X-Foundry-Signature")]
        signature_header: String,
        #[arg(long = "stop-file")]
        stop_file: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Adapters {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long)]
        transport: Option<String>,
        #[arg(long)]
        direction: Option<String>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Emit {
        #[arg(long)]
        addon: Option<String>,
        #[arg(long = "adapter")]
        adapter_id: String,
        #[arg(long = "event-type")]
        event_type: String,
        #[arg(long)]
        action: String,
        #[arg(long, default_value = "foundry")]
        origin: String,
        #[arg(long, default_value = "{}")]
        payload: String,
        #[arg(long = "dry-run")]
        dry_run: bool,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Route {
        #[arg(long = "event")]
        event_id: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    DispatchActivations {
        #[arg(long = "event")]
        event_id: String,
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long = "dry-run")]
        dry_run: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum IdentityCommands {
    Context {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Sync {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Registry {
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Memberships {
        #[arg(long = "subject-scope")]
        subject_scope: Option<String>,
        #[arg(long = "subject")]
        subject_id: Option<String>,
        #[arg(long = "organization")]
        organization_id: Option<String>,
        #[arg(long = "brand")]
        brand_id: Option<String>,
        #[arg(long = "product")]
        product_id: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    MembershipUpdate {
        #[arg(long = "subject-scope", default_value = "user")]
        subject_scope: String,
        #[arg(long = "subject")]
        subject_id: String,
        #[arg(long = "organization")]
        organization_id: String,
        #[arg(long = "brand")]
        brand_id: String,
        #[arg(long = "product")]
        product_id: String,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long = "grant")]
        grant_permissions: Vec<String>,
        #[arg(long = "revoke-grant")]
        revoke_grants: Vec<String>,
        #[arg(long = "deny")]
        deny_permissions: Vec<String>,
        #[arg(long = "remove-deny")]
        remove_denies: Vec<String>,
        #[arg(long = "expires-at")]
        expires_at: Option<String>,
        #[arg(long = "clear-expires-at")]
        clear_expires_at: bool,
        #[arg(long = "not-before")]
        not_before: Option<String>,
        #[arg(long = "clear-not-before")]
        clear_not_before: bool,
        #[arg(long, default_value = "foundry_cli")]
        source: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Link {
        #[arg(long = "left-scope")]
        left_scope: String,
        #[arg(long = "left-id")]
        left_id: String,
        #[arg(long = "right-scope")]
        right_scope: String,
        #[arg(long = "right-id")]
        right_id: String,
        #[arg(long = "type", default_value = "same_person")]
        link_type: String,
        #[arg(long, default_value = "foundry_cli")]
        source: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Unlink {
        #[arg(long = "left-scope")]
        left_scope: String,
        #[arg(long = "left-id")]
        left_id: String,
        #[arg(long = "right-scope")]
        right_scope: String,
        #[arg(long = "right-id")]
        right_id: String,
        #[arg(long = "type", default_value = "same_person")]
        link_type: String,
        #[arg(long, default_value = "foundry_cli")]
        source: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Links {
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Resolve {
        #[arg(long)]
        scope: String,
        #[arg(long)]
        id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    TenantIndex {
        #[arg(long = "resource-type")]
        resource_type: Option<String>,
        #[arg(long = "organization")]
        organization_id: Option<String>,
        #[arg(long = "brand")]
        brand_id: Option<String>,
        #[arg(long = "product")]
        product_id: Option<String>,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    TenantAudit {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    TenantPolicy {
        #[arg(long = "workflow")]
        workflow_id: String,
        #[arg(long, default_value = "audit")]
        mode: String,
        #[arg(long, default_value = "tenant policy")]
        action: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum OpsCommands {
    Snapshot {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8765)]
        port: u16,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
    },
    RendererEvent {
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long = "workflow")]
        workflow_id: String,
        #[arg(long = "addon")]
        addon_id: Option<String>,
        #[arg(long = "view")]
        view_id: String,
        #[arg(long = "event-kind")]
        event_kind: String,
        #[arg(long, default_value = "foundry-cli")]
        actor: String,
        #[arg(long)]
        payload: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SyncCommands {
    Executors {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long = "shim-dir")]
        shim_dirs: Vec<PathBuf>,
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
        #[arg(long)]
        home: Option<PathBuf>,
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
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long = "shim-dir")]
        shim_dirs: Vec<PathBuf>,
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

fn default_home_path() -> PathBuf {
    foundry_core::brand::env_var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Debug, Subcommand)]
enum ExecutorQuotaCommands {
    AiLimits {
        #[arg(long = "ai-limits-cmd", default_value = "ai-limits")]
        ai_limits_cmd: PathBuf,
        #[arg(long = "timeout-ms", default_value_t = 3000)]
        timeout_ms: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Record(Box<ExecutorQuotaRecordArgs>),
    Decide(Box<ExecutorModelDecisionArgs>),
}

#[derive(Debug, Args)]
struct ExecutorModelDecisionArgs {
    #[arg(long)]
    task: String,
    #[arg(long = "task-class", default_value = "general_reasoning")]
    task_class: String,
    #[arg(long, default_value = "medium")]
    difficulty: String,
    #[arg(long = "expected-input-tokens", default_value_t = 4000)]
    expected_input_tokens: u64,
    #[arg(long = "expected-output-tokens", default_value_t = 1000)]
    expected_output_tokens: u64,
    #[arg(long = "configured-decider")]
    configured_decider: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    output: OutputFormat,
}

#[derive(Debug, Args)]
struct ExecutorQuotaRecordArgs {
    #[arg(long)]
    executor: String,
    #[arg(long)]
    provider: String,
    #[arg(long)]
    model: String,
    #[arg(long)]
    locality: String,
    #[arg(long = "free-vs-paid")]
    free_vs_paid: String,
    #[arg(long = "remaining-quota")]
    remaining_quota: String,
    #[arg(long = "rate-limit-risk")]
    rate_limit_risk: String,
    #[arg(long = "cost")]
    monetary_or_token_cost: String,
    #[arg(long)]
    latency: String,
    #[arg(long = "expected-quality")]
    expected_quality: String,
    #[arg(long)]
    suitability: String,
    #[arg(long)]
    source: String,
    #[arg(long = "observed-at")]
    observed_at: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    output: OutputFormat,
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
enum WorktreeCommands {
    Discover {
        #[arg(long, default_value = ".")]
        repository: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Create {
        #[arg(long, default_value = ".")]
        repository: PathBuf,
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        branch: String,
        #[arg(long = "start-point")]
        start_point: Option<String>,
        #[arg(long = "allow-repository-mutation", default_value_t = false)]
        allow_repository_mutation: bool,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Register {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long, requires = "workflow")]
        task: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Bind {
        #[arg(long)]
        worktree: String,
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Init {
        #[arg(long)]
        worktree: String,
        #[arg(long = "allow-worktree-write", default_value_t = false)]
        allow_worktree_write: bool,
        #[arg(long, default_value_t = false)]
        force: bool,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ApproveConfig {
        #[arg(long)]
        worktree: String,
        #[arg(long = "allow-guardrail-update", default_value_t = false)]
        allow_guardrail_update: bool,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        #[arg(long)]
        repository: Option<PathBuf>,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inspect {
        #[arg(long)]
        worktree: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    PrepareTeamwork {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        repository: PathBuf,
        #[arg(long = "worktree-root")]
        worktree_root: PathBuf,
        #[arg(long = "branch-prefix", default_value = "foundry/teamwork")]
        branch_prefix: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "allow-repository-mutation", default_value_t = false)]
        allow_repository_mutation: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    IntegrateDependencies {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "allow-repository-mutation", default_value_t = false)]
        allow_repository_mutation: bool,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Sandbox {
        #[command(subcommand)]
        command: WorktreeSandboxCommands,
    },
    Guard {
        #[command(subcommand)]
        command: WorktreeGuardCommands,
    },
}

#[derive(Debug, Subcommand)]
enum WorktreeGuardCommands {
    Check {
        #[arg(long)]
        worktree: String,
        #[arg(long, default_value = "modify", value_parser = ["modify"])]
        operation: String,
        #[arg(long = "path", required = true)]
        paths: Vec<String>,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long, requires = "workflow")]
        task: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CreatePredecessor {
        #[arg(long)]
        worktree: String,
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "path", required = true)]
        paths: Vec<String>,
        #[arg(long)]
        goal: String,
        #[arg(long = "allow-workflow-mutation", default_value_t = false)]
        allow_workflow_mutation: bool,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum WorktreeSandboxCommands {
    Plan {
        #[arg(long)]
        worktree: String,
        #[arg(long, value_parser = ["preview", "test"])]
        purpose: String,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long, requires = "workflow")]
        task: Option<String>,
        #[arg(last = true)]
        command: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Run {
        #[arg(long)]
        worktree: String,
        #[arg(long, value_parser = ["preview", "test"])]
        purpose: String,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long, requires = "workflow")]
        task: Option<String>,
        #[arg(long = "allow-exec", default_value_t = false)]
        allow_exec: bool,
        #[arg(last = true)]
        command: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Start {
        #[arg(long)]
        worktree: String,
        #[arg(long, value_parser = ["preview", "test"])]
        purpose: String,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long, requires = "workflow")]
        task: Option<String>,
        #[arg(long = "allow-exec", default_value_t = false)]
        allow_exec: bool,
        #[arg(last = true)]
        command: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Status {
        #[arg(long)]
        sandbox: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Stop {
        #[arg(long)]
        sandbox: String,
        #[arg(long = "allow-stop", default_value_t = false)]
        allow_stop: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(hide = true)]
    Supervise {
        #[arg(long)]
        sandbox: String,
        #[arg(long = "allow-supervisor-exec", default_value_t = false)]
        allow_supervisor_exec: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SecurityCommands {
    SecretScan {
        #[arg(long, conflicts_with = "input_file")]
        input: Option<String>,
        #[arg(long = "input-file", conflicts_with = "input")]
        input_file: Option<PathBuf>,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long = "no-entropy", default_value_t = false)]
        no_entropy: bool,
        #[arg(long = "no-local-ai-fallback", default_value_t = false)]
        no_local_ai_fallback: bool,
        #[arg(long = "allow-external-ai", default_value_t = false)]
        allow_external_ai: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum CredentialVaultCommands {
    KeyInit {
        #[arg(long = "vault-bin")]
        vault_bin: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Describe {
        #[arg(long = "vault-bin")]
        vault_bin: Option<PathBuf>,
        #[arg(long)]
        contract: PathBuf,
        #[arg(long)]
        data: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Records {
        #[arg(long = "vault-bin")]
        vault_bin: Option<PathBuf>,
        #[arg(long)]
        contract: PathBuf,
        #[arg(long)]
        data: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Panel {
        #[arg(long = "vault-bin")]
        vault_bin: Option<PathBuf>,
        #[arg(long)]
        contract: PathBuf,
        #[arg(long)]
        data: PathBuf,
        #[arg(long)]
        open: bool,
        #[arg(long = "timeout-seconds")]
        timeout_seconds: Option<u64>,
        #[arg(long = "no-cli-fallback")]
        no_cli_fallback: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Exec {
        #[arg(long = "vault-bin")]
        vault_bin: Option<PathBuf>,
        #[arg(long)]
        contract: PathBuf,
        #[arg(long)]
        data: PathBuf,
        #[arg(long)]
        record: String,
        #[arg(long = "env")]
        env_mappings: Vec<String>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum AwsCommands {
    Check {
        #[arg(long = "aws-ops-bin")]
        aws_ops_bin: Option<PathBuf>,
        #[arg(long = "vault-contract")]
        vault_contract: Option<PathBuf>,
        #[arg(long = "vault-data")]
        vault_data: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inventory {
        #[arg(long = "aws-ops-bin")]
        aws_ops_bin: Option<PathBuf>,
        #[arg(long = "vault-contract")]
        vault_contract: Option<PathBuf>,
        #[arg(long = "vault-data")]
        vault_data: Option<PathBuf>,
        #[arg(long)]
        regions: Option<String>,
        #[arg(long = "all-regions")]
        all_regions: bool,
        #[arg(long)]
        full: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Raw {
        #[arg(long = "aws-ops-bin")]
        aws_ops_bin: Option<PathBuf>,
        #[arg(long = "vault-contract")]
        vault_contract: Option<PathBuf>,
        #[arg(long = "vault-data")]
        vault_data: Option<PathBuf>,
        #[arg(long = "allow-mutation")]
        allow_mutation: bool,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
        #[arg(last = true, required = true)]
        aws_args: Vec<String>,
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
        #[arg(long, default_value = "foundry_cli")]
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
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Pause {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Resume {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Stop {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "foundry_cli")]
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
        #[arg(long, default_value = "foundry-scheduler")]
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
        #[arg(long, default_value = "foundry-scheduler")]
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
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AddTask {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        description: String,
        #[arg(long, default_value = "medium")]
        priority: String,
        #[arg(long = "task-id")]
        task_id: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    UpdateTask {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long = "expected-output")]
        expected_output: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    SetPriority {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        priority: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AddDependency {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "depends-on")]
        depends_on: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RemoveDependency {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "depends-on")]
        depends_on: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    SetImpediment {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = "manual")]
        kind: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ClearImpediment {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(
            long = "impediment",
            help = "Impediment id to clear; omit to clear manual impediments only"
        )]
        impediment_id: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "expected-revision")]
        expected_revision: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    UpdateNodeBrain {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "default-brain")]
        default_brain: Option<String>,
        #[arg(long = "allowed-brain")]
        allowed_brains: Vec<String>,
        #[arg(long = "agent-slot")]
        agent_slots: Vec<String>,
        #[arg(long = "max-parallel-agents")]
        max_parallel_agents: Option<usize>,
        #[arg(long, default_value = "foundry_cli")]
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
        #[arg(long = "tag")]
        tags: Vec<String>,
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
    Decision {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        rationale: String,
        #[arg(long = "alternative")]
        alternatives: Vec<String>,
        #[arg(long = "trade-off")]
        trade_offs: Vec<String>,
        #[arg(long = "success-metric")]
        success_metrics: Vec<String>,
        #[arg(long = "backlog-mutation", default_value = "none")]
        backlog_mutation: String,
        #[arg(long, default_value = "human")]
        author: String,
        #[arg(long = "affected-goal")]
        affected_goals: Vec<String>,
        #[arg(long = "affected-task")]
        affected_tasks: Vec<String>,
        #[arg(long = "affected-artifact")]
        affected_artifacts: Vec<String>,
        #[arg(long, default_value = "foundry_cli")]
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
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, default_value_t = 1200)]
        budget: usize,
        #[arg(long, default_value_t = 900)]
        ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = ContextViewArg::Compact)]
        view: ContextViewArg,
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
        #[arg(long)]
        worktree: Option<String>,
        #[arg(
            long = "lane",
            value_name = "ID=EXECUTOR:COUNT",
            help = "Declare an independent parallel lane; repeat for multiple frontend/backend teams"
        )]
        lanes: Vec<String>,
        #[arg(
            long = "max-parallel-agents",
            requires = "lanes",
            help = "Bound total concurrent agents across the declared lanes"
        )]
        max_parallel_agents: Option<usize>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "idempotency-key")]
        idempotency_key: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
        #[arg(short = 'd', long = "detached")]
        detached: bool,
    },
    Status {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Drive {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "foundry_cli")]
        executor: String,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Step {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "foundry_cli")]
        executor: String,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ExecuteWave {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "auto")]
        executor: String,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long = "timeout-seconds", default_value_t = 1800)]
        timeout_seconds: u64,
        #[arg(long = "context-budget", default_value_t = DEFAULT_CONTEXT_BUDGET)]
        context_budget: usize,
        #[arg(
            long = "max-parallel",
            help = "Optional execution ceiling; the worker count is capped by admitted assignments"
        )]
        max_parallel: Option<usize>,
        #[arg(
            long = "allow-exec",
            help = "Explicitly authorize Foundry to start the admitted Codex/Agy processes"
        )]
        allow_exec: bool,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(
            long,
            default_value = "execute the dependency-ready, task-worktree-bound dispatch wave"
        )]
        reason: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CompleteTask {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "foundry_cli")]
        executor: String,
        #[arg(long)]
        summary: String,
        #[arg(long = "artifact")]
        artifacts: Vec<PathBuf>,
        #[arg(long = "evidence-command")]
        evidence_command: Option<String>,
        #[arg(long = "evidence-exit-code")]
        evidence_exit_code: Option<i32>,
        #[arg(long = "evidence-summary")]
        evidence_summary: Option<String>,
        #[arg(long = "estimated-usd", default_value_t = 0.0)]
        estimated_usd: f64,
        #[arg(long = "tokens-in", default_value_t = 0)]
        tokens_in: i64,
        #[arg(long = "tokens-out", default_value_t = 0)]
        tokens_out: i64,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long = "budget")]
        context_budget: Option<usize>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    FinalPackage {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    EnsureFinalAudit {
        #[arg(long)]
        workflow: String,
        #[arg(long, default_value = "foundry_cli")]
        executor: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
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
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Resume {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Heartbeat {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "foundry_cli")]
        executor: String,
        #[arg(long, default_value = "executor heartbeat")]
        summary: String,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    SwitchExecutor {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long)]
        executor: String,
        #[arg(long = "fallback-executor")]
        fallback_executors: Vec<String>,
        #[arg(long, default_value = "executor hot swap")]
        summary: String,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long, default_value = "executor limit or availability changed")]
        reason: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RecoverStale {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    DriveLoop {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, default_value = "foundry_cli")]
        executor: String,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long, default_value = "background_driver")]
        origin: String,
    },
    Supervise {
        #[arg(long, default_value = "foundry-request-supervisor")]
        executor: String,
        #[arg(long, default_value = "foundry-request-supervisor")]
        origin: String,
        #[arg(long = "ttl-seconds", default_value_t = 300)]
        ttl_seconds: u64,
        #[arg(long = "max-steps-per-run", default_value_t = 1)]
        max_steps_per_run: usize,
        #[arg(long)]
        continuous: bool,
        #[arg(long = "max-cycles", default_value_t = 1)]
        max_cycles: usize,
        #[arg(long = "interval-seconds", default_value_t = 30)]
        interval_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum McpCommands {
    Serve,
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
enum MemoryCommands {
    Policy {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Configure {
        #[arg(long = "project-root")]
        project_root: PathBuf,
        #[arg(long = "memory-level")]
        memory_level: String,
        #[arg(long = "default-scope")]
        default_scopes: Vec<String>,
        #[arg(long = "default-audience")]
        default_audience: String,
        #[arg(long = "privacy-mode")]
        privacy_mode: String,
        #[arg(long = "retention-mode")]
        retention_mode: String,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long)]
        reason: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Search {
        #[arg(long)]
        query: String,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "scope")]
        scopes: Vec<String>,
        #[arg(long)]
        audience: Option<String>,
        #[arg(long)]
        visibility: Option<String>,
        #[arg(long = "memory-level")]
        memory_level: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "organization")]
        organization_id: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long = "global-root")]
        global_root: Option<PathBuf>,
        #[arg(long = "organization-root")]
        organization_root: Option<PathBuf>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "processing-root")]
        processing_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Promote {
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "from-scope")]
        from_scope: String,
        #[arg(long = "to-scope")]
        to_scope: String,
        #[arg(long = "source-path")]
        source_path: PathBuf,
        #[arg(long = "source-start-line")]
        source_start_line: Option<usize>,
        #[arg(long = "source-end-line")]
        source_end_line: Option<usize>,
        #[arg(long)]
        summary: String,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = "internal")]
        visibility: String,
        #[arg(long)]
        shareability: Option<String>,
        #[arg(long = "organization")]
        organization_id: Option<String>,
        #[arg(long = "global-root")]
        global_root: Option<PathBuf>,
        #[arg(long = "organization-root")]
        organization_root: Option<PathBuf>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Promotions {
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "from-scope")]
        from_scope: Option<String>,
        #[arg(long = "to-scope")]
        to_scope: Option<String>,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Retention {
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "scope")]
        scopes: Vec<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "organization")]
        organization_id: Option<String>,
        #[arg(long = "global-root")]
        global_root: Option<PathBuf>,
        #[arg(long = "organization-root")]
        organization_root: Option<PathBuf>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "processing-root")]
        processing_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Cleanup {
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "scope")]
        scopes: Vec<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "organization")]
        organization_id: Option<String>,
        #[arg(long = "global-root")]
        global_root: Option<PathBuf>,
        #[arg(long = "organization-root")]
        organization_root: Option<PathBuf>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "processing-root")]
        processing_root: Option<PathBuf>,
        #[arg(long, default_value = "archive")]
        mode: String,
        #[arg(long = "archive-root")]
        archive_root: Option<PathBuf>,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        confirm: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum ImproveCommands {
    Candidates {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long = "workflow")]
        workflows: Vec<String>,
        #[arg(long = "goal-contains")]
        goal_contains: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    NormalizeCost {
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ApplyEventPolicy {
        #[arg(long)]
        workflow: String,
        #[arg(long = "recommendation")]
        recommendation_id: Option<String>,
        #[arg(long = "policy")]
        recommended_policy: Option<String>,
        #[arg(long)]
        apply: bool,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    BenchmarkEventPolicy {
        #[arg(long)]
        workflow: String,
        #[arg(long = "recommendation")]
        recommendation_id: Option<String>,
        #[arg(long = "policy")]
        recommended_policy: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    PromoteEventPolicy {
        #[arg(long)]
        workflow: String,
        #[arg(long = "recommendation")]
        recommendation_id: Option<String>,
        #[arg(long = "policy")]
        recommended_policy: Option<String>,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum InteractiveCommands {
    Home {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    GuidedCockpit {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    UiComposition {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Readiness {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ReplacementCli {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    MultimodalRuntime {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AddonCapabilities {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CoreBoundary {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Architecture {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    OperationalCockpit {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ImprovementLoop {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ReleaseGates {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Harness {
        #[arg(long, default_value = "codex")]
        executor: String,
        #[arg(long = "shim-dir")]
        shim_dir: Option<PathBuf>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        // foundry-brand-allow: legacy-compat
        #[arg(long = "foundry-first", visible_alias = "forge-first")]
        foundry_first: bool,
        #[arg(long = "observe-only")]
        observe_only: bool,
        #[arg(long = "workflow")]
        workflow_id: Option<String>,
        #[arg(long = "task")]
        task_id: Option<String>,
        #[arg(long = "run")]
        run_id: Option<String>,
        #[arg(long = "context-budget")]
        context_budget: Option<usize>,
        #[arg(long = "token-headroom")]
        token_headroom: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Sessions {
        #[arg(long = "provider")]
        provider_id: Option<String>,
        #[arg(long = "state")]
        lifecycle_state: Option<String>,
        #[arg(long)]
        readiness: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    CommandPalette {
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ActionRegistry {
        #[arg(long)]
        query: Option<String>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ActionInvocation {
        #[arg(long = "action")]
        action_id: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ActionDispatch {
        #[arg(long = "action")]
        action_id: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, default_value = "{}")]
        payload: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Autocomplete {
        #[arg(long)]
        input: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    PatchWorkbench {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Permissions {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Identity {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    TaskBoard {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    WorkflowMutation {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    WorkflowSidebar {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Artifacts {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    TokenUsage {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    WorkflowDag {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Schedules {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    EventRuntime {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ContextMemory {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    OperatingContext {
        #[arg(long = "project-root", default_value = ".")]
        project_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    StructuredLogs {
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
        #[arg(long = "context")]
        context: Vec<String>,
        #[arg(long, default_value = "foundry_cli")]
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
        #[arg(long, default_value = "foundry_cli")]
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
        #[arg(long, default_value = "foundry_cli")]
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
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Expire {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "foundry_cli")]
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
enum SmokeCommands {
    OperationalTui {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, default_value = "foundry_smoke")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    FoundryFirstHarness {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, default_value = "codex")]
        executor: String,
        #[arg(long = "real-cmd")]
        real_cmd: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    ReplacementCliEvidence {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, default_value = "arthur")]
        approved_by: String,
        #[arg(long, default_value = "codex")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    MultimodalRuntimeEvidence {
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "connected-runtime")]
        connected_runtime: Option<String>,
        #[arg(long, default_value = "arthur")]
        approved_by: String,
        #[arg(long, default_value = "codex")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum MilestoneCommands {
    Status {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Manifest {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "production-plan")]
    ProductionPlan {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "production-evidence-template")]
    ProductionEvidenceTemplate {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "release-version")]
        release_version: String,
        #[arg(long = "evidence-root")]
        evidence_root: PathBuf,
        #[arg(long)]
        template: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "production-evidence-assemble")]
    ProductionEvidenceAssemble {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "release-version")]
        release_version: String,
        #[arg(long = "evidence-root")]
        evidence_root: PathBuf,
        #[arg(long)]
        draft: PathBuf,
        #[arg(long = "receipt-dir", default_value = "receipts")]
        receipt_directory: PathBuf,
        #[arg(long, default_value = "production-readiness.json")]
        manifest: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "production-mission-evidence")]
    ProductionMissionEvidence {
        #[arg(long)]
        mission: String,
        #[arg(long)]
        receipt: String,
        #[arg(long = "evidence-root")]
        evidence_root: PathBuf,
        #[arg(long, default_value = "mission-operational-lifecycle.json")]
        artifact: PathBuf,
        #[arg(long = "release-version", default_value = env!("CARGO_PKG_VERSION"))]
        release_version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "production-readiness")]
    ProductionReadiness {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long = "evidence-root")]
        evidence_root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "attach-evidence")]
    AttachEvidence {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "capability", alias = "capability-id")]
        capability_id: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "evidence-plan")]
    EvidencePlan {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "capability", alias = "capability-id")]
        capability_id: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "connected-brain")]
        connected_brain: Option<String>,
        #[arg(long = "connected-runtime")]
        connected_runtime: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "prepare-evidence-inputs")]
    PrepareEvidenceInputs {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "capability", alias = "capability-id")]
        capability_id: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "connected-brain")]
        connected_brain: Option<String>,
        #[arg(long = "connected-runtime")]
        connected_runtime: Option<String>,
        #[arg(long = "provider-command")]
        provider_command: Option<PathBuf>,
        #[arg(long = "model-id")]
        model_id: Option<String>,
        #[arg(long = "approval-ref")]
        approval_ref: Option<String>,
        #[arg(long)]
        apply: bool,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "collect-evidence")]
    CollectEvidence {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "capability", alias = "capability-id")]
        capability_id: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "connected-brain")]
        connected_brain: Option<String>,
        #[arg(long = "connected-runtime")]
        connected_runtime: Option<String>,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "collect-ready-evidence")]
    CollectReadyEvidence {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "connected-brain")]
        connected_brain: Option<String>,
        #[arg(long = "connected-runtime")]
        connected_runtime: Option<String>,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Research {
        #[arg(long, default_value = "0.6")]
        version: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "export-demo")]
    ExportDemo {
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    #[command(name = "cli-demo")]
    CliDemo {
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "connected-brain")]
        connected_brain: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum MultimodalCommands {
    Status {
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    InstallPlan {
        #[arg(long)]
        capability: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Readiness {
        #[arg(long)]
        capability: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long)]
        allow: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    BenchmarkTemplate {
        #[arg(long)]
        capability: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    BenchmarkResult {
        #[arg(long)]
        capability: String,
        #[arg(long)]
        fixture: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long = "confirm-fixture-only")]
        confirm_fixture_only: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    RuntimeBenchmark {
        #[arg(long)]
        capability: String,
        #[arg(long)]
        fixture: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long = "confirm-runtime-execution")]
        confirm_runtime_execution: bool,
        #[arg(long = "allow-model")]
        allow_model: bool,
        #[arg(long = "connected-runtime")]
        connected_runtime: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    DemoPlan {
        #[arg(long)]
        demo: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    DemoReceipt {
        #[arg(long)]
        demo: String,
        #[arg(long)]
        fixture: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long = "approved-by")]
        approved_by: Option<String>,
        #[arg(long = "confirm-local-fixture")]
        confirm_local_fixture: bool,
        #[arg(long = "allow-model")]
        allow_model: bool,
        #[arg(long = "allow-camera")]
        allow_camera: bool,
        #[arg(long = "allow-microphone")]
        allow_microphone: bool,
        #[arg(long = "allow-screen")]
        allow_screen: bool,
        #[arg(long = "allow-input")]
        allow_input: bool,
        #[arg(long = "allow-filesystem")]
        allow_filesystem: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Guard {
        #[arg(long)]
        capability: String,
        #[arg(long)]
        action: String,
        #[arg(long = "enable-experimental")]
        enable_experimental: bool,
        #[arg(long = "project-root")]
        project_root: Option<PathBuf>,
        #[arg(long)]
        allow: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum PatchCommands {
    Plan {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        intent: String,
        #[arg(long = "path")]
        paths: Vec<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Apply {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "path")]
        paths: Vec<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "plan-artifact")]
        plan_artifact: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Review {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "path")]
        paths: Vec<String>,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long = "plan-artifact")]
        plan_artifact: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Diff {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "path")]
        paths: Vec<String>,
        #[arg(long = "file-index", default_value_t = 0)]
        file_index: usize,
        #[arg(long = "hunk-index", default_value_t = 0)]
        hunk_index: usize,
        #[arg(long = "context-lines", default_value_t = 3)]
        context_lines: usize,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Revert {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "apply-artifact")]
        apply_artifact: String,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Restore {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long = "revert-artifact")]
        revert_artifact: String,
        #[arg(long = "approved-by")]
        approved_by: String,
        #[arg(long = "confirm-restore")]
        confirm_restore: bool,
        #[arg(long, default_value = "foundry_cli")]
        origin: String,
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
        #[arg(long, default_value_t = 180)]
        sleep_seconds: u64,
        #[arg(long = "executor")]
        executors: Vec<String>,
        #[arg(long = "fallback-executor")]
        fallback_executors: Vec<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long = "validation-command")]
        validation_commands: Vec<String>,
        #[arg(long, default_value = "balanced")]
        mode: String,
        #[arg(long)]
        skip_self_update: bool,
        #[arg(long = "self-update-command")]
        self_update_command: Option<String>,
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

#[derive(Debug, Serialize)]
struct CliErrorEnvelope {
    schema_version: &'static str,
    status: &'static str,
    error: CliErrorDetail,
}

#[derive(Debug, Serialize)]
struct CliErrorDetail {
    code: &'static str,
    category: &'static str,
    message: String,
    retryable: bool,
    remediation: &'static str,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ContextViewArg {
    Full,
    Compact,
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

#[cfg(not(windows))]
fn spawn_detached_drive_loop(executable: &Path, store_path: &Path, run_id: &str) -> Result<()> {
    std::process::Command::new(executable)
        .arg("--store")
        .arg(store_path)
        .arg("request")
        .arg("drive-loop")
        .arg("--run")
        .arg(run_id)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(windows)]
fn spawn_detached_drive_loop(executable: &Path, store_path: &Path, run_id: &str) -> Result<()> {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS,
        PROCESS_INFORMATION, STARTUPINFOW,
    };

    let arguments = [
        executable.as_os_str().to_os_string(),
        OsString::from("--store"),
        store_path.as_os_str().to_os_string(),
        OsString::from("request"),
        OsString::from("drive-loop"),
        OsString::from("--run"),
        OsString::from(run_id),
    ];
    let mut command_line = Vec::<u16>::new();
    for (index, argument) in arguments.iter().enumerate() {
        if index > 0 {
            command_line.push(b' ' as u16);
        }
        append_windows_quoted_argument(&mut command_line, argument);
    }
    command_line.push(0);
    let mut application = executable.as_os_str().encode_wide().collect::<Vec<_>>();
    application.push(0);
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            0,
            DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
            null(),
            null(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        return Err(std::io::Error::last_os_error())
            .context("failed to spawn detached request drive loop");
    }
    unsafe {
        let _ = CloseHandle(process.hThread);
        let _ = CloseHandle(process.hProcess);
    }
    fn append_windows_quoted_argument(command_line: &mut Vec<u16>, argument: &OsStr) {
        command_line.push(b'"' as u16);
        let mut backslashes = 0usize;
        for unit in argument.encode_wide() {
            if unit == b'\\' as u16 {
                backslashes += 1;
                continue;
            }
            if unit == b'"' as u16 {
                command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
            } else {
                command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
            }
            backslashes = 0;
            command_line.push(unit);
        }
        command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
        command_line.push(b'"' as u16);
    }

    Ok(())
}

fn main() {
    if legacy_binary_invoked() {
        eprintln!("{LEGACY_BINARY_WARNING}");
    }
    match run_with_cli_stack() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if let Some(cli_error) = error.downcast_ref::<clap::Error>() {
                if matches!(
                    cli_error.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ) {
                    let _ = cli_error.print();
                    std::process::exit(cli_error.exit_code());
                }
            }
            if requested_json_output() {
                let response = classify_cli_error(&error);
                match serde_json::to_string(&response) {
                    Ok(json) => eprintln!("{json}"),
                    Err(_) => eprintln!(
                        "{{\"schema_version\":\"foundry.cli.error.v1\",\"status\":\"error\",\"error\":{{\"code\":\"internal_error\",\"category\":\"internal\",\"message\":\"Foundry command failed\",\"retryable\":false,\"remediation\":\"Retry with human output and inspect operator logs.\"}}}}"
                    ),
                }
            } else {
                eprintln!("{error:#}");
            }
            std::process::exit(1);
        }
    }
}

fn run_with_cli_stack() -> Result<i32> {
    // Clap materializes the complete command tree while parsing. The Foundry
    // CLI is large enough to overflow the default Windows main-thread stack,
    // including for early exits such as `--version`. Keep the public process
    // contract unchanged while giving command construction a bounded stack.
    std::thread::Builder::new()
        .name("foundry-cli".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(run)
        .context("failed to start the Foundry CLI worker")?
        .join()
        .map_err(|_| anyhow::anyhow!("Foundry CLI worker panicked"))?
}

fn legacy_binary_invoked() -> bool {
    env!("CARGO_BIN_NAME") == LEGACY_BINARY_NAME
}

fn explicit_store_argument_present() -> bool {
    std::env::args_os().any(|argument| {
        argument == "--store"
            || argument
                .to_str()
                .is_some_and(|value| value.starts_with("--store="))
    })
}

fn requested_json_output() -> bool {
    let mut arguments = std::env::args_os();
    while let Some(argument) = arguments.next() {
        if argument == "--output" {
            return arguments
                .next()
                .and_then(|value| value.to_str().map(str::to_owned))
                .is_some_and(|value| value.eq_ignore_ascii_case("json"));
        }
        if argument
            .to_str()
            .is_some_and(|value| value.eq_ignore_ascii_case("--output=json"))
        {
            return true;
        }
    }
    false
}

fn classify_cli_error(error: &anyhow::Error) -> CliErrorEnvelope {
    let message = error.to_string();
    let normalized = message.to_ascii_lowercase();
    let (code, category, retryable, remediation) = if normalized.contains("database is locked")
        || normalized.contains("database is busy")
        || normalized.contains("sqlite_busy")
    {
        (
            "store_busy",
            "availability",
            true,
            "Retry after the active SQLite writer completes.",
        )
    } else if normalized.contains("not found") {
        (
            "not_found",
            "input",
            false,
            "Inspect the requested identifier and retry.",
        )
    } else if normalized.contains("permission")
        || normalized.contains("unauthorized")
        || normalized.contains("forbidden")
        || normalized.contains("approval")
    {
        (
            "permission_denied",
            "authorization",
            false,
            "Use an authorized principal or provide the required explicit approval.",
        )
    } else if normalized.contains("unknown")
        || normalized.contains("invalid")
        || normalized.contains("unexpected")
        || normalized.contains("usage:")
        || normalized.contains("requires")
        || normalized.contains("required")
        || normalized.contains("must ")
    {
        (
            "invalid_argument",
            "input",
            false,
            "Correct the command input and retry.",
        )
    } else {
        (
            "internal_error",
            "internal",
            false,
            "Retry with human output and inspect operator logs.",
        )
    };

    CliErrorEnvelope {
        schema_version: "foundry.cli.error.v1",
        status: "error",
        error: CliErrorDetail {
            code,
            category,
            message,
            retryable,
            remediation,
        },
    }
}

fn parse_teamwork_lane_spec(value: &str) -> Result<foundry_core::teamwork::TeamworkLaneConfig> {
    let value = value.trim();
    let (lane_id, routing) = value.split_once('=').with_context(|| {
        format!("teamwork lane `{value}` must use ID=BRAIN:COUNT, for example frontend=agy:3")
    })?;
    let (brain, agent_count) = routing.rsplit_once(':').with_context(|| {
        format!("teamwork lane `{value}` must include a positive agent count after ':'")
    })?;
    let lane_id = lane_id.trim();
    let brain = brain.trim();
    if lane_id.is_empty() || brain.is_empty() {
        bail!("teamwork lane `{value}` requires non-empty lane and brain ids");
    }
    let agent_count = agent_count
        .trim()
        .parse::<usize>()
        .with_context(|| format!("teamwork lane `{value}` has an invalid agent count"))?;
    if agent_count == 0 {
        bail!("teamwork lane `{value}` requires at least one agent");
    }
    Ok(foundry_core::teamwork::TeamworkLaneConfig {
        id: lane_id.to_string(),
        brain: brain.to_string(),
        agent_count,
        parallel_group: "implementation-wave-001".to_string(),
        responsibility: format!("Deliver independent bounded work for the {lane_id} lane."),
    })
}

fn explicit_parallel_team_from_lane_specs(
    lane_specs: &[String],
    max_parallel_agents: Option<usize>,
    source: &str,
) -> Result<Option<CoreParallelTeamSpec>> {
    if lane_specs.is_empty() {
        if max_parallel_agents.is_some() {
            bail!("--max-parallel-agents requires at least one --lane ID=EXECUTOR:COUNT");
        }
        return Ok(None);
    }

    let lanes = lane_specs
        .iter()
        .map(|lane| parse_teamwork_lane_spec(lane))
        .collect::<Result<Vec<_>>>()?;
    let derived_parallelism = lanes.iter().try_fold(0usize, |total, lane| {
        total
            .checked_add(lane.agent_count)
            .context("parallel team lane agent count overflow")
    })?;
    let config = foundry_core::teamwork::TeamworkParallelConfig {
        lanes,
        max_parallel_agents: max_parallel_agents.unwrap_or(derived_parallelism),
    };
    foundry_core::teamwork::core_parallel_team_from_teamwork(&config, source).map(Some)
}

fn run() -> Result<i32> {
    let mut cli = Cli::try_parse()?;
    if legacy_binary_invoked() && !explicit_store_argument_present() {
        cli.store = PathBuf::from(LEGACY_DEFAULT_STORE_PATH);
    }
    if foundry_production_mode_enabled() && !cli.store.is_absolute() {
        anyhow::bail!(
            "FOUNDRY_PRODUCTION_MODE requires an absolute --store path to prevent state fragmentation"
        );
    }
    let Some(command) = cli.command else {
        return run_foundry_tui(&cli.store, Some(std::env::current_dir()?));
    };
    match command {
        Commands::Squad { command } => {
            match command {
                SquadCommands::Catalog { output } => {
                    print_response(output, &builtin_squad_catalog())?;
                }
                SquadCommands::Capabilities { output } => {
                    print_response(output, &mission_platform_catalog())?;
                }
                SquadCommands::InstallOriginals { output } => {
                    let store = FoundryStore::open(&cli.store)?;
                    print_response(output, &install_builtin_squads(&store)?)?;
                }
                SquadCommands::Validate { manifest, output } => {
                    let squad = read_squad_manifest(&manifest)?;
                    print_response(output, &validate_squad_definition(&squad)?)?;
                }
                SquadCommands::Install { manifest, output } => {
                    let store = FoundryStore::open(&cli.store)?;
                    let squad = read_squad_manifest(&manifest)?;
                    print_response(output, &install_squad(&store, &squad)?)?;
                }
                SquadCommands::List { output } => {
                    let store = FoundryStore::open(&cli.store)?;
                    print_response(output, &list_installed_squads(&store)?)?;
                }
                SquadCommands::Inspect {
                    id,
                    version,
                    output,
                } => {
                    let store = FoundryStore::open(&cli.store)?;
                    print_response(output, &load_squad(&store, &id, version.as_deref())?)?;
                }
                SquadCommands::Clone {
                    source_id,
                    source_version,
                    new_id,
                    new_name,
                    new_version,
                    output,
                } => {
                    let store = FoundryStore::open(&cli.store)?;
                    print_response(
                        output,
                        &clone_squad(
                            &store,
                            &source_id,
                            source_version.as_deref(),
                            &new_id,
                            &new_name,
                            &new_version,
                        )?,
                    )?;
                }
            }
            Ok(0)
        }
        Commands::Mission { command } => {
            let store = FoundryStore::open(&cli.store)?;
            match command {
                MissionCommands::Simulate {
                    goal,
                    squad,
                    squad_version,
                    without_rework,
                    worktree,
                    output,
                } => {
                    print_response(
                        output,
                        &simulate_mission_with_worktree(
                            &store,
                            &goal,
                            &squad,
                            squad_version.as_deref(),
                            !without_rework,
                            worktree.as_deref(),
                        )?,
                    )?;
                }
                MissionCommands::SimulatePlatform {
                    goal,
                    squad,
                    squad_version,
                    worktree,
                    output,
                } => {
                    let mission = simulate_mission_with_worktree(
                        &store,
                        &goal,
                        &squad,
                        squad_version.as_deref(),
                        true,
                        worktree.as_deref(),
                    )?;
                    let report = simulate_mission_platform_with_store(&store, &mission);
                    let exit_code = if report.failed_count > 0 { 1 } else { 0 };
                    print_response(output, &report)?;
                    return Ok(exit_code);
                }
                MissionCommands::Start {
                    goal,
                    squad,
                    squad_version,
                    worktree,
                    output,
                } => {
                    print_response(
                        output,
                        &start_mission(&store, &goal, &squad, squad_version.as_deref(), &worktree)?,
                    )?;
                }
                MissionCommands::Drive { id, output } => {
                    print_response(output, &drive_mission(&store, &id)?)?;
                }
                MissionCommands::Execute {
                    id,
                    task_id,
                    agent_id,
                    idempotency_key,
                    purpose,
                    approved_by,
                    approval_ttl_seconds,
                    dry_run,
                    allow_trusted_process_runtime,
                    requested_evidence,
                    command,
                    output,
                } => {
                    let mission = load_mission(&store, &id)?;
                    let agent = mission
                        .agents
                        .iter()
                        .find(|agent| agent.instance_id == agent_id)
                        .with_context(|| format!("mission agent not found: {agent_id}"))?;
                    let harness = mission
                        .harnesses
                        .iter()
                        .rev()
                        .find(|harness| {
                            harness.task_id == task_id && harness.agent_id == agent.definition_id
                        })
                        .with_context(|| {
                            format!(
                                "mission harness not found for task {task_id} and agent {agent_id}"
                            )
                        })?;
                    let mut request = MissionExecutionRequest {
                        idempotency_key,
                        mission_id: mission.id.clone(),
                        workflow_id: mission.workflow_id.clone(),
                        expected_mission_revision: mission.revision,
                        task_id,
                        agent_id,
                        executor_id: harness.runtime.clone(),
                        worktree: mission.worktree.clone(),
                        purpose,
                        command,
                        requested_evidence,
                        approval: None,
                        dry_run,
                        allow_trusted_process_runtime,
                    };
                    let plan = plan_mission_execution(&store, &request)?;
                    if !dry_run {
                        let approved_by = approved_by.as_deref().context(
                            "mission execution requires --approved-by unless --dry-run is set",
                        )?;
                        request.approval = Some(build_mission_execution_approval(
                            &plan,
                            approved_by,
                            approval_ttl_seconds,
                        )?);
                    }
                    let report = execute_mission_command(&store, request)?;
                    let successful = report.receipt.status == "completed"
                        && report.receipt.exit_code == Some(0)
                        && !report.receipt.timed_out;
                    let dry_run_ready = dry_run && report.receipt.status == "planned";
                    print_response(output, &report)?;
                    return Ok(if successful || dry_run_ready { 0 } else { 1 });
                }
                MissionCommands::Execution { command } => match command {
                    MissionExecutionCommands::List {
                        mission,
                        task,
                        output,
                    } => {
                        print_response(
                            output,
                            &list_mission_execution_receipts(
                                &store,
                                mission.as_deref(),
                                task.as_deref(),
                            )?,
                        )?;
                    }
                    MissionExecutionCommands::Inspect { receipt_id, output } => {
                        print_response(
                            output,
                            &inspect_mission_execution_receipt(&store, &receipt_id)?,
                        )?;
                    }
                    MissionExecutionCommands::Reconcile {
                        receipt_id,
                        outcome,
                        approved_by,
                        reason,
                        confirm_no_effect_retry,
                        output,
                    } => {
                        print_response(
                            output,
                            &reconcile_mission_execution(
                                &store,
                                MissionExecutionReconcileRequest {
                                    receipt_id,
                                    outcome,
                                    approved_by,
                                    reason,
                                    confirm_no_effect_retry,
                                },
                            )?,
                        )?;
                    }
                },
                MissionCommands::Submit {
                    id,
                    task_id,
                    agent_id,
                    idempotency_key,
                    receipt_id,
                    status,
                    summary,
                    artifacts,
                    risks,
                    followups,
                    output,
                } => {
                    print_response(
                        output,
                        &submit_mission(
                            &store,
                            &id,
                            MissionSubmission {
                                idempotency_key,
                                execution_receipt_id: receipt_id,
                                task_id,
                                agent_id,
                                status,
                                summary,
                                artifacts,
                                validations: Vec::new(),
                                risks,
                                followups,
                                tests_passed: 0,
                                tests_failed: 0,
                            },
                        )?,
                    )?;
                }
                MissionCommands::Resume { id, output } => {
                    print_response(output, &resume_mission(&store, &id)?)?;
                }
                MissionCommands::List { output } => {
                    print_response(output, &list_missions(&store)?)?;
                }
                MissionCommands::Inspect { id, output } => {
                    print_response(output, &load_mission(&store, &id)?)?;
                }
            }
            Ok(0)
        }
        Commands::Teamwork {
            goal,
            lanes,
            max_parallel_agents,
            repository,
            worktree_root,
            branch_prefix,
            allow_repository_mutation,
            detached,
            output,
            bypass_cache,
        } => {
            let store_path = cli.store.clone();
            let store = FoundryStore::open(store_path.clone())?;
            let response = if lanes.is_empty() {
                foundry_core::teamwork::plan_teamwork_workflow(
                    &store,
                    &goal,
                    detached,
                    bypass_cache,
                )?
            } else {
                let lanes = lanes
                    .iter()
                    .map(|lane| parse_teamwork_lane_spec(lane))
                    .collect::<Result<Vec<_>>>()?;
                let derived_parallelism = lanes.iter().try_fold(0usize, |total, lane| {
                    total
                        .checked_add(lane.agent_count)
                        .context("teamwork lane agent count overflow")
                })?;
                foundry_core::teamwork::plan_teamwork_workflow_with_config(
                    &store,
                    &goal,
                    detached,
                    bypass_cache,
                    foundry_core::teamwork::TeamworkParallelConfig {
                        lanes,
                        max_parallel_agents: max_parallel_agents.unwrap_or(derived_parallelism),
                    },
                )?
            };
            if allow_repository_mutation && repository.is_none() {
                bail!(
                    "teamwork --allow-repository-mutation requires --repository and --worktree-root"
                );
            }
            let worktree_preparation = repository
                .zip(worktree_root)
                .map(|(repository, worktree_root)| {
                    foundry_core::teamwork::prepare_teamwork_worktrees(
                        &store,
                        foundry_core::teamwork::TeamworkWorktreePrepareOptions {
                            workflow_id: response.workflow_id.clone(),
                            repository,
                            worktree_root,
                            branch_prefix: branch_prefix.clone(),
                            origin: "foundry_teamwork".to_string(),
                            allow_repository_mutation,
                        },
                    )
                })
                .transpose()?;
            if matches!(output, OutputFormat::Human) {
                println!("FOUNDRY TEAMWORK EXECUTION PLAN");
                println!("Goal: {}", response.goal);
                println!(
                    "Execution Mode: {}",
                    if detached { "Detached" } else { "Normal" }
                );
                println!("\nTEAM ROSTER");
                println!("\nTASK GRAPH");
                println!("\nEXECUTION STATUS");
                println!("Status: {}", response.status);
                if let Some(preparation) = &worktree_preparation {
                    println!("Worktree preparation: {}", preparation.status);
                    println!(
                        "Parallel branch worktrees: {}",
                        preparation.parallel_branch_worktrees
                    );
                    println!(
                        "Supporting agent worktrees: {}",
                        preparation.supporting_agent_worktrees
                    );
                }
            } else {
                if let Some(preparation) = &worktree_preparation {
                    print_response(
                        output,
                        &serde_json::json!({
                            "schema_version": "foundry.teamwork.prepared_plan.v1",
                            "teamwork": &response,
                            "worktree_preparation": preparation,
                        }),
                    )?;
                } else {
                    print_response(output, &response)?;
                }
            }
            if detached {
                if let Some(ref r_id) = response.run_id {
                    let current_exe = std::env::current_exe()?;
                    spawn_detached_drive_loop(&current_exe, &store_path, r_id)?;
                }
            }
            Ok(0)
        }
        Commands::Plan {
            goal,
            worktree,
            lanes,
            max_parallel_agents,
            addon_dirs,
            output,
            detached,
        } => {
            let store_path = cli.store.clone();
            let store = FoundryStore::open(store_path.clone())?;
            let project_root = match worktree.as_deref() {
                Some(selector) => resolve_worktree_selector_root(&store, selector)?,
                None => std::env::current_dir()?,
            };
            let dirs = addon_dirs_or_default(addon_dirs)
                .into_iter()
                .map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        project_root.join(path)
                    }
                })
                .collect::<Vec<_>>();
            let addon_catalog = load_addon_catalog_from_store(&store, &dirs)?;
            let operating_context = load_project_operating_context(&project_root)?;
            ensure_operating_context_policy(&store, &operating_context, "plan")?;
            let intent =
                parse_intent_with_catalog_and_context(&goal, &addon_catalog, operating_context);
            let mut workflow = create_workflow(intent);
            if let Some(parallel_team) = explicit_parallel_team_from_lane_specs(
                &lanes,
                max_parallel_agents,
                "foundry.plan.cli",
            )? {
                foundry_core::teamwork::materialize_explicit_parallel_team(
                    &mut workflow,
                    parallel_team,
                )?;
            }
            let _ = sanitize_workflow_secrets_for_storage(&store, &mut workflow, "foundry_plan")?;
            let reuse_candidates = find_reuse_candidates(&store, &workflow)?;
            let attached_subflows =
                attach_reuse_candidates_as_child_subflows(&mut workflow, &reuse_candidates);
            let planned_run = detached.then(|| {
                foundry_core::request::create_run_record(&workflow, "foundry_cli", "accepted")
            });
            store.with_transaction(|| {
                store.save_workflow(&workflow)?;
                store.record_event(
                    &workflow.id,
                    "workflow_planned",
                    &serde_json::to_value(&workflow)?,
                )?;
                if let Some(run) = planned_run.as_ref() {
                    foundry_core::request::save_run_record(&store, run)?;
                }
                Ok(())
            })?;
            let worktree_report = if let Some(selector) = worktree {
                if PathBuf::from(&selector).exists() {
                    Some(register_worktree(
                        &store,
                        WorktreeRegisterOptions {
                            path: PathBuf::from(selector),
                            id: None,
                            workflow_id: Some(workflow.id.clone()),
                            task_id: None,
                            origin: "foundry_plan".to_string(),
                            created_by_foundry: false,
                        },
                    )?)
                } else {
                    Some(bind_worktree(
                        &store,
                        &selector,
                        &workflow.id,
                        None,
                        "foundry_plan",
                    )?)
                }
            } else {
                None
            };
            let workflow = store.load_workflow(&workflow.id)?;
            let run_id = planned_run.as_ref().map(|run| run.run_id.clone());
            let response = serde_json::json!({
                "status": "planned",
                "workflow_id": workflow.id,
                "goal": workflow.goal,
                "runtime": workflow.runtime,
                "core_orchestration": workflow.core_orchestration,
                "tasks": workflow.tasks,
                "intent": workflow.intent,
                "reuse_candidates": reuse_candidates,
                "attached_subflows": attached_subflows,
                "worktree": worktree_report,
                "run_id": run_id,
            });
            print_response(output, &response)?;
            if detached {
                if let Some(ref r_id) = run_id {
                    let current_exe = std::env::current_exe()?;
                    std::process::Command::new(current_exe)
                        .arg("--store")
                        .arg(&store_path)
                        .arg("request")
                        .arg("drive-loop")
                        .arg("--run")
                        .arg(r_id)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()?;
                }
            }
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

            let store = FoundryStore::open(cli.store)?;
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
            let store = FoundryStore::open(cli.store)?;
            let report = inspect_workflow_with_focus(&store, &workflow, verbose, task.as_deref())?;
            match output {
                OutputFormat::Json => print_response(output, &report)?,
                OutputFormat::Human => println!("{}", report.diagram),
            }
            Ok(0)
        }
        Commands::Status { workflow, output } => {
            let store = FoundryStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let worktree = bound_worktree_context(&store, &workflow.id, None)?;
            let worktrees = list_registered_worktrees(&store, None, Some(&workflow.id))?;
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
                    "schema_version": "foundry.tokens.workflow_summary.v1",
                    "collection_name": tokens.name,
                    "token_count": tokens.tokens.len(),
                    "semantic_alias_count": tokens.semantic_aliases.len(),
                    "mode_count": tokens.modes.len(),
                    "resolution_schema_version": "foundry.tokens.resolution.v1",
                })
            });
            let response = serde_json::json!({
                "workflow_id": workflow.id,
                "status": workflow.status,
                "goal": workflow.goal,
                "runtime": workflow.runtime,
                "worktree": worktree,
                "worktrees": worktrees,
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
            project_root,
            budget,
            strict,
            view,
            output,
        } => {
            let store_path = cli.store.clone();
            let store = FoundryStore::open(cli.store)?;
            ensure_workflow_policy(&store, &workflow, "context request")?;
            let workflow = store.load_workflow(&workflow)?;
            let latest_checkpoint = load_latest_task_checkpoint(&store, &workflow.id, &task)?;
            let workflow_secret_scan_input = serde_json::to_string(&workflow)?;
            let tenant_context = serde_json::to_value(&workflow.intent.operating_context)?;
            let _ = sanitize_prompt_secrets_with_vault(
                &workflow_secret_scan_input,
                SecretSanitizationOptions::default(),
                SecretVaultPersistOptions {
                    store: &store,
                    workflow_id: Some(&workflow.id),
                    origin: "foundry_context",
                    tenant_context: &tenant_context,
                },
            )?;
            let effective_project_root = resolve_effective_project_root(
                &store,
                &workflow.id,
                Some(&task),
                project_root.as_deref(),
            )?;
            let bound_worktree = bound_worktree_context(&store, &workflow.id, Some(&task))?;
            let context = if bound_worktree.is_some() {
                build_context_package_with_checkpoint_project_and_worktree(
                    &workflow,
                    &task,
                    budget,
                    latest_checkpoint,
                    effective_project_root.as_deref(),
                    bound_worktree,
                )?
            } else {
                build_context_package_with_checkpoint_and_project(
                    &workflow,
                    &task,
                    budget,
                    latest_checkpoint,
                    effective_project_root.as_deref(),
                )?
            };
            let exit_code = if strict && !context.handoff_ready {
                1
            } else {
                0
            };
            match view {
                ContextViewArg::Full => print_response(output, &context)?,
                ContextViewArg::Compact => {
                    let predecessor_plans = build_predecessor_handoff_plans(
                        &store,
                        &workflow,
                        &task,
                        budget,
                        project_root.as_deref(),
                    )?;
                    print_response(
                        output,
                        &build_compact_context_view_with_predecessor_plans(
                            &context,
                            &workflow,
                            &store_path,
                            effective_project_root.as_deref(),
                            &predecessor_plans,
                        ),
                    )?
                }
            }
            Ok(exit_code)
        }
        Commands::Run {
            workflow,
            simulate,
            output,
        } => {
            if !simulate {
                anyhow::bail!("v0 execution requires --simulate; real provider execution is intentionally not enabled");
            }
            let store = FoundryStore::open(cli.store)?;
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
            let store = FoundryStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let report = validate_workflow(&workflow);
            let exit_code = if report.promotable { 0 } else { 1 };
            print_response(output, &report)?;
            Ok(exit_code)
        }
        Commands::Store { command } => match command {
            StoreCommands::Check { output } => {
                let report = check_store(&cli.store)?;
                let exit_code = if report.healthy { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            StoreCommands::Backup {
                destination,
                output,
            } => {
                let report = backup_store(&cli.store, &destination)?;
                print_response(output, &report)?;
                Ok(0)
            }
            StoreCommands::Restore {
                source,
                approved_by,
                confirm_restore,
                output,
            } => {
                let report = restore_store(&cli.store, &source, &approved_by, confirm_restore)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Improve {
            command,
            workflow,
            target_version,
            output,
        } => {
            let store = FoundryStore::open(cli.store)?;
            match command {
                Some(ImproveCommands::Candidates {
                    limit,
                    workflows,
                    goal_contains,
                    output,
                }) => {
                    let report = rank_improvement_candidates_with_filter(
                        &store,
                        limit,
                        ImprovementCandidateFilter {
                            workflow_ids: workflows,
                            goal_contains,
                        },
                    )?;
                    print_response(output, &report)?;
                    Ok(0)
                }
                Some(ImproveCommands::NormalizeCost {
                    workflow,
                    all,
                    limit,
                    origin,
                    output,
                }) => {
                    if all && workflow.is_some() {
                        anyhow::bail!(
                            "`foundry improve normalize-cost` accepts either --workflow <id> or --all, not both"
                        );
                    }
                    if all {
                        let report =
                            normalize_avoidable_ai_costs_for_candidates(&store, limit, &origin)?;
                        print_response(output, &report)?;
                    } else if let Some(workflow) = workflow {
                        let report = normalize_avoidable_ai_costs(&store, &workflow, &origin)?;
                        print_response(output, &report)?;
                    } else {
                        anyhow::bail!(
                            "`foundry improve normalize-cost` requires --workflow <id> or --all"
                        );
                    }
                    Ok(0)
                }
                Some(ImproveCommands::ApplyEventPolicy {
                    workflow,
                    recommendation_id,
                    recommended_policy,
                    apply,
                    approved_by,
                    origin,
                    output,
                }) => {
                    let report = apply_event_improvement_policy(
                        &store,
                        &workflow,
                        recommendation_id.as_deref(),
                        recommended_policy.as_deref(),
                        apply,
                        approved_by.as_deref(),
                        &origin,
                    )?;
                    print_response(output, &report)?;
                    Ok(0)
                }
                Some(ImproveCommands::BenchmarkEventPolicy {
                    workflow,
                    recommendation_id,
                    recommended_policy,
                    origin,
                    output,
                }) => {
                    let report = benchmark_event_improvement_policy(
                        &store,
                        &workflow,
                        recommendation_id.as_deref(),
                        recommended_policy.as_deref(),
                        &origin,
                    )?;
                    print_response(output, &report)?;
                    Ok(0)
                }
                Some(ImproveCommands::PromoteEventPolicy {
                    workflow,
                    recommendation_id,
                    recommended_policy,
                    approved_by,
                    origin,
                    output,
                }) => {
                    let report = promote_event_improvement_policy(
                        &store,
                        &workflow,
                        recommendation_id.as_deref(),
                        recommended_policy.as_deref(),
                        approved_by.as_deref(),
                        &origin,
                    )?;
                    print_response(output, &report)?;
                    Ok(0)
                }
                None => {
                    let Some(workflow) = workflow else {
                        anyhow::bail!(
                            "`foundry improve` requires --workflow or a subcommand such as `candidates`"
                        );
                    };
                    let workflow = store.load_workflow(&workflow)?;
                    let proposal = generate_improvement(&store, &workflow, target_version)?;
                    print_response(output, &proposal)?;
                    Ok(0)
                }
            }
        }
        Commands::Artifacts { workflow, output } => {
            let store = FoundryStore::open(cli.store)?;
            let loaded_workflow = store.load_workflow(&workflow)?;
            let artifact_tags = loaded_workflow
                .artifacts
                .iter()
                .map(|artifact| (artifact.path.clone(), artifact.tags.clone()))
                .collect::<BTreeMap<_, _>>();
            let artifacts =
                list_workflow_artifacts_with_tags(&store.base_dir(), &workflow, &artifact_tags)?;
            let response = serde_json::json!({
                "workflow_id": workflow,
                "artifacts": artifacts,
            });
            print_response(output, &response)?;
            Ok(0)
        }
        Commands::Events { command } => match command {
            EventCommands::List {
                workflow,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_workflow_event_stream(&store, &workflow, limit)?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::Timeline {
                workflow,
                organization,
                brand,
                product,
                limit,
                after_sequence,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = build_global_event_timeline_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    limit,
                    after_sequence,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::Observability {
                workflow,
                organization,
                brand,
                product,
                node,
                addon,
                limit,
                after_sequence,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = build_event_observability_index_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    node.as_deref(),
                    addon.as_deref(),
                    limit,
                    after_sequence,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::ObservabilityHistory {
                workflow,
                organization,
                brand,
                product,
                node,
                addon,
                bucket,
                group_by,
                limit,
                after_sequence,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = build_event_observability_history_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    node.as_deref(),
                    addon.as_deref(),
                    Some(bucket.as_str()),
                    Some(group_by.as_str()),
                    limit,
                    after_sequence,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::ImprovementPolicy {
                workflow,
                organization,
                brand,
                product,
                node,
                addon,
                min_events,
                min_duration_ms,
                min_retries,
                min_context_pressure_bps,
                min_wait_seconds,
                limit,
                after_sequence,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = build_event_improvement_policy_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    node.as_deref(),
                    addon.as_deref(),
                    min_events,
                    min_duration_ms,
                    min_retries,
                    min_context_pressure_bps,
                    min_wait_seconds,
                    limit,
                    after_sequence,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::Ingest {
                origin,
                action,
                project_root,
                input,
                input_file,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let data = read_mcp_input(input, input_file)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = ingest_inbound_event_with_context(
                    &store,
                    InboundEventIngestInput {
                        origin,
                        action,
                        data,
                    },
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::Inbox {
                status,
                limit,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = list_inbound_event_inbox_for_context(
                    &store,
                    status.as_deref(),
                    limit,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::Scan {
                status,
                limit,
                project_root,
                dispatch_activations,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = scan_inbound_event_inbox(
                    &store,
                    &project_root,
                    status.as_deref(),
                    limit,
                    dispatch_activations,
                )?;
                print_response(output, &report)?;
                Ok(if report.failed_count > 0 { 1 } else { 0 })
            }
            EventCommands::Worker {
                status,
                limit,
                project_root,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                stop_file,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = run_inbound_event_worker_loop(
                    &store,
                    &project_root,
                    InboundEventWorkerLoopOptions {
                        status: status.as_deref(),
                        limit,
                        max_cycles,
                        interval_seconds,
                        idle_exit,
                        dispatch_activations,
                        stop_file: stop_file.as_deref(),
                    },
                )?;
                print_response(output, &report)?;
                Ok(if report.failed_count > 0 { 1 } else { 0 })
            }
            EventCommands::ServicePlan {
                service_kind,
                project_root,
                status,
                limit,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                host,
                port,
                path,
                origin,
                action,
                schema,
                route,
                max_requests,
                max_body_bytes,
                hmac_secret_env,
                signature_header,
                lease_seconds,
                heartbeat_seconds,
                backoff_initial_seconds,
                backoff_max_seconds,
                shutdown_grace_seconds,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_event_service_plan(
                    &store,
                    &project_root,
                    &service_kind,
                    status.as_deref(),
                    limit,
                    max_cycles,
                    interval_seconds,
                    idle_exit,
                    dispatch_activations,
                    &host,
                    port,
                    &path,
                    origin.as_deref(),
                    action.as_deref(),
                    schema.as_deref(),
                    route,
                    max_requests,
                    max_body_bytes,
                    hmac_secret_env.as_deref(),
                    &signature_header,
                    lease_seconds,
                    heartbeat_seconds,
                    backoff_initial_seconds,
                    backoff_max_seconds,
                    shutdown_grace_seconds,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::ServiceRun {
                service_kind,
                project_root,
                status,
                limit,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                stop_file,
                host,
                port,
                path,
                origin,
                action,
                schema,
                route,
                max_requests,
                max_body_bytes,
                hmac_secret_env,
                signature_header,
                lease_owner,
                lease_seconds,
                heartbeat_seconds,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let normalized_kind = service_kind.trim();
                let report = if normalized_kind == "worker" {
                    run_event_worker_service(
                        &store,
                        &project_root,
                        status.as_deref(),
                        limit,
                        max_cycles,
                        interval_seconds,
                        idle_exit,
                        dispatch_activations,
                        stop_file.as_deref(),
                        &lease_owner,
                        lease_seconds,
                        heartbeat_seconds,
                    )?
                } else if matches!(
                    normalized_kind,
                    "webhook_ingress" | "webhook-ingress" | "webhook"
                ) {
                    run_event_webhook_ingress_service(
                        &store,
                        &project_root,
                        &host,
                        port,
                        &path,
                        origin.as_deref(),
                        action.as_deref(),
                        schema.as_deref(),
                        route,
                        max_requests,
                        max_body_bytes,
                        hmac_secret_env.as_deref(),
                        &signature_header,
                        stop_file.as_deref(),
                        &lease_owner,
                        lease_seconds,
                        heartbeat_seconds,
                    )?
                } else {
                    anyhow::bail!(
                        "unsupported event service kind for service-run: {normalized_kind}"
                    );
                };
                print_response(output, &report)?;
                Ok(
                    if report.status == "event_service_run_completed_with_failures" {
                        1
                    } else {
                        0
                    },
                )
            }
            EventCommands::ServiceSupervise {
                service_kind,
                project_root,
                status,
                limit,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                stop_file,
                host,
                port,
                path,
                origin,
                action,
                schema,
                route,
                max_requests,
                max_body_bytes,
                hmac_secret_env,
                signature_header,
                lease_owner,
                lease_seconds,
                heartbeat_seconds,
                max_runs,
                backoff_initial_seconds,
                backoff_max_seconds,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = run_event_service_supervisor(
                    &store,
                    &project_root,
                    &service_kind,
                    status.as_deref(),
                    limit,
                    max_cycles,
                    interval_seconds,
                    idle_exit,
                    dispatch_activations,
                    &host,
                    port,
                    &path,
                    origin.as_deref(),
                    action.as_deref(),
                    schema.as_deref(),
                    route,
                    max_requests,
                    max_body_bytes,
                    hmac_secret_env.as_deref(),
                    &signature_header,
                    stop_file.as_deref(),
                    &lease_owner,
                    lease_seconds,
                    heartbeat_seconds,
                    max_runs,
                    backoff_initial_seconds,
                    backoff_max_seconds,
                )?;
                print_response(output, &report)?;
                Ok(
                    if report.status == "event_service_supervisor_failed"
                        || report.status == "event_service_supervisor_completed_with_failures"
                    {
                        1
                    } else {
                        0
                    },
                )
            }
            EventCommands::RuntimeReconcile {
                project_root,
                status,
                limit,
                service_limit,
                execute,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                recover_stale_services,
                stop_file,
                lease_owner,
                lease_seconds,
                heartbeat_seconds,
                max_runs,
                backoff_initial_seconds,
                backoff_max_seconds,
                scan_schedules,
                schedule_executor,
                schedule_max_workers,
                schedule_ttl_seconds,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = run_event_runtime_reconcile(
                    &store,
                    &project_root,
                    status.as_deref(),
                    limit,
                    service_limit,
                    execute,
                    max_cycles,
                    interval_seconds,
                    idle_exit,
                    dispatch_activations,
                    recover_stale_services,
                    stop_file.as_deref(),
                    &lease_owner,
                    lease_seconds,
                    heartbeat_seconds,
                    max_runs,
                    backoff_initial_seconds,
                    backoff_max_seconds,
                    scan_schedules,
                    &schedule_executor,
                    schedule_max_workers,
                    schedule_ttl_seconds,
                )?;
                print_response(output, &report)?;
                Ok(
                    if report.status == "event_runtime_reconcile_executed_with_failures" {
                        1
                    } else {
                        0
                    },
                )
            }
            EventCommands::RuntimeDaemon {
                project_root,
                status,
                limit,
                service_limit,
                execute,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                continuous,
                cycle_retention,
                recover_stale_services,
                stop_file,
                lease_owner,
                lease_seconds,
                heartbeat_seconds,
                max_runs,
                backoff_initial_seconds,
                backoff_max_seconds,
                scan_schedules,
                schedule_executor,
                schedule_max_workers,
                schedule_ttl_seconds,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = run_event_runtime_daemon(
                    &store,
                    &project_root,
                    status.as_deref(),
                    limit,
                    service_limit,
                    execute,
                    max_cycles,
                    interval_seconds,
                    idle_exit,
                    dispatch_activations,
                    continuous,
                    cycle_retention,
                    recover_stale_services,
                    stop_file.as_deref(),
                    &lease_owner,
                    lease_seconds,
                    heartbeat_seconds,
                    max_runs,
                    backoff_initial_seconds,
                    backoff_max_seconds,
                    scan_schedules,
                    &schedule_executor,
                    schedule_max_workers,
                    schedule_ttl_seconds,
                )?;
                print_response(output, &report)?;
                Ok(
                    if report.status == "event_runtime_daemon_completed_with_failures" {
                        1
                    } else {
                        0
                    },
                )
            }
            EventCommands::Services {
                project_root,
                service_kind,
                status,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_event_services(
                    &store,
                    &project_root,
                    service_kind.as_deref(),
                    status.as_deref(),
                    limit,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::ServicesRecover {
                project_root,
                service_kind,
                limit,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = recover_stale_event_services(
                    &store,
                    &project_root,
                    service_kind.as_deref(),
                    limit,
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::WebhookIngress {
                host,
                port,
                path,
                origin,
                action,
                transport,
                schema,
                project_root,
                route,
                max_requests,
                max_body_bytes,
                hmac_secret_env,
                signature_header,
                stop_file,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = run_event_webhook_ingress_server(
                    &store,
                    &host,
                    port,
                    &path,
                    &origin,
                    &action,
                    &transport,
                    schema.as_deref(),
                    &project_root,
                    route,
                    max_requests,
                    max_body_bytes,
                    hmac_secret_env.as_deref(),
                    &signature_header,
                    stop_file.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(if report.failed_count > 0 { 1 } else { 0 })
            }
            EventCommands::Adapters {
                addon,
                transport,
                direction,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = list_addon_event_adapters(
                    &catalog,
                    addon.as_deref(),
                    transport.as_deref(),
                    direction.as_deref(),
                );
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::Route {
                event_id,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = route_inbound_event(&store, &event_id, &project_root)?;
                print_response(output, &report)?;
                Ok(0)
            }
            EventCommands::DispatchActivations {
                event_id,
                project_root,
                dry_run,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    dispatch_inbound_event_activations(&store, &event_id, &project_root, dry_run)?;
                let should_fail = report.blocked_count > 0
                    || report.skipped_count > 0
                    || (report.dispatch_attempt_count == 0 && report.activation_count > 0);
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            EventCommands::Emit {
                addon,
                adapter_id,
                event_type,
                action,
                origin,
                payload,
                dry_run,
                project_root,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let payload: Value = serde_json::from_str(&payload)?;
                let report = emit_event_egress(
                    &store,
                    &catalog,
                    EventEgressEmitInput {
                        adapter_id,
                        addon_id: addon,
                        event_type,
                        action,
                        origin,
                        payload,
                        dry_run,
                    },
                    &operating_context,
                )?;
                let success = report
                    .delivery
                    .as_ref()
                    .map(|delivery| delivery.success)
                    .unwrap_or(true);
                print_response(output, &report)?;
                Ok(if success { 0 } else { 1 })
            }
        },
        Commands::Identity { command } => match command {
            IdentityCommands::Context {
                project_root,
                output,
            } => {
                let report = inspect_project_operating_context(&project_root)?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::Sync {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = sync_project_operating_context(&store, &project_root)?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::Registry { scope, id, output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_identity_registry(&store, scope.as_deref(), id.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::Memberships {
                subject_scope,
                subject_id,
                organization_id,
                brand_id,
                product_id,
                status,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_identity_memberships(
                    &store,
                    subject_scope.as_deref(),
                    subject_id.as_deref(),
                    organization_id.as_deref(),
                    brand_id.as_deref(),
                    product_id.as_deref(),
                    status.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::MembershipUpdate {
                subject_scope,
                subject_id,
                organization_id,
                brand_id,
                product_id,
                role,
                status,
                grant_permissions,
                revoke_grants,
                deny_permissions,
                remove_denies,
                expires_at,
                clear_expires_at,
                not_before,
                clear_not_before,
                source,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = update_identity_membership(
                    &store,
                    IdentityMembershipUpdateInput {
                        subject_scope,
                        subject_id,
                        organization_id,
                        brand_id,
                        product_id,
                        role,
                        status,
                        grant_permissions,
                        revoke_grants,
                        deny_permissions,
                        remove_denies,
                        expires_at,
                        clear_expires_at,
                        not_before,
                        clear_not_before,
                        source,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::Link {
                left_scope,
                left_id,
                right_scope,
                right_id,
                link_type,
                source,
                reason,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = link_identity(
                    &store,
                    IdentityLinkInput {
                        left_scope,
                        left_id,
                        right_scope,
                        right_id,
                        link_type,
                        source,
                        reason,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::Unlink {
                left_scope,
                left_id,
                right_scope,
                right_id,
                link_type,
                source,
                reason,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = unlink_identity(
                    &store,
                    IdentityLinkInput {
                        left_scope,
                        left_id,
                        right_scope,
                        right_id,
                        link_type,
                        source,
                        reason,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::Links {
                scope,
                id,
                status,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_identity_links(
                    &store,
                    scope.as_deref(),
                    id.as_deref(),
                    status.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::Resolve { scope, id, output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = resolve_identity(&store, &scope, &id)?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::TenantIndex {
                resource_type,
                organization_id,
                brand_id,
                product_id,
                workflow_id,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_tenant_index(
                    &store,
                    resource_type.as_deref(),
                    organization_id.as_deref(),
                    brand_id.as_deref(),
                    product_id.as_deref(),
                    workflow_id.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            IdentityCommands::TenantAudit { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = audit_tenant_index(&store)?;
                print_response(output, &report)?;
                Ok(if report.status == "tenant_index_complete" {
                    0
                } else {
                    1
                })
            }
            IdentityCommands::TenantPolicy {
                workflow_id,
                mode,
                action,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    evaluate_tenant_policy_for_action(&store, &workflow_id, &mode, &action)?;
                let should_fail = report.mode == "enforce" && !report.allowed;
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
        },
        Commands::Addons { command } => match command {
            AddonCommands::Installed { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_installed_addons(&store)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Capabilities {
                addon,
                capability,
                lifecycle,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_addon_capability_index(
                    &store,
                    addon.as_deref(),
                    capability.as_deref(),
                    lifecycle.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Observability {
                addon,
                lifecycle,
                addon_dirs,
                dispatch_limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = addon_observability_report(
                    &store,
                    &catalog,
                    addon.as_deref(),
                    lifecycle.as_deref(),
                    dispatch_limit,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Permissions {
                addon,
                permission,
                status,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_addon_permission_authorizations(
                    &store,
                    addon.as_deref(),
                    permission.as_deref(),
                    status.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::AuthorizePermission {
                addon,
                permission,
                risk,
                approved_by,
                source,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = authorize_addon_permission(
                    &store,
                    &addon,
                    &permission,
                    &risk,
                    &approved_by,
                    &source,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::RevokePermission {
                addon,
                permission,
                approved_by,
                source,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    revoke_addon_permission(&store, &addon, &permission, &approved_by, &source)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Catalog { addon_dirs, output } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                print_response(output, &catalog)?;
                Ok(0)
            }
            AddonCommands::Contracts {
                addon,
                contract_type,
                capability,
                lifecycle,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = list_addon_runtime_contracts(
                    &catalog,
                    addon.as_deref(),
                    contract_type.as_deref(),
                    capability.as_deref(),
                    lifecycle.as_deref(),
                );
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Planners {
                addon,
                capability,
                workflow_extension,
                lifecycle,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = list_addon_planner_registry(
                    &catalog,
                    addon.as_deref(),
                    capability.as_deref(),
                    workflow_extension.as_deref(),
                    lifecycle.as_deref(),
                );
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::ContractPolicy {
                addon,
                contract,
                contract_type,
                capability,
                lifecycle,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = evaluate_addon_runtime_contract_policy(
                    &catalog,
                    addon.as_deref(),
                    contract.as_deref(),
                    contract_type.as_deref(),
                    capability.as_deref(),
                    lifecycle.as_deref(),
                );
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Views {
                addon,
                surface,
                lifecycle,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = list_addon_views(
                    &catalog,
                    addon.as_deref(),
                    surface.as_deref(),
                    lifecycle.as_deref(),
                );
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::DispatchContract {
                addon,
                contract,
                input,
                source,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let input_value: serde_json::Value = serde_json::from_str(&input)?;
                let report = enqueue_addon_runtime_contract_dispatch(
                    &store,
                    &catalog,
                    addon.as_deref(),
                    &contract,
                    input_value,
                    &source,
                    dry_run,
                )?;
                let should_fail = report.blocked_count > 0;
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::DispatchPlanner {
                addon,
                contract,
                goal,
                constraints,
                workflow_id,
                task_id,
                context,
                source,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let context_value: serde_json::Value = serde_json::from_str(&context)?;
                let report = enqueue_addon_planner_dispatch(
                    &store,
                    &catalog,
                    AddonPlannerDispatchInput {
                        addon_id: addon.as_deref(),
                        contract_id: &contract,
                        goal: &goal,
                        constraints: &constraints,
                        workflow_id: workflow_id.as_deref(),
                        task_id: task_id.as_deref(),
                        context: context_value,
                        source: &source,
                        dry_run,
                    },
                )?;
                let should_fail = report.blocked_count > 0;
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::ExecutePlanner {
                addon,
                contract,
                worker,
                goal,
                constraints,
                workflow_id,
                task_id,
                context,
                lease_seconds,
                source,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let context_value: serde_json::Value = serde_json::from_str(&context)?;
                let report = execute_addon_planning_strategy(
                    &store,
                    &catalog,
                    AddonPlanningStrategyInput {
                        dispatch: AddonPlannerDispatchInput {
                            addon_id: addon.as_deref(),
                            contract_id: &contract,
                            goal: &goal,
                            constraints: &constraints,
                            workflow_id: workflow_id.as_deref(),
                            task_id: task_id.as_deref(),
                            context: context_value,
                            source: &source,
                            dry_run,
                        },
                        worker_id: &worker,
                        lease_seconds,
                    },
                )?;
                let should_fail = matches!(
                    report.status.as_str(),
                    "planning_strategy_dispatch_blocked"
                        | "planning_strategy_execution_failed"
                        | "planning_strategy_result_invalid"
                );
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::ExecuteValidator {
                addon,
                contract,
                worker,
                subject,
                workflow_id,
                task_id,
                input,
                context,
                lease_seconds,
                source,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let input_value: serde_json::Value = serde_json::from_str(&input)?;
                let context_value: serde_json::Value = serde_json::from_str(&context)?;
                let report = execute_addon_validator(
                    &store,
                    &catalog,
                    AddonValidatorExecutionInput {
                        dispatch: AddonValidatorDispatchInput {
                            addon_id: addon.as_deref(),
                            contract_id: &contract,
                            subject: &subject,
                            workflow_id: workflow_id.as_deref(),
                            task_id: task_id.as_deref(),
                            input: input_value,
                            context: context_value,
                            source: &source,
                            dry_run,
                        },
                        worker_id: &worker,
                        lease_seconds,
                    },
                )?;
                let should_fail = matches!(
                    report.status.as_str(),
                    "addon_validator_dispatch_blocked"
                        | "addon_validator_execution_failed"
                        | "addon_validator_result_invalid"
                );
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::ApplyValidatorOutcome {
                dispatch_id,
                workflow_id,
                task_id,
                expected_revision,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = apply_addon_validator_outcome(
                    &store,
                    AddonValidatorOutcomeApplyInput {
                        dispatch_id: &dispatch_id,
                        workflow_id: &workflow_id,
                        task_id: &task_id,
                        expected_revision,
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::ExecuteExecutor {
                addon,
                contract,
                worker,
                task_ref,
                workflow_id,
                input,
                context,
                lease_seconds,
                source,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let input_value: serde_json::Value = serde_json::from_str(&input)?;
                let context_value: serde_json::Value = serde_json::from_str(&context)?;
                let report = execute_addon_executor(
                    &store,
                    &catalog,
                    AddonExecutorExecutionInput {
                        dispatch: AddonExecutorDispatchInput {
                            addon_id: addon.as_deref(),
                            contract_id: &contract,
                            task_ref: &task_ref,
                            input: input_value,
                            context: context_value,
                            source: &source,
                            dry_run,
                        },
                        worker_id: &worker,
                        lease_seconds,
                        workflow_id: workflow_id.as_deref(),
                    },
                )?;
                let should_fail = matches!(
                    report.status.as_str(),
                    "addon_executor_dispatch_blocked"
                        | "addon_executor_execution_failed"
                        | "addon_executor_result_invalid"
                        | "addon_executor_failed"
                        | "addon_executor_needs_retry"
                );
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::ExecuteHandoff {
                addon,
                contract,
                worker,
                handoff_ref,
                input,
                context,
                lease_seconds,
                source,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let input_value: serde_json::Value = serde_json::from_str(&input)?;
                let context_value: serde_json::Value = serde_json::from_str(&context)?;
                let report = execute_addon_handoff(
                    &store,
                    &catalog,
                    AddonHandoffExecutionInput {
                        dispatch: AddonHandoffDispatchInput {
                            addon_id: addon.as_deref(),
                            contract_id: &contract,
                            handoff_ref: &handoff_ref,
                            input: input_value,
                            context: context_value,
                            source: &source,
                            dry_run,
                        },
                        worker_id: &worker,
                        lease_seconds,
                    },
                )?;
                let should_fail = matches!(
                    report.status.as_str(),
                    "addon_handoff_dispatch_blocked"
                        | "addon_handoff_execution_failed"
                        | "addon_handoff_result_invalid"
                        | "addon_handoff_failed"
                        | "addon_handoff_needs_followup"
                );
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::Dispatches {
                addon,
                contract,
                status,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_addon_runtime_contract_dispatches(
                    &store,
                    addon.as_deref(),
                    contract.as_deref(),
                    status.as_deref(),
                    limit,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::RunDispatch {
                dispatch,
                worker,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = run_addon_runtime_contract_dispatch(
                    &store, &catalog, &dispatch, &worker, dry_run,
                )?;
                let should_fail = report.blocked_count > 0;
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::DispatchWorker {
                status,
                limit,
                worker,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = run_addon_runtime_contract_dispatch_worker(
                    &store,
                    &catalog,
                    status.as_deref(),
                    limit,
                    &worker,
                    dry_run,
                )?;
                let should_fail = report.blocked_count > 0 || report.failed_count > 0;
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::ExecuteDispatch {
                dispatch,
                worker,
                lease_seconds,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = execute_addon_runtime_contract_dispatch(
                    &store,
                    &catalog,
                    &dispatch,
                    &worker,
                    lease_seconds,
                    dry_run,
                )?;
                let should_fail = matches!(
                    report.status.as_str(),
                    "runtime_contract_dispatch_not_claimed"
                        | "runtime_contract_dispatch_worker_rejected"
                        | "runtime_contract_dispatch_completion_rejected"
                        | "runtime_contract_dispatch_blocked"
                );
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::ClaimDispatch {
                dispatch,
                worker,
                lease_seconds,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = claim_addon_runtime_contract_dispatch(
                    &store,
                    &catalog,
                    &dispatch,
                    &worker,
                    lease_seconds,
                    dry_run,
                )?;
                let should_fail = !matches!(
                    report.status.as_str(),
                    "runtime_contract_dispatch_claimed" | "runtime_contract_dispatch_dry_run"
                );
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::CompleteDispatch {
                dispatch,
                worker,
                status,
                result,
                signature,
                attestation,
                dry_run,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let result_value: serde_json::Value = serde_json::from_str(&result)?;
                let attestation_value: serde_json::Value = serde_json::from_str(&attestation)?;
                let report = complete_addon_runtime_contract_dispatch(
                    &store,
                    &catalog,
                    AddonRuntimeContractCompletionInput {
                        dispatch_id: &dispatch,
                        worker_id: &worker,
                        completion_status: &status,
                        result: result_value,
                        signature: signature.as_deref(),
                        attestation: attestation_value,
                        dry_run,
                    },
                )?;
                let should_fail = matches!(
                    report.status.as_str(),
                    "runtime_contract_dispatch_not_claimed"
                        | "runtime_contract_dispatch_completion_rejected"
                        | "runtime_contract_dispatch_blocked"
                );
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::RegisterWorker {
                worker,
                runtime,
                status,
                trust_level,
                source,
                data,
                rotation_approved_by,
                rotation_reason,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let data_value: serde_json::Value = serde_json::from_str(&data)?;
                let report = register_addon_runtime_worker(
                    &store,
                    AddonRuntimeWorkerRegistrationInput {
                        worker_id: &worker,
                        runtime: &runtime,
                        status: &status,
                        trust_level: &trust_level,
                        source: &source,
                        data: data_value,
                        rotation_approved_by: rotation_approved_by.as_deref(),
                        rotation_reason: rotation_reason.as_deref(),
                    },
                )?;
                let should_fail = report.status == "runtime_worker_registration_blocked";
                print_response(output, &report)?;
                Ok(if should_fail { 1 } else { 0 })
            }
            AddonCommands::Workers {
                runtime,
                status,
                trust_level,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_addon_runtime_workers(
                    &store,
                    runtime.as_deref(),
                    status.as_deref(),
                    trust_level.as_deref(),
                    limit,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Resolve {
                goal,
                addon_dirs,
                registry_sources,
                registry_cache_dir,
                allow_remote_registry,
                registry_max_bytes,
                registry_max_packages,
                registry_lock_path,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = if registry_sources.is_empty() {
                    resolve_goal_capabilities_with_store(&store, &goal, &catalog)?
                } else {
                    resolve_goal_capabilities_with_registry_sync(
                        &store,
                        &goal,
                        &catalog,
                        CapabilityRegistrySyncInput {
                            registry_sources: &registry_sources,
                            cache_dir: registry_cache_dir.as_deref(),
                            allow_remote: allow_remote_registry,
                            max_bytes: registry_max_bytes,
                            max_packages: registry_max_packages,
                            lock_path: registry_lock_path.as_deref(),
                        },
                    )?
                };
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Validate { addon_dirs, output } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let catalog = load_addon_catalog_from_store(&store, &dirs)?;
                let report = validate_addon_catalog(&catalog);
                print_response(output, &report)?;
                Ok(if report.status == "valid" { 0 } else { 1 })
            }
            AddonCommands::LifecyclePlan {
                action,
                id,
                manifest,
                package_path,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = plan_addon_lifecycle(
                    &store,
                    &action,
                    id.as_deref(),
                    manifest.as_deref(),
                    package_path.as_deref(),
                    &dirs,
                )?;
                print_response(output, &report)?;
                Ok(if report.ready_to_apply { 0 } else { 1 })
            }
            AddonCommands::Install {
                manifest,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = install_addon(&store, &manifest, &dirs)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Package {
                manifest,
                addon_dirs,
                repository,
                channel,
                signature,
                public_key,
                package_path,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = package_addon(
                    &store,
                    AddonPackageInput {
                        manifest_path: &manifest,
                        addon_dirs: &dirs,
                        repository: repository.as_deref(),
                        channel: &channel,
                        signature: signature.as_deref(),
                        public_key: public_key.as_deref(),
                        package_path: package_path.as_deref(),
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::TrustKey {
                repository,
                channel,
                public_key,
                trust_level,
                approved_by,
                source,
                data,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let data_value: serde_json::Value = serde_json::from_str(&data)?;
                let report = trust_addon_package_key(
                    &store,
                    AddonTrustKeyInput {
                        repository: &repository,
                        channel: &channel,
                        public_key: &public_key,
                        trust_level: &trust_level,
                        approved_by: &approved_by,
                        source: &source,
                        data: data_value,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::TrustStore {
                repository,
                channel,
                public_key,
                status,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_addon_trust_store(
                    &store,
                    repository.as_deref(),
                    channel.as_deref(),
                    public_key.as_deref(),
                    status.as_deref(),
                    limit,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::PublishPackage {
                package_path,
                source,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = publish_addon_package(&store, &package_path, &source)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::FetchPackage {
                source,
                cache_dir,
                expected_sha256,
                lock_path,
                allow_remote,
                max_bytes,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = fetch_addon_package(
                    &store,
                    &source,
                    cache_dir.as_deref(),
                    expected_sha256.as_deref(),
                    allow_remote,
                    max_bytes,
                    lock_path.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::SyncRegistry {
                source,
                cache_dir,
                lock_path,
                allow_remote,
                max_bytes,
                max_packages,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = sync_addon_package_registry(
                    &store,
                    &source,
                    cache_dir.as_deref(),
                    allow_remote,
                    max_bytes,
                    max_packages,
                    lock_path.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::PackageLock {
                repository,
                channel,
                addon,
                status,
                write,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = create_addon_package_lock(
                    &store,
                    repository.as_deref(),
                    channel.as_deref(),
                    addon.as_deref(),
                    status.as_deref(),
                    write.as_deref(),
                    limit,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Marketplace {
                repository,
                channel,
                addon,
                status,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_addon_marketplace(
                    &store,
                    repository.as_deref(),
                    channel.as_deref(),
                    addon.as_deref(),
                    status.as_deref(),
                    limit,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::InstallPackage {
                package_path,
                addon_dirs,
                lock_path,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report =
                    install_addon_package(&store, &package_path, &dirs, lock_path.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::MigrationWorkflow {
                from_manifest,
                to_manifest,
                action,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = create_addon_migration_workflow(
                    &store,
                    &from_manifest,
                    &to_manifest,
                    &action,
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Upgrade {
                manifest,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = upgrade_addon(&store, &manifest, &dirs)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Downgrade {
                manifest,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = downgrade_addon(&store, &manifest, &dirs)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Enable {
                id,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = enable_addon(&store, &id, &dirs)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Disable {
                id,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = disable_addon(&store, &id, &dirs)?;
                print_response(output, &report)?;
                Ok(0)
            }
            AddonCommands::Uninstall {
                id,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = uninstall_addon(&store, &id, &dirs)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Cli { command } => match command {
            CliCommands::Create {
                name,
                goal,
                source,
                commands,
                compound_commands,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = create_cli_factory_plan(
                    &store,
                    CliFactoryCreateInput {
                        name,
                        goal,
                        source,
                        commands,
                        compound_commands,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Harness { command } => match command {
            HarnessCommands::TokenHeadroom {
                content,
                content_kind,
                budget_tokens,
                source,
                reversible,
                persist,
                output,
            } => {
                let report = analyze_token_headroom(
                    &content,
                    content_kind.as_deref(),
                    budget_tokens,
                    &source,
                    reversible,
                );
                let report = if persist {
                    let store = FoundryStore::open(cli.store)?;
                    persist_token_headroom_report(&store, report, &content)?
                } else {
                    report
                };
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::RetrieveHeadroom {
                retrieval_ref,
                include_content,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = retrieve_headroom_blob(&store, &retrieval_ref, include_content)?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::HeadroomStats {
                source,
                content_kind,
                limit,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_headroom_stats_report(
                    &store,
                    HeadroomStatsOptions {
                        source: source.as_deref(),
                        content_kind: content_kind.as_deref(),
                        limit,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::Mode {
                foundry_first,
                observe_only,
                project_root,
                output,
            } => {
                let report = build_harness_mode_report(HarnessModeOptions {
                    foundry_first,
                    observe_only,
                    project_root: project_root.as_deref(),
                });
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::Doctor {
                executor,
                shim_dir,
                foundry_first,
                observe_only,
                project_root,
                workflow_id,
                task_id,
                run_id,
                context_budget,
                token_headroom,
                no_token_headroom,
                output,
            } => {
                let (effective_foundry_first, _) = resolve_harness_foundry_first_source_for_project(
                    foundry_first,
                    observe_only,
                    project_root.as_deref(),
                );
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: project_root.as_deref(),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first: effective_foundry_first,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = build_harness_doctor_report(HarnessDoctorOptions {
                    shim_dir: &shim_dir,
                    executor: &executor,
                    foundry_first,
                    observe_only,
                    project_root: project_root.as_deref(),
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    run_id: run_id.as_deref(),
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    require_token_headroom_for_foundry_first: runtime_policy
                        .require_token_headroom_for_foundry_first,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::HeadroomPlan {
                executor,
                command,
                foundry_first,
                observe_only,
                project_root,
                workflow_id,
                task_id,
                run_id,
                context_budget,
                token_headroom,
                no_token_headroom,
                output,
            } => {
                let (foundry_first, foundry_first_source) =
                    resolve_harness_foundry_first_source_for_project(
                        foundry_first,
                        observe_only,
                        project_root.as_deref(),
                    );
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: project_root.as_deref(),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = build_harness_headroom_plan(HarnessHeadroomPlanOptions {
                    executor: &executor,
                    command: &command,
                    foundry_first,
                    foundry_first_source,
                    project_root: project_root.as_deref(),
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    run_id: run_id.as_deref(),
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    require_token_headroom_for_foundry_first: runtime_policy
                        .require_token_headroom_for_foundry_first,
                });
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::AdoptionPlan {
                executor,
                shim_dir,
                foundry_first,
                observe_only,
                project_root,
                workflow_id,
                task_id,
                run_id,
                context_budget,
                token_headroom,
                no_token_headroom,
                output,
            } => {
                let (effective_foundry_first, _) = resolve_harness_foundry_first_source_for_project(
                    foundry_first,
                    observe_only,
                    project_root.as_deref(),
                );
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: project_root.as_deref(),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first: effective_foundry_first,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = build_harness_adoption_plan(HarnessAdoptionPlanOptions {
                    shim_dir: &shim_dir,
                    executor: &executor,
                    foundry_first,
                    observe_only,
                    project_root: project_root.as_deref(),
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    run_id: run_id.as_deref(),
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    require_token_headroom_for_foundry_first: runtime_policy
                        .require_token_headroom_for_foundry_first,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::ActivationProfile {
                shim_dir,
                executor,
                project_root,
                shell_rc,
                apply,
                approved_by,
                context_budget,
                token_headroom,
                no_token_headroom,
                output,
            } => {
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: project_root.as_deref(),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first: true,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = build_harness_activation_profile(HarnessActivationProfileOptions {
                    shim_dir: &shim_dir,
                    executor: &executor,
                    project_root: project_root.as_deref(),
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    apply,
                    shell_rc: shell_rc.as_deref(),
                    approved_by: approved_by.as_deref(),
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::WrapPlan {
                executor,
                command,
                foundry_first,
                observe_only,
                workflow_id,
                task_id,
                run_id,
                context_budget,
                token_headroom,
                no_token_headroom,
                project_root,
                output,
            } => {
                let (foundry_first, foundry_first_source) =
                    resolve_harness_foundry_first_source_for_project(
                        foundry_first,
                        observe_only,
                        project_root.as_deref(),
                    );
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: project_root.as_deref(),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = build_cli_wrapper_plan(CliWrapperPlanOptions {
                    executor: &executor,
                    command: &command,
                    foundry_first,
                    foundry_first_source,
                    project_root: project_root.as_deref(),
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    run_id: run_id.as_deref(),
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    require_token_headroom_for_foundry_first: runtime_policy
                        .require_token_headroom_for_foundry_first,
                });
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::Bootstrap {
                executor,
                shim_dir,
                project_root,
                context_budget,
                token_headroom,
                no_token_headroom,
                apply,
                approved_by,
                force,
                output,
            } => {
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: Some(project_root.as_path()),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first: true,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = build_harness_bootstrap_report(HarnessBootstrapOptions {
                    shim_dir: &shim_dir,
                    executor: &executor,
                    project_root: &project_root,
                    store_path: Some(cli.store.as_path()),
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    apply,
                    approved_by: approved_by.as_deref(),
                    force,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::InstallShims {
                shim_dir,
                executor,
                real_cmd,
                foundry_first,
                observe_only,
                workflow_id,
                task_id,
                run_id,
                context_budget,
                token_headroom,
                no_token_headroom,
                project_root,
                force,
                output,
            } => {
                let (foundry_first, foundry_first_source) =
                    resolve_harness_foundry_first_source_for_project(
                        foundry_first,
                        observe_only,
                        project_root.as_deref(),
                    );
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: project_root.as_deref(),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = install_cli_harness_shim(CliShimInstallOptions {
                    shim_dir: &shim_dir,
                    executor: &executor,
                    real_cmd: real_cmd.as_deref(),
                    store_path: Some(cli.store.as_path()),
                    foundry_first,
                    foundry_first_source,
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    run_id: run_id.as_deref(),
                    context_budget: runtime_policy.context_budget,
                    token_headroom: runtime_policy.token_headroom,
                    force,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::InstallProviderAdapter {
                shim_dir,
                executor,
                real_cmd,
                project_root,
                token_headroom,
                no_token_headroom,
                force,
                output,
            } => {
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: project_root.as_deref(),
                    context_budget: None,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first: true,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = install_cli_provider_adapter(ProviderAdapterInstallOptions {
                    shim_dir: &shim_dir,
                    executor: &executor,
                    real_cmd: real_cmd.as_deref(),
                    project_root: project_root.as_deref(),
                    token_headroom: runtime_policy.token_headroom,
                    force,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::ShimStatus {
                shim_dir,
                executor,
                output,
            } => {
                let report = inspect_cli_harness_shim_status(CliShimStatusOptions {
                    shim_dir: &shim_dir,
                    executor: &executor,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            HarnessCommands::Exec {
                executor,
                foundry_first,
                observe_only,
                workflow_id,
                task_id,
                run_id,
                context_budget,
                token_headroom,
                no_token_headroom,
                execute,
                allow_exec,
                secret_env,
                secret_permissions,
                project_root,
                cwd,
                command,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let effective_project_root = if let Some(workflow_id) = workflow_id.as_deref() {
                    resolve_effective_project_root(
                        &store,
                        workflow_id,
                        task_id.as_deref(),
                        project_root.as_deref(),
                    )?
                } else {
                    project_root.clone()
                };
                let bound_root = workflow_id
                    .as_deref()
                    .map(|workflow_id| {
                        resolve_effective_project_root(
                            &store,
                            workflow_id,
                            task_id.as_deref(),
                            None,
                        )
                    })
                    .transpose()?
                    .flatten();
                let effective_cwd = if let Some(bound_root) = bound_root.as_ref() {
                    if let Some(explicit_cwd) = cwd.as_deref() {
                        let explicit_cwd = std::fs::canonicalize(explicit_cwd)?;
                        let canonical_bound = std::fs::canonicalize(bound_root)?;
                        if explicit_cwd != canonical_bound {
                            anyhow::bail!(
                                "explicit cwd {} conflicts with bound worktree {}",
                                explicit_cwd.display(),
                                canonical_bound.display()
                            );
                        }
                    }
                    Some(bound_root.clone())
                } else {
                    cwd.clone()
                };
                let (foundry_first, foundry_first_source) =
                    resolve_harness_foundry_first_source_for_project(
                        foundry_first,
                        observe_only,
                        effective_project_root.as_deref(),
                    );
                let (token_headroom_input, token_headroom_source) =
                    harness_cli_token_headroom_input(token_headroom, no_token_headroom);
                let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                    project_root: effective_project_root.as_deref(),
                    context_budget,
                    context_budget_source: "explicit_flag",
                    token_headroom: token_headroom_input,
                    token_headroom_source,
                    foundry_first,
                    default_context_budget: DEFAULT_CONTEXT_BUDGET,
                });
                let report = run_cli_harness_exec(CliHarnessExecOptions {
                    store: Some(&store),
                    executor: &executor,
                    command: &command,
                    foundry_first,
                    foundry_first_source,
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    run_id: run_id.as_deref(),
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    require_token_headroom_for_foundry_first: runtime_policy
                        .require_token_headroom_for_foundry_first,
                    dry_run: !execute,
                    allow_exec,
                    secret_env: &secret_env,
                    secret_permissions: &secret_permissions,
                    project_root: effective_project_root.as_deref(),
                    cwd: effective_cwd.as_deref(),
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Cost { command } => match command {
            CostCommands::Ledger {
                workflow,
                organization,
                brand,
                product,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = build_cost_ledger_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            CostCommands::Materialize {
                workflow,
                organization,
                brand,
                product,
                source_kind,
                addon,
                limit,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = materialize_cost_ledger_index_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    source_kind.as_deref(),
                    addon.as_deref(),
                    limit,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            CostCommands::Incremental {
                after_sequence,
                organization,
                brand,
                product,
                source_kind,
                addon,
                limit,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = materialize_cost_ledger_incremental_for_context(
                    &store,
                    after_sequence,
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    source_kind.as_deref(),
                    addon.as_deref(),
                    limit,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            CostCommands::History {
                workflow,
                organization,
                brand,
                product,
                source_kind,
                addon,
                bucket,
                group_by,
                limit,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = build_cost_ledger_history_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    source_kind.as_deref(),
                    addon.as_deref(),
                    Some(&bucket),
                    Some(&group_by),
                    limit,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            CostCommands::Maintain {
                workflow,
                organization,
                brand,
                product,
                source_kind,
                addon,
                bucket,
                group_by,
                limit,
                retention_days,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = maintain_cost_ledger_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    source_kind.as_deref(),
                    addon.as_deref(),
                    Some(&bucket),
                    Some(&group_by),
                    limit,
                    retention_days,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            CostCommands::Daemon {
                workflow,
                organization,
                brand,
                product,
                source_kind,
                addon,
                bucket,
                group_by,
                limit,
                retention_days,
                max_cycles,
                interval_seconds,
                idle_exit,
                origin,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = run_cost_ledger_daemon_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    source_kind.as_deref(),
                    addon.as_deref(),
                    Some(&bucket),
                    Some(&group_by),
                    limit,
                    retention_days,
                    max_cycles,
                    interval_seconds,
                    idle_exit,
                    &origin,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            CostCommands::Retention {
                workflow,
                organization,
                brand,
                product,
                source_kind,
                addon,
                retention_days,
                limit,
                apply,
                approved_by,
                reason,
                confirm,
                origin,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let operating_context = load_project_operating_context(&project_root)?;
                let report = apply_cost_ledger_retention_for_context(
                    &store,
                    workflow.as_deref(),
                    organization.as_deref(),
                    brand.as_deref(),
                    product.as_deref(),
                    source_kind.as_deref(),
                    addon.as_deref(),
                    retention_days,
                    limit,
                    apply,
                    approved_by.as_deref(),
                    reason.as_deref(),
                    confirm,
                    &origin,
                    &operating_context,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Skill { command } => match command {
            SkillCommands::Install {
                home,
                target,
                executor_paths,
                shim_dirs,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let home = home.unwrap_or_else(default_home_path);
                let store = FoundryStore::open(cli.store)?;
                let report = install_skill(&home, &target)?;
                let executor_sync = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home: home.clone(),
                        executor_paths,
                        shim_dirs,
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
                shim_dirs,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let home = home.unwrap_or_else(default_home_path);
                let store = FoundryStore::open(cli.store)?;
                let report = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home,
                        executor_paths,
                        shim_dirs,
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
                let home = home.unwrap_or_else(default_home_path);
                let store = FoundryStore::open(cli.store)?;
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
                shim_dirs,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let home = home.unwrap_or_else(default_home_path);
                let store = FoundryStore::open(cli.store)?;
                let executor_sync = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home: home.clone(),
                        executor_paths,
                        shim_dirs,
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
            let store = FoundryStore::open(cli.store)?;
            let report = load_executors(&store)?;
            print_response(output, &report)?;
            Ok(0)
        }
        Commands::Brains { output } => {
            let store = FoundryStore::open(cli.store)?;
            let report = load_executors(&store)?;
            print_response(output, &report.brain_router)?;
            Ok(0)
        }
        Commands::Sessions {
            command,
            provider_id,
            lifecycle_state,
            readiness,
            output,
        } => {
            let store = FoundryStore::open(cli.store)?;
            let report = load_executors(&store)?;
            match command {
                Some(SessionCommands::History { session_id, output }) => {
                    let history = build_brain_session_history_report(
                        &store,
                        &report.brain_router,
                        &session_id,
                    )?;
                    print_response(output, &history)?;
                }
                Some(SessionCommands::Lifecycle {
                    session_id,
                    state,
                    workflow_id,
                    task_id,
                    run_id,
                    origin,
                    note,
                    output,
                }) => {
                    let receipt = record_brain_session_lifecycle(
                        &store,
                        &report.brain_router,
                        BrainSessionLifecycleOptions {
                            session_id: &session_id,
                            state: &state,
                            workflow_id: workflow_id.as_deref(),
                            task_id: task_id.as_deref(),
                            run_id: run_id.as_deref(),
                            origin: &origin,
                            note: note.as_deref(),
                        },
                    )?;
                    print_response(output, &receipt)?;
                }
                None => {
                    let sessions = build_brain_sessions_report_with_options(
                        &store,
                        &report.brain_router,
                        BrainSessionsReportOptions {
                            provider_id,
                            lifecycle_state,
                            readiness,
                        },
                    )?;
                    print_response(output, &sessions)?;
                }
            }
            Ok(0)
        }
        Commands::Shells {
            executor,
            workflow,
            task,
            run_id,
            context_budget,
            ttl_seconds,
            record_session,
            origin,
            output,
        } => {
            let store = FoundryStore::open(cli.store)?;
            let report = load_executors(&store)?;
            let options = ShellLaunchPlanOptions {
                executor_filter: executor,
                workflow_id: workflow,
                task_id: task,
                run_id,
                context_budget: Some(context_budget),
                ttl_seconds: Some(ttl_seconds),
            };
            if record_session {
                let receipt =
                    record_shell_session_plan(&store, &report.brain_router, options, &origin)?;
                print_response(output, &receipt)?;
            } else {
                let launch_plan = build_shell_launch_plan(&report.brain_router, options);
                print_response(output, &launch_plan)?;
            }
            Ok(0)
        }
        Commands::ExecutorQuota { command } => match command {
            ExecutorQuotaCommands::AiLimits {
                ai_limits_cmd,
                timeout_ms,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = import_ai_limits_observations(&store, &ai_limits_cmd, timeout_ms)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ExecutorQuotaCommands::Decide(args) => {
                let store = FoundryStore::open(cli.store)?;
                let report = decide_executor_model_for_task(
                    &store,
                    ExecutorModelDecisionOptions {
                        task: args.task,
                        task_class: args.task_class,
                        difficulty: args.difficulty,
                        expected_input_tokens: args.expected_input_tokens,
                        expected_output_tokens: args.expected_output_tokens,
                        configured_decider: args.configured_decider,
                    },
                )?;
                print_response(args.output, &report)?;
                Ok(0)
            }
            ExecutorQuotaCommands::Record(args) => {
                let store = FoundryStore::open(cli.store)?;
                let observation = build_executor_quota_observation(
                    args.executor,
                    args.provider,
                    args.model,
                    args.locality,
                    args.free_vs_paid,
                    args.remaining_quota,
                    args.rate_limit_risk,
                    args.monetary_or_token_cost,
                    args.latency,
                    args.expected_quality,
                    args.suitability,
                    args.source,
                    args.observed_at,
                )?;
                store.save_executor_quota(
                    &observation.executor,
                    &observation.provider,
                    observation.model.as_deref().unwrap_or(""),
                    &serde_json::to_value(&observation)?,
                )?;
                let response = serde_json::json!({
                    "schema_version": "foundry.executor_quota_record.v1",
                    "status": "executor_quota_recorded",
                    "observation": observation,
                });
                store.record_event("_system", "executor_quota_recorded", &response)?;
                print_response(args.output, &response)?;
                Ok(0)
            }
        },
        Commands::Runtimes { output } => {
            let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
        Commands::Worktree { command } => match command {
            WorktreeCommands::Discover { repository, output } => {
                let report = discover_worktrees(&repository)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::Create {
                repository,
                path,
                branch,
                start_point,
                allow_repository_mutation,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = create_worktree(
                    &store,
                    WorktreeCreateOptions {
                        repository,
                        path,
                        branch,
                        start_point,
                        allow_repository_mutation,
                        origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::Register {
                path,
                id,
                workflow,
                task,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = register_worktree(
                    &store,
                    WorktreeRegisterOptions {
                        path,
                        id,
                        workflow_id: workflow,
                        task_id: task,
                        origin,
                        created_by_foundry: false,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::Bind {
                worktree,
                workflow,
                task,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = bind_worktree(&store, &worktree, &workflow, task.as_deref(), &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::Init {
                worktree,
                allow_worktree_write,
                force,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    initialize_worktree(&store, &worktree, allow_worktree_write, force, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::ApproveConfig {
                worktree,
                allow_guardrail_update,
                approved_by,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = approve_worktree_config(
                    &store,
                    &worktree,
                    allow_guardrail_update,
                    &approved_by,
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::List {
                repository,
                workflow,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    list_registered_worktrees(&store, repository.as_deref(), workflow.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::Inspect { worktree, output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = inspect_registered_worktree(&store, &worktree)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::PrepareTeamwork {
                workflow,
                repository,
                worktree_root,
                branch_prefix,
                origin,
                allow_repository_mutation,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = foundry_core::teamwork::prepare_teamwork_worktrees(
                    &store,
                    foundry_core::teamwork::TeamworkWorktreePrepareOptions {
                        workflow_id: workflow,
                        repository,
                        worktree_root,
                        branch_prefix,
                        origin,
                        allow_repository_mutation,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorktreeCommands::IntegrateDependencies {
                workflow,
                task,
                allow_repository_mutation,
                approved_by,
                reason,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = foundry_core::teamwork_fan_in::integrate_worktree_dependencies(
                    &store,
                    &foundry_core::teamwork_fan_in::IntegrateDependenciesOptions {
                        workflow_id: &workflow,
                        task_id: &task,
                        allow_repository_mutation,
                        approved_by: approved_by.as_deref().unwrap_or_default(),
                        reason: reason.as_deref().unwrap_or_default(),
                        origin: &origin,
                    },
                )?;
                let exit_code = i32::from(!report.success);
                print_response(output, &report)?;
                Ok(exit_code)
            }
            WorktreeCommands::Sandbox { command } => match command {
                WorktreeSandboxCommands::Plan {
                    worktree,
                    purpose,
                    workflow,
                    task,
                    command,
                    output,
                } => {
                    let store = FoundryStore::open(cli.store)?;
                    let report = plan_worktree_sandbox(
                        &store,
                        WorktreeSandboxRequest {
                            worktree,
                            purpose,
                            workflow_id: workflow,
                            task_id: task,
                            command,
                        },
                    )?;
                    let exit_code = if report.allowed { 0 } else { 1 };
                    print_response(output, &report)?;
                    Ok(exit_code)
                }
                WorktreeSandboxCommands::Run {
                    worktree,
                    purpose,
                    workflow,
                    task,
                    allow_exec,
                    command,
                    output,
                } => {
                    let store = FoundryStore::open(cli.store)?;
                    let report = run_worktree_sandbox(
                        &store,
                        WorktreeSandboxRequest {
                            worktree,
                            purpose,
                            workflow_id: workflow,
                            task_id: task,
                            command,
                        },
                        allow_exec,
                    )?;
                    let exit_code = if report.status == "sandbox_completed"
                        && report.executed
                        && report.exit_code == Some(0)
                        && !report.timed_out
                        && report.error.is_none()
                    {
                        0
                    } else {
                        1
                    };
                    print_response(output, &report)?;
                    Ok(exit_code)
                }
                WorktreeSandboxCommands::Start {
                    worktree,
                    purpose,
                    workflow,
                    task,
                    allow_exec,
                    command,
                    output,
                } => {
                    let store = FoundryStore::open(cli.store)?;
                    let report = start_worktree_sandbox(
                        &store,
                        WorktreeSandboxRequest {
                            worktree,
                            purpose,
                            workflow_id: workflow,
                            task_id: task,
                            command,
                        },
                        allow_exec,
                    )?;
                    let exit_code = if matches!(
                        report.status.as_str(),
                        "sandbox_starting" | "sandbox_running" | "sandbox_completed"
                    ) {
                        0
                    } else {
                        1
                    };
                    print_response(output, &report)?;
                    Ok(exit_code)
                }
                WorktreeSandboxCommands::Status { sandbox, output } => {
                    let store = FoundryStore::open(cli.store)?;
                    let report = inspect_worktree_sandbox_lifecycle(&store, &sandbox)?;
                    print_response(output, &report)?;
                    Ok(0)
                }
                WorktreeSandboxCommands::Stop {
                    sandbox,
                    allow_stop,
                    output,
                } => {
                    let store = FoundryStore::open(cli.store)?;
                    let report = stop_worktree_sandbox(&store, &sandbox, allow_stop)?;
                    let exit_code = if report.status == "sandbox_stop_failed" {
                        1
                    } else {
                        0
                    };
                    print_response(output, &report)?;
                    Ok(exit_code)
                }
                WorktreeSandboxCommands::Supervise {
                    sandbox,
                    allow_supervisor_exec,
                } => {
                    let store = FoundryStore::open(cli.store)?;
                    let _ = supervise_worktree_sandbox(&store, &sandbox, allow_supervisor_exec)?;
                    Ok(0)
                }
            },
            WorktreeCommands::Guard { command } => match command {
                WorktreeGuardCommands::Check {
                    worktree,
                    operation,
                    paths,
                    reason,
                    workflow,
                    task,
                    output,
                } => {
                    let store = FoundryStore::open(cli.store)?;
                    let report = evaluate_worktree_modification_guard(
                        &store,
                        WorktreeModificationGuardRequest {
                            worktree,
                            operation,
                            paths,
                            reason,
                            workflow_id: workflow,
                            task_id: task,
                        },
                    )?;
                    let exit_code = if report.allowed { 0 } else { 1 };
                    print_response(output, &report)?;
                    Ok(exit_code)
                }
                WorktreeGuardCommands::CreatePredecessor {
                    worktree,
                    workflow,
                    task,
                    paths,
                    goal,
                    allow_workflow_mutation,
                    approved_by,
                    origin,
                    output,
                } => {
                    let store = FoundryStore::open(cli.store)?;
                    let report = create_worktree_guard_predecessor_task(
                        &store,
                        &worktree,
                        &workflow,
                        &task,
                        paths,
                        &goal,
                        allow_workflow_mutation,
                        &approved_by,
                        &origin,
                    )?;
                    print_response(output, &report)?;
                    Ok(0)
                }
            },
        },
        Commands::Security { command } => match command {
            SecurityCommands::SecretScan {
                input,
                input_file,
                workflow_id,
                origin,
                scope,
                no_entropy,
                no_local_ai_fallback,
                allow_external_ai,
                output,
            } => {
                let text = if let Some(input) = input {
                    input
                } else if let Some(input_file) = input_file {
                    std::fs::read_to_string(&input_file)?
                } else {
                    String::new()
                };
                let options = SecretSanitizationOptions {
                    scope,
                    enable_entropy: !no_entropy,
                    enable_local_ai_fallback: !no_local_ai_fallback,
                    allow_external_ai,
                    ..SecretSanitizationOptions::default()
                };
                let store = FoundryStore::open(cli.store)?;
                let tenant_context = workflow_id
                    .as_deref()
                    .and_then(|workflow_id| store.load_workflow(workflow_id).ok())
                    .map(|workflow| serde_json::to_value(&workflow.intent.operating_context))
                    .transpose()?
                    .unwrap_or_else(|| {
                        serde_json::to_value(OperatingContextSpec::default())
                            .unwrap_or_else(|_| serde_json::json!({}))
                    });
                let report = sanitize_prompt_secrets_with_vault(
                    &text,
                    options,
                    SecretVaultPersistOptions {
                        store: &store,
                        workflow_id: workflow_id.as_deref(),
                        origin: &origin,
                        tenant_context: &tenant_context,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = list_workflows_with_filters(
                    &store,
                    WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All)
                        .only_scheduled_or_looping(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Inspect { workflow, output } => {
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = update_loop_state(&store, &workflow, &task, "stopped", &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::RunDue { workflow, output } => {
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = if max_workers > 1 {
                    scan_due_workflows_parallel(&store, &executor, max_workers, ttl_seconds)?
                } else {
                    scan_due_workflows(&store, &executor, ttl_seconds)?
                };
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::Summary { output } => {
                let store = FoundryStore::open(cli.store)?;
                let workflows = store.load_workflows()?;
                let task_slices: Vec<&[foundry_core::graph::AtomicTask]> =
                    workflows.iter().map(|wf| wf.tasks.as_slice()).collect();
                let report = aggregate_summary(&task_slices);
                print_response(output, &report)?;
                Ok(0)
            }
            ScheduleCommands::LoopSummary { output } => {
                let store = FoundryStore::open(cli.store)?;
                let workflows = store.load_workflows()?;
                let task_slices: Vec<&[foundry_core::graph::AtomicTask]> =
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = list_cluster_nodes(&store)?;
                print_response(output, &report)?;
                Ok(0)
            }
            ClusterCommands::Leases { node_id, output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_cluster_node_leases(&store, node_id.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            ClusterCommands::Place {
                workflow,
                task,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = update_workflow_goal_with_expected_revision(
                    &store,
                    &workflow,
                    &goal,
                    &origin,
                    expected_revision,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::AddTask {
                workflow,
                description,
                priority,
                task_id,
                origin,
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = add_workflow_task(
                    &store,
                    &workflow,
                    WorkflowTaskAddInput {
                        task_id,
                        description,
                        priority,
                        origin,
                        expected_revision,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::UpdateTask {
                workflow,
                task,
                title,
                goal,
                expected_output,
                origin,
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = update_workflow_task_with_expected_revision(
                    &store,
                    &workflow,
                    WorkflowTaskUpdateInput {
                        task_id: &task,
                        title: title.as_deref(),
                        goal: goal.as_deref(),
                        expected_output: expected_output.as_deref(),
                        origin: &origin,
                    },
                    expected_revision,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::SetPriority {
                workflow,
                task,
                priority,
                origin,
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = set_workflow_task_priority(
                    &store,
                    &workflow,
                    WorkflowTaskPriorityInput {
                        task_id: task,
                        priority,
                        origin,
                        expected_revision,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::AddDependency {
                workflow,
                task,
                depends_on,
                origin,
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = add_workflow_task_dependency(
                    &store,
                    &workflow,
                    WorkflowTaskDependencyInput {
                        task_id: task,
                        dependency_task_id: depends_on,
                        origin,
                        expected_revision,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::RemoveDependency {
                workflow,
                task,
                depends_on,
                origin,
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = remove_workflow_task_dependency(
                    &store,
                    &workflow,
                    WorkflowTaskDependencyInput {
                        task_id: task,
                        dependency_task_id: depends_on,
                        origin,
                        expected_revision,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::SetImpediment {
                workflow,
                task,
                reason,
                kind,
                origin,
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = set_workflow_task_impediment(
                    &store,
                    &workflow,
                    WorkflowTaskImpedimentInput {
                        task_id: task,
                        reason,
                        kind,
                        origin,
                        expected_revision,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::ClearImpediment {
                workflow,
                task,
                impediment_id,
                origin,
                expected_revision,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = clear_workflow_task_impediment(
                    &store,
                    &workflow,
                    WorkflowTaskImpedimentClearInput {
                        task_id: task,
                        impediment_id,
                        origin,
                        expected_revision,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::UpdateNodeBrain {
                workflow,
                task,
                default_brain,
                allowed_brains,
                agent_slots,
                max_parallel_agents,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let parsed_slots = agent_slots
                    .iter()
                    .map(|slot| parse_node_brain_agent_slot(slot))
                    .collect::<Result<Vec<_>>>()?;
                let report = update_workflow_node_brain_routing(
                    &store,
                    &workflow,
                    WorkflowNodeBrainRoutingUpdateInput {
                        task_id: task,
                        default_brain,
                        allowed_brains,
                        agent_slots: parsed_slots,
                        max_parallel_agents,
                        origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::AttachArtifact {
                workflow,
                path,
                kind,
                tags,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = attach_workflow_artifact_with_tags(
                    &store, &workflow, &path, &kind, &origin, &tags,
                )?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let artifact = match kind.as_str() {
                    "screen" => CreativeArtifact::new_screen(
                        &title,
                        foundry_core::ir::ScreenSpec {
                            schema_version: foundry_core::ir::ir_schema_version(),
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
                        foundry_core::ir::WhiteboardSpec {
                            schema_version: foundry_core::ir::ir_schema_version(),
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
                        foundry_core::ir::DocumentSpec {
                            schema_version: foundry_core::ir::ir_schema_version(),
                            title: title.clone(),
                            author: origin.clone(),
                            front_matter: std::collections::BTreeMap::new(),
                            sections: Vec::new(),
                        },
                    ),
                    "slide_deck" => CreativeArtifact::new_slide_deck(
                        &title,
                        foundry_core::ir::SlideDeckSpec {
                            schema_version: foundry_core::ir::ir_schema_version(),
                            title: title.clone(),
                            theme: "default".to_string(),
                            slides: Vec::new(),
                        },
                    ),
                    "component" => CreativeArtifact::new_component(
                        &title,
                        foundry_core::ir::ComponentSpec {
                            schema_version: foundry_core::ir::ir_schema_version(),
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
                let store = FoundryStore::open(cli.store)?;
                let report = list_creative_artifacts(&store, &workflow)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::InspectCreative {
                workflow,
                artifact,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let token_collection = TokenCollection {
                    schema_version: foundry_core::ir::ir_schema_version(),
                    description: format!("Design tokens for {name}"),
                    tokens: vec![
                        foundry_core::ir::DesignToken {
                            name: "color.primary".to_string(),
                            value: "#3B82F6".to_string(),
                            token_type: foundry_core::ir::TokenType::Color,
                            description: "Primary brand color".to_string(),
                            group: "color".to_string(),
                            extensions: std::collections::BTreeMap::new(),
                        },
                        foundry_core::ir::DesignToken {
                            name: "spacing.md".to_string(),
                            value: "16px".to_string(),
                            token_type: foundry_core::ir::TokenType::Spacing,
                            description: "Medium spacing".to_string(),
                            group: "spacing".to_string(),
                            extensions: std::collections::BTreeMap::new(),
                        },
                    ],
                    semantic_aliases: vec![foundry_core::ir::SemanticAlias {
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
                let store = FoundryStore::open(cli.store)?;
                let report = get_workflow_token_collection(&store, &workflow)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::ResolveTokens {
                workflow,
                mode,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = patch_workflow_token(&store, &workflow, &token, &value, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::Decision {
                workflow,
                title,
                rationale,
                alternatives,
                trade_offs,
                success_metrics,
                backlog_mutation,
                author,
                affected_goals,
                affected_tasks,
                affected_artifacts,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = foundry_core::workflow::record_product_decision(
                    &store,
                    &workflow,
                    ProductDecisionInput {
                        title,
                        rationale,
                        alternatives,
                        trade_offs,
                        success_metrics,
                        backlog_mutation,
                        author,
                        affected_goals,
                        affected_tasks,
                        affected_artifacts,
                        origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Task { command } => match command {
            TaskCommands::Handoff {
                workflow,
                task,
                executor,
                project_root,
                budget,
                ttl_seconds,
                view,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_task_handoff_response_with_project(
                    &store,
                    &workflow,
                    &task,
                    &executor,
                    budget,
                    ttl_seconds,
                    project_root.as_deref(),
                    match view {
                        ContextViewArg::Compact => TaskHandoffView::Compact,
                        ContextViewArg::Full => TaskHandoffView::Full,
                    },
                )?;
                let exit_code = if report.allowed() { 0 } else { 1 };
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = validate_executor_response_file(&store, &workflow, &task, &response)?;
                let exit_code = if report.accepted { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
        },
        Commands::Request { command } => match command {
            RequestCommands::Start {
                goal,
                worktree,
                lanes,
                max_parallel_agents,
                origin,
                idempotency_key,
                output,
                detached,
            } => {
                let store_path = cli.store.clone();
                let store = FoundryStore::open(store_path.clone())?;
                let selected_project_root = worktree
                    .as_deref()
                    .map(|selector| resolve_worktree_selector_root(&store, selector))
                    .transpose()?;
                let parallel_team = explicit_parallel_team_from_lane_specs(
                    &lanes,
                    max_parallel_agents,
                    "foundry.request.start.cli",
                )?;
                let mut report = if let Some(project_root) = selected_project_root.as_deref() {
                    start_async_request_with_project_idempotency_and_parallel_team(
                        &store,
                        &goal,
                        &origin,
                        project_root,
                        idempotency_key.as_deref(),
                        parallel_team.clone(),
                    )?
                } else {
                    start_async_request_with_idempotency_and_parallel_team(
                        &store,
                        &goal,
                        &origin,
                        idempotency_key.as_deref(),
                        parallel_team,
                    )?
                };
                if let Some(selector) = worktree {
                    if PathBuf::from(&selector).exists() {
                        register_worktree(
                            &store,
                            WorktreeRegisterOptions {
                                path: PathBuf::from(selector),
                                id: None,
                                workflow_id: Some(report.workflow_id.clone()),
                                task_id: None,
                                origin: origin.clone(),
                                created_by_foundry: false,
                            },
                        )?;
                    } else {
                        bind_worktree(&store, &selector, &report.workflow_id, None, &origin)?;
                    }
                    report.worktree = bound_worktree_context(&store, &report.workflow_id, None)?;
                }
                print_response(output, &report)?;
                if detached && !report.idempotent_replay {
                    let current_exe = std::env::current_exe()?;
                    let child = std::process::Command::new(current_exe)
                        .arg("--store")
                        .arg(&store_path)
                        .arg("request")
                        .arg("drive-loop")
                        .arg("--run")
                        .arg(&report.run_id)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()?;
                    store.record_event(
                        &report.workflow_id,
                        "async_request_detached_driver_spawned",
                        &serde_json::json!({
                            "schema_version": "foundry.request_detached_driver.v1",
                            "run_id": report.run_id,
                            "pid": child.id(),
                            "origin": origin,
                        }),
                    )?;
                }
                Ok(0)
            }
            RequestCommands::Status { run_id, output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = load_request_status(&store, &run_id)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Drive {
                run_id,
                executor,
                ttl_seconds,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = drive_request(&store, &run_id, &executor, ttl_seconds, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Step {
                run_id,
                executor,
                ttl_seconds,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = step_request(&store, &run_id, &executor, ttl_seconds, &origin)?;
                let exit_code = if report.status == "validation_failed" {
                    1
                } else {
                    0
                };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            RequestCommands::ExecuteWave {
                run_id,
                executor,
                ttl_seconds,
                timeout_seconds,
                context_budget,
                max_parallel,
                allow_exec,
                approved_by,
                reason,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = execute_request_executor_wave(
                    &store,
                    &RequestExecutorWaveOptions {
                        run_id: &run_id,
                        requested_executor: &executor,
                        ttl_seconds,
                        timeout_seconds,
                        context_budget,
                        max_parallel,
                        allow_exec,
                        approved_by: &approved_by,
                        reason: &reason,
                        origin: &origin,
                    },
                )?;
                let exit_code = if !report.success && report.status != "execution_not_started" {
                    1
                } else {
                    0
                };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            RequestCommands::CompleteTask {
                run_id,
                task,
                executor,
                summary,
                artifacts,
                evidence_command,
                evidence_exit_code,
                evidence_summary,
                estimated_usd,
                tokens_in,
                tokens_out,
                ttl_seconds,
                context_budget,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = complete_ready_task(
                    &store,
                    &run_id,
                    RequestTaskCompletionInput {
                        task_id: &task,
                        executor: &executor,
                        summary: &summary,
                        artifact_paths: &artifacts,
                        evidence_command: evidence_command.as_deref(),
                        evidence_exit_code,
                        evidence_summary: evidence_summary.as_deref(),
                        estimated_usd,
                        tokens_in,
                        tokens_out,
                        ttl_seconds,
                        context_budget,
                        origin: &origin,
                    },
                )?;
                let exit_code =
                    if matches!(report.status.as_str(), "not_ready" | "validation_failed") {
                        1
                    } else {
                        0
                    };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            RequestCommands::FinalPackage {
                run_id,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = create_final_delivery_package(&store, &run_id, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::EnsureFinalAudit {
                workflow,
                executor,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = ensure_final_audit(&store, &workflow, &executor, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::List { status, output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_requests(&store, status.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Cancel {
                run_id,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = cancel_request(&store, &run_id, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Resume {
                run_id,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = resume_async_request(&store, &run_id, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Heartbeat {
                run_id,
                executor,
                summary,
                ttl_seconds,
                pid,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = heartbeat_request(
                    &store,
                    &run_id,
                    &executor,
                    &summary,
                    ttl_seconds,
                    pid,
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::SwitchExecutor {
                run_id,
                executor,
                fallback_executors,
                summary,
                ttl_seconds,
                pid,
                reason,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = switch_request_executor(
                    &store,
                    &run_id,
                    RequestExecutorSwitchInput {
                        executor,
                        fallback_executors,
                        summary,
                        ttl_seconds,
                        pid,
                        origin,
                        reason,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::RecoverStale {
                run_id,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = recover_stale_request(&store, &run_id, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Supervise {
                executor,
                origin,
                ttl_seconds,
                max_steps_per_run,
                continuous,
                max_cycles,
                interval_seconds,
                output,
            } => {
                if continuous && interval_seconds == 0 {
                    anyhow::bail!(
                        "continuous request supervision requires interval-seconds at least 1"
                    );
                }
                if !continuous && max_cycles != 1 {
                    anyhow::bail!("max-cycles only applies continuous request supervision");
                }

                let store = FoundryStore::open(cli.store)?;
                let options =
                    RequestSupervisorOptions::new(executor, origin, ttl_seconds, max_steps_per_run);
                let mut cycle = 0usize;
                loop {
                    cycle = cycle
                        .checked_add(1)
                        .context("request supervisor cycle overflow")?;
                    let report = supervise_requests_once(&store, &options)?;
                    let failed = report.status == "request_supervisor_completed_with_failures";
                    print_response(output, &report)?;
                    if failed {
                        return Ok(1);
                    }
                    if !continuous || (max_cycles > 0 && cycle >= max_cycles) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(interval_seconds));
                }
                Ok(0)
            }
            RequestCommands::DriveLoop {
                run_id,
                executor,
                ttl_seconds,
                origin,
            } => {
                let store = FoundryStore::open(cli.store.clone())?;
                loop {
                    let report = step_request(&store, &run_id, &executor, ttl_seconds, &origin)?;
                    let stops_loop = |status: &str| {
                        matches!(
                            status,
                            "complete"
                                | "completed"
                                | "failed"
                                | "cancelled"
                                | "blocked"
                                | "needs_attention"
                                | "completion_audit_required"
                                | "rework_required"
                                | "handoff_required"
                                | "validation_failed"
                        )
                    };
                    let needs_attention = |status: &str| {
                        matches!(
                            status,
                            "completion_audit_required"
                                | "rework_required"
                                | "handoff_required"
                                | "validation_failed"
                        )
                    };
                    let attention_boundary = needs_attention(&report.status)
                        || needs_attention(&report.drive_before.status)
                        || report
                            .drive_after
                            .as_ref()
                            .is_some_and(|drive| needs_attention(&drive.status));
                    if stops_loop(&report.status)
                        || stops_loop(&report.drive_before.status)
                        || report
                            .drive_after
                            .as_ref()
                            .is_some_and(|drive| stops_loop(&drive.status))
                    {
                        if attention_boundary {
                            let supervisor = supervise_request_once(
                                &store,
                                &run_id,
                                &RequestSupervisorOptions::new(
                                    executor.clone(),
                                    origin.clone(),
                                    ttl_seconds,
                                    1,
                                ),
                            )?;
                            if supervisor.error.is_some() {
                                anyhow::bail!(
                                    "request drive loop could not persist manual-attention boundary"
                                );
                            }
                        }
                        break;
                    }
                }
                Ok(0)
            }
        },
        Commands::Memory { command } => match command {
            MemoryCommands::Policy {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = memory_policy_report_for_project(&store, project_root.as_deref());
                print_response(output, &report)?;
                Ok(0)
            }
            MemoryCommands::Configure {
                project_root,
                memory_level,
                default_scopes,
                default_audience,
                privacy_mode,
                retention_mode,
                approved_by,
                reason,
                output,
            } => {
                let report = configure_memory_governance(MemoryGovernanceConfigOptions {
                    project_root,
                    memory_level,
                    default_scopes,
                    default_audience,
                    privacy_mode,
                    retention_mode,
                    approved_by,
                    reason,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            MemoryCommands::Search {
                query,
                workflow_id,
                scopes,
                audience,
                visibility,
                memory_level,
                run_id,
                organization_id,
                limit,
                global_root,
                organization_root,
                project_root,
                processing_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = search_memory(
                    &store,
                    MemorySearchOptions {
                        query,
                        workflow_id,
                        scopes,
                        audience,
                        visibility,
                        memory_level,
                        run_id,
                        organization_id,
                        limit,
                        global_root,
                        organization_root,
                        project_root,
                        processing_root,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MemoryCommands::Promote {
                workflow_id,
                from_scope,
                to_scope,
                source_path,
                source_start_line,
                source_end_line,
                summary,
                approved_by,
                reason,
                visibility,
                shareability,
                organization_id,
                global_root,
                organization_root,
                project_root,
                dry_run,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = promote_memory(
                    &store,
                    MemoryPromotionOptions {
                        workflow_id,
                        from_scope,
                        to_scope,
                        source_path,
                        source_start_line,
                        source_end_line,
                        summary,
                        approved_by,
                        reason,
                        visibility,
                        shareability,
                        organization_id,
                        global_root,
                        organization_root,
                        project_root,
                        dry_run,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MemoryCommands::Promotions {
                workflow_id,
                from_scope,
                to_scope,
                approved_by,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    list_memory_promotions(&store, from_scope, to_scope, approved_by, workflow_id)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MemoryCommands::Retention {
                workflow_id,
                scopes,
                run_id,
                organization_id,
                global_root,
                organization_root,
                project_root,
                processing_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = memory_retention_report(
                    &store,
                    MemoryRetentionOptions {
                        workflow_id,
                        scopes,
                        run_id,
                        organization_id,
                        global_root,
                        organization_root,
                        project_root,
                        processing_root,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MemoryCommands::Cleanup {
                workflow_id,
                scopes,
                run_id,
                organization_id,
                global_root,
                organization_root,
                project_root,
                processing_root,
                mode,
                archive_root,
                approved_by,
                reason,
                dry_run,
                confirm,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = memory_cleanup_report(
                    &store,
                    MemoryCleanupOptions {
                        workflow_id,
                        scopes,
                        run_id,
                        organization_id,
                        global_root,
                        organization_root,
                        project_root,
                        processing_root,
                        mode,
                        archive_root,
                        approved_by,
                        reason,
                        dry_run,
                        confirm,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Mcp { command } => match command {
            McpCommands::Serve => {
                let store = FoundryStore::open(cli.store)?;
                serve_stdio(&store)?;
                Ok(0)
            }
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
                let store = FoundryStore::open(cli.store)?;
                let input = read_mcp_input(input, input_file)?;
                let report = call_mcp_tool(&store, &tool, input)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Tui {
            project_root,
            output,
        } => {
            let store = FoundryStore::open(cli.store)?;
            let report = build_foundry_tui(&store, project_root)?;
            match output {
                OutputFormat::Json => print_response(output, &report)?,
                OutputFormat::Human => println!("{}", render_foundry_tui(&report)),
            }
            Ok(0)
        }
        Commands::Interactive { command } => match command {
            InteractiveCommands::Home {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_home_with_options(
                    &store,
                    InteractiveHomeOptions { project_root },
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_home(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::GuidedCockpit { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_guided_cockpit(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_guided_cockpit(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::UiComposition {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_ui_composition(&store, project_root)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_ui_composition(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::Readiness { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_readiness(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_readiness(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::ReplacementCli {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_replacement_cli_with_options(
                    &store,
                    InteractiveReplacementCliOptions { project_root },
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_replacement_cli(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::MultimodalRuntime {
                project_root,
                enable_experimental,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_multimodal_runtime(
                    &store,
                    &project_root,
                    enable_experimental,
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_multimodal_runtime(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::AddonCapabilities {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_addon_capabilities_for_project(
                    &store,
                    project_root.as_deref(),
                );
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_addon_capabilities(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::CoreBoundary {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    build_interactive_core_boundary_for_project(&store, project_root.as_deref());
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_core_boundary(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::Architecture {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_architecture_compass(&store, project_root)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_architecture_compass(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::OperationalCockpit { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_operational_cockpit(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_operational_cockpit(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::ImprovementLoop { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_improvement_loop(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_improvement_loop(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::ReleaseGates {
                version,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    build_interactive_release_gates(&store, &version, project_root.as_deref())?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_release_gates(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::Harness {
                executor,
                shim_dir,
                project_root,
                foundry_first,
                observe_only,
                workflow_id,
                task_id,
                run_id,
                context_budget,
                token_headroom,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let mut options = InteractiveHarnessOptions::default_for_current_dir();
                options.executor = executor;
                if let Some(shim_dir) = shim_dir {
                    options.shim_dir = shim_dir;
                }
                if let Some(project_root) = project_root {
                    options.project_root = Some(project_root);
                }
                options.foundry_first = foundry_first;
                options.observe_only = observe_only;
                options.workflow_id = workflow_id;
                options.task_id = task_id;
                options.run_id = run_id;
                options.context_budget = context_budget;
                options.token_headroom = token_headroom.then_some(true);
                let report = build_interactive_harness(&store, options)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_harness(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::Sessions {
                provider_id,
                lifecycle_state,
                readiness,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_sessions(
                    &store,
                    InteractiveSessionsOptions {
                        provider_id,
                        lifecycle_state,
                        readiness,
                    },
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_sessions(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::CommandPalette {
                query,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_command_palette_for_project(
                    &store,
                    query.as_deref(),
                    project_root.as_deref(),
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_command_palette(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::ActionRegistry {
                query,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_action_registry_for_project(
                    &store,
                    query.as_deref(),
                    project_root.as_deref(),
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_action_registry(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::ActionInvocation {
                action_id,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_action_invocation_for_project(
                    &store,
                    &action_id,
                    project_root.as_deref(),
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_action_invocation(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::ActionDispatch {
                action_id,
                project_root,
                origin,
                payload,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let payload: Value = serde_json::from_str(&payload)?;
                let report = dispatch_interactive_action_hooks_for_project(
                    &store,
                    &action_id,
                    project_root.as_deref(),
                    &origin,
                    payload,
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_action_dispatch(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::Autocomplete {
                input,
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_autocomplete_for_project(
                    &store,
                    &input,
                    project_root.as_deref(),
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_autocomplete(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::PatchWorkbench { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_patch_workbench(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_patch_workbench(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::Permissions { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_permissions(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_permissions(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::Identity {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_identity(&store, &project_root)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_identity(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::TaskBoard { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_task_board(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_task_board(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::WorkflowMutation { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_workflow_mutation(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_workflow_mutation(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::WorkflowSidebar { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_workflow_sidebar(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_workflow_sidebar(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::Artifacts { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_artifacts(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_artifacts(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::TokenUsage { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_token_usage(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_token_usage(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::WorkflowDag { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_workflow_dag(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_workflow_dag(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::Schedules { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_schedules(&store);
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_interactive_schedules(&report)),
                }
                Ok(0)
            }
            InteractiveCommands::EventRuntime {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_event_runtime(&store, &project_root)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_event_runtime(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::ContextMemory {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_context_memory(&store, &project_root)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_context_memory(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::OperatingContext {
                project_root,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_operating_context(&store, &project_root)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_operating_context(&report))
                    }
                }
                Ok(0)
            }
            InteractiveCommands::StructuredLogs { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_interactive_structured_logs(&store)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_interactive_structured_logs(&report))
                    }
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
                context,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = if context.is_empty() {
                    route_interactive_input(&store, &input, &origin)?
                } else {
                    route_interactive_input_with_context(&store, &input, &origin, &context)?
                };
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
                let store = FoundryStore::open(cli.store)?;
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
                        expected_revision: None,
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = expire_human_interaction(&store, &workflow, &task, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            InteractionCommands::List { output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = list_human_interactions(&store)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Ops { command } => match command {
            OpsCommands::Snapshot {
                project_root,
                addon_dirs,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = build_ops_snapshot_with_addon_dirs_and_project(
                    &store,
                    &dirs,
                    project_root.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            OpsCommands::Serve {
                host,
                port,
                project_root,
                addon_dirs,
            } => {
                let dirs = addon_dirs_or_default(addon_dirs);
                serve_ops_console_with_addon_dirs_and_project(
                    cli.store,
                    &host,
                    port,
                    &dirs,
                    project_root,
                )?;
                Ok(0)
            }
            OpsCommands::RendererEvent {
                addon_dirs,
                workflow_id,
                addon_id,
                view_id,
                event_kind,
                actor,
                payload,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let dirs = addon_dirs_or_default(addon_dirs);
                let report = record_addon_renderer_client_event(
                    &store,
                    &dirs,
                    OpsAddonRendererClientEventInput {
                        workflow_id: &workflow_id,
                        addon_id: addon_id.as_deref(),
                        view_id: &view_id,
                        event_kind: &event_kind,
                        actor: &actor,
                        payload: payload.as_deref(),
                    },
                )?;
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
                let store = FoundryStore::open(cli.store)?;
                let report = build_milestone_manifest_with_store(&version, Some(&store))?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::ProductionPlan { version, output } => {
                let report = build_production_readiness_plan(&version)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::ProductionEvidenceTemplate {
                version,
                release_version,
                evidence_root,
                template,
                output,
            } => {
                let report =
                    write_production_evidence_template(ProductionEvidenceTemplateOptions {
                        version: &version,
                        release_version: &release_version,
                        evidence_root: &evidence_root,
                        template_path: &template,
                    })?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::ProductionEvidenceAssemble {
                version,
                release_version,
                evidence_root,
                draft,
                receipt_directory,
                manifest,
                output,
            } => {
                let report = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
                    version: &version,
                    release_version: &release_version,
                    evidence_root: &evidence_root,
                    draft_path: &draft,
                    receipt_directory: &receipt_directory,
                    manifest_path: &manifest,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::ProductionMissionEvidence {
                mission,
                receipt,
                evidence_root,
                artifact,
                release_version,
                output,
            } => {
                let artifact_value = artifact
                    .to_str()
                    .context("production mission evidence artifact path must be valid UTF-8")?;
                let store = FoundryStore::open(&cli.store)?;
                let package = build_production_mission_lifecycle_evidence(
                    &store,
                    &release_version,
                    &mission,
                    &receipt,
                    artifact_value,
                )?;
                let artifact_bytes = serde_json::to_vec(&package.artifact)
                    .context("failed to serialize production mission lifecycle artifact")?;
                let artifact_sha256 = format!("{:x}", Sha256::digest(&artifact_bytes));
                if package.manifest_section.evidence.artifact_sha256 != artifact_sha256 {
                    bail!(
                        "production mission lifecycle artifact SHA-256 differs from manifest section"
                    );
                }
                let artifact_path = write_contained_production_evidence_artifact(
                    &evidence_root,
                    &artifact,
                    &artifact_bytes,
                )?;
                let persisted_bytes = std::fs::read(&artifact_path).with_context(|| {
                    format!(
                        "failed to verify production mission evidence artifact {}",
                        artifact_path.display()
                    )
                })?;
                if persisted_bytes != artifact_bytes {
                    bail!("persisted production mission evidence differs from canonical bytes");
                }
                print_response(output, &package)?;
                Ok(0)
            }
            MilestoneCommands::ProductionReadiness {
                version,
                manifest,
                evidence_root,
                output,
            } => {
                let report = evaluate_production_readiness(ProductionReadinessOptions {
                    version: &version,
                    manifest_path: &manifest,
                    evidence_root: &evidence_root,
                    store_path: &cli.store,
                })?;
                let production_ready = report.production_ready;
                print_response(output, &report)?;
                Ok(if production_ready { 0 } else { 1 })
            }
            MilestoneCommands::AttachEvidence {
                version,
                capability_id,
                kind,
                summary,
                artifact,
                approved_by,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = attach_milestone_evidence(
                    &store,
                    MilestoneAttachEvidenceOptions {
                        version: &version,
                        capability_id: &capability_id,
                        kind: &kind,
                        summary: &summary,
                        artifact_path: &artifact,
                        approved_by: &approved_by,
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::EvidencePlan {
                version,
                capability_id,
                project_root,
                connected_brain,
                connected_runtime,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_milestone_evidence_plan(
                    &store,
                    MilestoneEvidencePlanOptions {
                        version: &version,
                        capability_id: &capability_id,
                        project_root: project_root.as_deref(),
                        connected_brain: connected_brain.as_deref(),
                        connected_runtime: connected_runtime.as_deref(),
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::PrepareEvidenceInputs {
                version,
                capability_id,
                project_root,
                connected_brain,
                connected_runtime,
                provider_command,
                model_id,
                approval_ref,
                apply,
                approved_by,
                force,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = prepare_milestone_evidence_inputs(
                    &store,
                    MilestonePrepareEvidenceInputsOptions {
                        version: &version,
                        capability_id: &capability_id,
                        project_root: project_root.as_deref(),
                        connected_brain: connected_brain.as_deref(),
                        connected_runtime: connected_runtime.as_deref(),
                        provider_command: provider_command.as_deref(),
                        model_id: model_id.as_deref(),
                        approval_ref: approval_ref.as_deref(),
                        apply,
                        approved_by: approved_by.as_deref(),
                        force,
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::CollectEvidence {
                version,
                capability_id,
                kind,
                project_root,
                connected_brain,
                connected_runtime,
                approved_by,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = collect_milestone_evidence(
                    &store,
                    MilestoneCollectEvidenceOptions {
                        version: &version,
                        capability_id: &capability_id,
                        kind: kind.as_deref(),
                        project_root: project_root.as_deref(),
                        connected_brain: connected_brain.as_deref(),
                        connected_runtime: connected_runtime.as_deref(),
                        approved_by: &approved_by,
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::CollectReadyEvidence {
                version,
                project_root,
                connected_brain,
                connected_runtime,
                approved_by,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = collect_ready_milestone_evidence(
                    &store,
                    MilestoneCollectReadyEvidenceOptions {
                        version: &version,
                        project_root: project_root.as_deref(),
                        connected_brain: connected_brain.as_deref(),
                        connected_runtime: connected_runtime.as_deref(),
                        approved_by: &approved_by,
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::Research { version, output } => {
                let report = build_milestone_research(&version)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::ExportDemo { origin, output } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_milestone_export_demo(&store, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MilestoneCommands::CliDemo {
                origin,
                project_root,
                connected_brain,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_replacement_cli_demo_with_options(
                    &store,
                    &origin,
                    MilestoneCliDemoOptions {
                        project_root: project_root.as_deref(),
                        connected_brain: connected_brain.as_deref(),
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Multimodal { command } => match command {
            MultimodalCommands::Status {
                enable_experimental,
                project_root,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report = build_multimodal_status_with_feature_flag(feature_flag);
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::InstallPlan {
                capability,
                enable_experimental,
                project_root,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report = build_multimodal_install_plan(&capability, feature_flag.enabled)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::Readiness {
                capability,
                enable_experimental,
                project_root,
                allow,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report = build_multimodal_readiness(MultimodalReadinessOptions {
                    capability_id: &capability,
                    enable_experimental: feature_flag.enabled,
                    explicit_allow: allow,
                    project_root: project_root.as_deref(),
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::BenchmarkTemplate {
                capability,
                enable_experimental,
                project_root,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report =
                    build_multimodal_benchmark_template(&capability, feature_flag.enabled)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::BenchmarkResult {
                capability,
                fixture,
                enable_experimental,
                project_root,
                approved_by,
                confirm_fixture_only,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report = build_multimodal_benchmark_result(MultimodalBenchmarkResultOptions {
                    capability_id: &capability,
                    fixture_id: &fixture,
                    enable_experimental: feature_flag.enabled,
                    approved_by: approved_by.as_deref(),
                    confirm_fixture_only,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::RuntimeBenchmark {
                capability,
                fixture,
                enable_experimental,
                project_root,
                approved_by,
                confirm_runtime_execution,
                allow_model,
                connected_runtime,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report =
                    build_multimodal_runtime_benchmark(MultimodalRuntimeBenchmarkOptions {
                        capability_id: &capability,
                        fixture_id: &fixture,
                        enable_experimental: feature_flag.enabled,
                        project_root: project_root.as_deref(),
                        approved_by: approved_by.as_deref(),
                        confirm_runtime_execution,
                        allow_model,
                        connected_runtime: connected_runtime.as_deref(),
                    })?;
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::DemoPlan {
                demo,
                enable_experimental,
                project_root,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report = build_multimodal_demo_plan(&demo, feature_flag.enabled)?;
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::DemoReceipt {
                demo,
                fixture,
                enable_experimental,
                project_root,
                approved_by,
                confirm_local_fixture,
                allow_model,
                allow_camera,
                allow_microphone,
                allow_screen,
                allow_input,
                allow_filesystem,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report = build_multimodal_demo_receipt(MultimodalDemoReceiptOptions {
                    demo_id: &demo,
                    fixture_id: &fixture,
                    enable_experimental: feature_flag.enabled,
                    approved_by: approved_by.as_deref(),
                    confirm_local_fixture,
                    allow_model,
                    allow_camera,
                    allow_microphone,
                    allow_screen,
                    allow_input,
                    allow_filesystem,
                })?;
                print_response(output, &report)?;
                Ok(0)
            }
            MultimodalCommands::Guard {
                capability,
                action,
                enable_experimental,
                project_root,
                allow,
                output,
            } => {
                let feature_flag =
                    resolve_multimodal_feature_flag(enable_experimental, project_root.as_deref());
                let report =
                    evaluate_multimodal_guard(&capability, &action, feature_flag.enabled, allow)?;
                let exit_code = if report.allowed { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
        },
        Commands::Patch { command } => match command {
            PatchCommands::Plan {
                workflow,
                task,
                intent,
                paths,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_patch_plan(&store, &workflow, &task, paths, &intent, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            PatchCommands::Apply {
                workflow,
                task,
                paths,
                origin,
                plan_artifact,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_patch_apply(
                    &store,
                    &workflow,
                    &task,
                    paths,
                    &origin,
                    plan_artifact.as_deref(),
                    None,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            PatchCommands::Review {
                workflow,
                task,
                paths,
                origin,
                plan_artifact,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_patch_review(
                    &store,
                    &workflow,
                    &task,
                    paths,
                    &origin,
                    plan_artifact.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            PatchCommands::Diff {
                workflow,
                task,
                paths,
                file_index,
                hunk_index,
                context_lines,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_patch_diff(
                    &store,
                    &workflow,
                    &task,
                    paths,
                    PatchDiffOptions {
                        file_index,
                        hunk_index,
                        context_lines,
                        origin: &origin,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            PatchCommands::Revert {
                workflow,
                task,
                apply_artifact,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report =
                    build_patch_revert(&store, &workflow, &task, &apply_artifact, &origin, None)?;
                print_response(output, &report)?;
                Ok(0)
            }
            PatchCommands::Restore {
                workflow,
                task,
                revert_artifact,
                approved_by,
                confirm_restore,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_patch_restore(
                    &store,
                    &workflow,
                    &task,
                    &revert_artifact,
                    &approved_by,
                    confirm_restore,
                    &origin,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Smoke { command } => match command {
            SmokeCommands::OperationalTui {
                project_root,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_operational_tui_smoke(&store, project_root.as_deref(), &origin)?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => println!("{}", render_operational_tui_smoke(&report)),
                }
                Ok(0)
            }
            SmokeCommands::FoundryFirstHarness {
                project_root,
                executor,
                real_cmd,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_foundry_first_harness_smoke(
                    &store,
                    project_root.as_deref(),
                    &executor,
                    real_cmd.as_deref(),
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_foundry_first_harness_smoke(&report))
                    }
                }
                Ok(0)
            }
            SmokeCommands::ReplacementCliEvidence {
                project_root,
                approved_by,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_replacement_cli_evidence_smoke(
                    &store,
                    project_root.as_deref(),
                    &approved_by,
                    &origin,
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_replacement_cli_evidence_smoke(&report))
                    }
                }
                Ok(0)
            }
            SmokeCommands::MultimodalRuntimeEvidence {
                project_root,
                connected_runtime,
                approved_by,
                origin,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = build_multimodal_runtime_evidence_smoke(
                    &store,
                    project_root.as_deref(),
                    connected_runtime.as_deref(),
                    &approved_by,
                    &origin,
                )?;
                match output {
                    OutputFormat::Json => print_response(output, &report)?,
                    OutputFormat::Human => {
                        println!("{}", render_multimodal_runtime_evidence_smoke(&report))
                    }
                }
                Ok(0)
            }
        },
        Commands::Aws { command } => match command {
            AwsCommands::Check {
                aws_ops_bin,
                vault_contract,
                vault_data,
                output,
            } => {
                let report = run_aws_ops_check(
                    aws_ops_bin.as_deref(),
                    vault_contract.as_deref(),
                    vault_data.as_deref(),
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AwsCommands::Inventory {
                aws_ops_bin,
                vault_contract,
                vault_data,
                regions,
                all_regions,
                full,
                output,
            } => {
                let report = run_aws_ops_inventory(
                    aws_ops_bin.as_deref(),
                    vault_contract.as_deref(),
                    vault_data.as_deref(),
                    regions.as_deref(),
                    all_regions,
                    full,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            AwsCommands::Raw {
                aws_ops_bin,
                vault_contract,
                vault_data,
                allow_mutation,
                reason,
                output,
                aws_args,
            } => run_aws_ops_raw(
                aws_ops_bin.as_deref(),
                vault_contract.as_deref(),
                vault_data.as_deref(),
                allow_mutation,
                reason.as_deref(),
                &aws_args,
            )
            .map(|report| {
                print_response(output, &report)?;
                Ok(0)
            })?,
        },
        Commands::CredentialVault { command } => match command {
            CredentialVaultCommands::KeyInit { vault_bin, output } => {
                let report = run_credential_vault_key_init(vault_bin.as_deref())?;
                print_response(output, &report)?;
                Ok(0)
            }
            CredentialVaultCommands::Describe {
                vault_bin,
                contract,
                data,
                output,
            } => {
                let report = run_credential_vault_describe(vault_bin.as_deref(), &contract, &data)?;
                print_response(output, &report)?;
                Ok(0)
            }
            CredentialVaultCommands::Records {
                vault_bin,
                contract,
                data,
                output,
            } => {
                let report = run_credential_vault_records(vault_bin.as_deref(), &contract, &data)?;
                print_response(output, &report)?;
                Ok(0)
            }
            CredentialVaultCommands::Panel {
                vault_bin,
                contract,
                data,
                open,
                timeout_seconds,
                no_cli_fallback,
                output,
            } => {
                let report = run_credential_vault_panel(
                    vault_bin.as_deref(),
                    &contract,
                    &data,
                    open,
                    timeout_seconds,
                    no_cli_fallback,
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            CredentialVaultCommands::Exec {
                vault_bin,
                contract,
                data,
                record,
                env_mappings,
                command,
            } => run_credential_vault_exec(
                vault_bin.as_deref(),
                &contract,
                &data,
                &record,
                &env_mappings,
                &command,
            ),
        },
        Commands::SelfRun { command } => match command {
            SelfCommands::Run {
                repo,
                until,
                max_cycles,
                sleep_seconds,
                executors,
                fallback_executors,
                goal,
                validation_commands,
                mode,
                skip_self_update,
                self_update_command,
                dry_run,
                push,
                output,
            } => {
                let store = FoundryStore::open(cli.store)?;
                let report = run_self_evolution(
                    &store,
                    SelfRunOptions {
                        repo,
                        until,
                        max_cycles,
                        sleep_seconds,
                        executors,
                        fallback_executors,
                        goal,
                        validation_commands,
                        mode,
                        skip_self_update,
                        self_update_command,
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

fn foundry_production_mode_enabled() -> bool {
    foundry_core::brand::env_var("FOUNDRY_PRODUCTION_MODE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

fn write_contained_production_evidence_artifact(
    evidence_root: &Path,
    artifact: &Path,
    bytes: &[u8],
) -> Result<PathBuf> {
    if artifact.as_os_str().is_empty()
        || artifact.is_absolute()
        || !artifact
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("production mission evidence artifact must be a contained relative path");
    }

    let root_metadata = std::fs::symlink_metadata(evidence_root).with_context(|| {
        format!(
            "failed to inspect production mission evidence root {}",
            evidence_root.display()
        )
    })?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        bail!("production mission evidence root must be a non-symlink directory");
    }
    let canonical_root = std::fs::canonicalize(evidence_root).with_context(|| {
        format!(
            "failed to resolve production mission evidence root {}",
            evidence_root.display()
        )
    })?;

    let mut canonical_parent = canonical_root.clone();
    if let Some(parent) = artifact.parent() {
        for component in parent.components() {
            let Component::Normal(segment) = component else {
                bail!("production mission evidence parent must stay contained");
            };
            let candidate = canonical_parent.join(segment);
            match std::fs::symlink_metadata(&candidate) {
                Ok(metadata) => {
                    if !metadata.is_dir() || metadata.file_type().is_symlink() {
                        bail!(
                            "production mission evidence parent must be a non-symlink directory: {}",
                            candidate.display()
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&candidate).with_context(|| {
                        format!(
                            "failed to create production mission evidence directory {}",
                            candidate.display()
                        )
                    })?;
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to inspect production mission evidence directory {}",
                            candidate.display()
                        )
                    });
                }
            }
            canonical_parent = std::fs::canonicalize(&candidate).with_context(|| {
                format!(
                    "failed to resolve production mission evidence directory {}",
                    candidate.display()
                )
            })?;
            if !canonical_parent.starts_with(&canonical_root) {
                bail!("production mission evidence parent escapes evidence root");
            }
        }
    }

    let file_name = artifact
        .file_name()
        .context("production mission evidence artifact requires a file name")?;
    let target = canonical_parent.join(file_name);
    match std::fs::symlink_metadata(&target) {
        Ok(_) => {
            bail!("production mission evidence artifact already exists and will not be overwritten")
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect production mission evidence artifact {}",
                    target.display()
                )
            });
        }
    }
    let temporary = canonical_parent.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    let mut published = false;
    let persist_result = (|| -> Result<()> {
        use std::io::Write as _;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| {
                format!(
                    "failed to create production mission evidence staging file {}",
                    temporary.display()
                )
            })?;
        file.write_all(bytes).with_context(|| {
            format!(
                "failed to write production mission evidence staging file {}",
                temporary.display()
            )
        })?;
        file.sync_all().with_context(|| {
            format!(
                "failed to sync production mission evidence staging file {}",
                temporary.display()
            )
        })?;
        drop(file);
        std::fs::hard_link(&temporary, &target).with_context(|| {
            format!(
                "failed to publish no-overwrite production mission evidence artifact {}",
                target.display()
            )
        })?;
        published = true;
        std::fs::remove_file(&temporary).with_context(|| {
            format!(
                "failed to remove production mission evidence staging link {}",
                temporary.display()
            )
        })?;
        std::fs::File::open(&canonical_parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| {
                format!(
                    "failed to sync production mission evidence directory {}",
                    canonical_parent.display()
                )
            })?;
        Ok(())
    })();
    if let Err(error) = persist_result {
        let mut cleanup_failures = Vec::new();
        if published {
            if let Err(cleanup_error) = std::fs::remove_file(&target) {
                cleanup_failures.push(format!("{}: {cleanup_error}", target.display()));
            }
        }
        match std::fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {}
            Err(cleanup_error) => {
                cleanup_failures.push(format!("{}: {cleanup_error}", temporary.display()));
            }
        }
        if cleanup_failures.is_empty() {
            return Err(error);
        }
        bail!(
            "{error:#}; additionally failed rollback production mission evidence: {}",
            cleanup_failures.join(", ")
        );
    }
    Ok(target)
}

fn print_response<T: Serialize>(format: OutputFormat, value: &T) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
        OutputFormat::Human => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}

fn harness_cli_token_headroom_input(
    token_headroom: bool,
    no_token_headroom: bool,
) -> (Option<bool>, &'static str) {
    if no_token_headroom {
        (Some(false), "no_token_headroom_flag")
    } else if token_headroom {
        (Some(true), "explicit_flag")
    } else {
        (None, "default")
    }
}

fn addon_dirs_or_default(addon_dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    if addon_dirs.is_empty() {
        default_addon_dirs()
    } else {
        addon_dirs
    }
}

#[allow(clippy::too_many_arguments)]
fn build_executor_quota_observation(
    executor: String,
    provider: String,
    model: String,
    locality: String,
    free_vs_paid: String,
    remaining_quota: String,
    rate_limit_risk: String,
    monetary_or_token_cost: String,
    latency: String,
    expected_quality: String,
    suitability: String,
    source: String,
    observed_at: Option<String>,
) -> Result<ExecutorQuotaObservation> {
    let executor = required_cli_value("executor", executor)?;
    let provider = required_cli_value("provider", provider)?;
    let model = required_cli_value("model", model)?;
    let locality = required_cli_value("locality", locality)?;
    let free_vs_paid = required_cli_value("free-vs-paid", free_vs_paid)?;
    let remaining_quota = required_cli_value("remaining-quota", remaining_quota)?;
    let rate_limit_risk = required_cli_value("rate-limit-risk", rate_limit_risk)?;
    let monetary_or_token_cost = required_cli_value("cost", monetary_or_token_cost)?;
    let latency = required_cli_value("latency", latency)?;
    let expected_quality = required_cli_value("expected-quality", expected_quality)?;
    let suitability = required_cli_value("suitability", suitability)?;
    let source = required_cli_value("source", source)?;

    Ok(ExecutorQuotaObservation {
        executor,
        provider,
        model: Some(model),
        local_vs_non_local: locality,
        free_vs_paid_if_known: free_vs_paid,
        remaining_quota,
        rate_limit_risk,
        monetary_or_token_cost,
        latency,
        expected_quality,
        suitability,
        source,
        observed_at: observed_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
    })
}

fn required_cli_value(name: &str, value: String) -> Result<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        anyhow::bail!("{name} must not be empty");
    }
    Ok(value)
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
