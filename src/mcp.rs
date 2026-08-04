use crate::addon::{
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
    validate_addon_catalog, AddonExecutorDispatchInput as AddonExecutorDispatchRequest,
    AddonExecutorExecutionInput as AddonExecutorExecutionRequest,
    AddonHandoffDispatchInput as AddonHandoffDispatchRequest,
    AddonHandoffExecutionInput as AddonHandoffExecutionRequest,
    AddonPackageInput as AddonPackageRequest,
    AddonPlannerDispatchInput as AddonPlannerDispatchRequest,
    AddonPlanningStrategyInput as AddonPlanningStrategyRequest,
    AddonRuntimeContractCompletionInput as AddonRuntimeContractCompletionRequest,
    AddonRuntimeWorkerRegistrationInput as AddonRuntimeWorkerRegistrationRequest,
    AddonTrustKeyInput as AddonTrustKeyRequest,
    AddonValidatorDispatchInput as AddonValidatorDispatchRequest,
    AddonValidatorExecutionInput as AddonValidatorExecutionRequest,
    AddonValidatorOutcomeApplyInput as AddonValidatorOutcomeApplyRequest,
    CapabilityRegistrySyncInput,
};
use crate::artifact::{hex_sha256, list_workflow_artifacts, ListedArtifact};
use crate::aws_ops::{
    run_check as run_aws_ops_check, run_inventory as run_aws_ops_inventory,
    run_raw as run_aws_ops_raw, AWS_OPS_COMMAND_SCHEMA,
};
use crate::checkpoint::load_latest_task_checkpoint;
use crate::cli_factory::{create_cli_factory_plan, CliFactoryCreateInput};
use crate::cli_integration::{
    analyze_token_headroom, build_cli_wrapper_plan, build_harness_activation_profile,
    build_harness_adoption_plan, build_harness_bootstrap_report, build_harness_doctor_report,
    build_harness_headroom_plan, build_harness_mode_report, build_headroom_stats_report,
    inspect_cli_harness_shim_status, install_cli_harness_shim, install_cli_provider_adapter,
    persist_token_headroom_report, resolve_harness_forge_first_source_for_project,
    resolve_harness_runtime_policy, retrieve_headroom_blob, run_cli_harness_exec,
    CliHarnessExecOptions, CliShimInstallOptions, CliShimStatusOptions, CliWrapperPlanOptions,
    HarnessActivationProfileOptions, HarnessAdoptionPlanOptions, HarnessBootstrapOptions,
    HarnessDoctorOptions, HarnessHeadroomPlanOptions, HarnessModeOptions,
    HarnessRuntimePolicyOptions, HeadroomStatsOptions, ProviderAdapterInstallOptions,
};
use crate::context::{
    build_compact_context_view_with_predecessor_plans,
    build_context_package_with_checkpoint_and_project,
    build_context_package_with_checkpoint_project_and_worktree,
    CONTEXT_COMPACT_VIEW_SCHEMA_VERSION, CONTEXT_SCHEMA_VERSION, DEFAULT_CONTEXT_BUDGET,
};
use crate::cost::{
    apply_cost_ledger_retention_for_context, build_cost_ledger_for_context,
    build_cost_ledger_history_for_context, maintain_cost_ledger_for_context,
    materialize_cost_ledger_incremental_for_context, materialize_cost_ledger_index_for_context,
    run_cost_ledger_daemon_for_context,
};
use crate::credential_vault::{
    run_describe as run_credential_vault_describe, run_records as run_credential_vault_records,
    CREDENTIAL_VAULT_COMMAND_SCHEMA,
};
use crate::event::{
    build_event_improvement_policy_for_context, build_event_observability_history_for_context,
    build_event_observability_index_for_context, build_event_service_plan,
    build_global_event_timeline_for_context, build_workflow_event_stream,
    dispatch_inbound_event_activations, emit_event_egress, ingest_inbound_event_with_context,
    list_event_services, list_inbound_event_inbox_for_context, recover_stale_event_services,
    route_inbound_event, run_event_runtime_daemon, run_event_runtime_reconcile,
    run_event_service_supervisor, run_event_webhook_ingress_service, run_event_worker_service,
    run_inbound_event_worker_loop, scan_inbound_event_inbox, EventEgressEmitInput,
    InboundEventIngestInput, InboundEventWorkerLoopOptions,
};
use crate::executor::{
    build_brain_session_history_report, build_brain_sessions_report_with_options,
    build_shell_launch_plan, load_executors, record_brain_session_lifecycle,
    record_shell_session_plan, BrainSessionLifecycleOptions, BrainSessionsReportOptions,
    ShellLaunchPlanOptions,
};
use crate::executor_runtime::{
    execute_request_executor_wave, RequestExecutorWaveOptions,
    DEFAULT_REQUEST_EXECUTOR_WAVE_CONTEXT_BUDGET, DEFAULT_REQUEST_EXECUTOR_WAVE_EXECUTOR,
    DEFAULT_REQUEST_EXECUTOR_WAVE_TIMEOUT_SECONDS, DEFAULT_REQUEST_EXECUTOR_WAVE_TTL_SECONDS,
};
use crate::graph::CoreParallelTeamSpec;
use crate::handoff::{
    build_predecessor_handoff_plans, build_task_handoff_response_with_project, TaskHandoffView,
    EXECUTOR_HANDOFF_COMPACT_SCHEMA_VERSION, EXECUTOR_HANDOFF_SCHEMA_VERSION,
};
use crate::identity::{
    audit_tenant_index, ensure_workflow_policy, evaluate_tenant_policy_for_action,
    inspect_project_operating_context, link_identity, list_identity_links,
    list_identity_memberships, list_identity_registry, list_tenant_index,
    load_project_operating_context, resolve_identity, sync_project_operating_context,
    unlink_identity, update_identity_membership, IdentityLinkInput, IdentityMembershipUpdateInput,
};
use crate::improve::{
    apply_event_improvement_policy, benchmark_event_improvement_policy,
    promote_event_improvement_policy, rank_improvement_candidates_with_filter,
    ImprovementCandidateFilter,
};
use crate::inspection::inspect_workflow_with_focus;
use crate::interaction::{
    answer_human_interaction, create_choice_interaction, create_form_interaction,
    expire_human_interaction, list_human_interactions, CreateChoiceInteractionRequest,
};
use crate::interactive::{
    build_interactive_action_invocation_for_project, build_interactive_action_registry_for_project,
    build_interactive_addon_capabilities_for_project, build_interactive_architecture_compass,
    build_interactive_artifacts, build_interactive_autocomplete_for_project,
    build_interactive_command_palette_for_project, build_interactive_context_memory,
    build_interactive_core_boundary_for_project, build_interactive_guided_cockpit,
    build_interactive_harness, build_interactive_home_with_options, build_interactive_identity,
    build_interactive_improvement_loop, build_interactive_multimodal_runtime,
    build_interactive_operating_context, build_interactive_operational_cockpit,
    build_interactive_patch_workbench, build_interactive_permissions, build_interactive_readiness,
    build_interactive_release_gates, build_interactive_replacement_cli_with_options,
    build_interactive_schedules, build_interactive_sessions, build_interactive_structured_logs,
    build_interactive_task_board, build_interactive_token_usage, build_interactive_ui_composition,
    build_interactive_workflow_dag, build_interactive_workflow_mutation,
    build_interactive_workflow_sidebar, dispatch_interactive_action_hooks_for_project,
    route_interactive_input, slash_command_catalog, InteractiveHarnessOptions,
    InteractiveHomeOptions, InteractiveReplacementCliOptions, InteractiveSessionsOptions,
};
use crate::ir::{CreativeArtifact, TokenCollection};
use crate::memory::{
    configure_memory_governance, list_memory_promotions, memory_cleanup_report,
    memory_policy_report_for_project, memory_retention_report, promote_memory, search_memory,
    MemoryCleanupOptions, MemoryGovernanceConfigOptions, MemoryPromotionOptions,
    MemoryRetentionOptions, MemorySearchOptions,
};
use crate::milestone::{
    attach_milestone_evidence, build_milestone_evidence_plan, build_milestone_export_demo,
    build_milestone_manifest_with_store, build_milestone_research, build_milestone_status,
    build_production_readiness_plan, build_replacement_cli_demo_with_options,
    collect_milestone_evidence, collect_ready_milestone_evidence, evaluate_production_readiness,
    prepare_milestone_evidence_inputs, MilestoneAttachEvidenceOptions, MilestoneCliDemoOptions,
    MilestoneCollectEvidenceOptions, MilestoneCollectReadyEvidenceOptions,
    MilestoneEvidencePlanOptions, MilestonePrepareEvidenceInputsOptions,
    ProductionReadinessOptions,
};
use crate::mission::{
    drive_mission, load_mission, resume_mission, start_mission, submit_mission, MissionSubmission,
};
use crate::mission_executor::{
    build_mission_execution_approval, execute_mission_command, inspect_mission_execution_receipt,
    list_mission_execution_receipts, plan_mission_execution, reconcile_mission_execution,
    MissionExecutionReconcileRequest, MissionExecutionRequest,
};
use crate::multimodal::{
    build_multimodal_benchmark_result, build_multimodal_benchmark_template,
    build_multimodal_demo_plan, build_multimodal_demo_receipt, build_multimodal_install_plan,
    build_multimodal_readiness, build_multimodal_runtime_benchmark,
    build_multimodal_status_with_feature_flag, evaluate_multimodal_guard,
    resolve_multimodal_feature_flag, MultimodalBenchmarkResultOptions,
    MultimodalDemoReceiptOptions, MultimodalReadinessOptions, MultimodalRuntimeBenchmarkOptions,
};
use crate::ops::{
    build_ops_snapshot_with_addon_dirs_and_project, record_addon_renderer_client_event,
    OpsAddonRendererClientEventInput,
};
use crate::patch::{
    build_patch_apply, build_patch_diff, build_patch_plan, build_patch_restore, build_patch_revert,
    build_patch_review, PatchDiffOptions,
};
use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
};
use crate::request::{
    cancel_request, complete_ready_task, create_final_delivery_package, drive_request,
    ensure_final_audit, heartbeat_request, list_requests, load_request_status,
    recover_stale_request, resume_async_request,
    start_async_request_with_idempotency_and_parallel_team, step_request, switch_request_executor,
    RequestExecutorSwitchInput, RequestTaskCompletionInput,
};
use crate::schedule::{
    aggregate_summary, build_schedule_worker_status, create_daily_goal_research_workflow,
    run_due_workflow, scan_due_workflows, scan_due_workflows_parallel, update_loop_state,
    update_workflow_schedule, ScheduleUpdateOptions,
};
use crate::security::{
    sanitize_prompt_secrets_with_vault, SecretSanitizationOptions, SecretVaultPersistOptions,
};
use crate::storage::ForgeStore;
use crate::teamwork::{
    plan_teamwork_workflow, plan_teamwork_workflow_with_config, TeamworkLaneConfig,
    TeamworkParallelConfig,
};
use crate::validation::{validate_workflow, ValidationReport};
use crate::workflow::{
    add_workflow_task, add_workflow_task_dependency, attach_creative_artifact,
    attach_workflow_artifact_with_tags, clear_workflow_task_impediment,
    get_workflow_token_collection, inspect_creative_artifact, inspect_creative_collaboration,
    list_creative_artifacts, parse_node_brain_agent_slot, patch_workflow_token,
    record_creative_collaboration_event, remove_workflow_task_dependency, resolve_workflow_tokens,
    set_workflow_task_impediment, set_workflow_task_priority, set_workflow_token_collection,
    update_workflow_goal_with_expected_revision, update_workflow_node_brain_routing,
    update_workflow_task_with_expected_revision, CreativeCollaborationEventRequest,
    WorkflowNodeBrainRoutingUpdateInput, WorkflowTaskAddInput, WorkflowTaskDependencyInput,
    WorkflowTaskImpedimentClearInput, WorkflowTaskImpedimentInput, WorkflowTaskPriorityInput,
    WorkflowTaskUpdateInput,
};
use crate::worktree::{
    bound_worktree_context, create_worktree_guard_predecessor_task,
    evaluate_worktree_modification_guard, inspect_worktree_sandbox_lifecycle,
    plan_worktree_sandbox, resolve_effective_project_root, run_worktree_sandbox,
    start_worktree_sandbox, stop_worktree_sandbox, WorktreeModificationGuardRequest,
    WorktreeSandboxRequest,
};
use anyhow::{bail, Context, Result};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Value};
use std::env;
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

#[derive(Debug, Clone)]
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

impl Serialize for McpToolSpec {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let output_schema_selector = output_schema_selector(&self.name);
        let mut state = serializer.serialize_struct(
            "McpToolSpec",
            8 + usize::from(output_schema_selector.is_some()),
        )?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("title", &self.title)?;
        state.serialize_field("description", &self.description)?;
        state.serialize_field("input_schema", &self.input_schema)?;
        state.serialize_field("output_schema", &self.output_schema)?;
        state.serialize_field("forge_command", &self.forge_command)?;
        state.serialize_field("async_safe", &self.async_safe)?;
        state.serialize_field("mutates_workflow", &self.mutates_workflow)?;
        if let Some(selector) = output_schema_selector {
            state.serialize_field("output_schema_selector", &selector)?;
        }
        state.end()
    }
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
#[serde(deny_unknown_fields)]
struct MissionStartInput {
    goal: String,
    squad: Option<String>,
    squad_version: Option<String>,
    worktree: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionIdInput {
    mission_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionExecuteInput {
    mission_id: String,
    task_id: String,
    agent_id: String,
    idempotency_key: String,
    purpose: String,
    command: Vec<String>,
    evidence: Option<Vec<String>>,
    approved_by: Option<String>,
    approval_ttl_seconds: Option<u64>,
    dry_run: Option<bool>,
    allow_trusted_process_runtime: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionSubmitInput {
    mission_id: String,
    task_id: String,
    agent_id: String,
    idempotency_key: String,
    receipt_id: String,
    status: Option<String>,
    summary: String,
    artifacts: Option<Vec<String>>,
    risks: Option<Vec<String>>,
    followups: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionExecutionListInput {
    mission_id: Option<String>,
    task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionExecutionInspectInput {
    receipt_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionExecutionReconcileInput {
    receipt_id: String,
    outcome: String,
    approved_by: String,
    reason: String,
    confirm_no_effect_retry: bool,
}

#[derive(Debug, Deserialize)]
struct WorkflowListInput {
    lifecycle: Option<String>,
    context_action: Option<String>,
    quality_action: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImprovementCandidatesInput {
    limit: Option<usize>,
    workflow_ids: Option<Vec<String>>,
    goal_contains: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ImproveApplyEventPolicyInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    recommendation: Option<String>,
    recommendation_id: Option<String>,
    policy: Option<String>,
    recommended_policy: Option<String>,
    apply: Option<bool>,
    approved_by: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImproveBenchmarkEventPolicyInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    recommendation: Option<String>,
    recommendation_id: Option<String>,
    policy: Option<String>,
    recommended_policy: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImprovePromoteEventPolicyInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    recommendation: Option<String>,
    recommendation_id: Option<String>,
    policy: Option<String>,
    recommended_policy: Option<String>,
    approved_by: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CliFactoryCreateMcpInput {
    name: String,
    goal: Option<String>,
    source: Option<String>,
    commands: Option<Vec<String>>,
    command: Option<Vec<String>>,
    compound_commands: Option<Vec<String>>,
    compound_command: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct MemorySearchInput {
    query: String,
    workflow_id: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    audience: Option<String>,
    visibility: Option<String>,
    memory_level: Option<String>,
    run_id: Option<String>,
    organization_id: Option<String>,
    limit: Option<usize>,
    global_root: Option<String>,
    organization_root: Option<String>,
    project_root: Option<String>,
    processing_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryPolicyInput {
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryConfigureInput {
    project_root: String,
    memory_level: String,
    #[serde(default)]
    default_scopes: Vec<String>,
    default_audience: String,
    privacy_mode: String,
    retention_mode: String,
    approved_by: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct MemoryPromotionInput {
    workflow_id: Option<String>,
    from_scope: String,
    to_scope: String,
    source_path: String,
    source_start_line: Option<usize>,
    source_end_line: Option<usize>,
    summary: String,
    approved_by: String,
    reason: String,
    visibility: Option<String>,
    shareability: Option<String>,
    organization_id: Option<String>,
    global_root: Option<String>,
    organization_root: Option<String>,
    project_root: Option<String>,
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MemoryPromotionIndexInput {
    workflow_id: Option<String>,
    from_scope: Option<String>,
    to_scope: Option<String>,
    approved_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryRetentionInput {
    workflow_id: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    run_id: Option<String>,
    organization_id: Option<String>,
    global_root: Option<String>,
    organization_root: Option<String>,
    project_root: Option<String>,
    processing_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryCleanupInput {
    workflow_id: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    run_id: Option<String>,
    organization_id: Option<String>,
    global_root: Option<String>,
    organization_root: Option<String>,
    project_root: Option<String>,
    processing_root: Option<String>,
    mode: Option<String>,
    archive_root: Option<String>,
    approved_by: Option<String>,
    reason: Option<String>,
    dry_run: Option<bool>,
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AddonCatalogInput {
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonCapabilityIndexInput {
    addon: Option<String>,
    addon_id: Option<String>,
    capability: Option<String>,
    capability_id: Option<String>,
    lifecycle: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonObservabilityInput {
    addon: Option<String>,
    addon_id: Option<String>,
    lifecycle: Option<String>,
    addon_dirs: Option<Vec<String>>,
    dispatch_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeContractInput {
    addon: Option<String>,
    addon_id: Option<String>,
    contract: Option<String>,
    contract_id: Option<String>,
    contract_type: Option<String>,
    capability: Option<String>,
    capability_id: Option<String>,
    lifecycle: Option<String>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonPlannerRegistryInput {
    addon: Option<String>,
    addon_id: Option<String>,
    capability: Option<String>,
    capability_id: Option<String>,
    workflow_extension: Option<String>,
    workflow_extension_id: Option<String>,
    lifecycle: Option<String>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeDispatchInput {
    addon: Option<String>,
    addon_id: Option<String>,
    contract: Option<String>,
    contract_id: Option<String>,
    input: Option<serde_json::Value>,
    source: Option<String>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonPlannerDispatchInput {
    addon: Option<String>,
    addon_id: Option<String>,
    contract: Option<String>,
    contract_id: Option<String>,
    worker: Option<String>,
    worker_id: Option<String>,
    goal: String,
    constraints: Option<Vec<String>>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    context: Option<serde_json::Value>,
    lease_seconds: Option<u64>,
    source: Option<String>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonValidatorExecutionInput {
    addon: Option<String>,
    addon_id: Option<String>,
    contract: Option<String>,
    contract_id: Option<String>,
    worker: Option<String>,
    worker_id: Option<String>,
    subject: String,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    input: Option<serde_json::Value>,
    context: Option<serde_json::Value>,
    lease_seconds: Option<u64>,
    source: Option<String>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonValidatorOutcomeApplyInput {
    dispatch: Option<String>,
    dispatch_id: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    expected_revision: u64,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonExecutorExecutionInput {
    addon: Option<String>,
    addon_id: Option<String>,
    contract: Option<String>,
    contract_id: Option<String>,
    worker: Option<String>,
    worker_id: Option<String>,
    task: Option<String>,
    task_ref: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    input: Option<serde_json::Value>,
    context: Option<serde_json::Value>,
    lease_seconds: Option<u64>,
    source: Option<String>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonHandoffExecutionInput {
    addon: Option<String>,
    addon_id: Option<String>,
    contract: Option<String>,
    contract_id: Option<String>,
    worker: Option<String>,
    worker_id: Option<String>,
    handoff: Option<String>,
    handoff_ref: Option<String>,
    input: Option<serde_json::Value>,
    context: Option<serde_json::Value>,
    lease_seconds: Option<u64>,
    source: Option<String>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeDispatchListInput {
    addon: Option<String>,
    addon_id: Option<String>,
    contract: Option<String>,
    contract_id: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HarnessTokenHeadroomInput {
    content: String,
    content_kind: Option<String>,
    kind: Option<String>,
    budget_tokens: Option<usize>,
    source: Option<String>,
    reversible: Option<bool>,
    persist: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessRetrieveHeadroomInput {
    #[serde(alias = "ref")]
    retrieval_ref: String,
    include_content: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessHeadroomStatsInput {
    source: Option<String>,
    content_kind: Option<String>,
    kind: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HarnessModeInput {
    forge_first: Option<bool>,
    observe_only: Option<bool>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HarnessDoctorInput {
    shim_dir: String,
    executor: String,
    forge_first: Option<bool>,
    observe_only: Option<bool>,
    project_root: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessHeadroomPlanInput {
    executor: String,
    command: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    forge_first: Option<bool>,
    observe_only: Option<bool>,
    project_root: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessAdoptionPlanInput {
    shim_dir: String,
    executor: String,
    forge_first: Option<bool>,
    observe_only: Option<bool>,
    project_root: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessActivationProfileInput {
    shim_dir: String,
    executor: String,
    project_root: Option<String>,
    shell_rc: Option<String>,
    apply: Option<bool>,
    approved_by: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveHarnessInput {
    executor: Option<String>,
    shim_dir: Option<String>,
    forge_first: Option<bool>,
    observe_only: Option<bool>,
    project_root: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveCommandPaletteInput {
    query: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveActionInvocationInput {
    action: Option<String>,
    action_id: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveActionDispatchInput {
    action: Option<String>,
    action_id: Option<String>,
    project_root: Option<String>,
    origin: Option<String>,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveAutocompleteInput {
    input: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveIdentityInput {
    project_root: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveProjectRootInput {
    project_root: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct InteractiveMultimodalRuntimeInput {
    project_root: Option<String>,
    enable_experimental: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessWrapPlanInput {
    executor: String,
    command: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    forge_first: Option<bool>,
    project_root: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessBootstrapInput {
    shim_dir: String,
    executor: String,
    project_root: String,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
    apply: Option<bool>,
    approved_by: Option<String>,
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessInstallShimsInput {
    shim_dir: String,
    executor: String,
    real_cmd: Option<String>,
    real_command: Option<String>,
    forge_first: Option<bool>,
    project_root: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessInstallProviderAdapterInput {
    shim_dir: String,
    executor: String,
    real_cmd: Option<String>,
    real_command: Option<String>,
    project_root: Option<String>,
    token_headroom: Option<bool>,
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessShimStatusInput {
    shim_dir: String,
    executor: String,
}

#[derive(Debug, Deserialize)]
struct HarnessExecInput {
    executor: String,
    command: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    forge_first: Option<bool>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
    dry_run: Option<bool>,
    allow_exec: Option<bool>,
    secret_env: Option<Vec<String>>,
    secret_permissions: Option<Vec<String>>,
    project_root: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ShellLaunchPlanInput {
    executor: Option<String>,
    brain: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    ttl_seconds: Option<u64>,
    origin: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BrainSessionsInput {
    provider: Option<String>,
    provider_id: Option<String>,
    state: Option<String>,
    lifecycle_state: Option<String>,
    readiness: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrainSessionHistoryInput {
    session: Option<String>,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrainSessionLifecycleInput {
    session: Option<String>,
    session_id: Option<String>,
    state: String,
    workflow: Option<String>,
    workflow_id: Option<String>,
    task: Option<String>,
    task_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    origin: Option<String>,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeDispatchRunInput {
    dispatch: Option<String>,
    dispatch_id: Option<String>,
    worker: Option<String>,
    worker_id: Option<String>,
    lease_seconds: Option<u64>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeDispatchWorkerInput {
    status: Option<String>,
    limit: Option<usize>,
    worker: Option<String>,
    worker_id: Option<String>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeDispatchClaimInput {
    dispatch: Option<String>,
    dispatch_id: Option<String>,
    worker: Option<String>,
    worker_id: Option<String>,
    lease_seconds: Option<u64>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeDispatchCompleteInput {
    dispatch: Option<String>,
    dispatch_id: Option<String>,
    worker: Option<String>,
    worker_id: Option<String>,
    status: Option<String>,
    result: Option<serde_json::Value>,
    signature: Option<String>,
    attestation: Option<serde_json::Value>,
    dry_run: Option<bool>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeWorkerRegisterInput {
    worker: Option<String>,
    worker_id: Option<String>,
    runtime: String,
    status: Option<String>,
    trust_level: Option<String>,
    source: Option<String>,
    data: Option<serde_json::Value>,
    rotation_approved_by: Option<String>,
    rotation_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonRuntimeWorkerListInput {
    runtime: Option<String>,
    status: Option<String>,
    trust_level: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AddonViewsInput {
    addon: Option<String>,
    addon_id: Option<String>,
    surface: Option<String>,
    lifecycle: Option<String>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonPermissionAuthorizationInput {
    addon: Option<String>,
    addon_id: Option<String>,
    permission: Option<String>,
    permission_id: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonPermissionMutationInput {
    addon: Option<String>,
    addon_id: Option<String>,
    permission: Option<String>,
    permission_id: Option<String>,
    risk: Option<String>,
    approved_by: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonResolveInput {
    goal: String,
    addon_dirs: Option<Vec<String>>,
    registry_source: Option<String>,
    registry_sources: Option<Vec<String>>,
    registry_cache_dir: Option<String>,
    allow_remote_registry: Option<bool>,
    registry_max_bytes: Option<u64>,
    registry_max_packages: Option<usize>,
    registry_lock: Option<String>,
    registry_lock_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonInstallInput {
    manifest: String,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonLifecyclePlanInput {
    action: String,
    id: Option<String>,
    addon_id: Option<String>,
    manifest: Option<String>,
    package: Option<String>,
    package_path: Option<String>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AddonPackageInput {
    manifest: String,
    addon_dirs: Option<Vec<String>>,
    repository: Option<String>,
    channel: Option<String>,
    signature: Option<String>,
    public_key: Option<String>,
    package_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonTrustKeyInput {
    repository: String,
    channel: Option<String>,
    public_key: String,
    trust_level: Option<String>,
    approved_by: Option<String>,
    source: Option<String>,
    data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AddonTrustStoreInput {
    repository: Option<String>,
    channel: Option<String>,
    public_key: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AddonPackagePathInput {
    package: Option<String>,
    package_path: Option<String>,
    source: Option<String>,
    addon_dirs: Option<Vec<String>>,
    lock: Option<String>,
    lock_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonPackageFetchInput {
    source: String,
    cache_dir: Option<String>,
    expected_sha256: Option<String>,
    lock: Option<String>,
    lock_path: Option<String>,
    allow_remote: Option<bool>,
    max_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AddonRegistrySyncInput {
    source: String,
    cache_dir: Option<String>,
    lock: Option<String>,
    lock_path: Option<String>,
    allow_remote: Option<bool>,
    max_bytes: Option<u64>,
    max_packages: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AddonMarketplaceInput {
    repository: Option<String>,
    channel: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AddonPackageLockInput {
    repository: Option<String>,
    channel: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    status: Option<String>,
    write: Option<String>,
    write_path: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AddonMigrationWorkflowInput {
    from_manifest: String,
    to_manifest: String,
    action: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonLifecycleInput {
    id: String,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WorkflowInspectInput {
    workflow_id: String,
    task_id: Option<String>,
    verbose: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WorkflowEventsInput {
    workflow_id: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct OpsSnapshotInput {
    project_root: Option<String>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct OpsAddonRendererEventInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    view: Option<String>,
    view_id: Option<String>,
    event_kind: String,
    actor: Option<String>,
    payload: Option<Value>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct EventTimelineInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventObservabilityInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    node: Option<String>,
    node_ref: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventObservabilityHistoryInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    node: Option<String>,
    node_ref: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    bucket: Option<String>,
    group_by: Option<String>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventImprovementPolicyInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    node: Option<String>,
    node_ref: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    min_events: Option<usize>,
    min_event_count: Option<usize>,
    min_duration_ms: Option<i64>,
    min_total_duration_ms: Option<i64>,
    min_retries: Option<i64>,
    min_total_retry_count: Option<i64>,
    min_context_pressure_bps: Option<i64>,
    min_wait_seconds: Option<i64>,
    min_total_wait_seconds: Option<i64>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostLedgerInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostLedgerMaterializeInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    source_kind: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostLedgerIncrementalInput {
    after_sequence: Option<i64>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    source_kind: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostLedgerHistoryInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    source_kind: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    bucket: Option<String>,
    group_by: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostLedgerMaintainInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    source_kind: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    bucket: Option<String>,
    group_by: Option<String>,
    limit: Option<usize>,
    retention_days: Option<i64>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostLedgerDaemonInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    source_kind: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    bucket: Option<String>,
    group_by: Option<String>,
    limit: Option<usize>,
    retention_days: Option<i64>,
    max_cycles: Option<usize>,
    interval_seconds: Option<u64>,
    idle_exit: Option<bool>,
    origin: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostLedgerRetentionInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    source_kind: Option<String>,
    addon: Option<String>,
    addon_id: Option<String>,
    retention_days: Option<i64>,
    limit: Option<usize>,
    apply: Option<bool>,
    approved_by: Option<String>,
    reason: Option<String>,
    confirm: Option<bool>,
    origin: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboundEventMcpIngestInput {
    origin: String,
    action: String,
    #[serde(default)]
    data: Value,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboundEventInboxInput {
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboundEventScanInput {
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
    dispatch_activations: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct InboundEventWorkerLoopInput {
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
    max_cycles: Option<usize>,
    interval_seconds: Option<u64>,
    idle_exit: Option<bool>,
    dispatch_activations: Option<bool>,
    stop_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventServicePlanInput {
    kind: Option<String>,
    service_kind: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
    max_cycles: Option<usize>,
    interval_seconds: Option<u64>,
    idle_exit: Option<bool>,
    dispatch_activations: Option<bool>,
    host: Option<String>,
    port: Option<u16>,
    path: Option<String>,
    origin: Option<String>,
    action: Option<String>,
    schema: Option<String>,
    route: Option<bool>,
    max_requests: Option<usize>,
    max_body_bytes: Option<usize>,
    hmac_secret_env: Option<String>,
    signature_header: Option<String>,
    lease_seconds: Option<u64>,
    heartbeat_seconds: Option<u64>,
    backoff_initial_seconds: Option<u64>,
    backoff_max_seconds: Option<u64>,
    shutdown_grace_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EventServiceRunInput {
    kind: Option<String>,
    service_kind: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
    max_cycles: Option<usize>,
    interval_seconds: Option<u64>,
    idle_exit: Option<bool>,
    dispatch_activations: Option<bool>,
    stop_file: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    path: Option<String>,
    origin: Option<String>,
    action: Option<String>,
    schema: Option<String>,
    route: Option<bool>,
    max_requests: Option<usize>,
    max_body_bytes: Option<usize>,
    hmac_secret_env: Option<String>,
    signature_header: Option<String>,
    lease_owner: Option<String>,
    lease_seconds: Option<u64>,
    heartbeat_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EventServiceSuperviseInput {
    kind: Option<String>,
    service_kind: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
    max_cycles: Option<usize>,
    interval_seconds: Option<u64>,
    idle_exit: Option<bool>,
    dispatch_activations: Option<bool>,
    stop_file: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    path: Option<String>,
    origin: Option<String>,
    action: Option<String>,
    schema: Option<String>,
    route: Option<bool>,
    max_requests: Option<usize>,
    max_body_bytes: Option<usize>,
    hmac_secret_env: Option<String>,
    signature_header: Option<String>,
    lease_owner: Option<String>,
    lease_seconds: Option<u64>,
    heartbeat_seconds: Option<u64>,
    max_runs: Option<usize>,
    backoff_initial_seconds: Option<u64>,
    backoff_max_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EventRuntimeReconcileInput {
    status: Option<String>,
    limit: Option<usize>,
    service_limit: Option<usize>,
    project_root: Option<String>,
    execute: Option<bool>,
    max_cycles: Option<usize>,
    interval_seconds: Option<u64>,
    idle_exit: Option<bool>,
    dispatch_activations: Option<bool>,
    recover_stale_services: Option<bool>,
    continuous: Option<bool>,
    cycle_retention: Option<usize>,
    stop_file: Option<String>,
    lease_owner: Option<String>,
    lease_seconds: Option<u64>,
    heartbeat_seconds: Option<u64>,
    max_runs: Option<usize>,
    backoff_initial_seconds: Option<u64>,
    backoff_max_seconds: Option<u64>,
    scan_schedules: Option<bool>,
    schedule_executor: Option<String>,
    schedule_max_workers: Option<usize>,
    schedule_ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EventServicesInput {
    kind: Option<String>,
    service_kind: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventAdaptersInput {
    addon: Option<String>,
    addon_id: Option<String>,
    transport: Option<String>,
    direction: Option<String>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct EventEmitInput {
    addon: Option<String>,
    addon_id: Option<String>,
    adapter_id: String,
    event_type: String,
    action: String,
    origin: Option<String>,
    payload: Option<Value>,
    dry_run: Option<bool>,
    project_root: Option<String>,
    addon_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct InboundEventRouteInput {
    event_id: String,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboundEventActivationDispatchInput {
    event_id: String,
    project_root: Option<String>,
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct IdentityContextInput {
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdentityRegistryInput {
    scope: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdentityMembershipInput {
    subject_scope: Option<String>,
    subject: Option<String>,
    subject_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdentityMembershipUpdateMcpInput {
    subject_scope: Option<String>,
    subject: Option<String>,
    subject_id: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    role: Option<String>,
    status: Option<String>,
    #[serde(default)]
    grant_permissions: Vec<String>,
    #[serde(default)]
    revoke_grants: Vec<String>,
    #[serde(default)]
    deny_permissions: Vec<String>,
    #[serde(default)]
    remove_denies: Vec<String>,
    expires_at: Option<String>,
    clear_expires_at: Option<bool>,
    not_before: Option<String>,
    clear_not_before: Option<bool>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdentityLinkMcpInput {
    left_scope: String,
    left_id: String,
    right_scope: String,
    right_id: String,
    #[serde(default)]
    link_type: Option<String>,
    source: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdentityLinksInput {
    scope: Option<String>,
    id: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdentityResolveInput {
    scope: String,
    id: String,
}

#[derive(Debug, Deserialize)]
struct TenantIndexInput {
    resource_type: Option<String>,
    organization: Option<String>,
    organization_id: Option<String>,
    brand: Option<String>,
    brand_id: Option<String>,
    product: Option<String>,
    product_id: Option<String>,
    workflow: Option<String>,
    workflow_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TenantPolicyInput {
    workflow: Option<String>,
    workflow_id: Option<String>,
    mode: Option<String>,
    action: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InteractiveRouteInput {
    input: String,
    origin: Option<String>,
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
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    lanes: Vec<TeamworkLaneInput>,
    max_parallel_agents: Option<usize>,
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
struct RunDriveInput {
    run_id: String,
    executor: Option<String>,
    ttl_seconds: Option<u64>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunStepInput {
    run_id: String,
    executor: Option<String>,
    ttl_seconds: Option<u64>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunExecuteWaveInput {
    run_id: String,
    executor: Option<String>,
    ttl_seconds: Option<u64>,
    timeout_seconds: Option<u64>,
    context_budget: Option<usize>,
    max_parallel: Option<usize>,
    allow_exec: bool,
    approved_by: String,
    reason: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunCompleteTaskInput {
    run_id: String,
    task_id: String,
    executor: Option<String>,
    summary: String,
    #[serde(default)]
    artifacts: Vec<String>,
    evidence_command: Option<String>,
    evidence_exit_code: Option<i32>,
    evidence_summary: Option<String>,
    estimated_usd: Option<f64>,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    ttl_seconds: Option<u64>,
    context_budget: Option<usize>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EnsureFinalAuditInput {
    workflow_id: String,
    executor: Option<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunSwitchExecutorInput {
    run_id: String,
    executor: String,
    #[serde(default)]
    fallback_executors: Vec<String>,
    summary: Option<String>,
    ttl_seconds: Option<u64>,
    pid: Option<u32>,
    reason: Option<String>,
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
    expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkflowAddTaskInput {
    workflow_id: String,
    task_id: Option<String>,
    description: String,
    priority: String,
    origin: String,
    expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkflowUpdateTaskInput {
    workflow_id: String,
    task_id: String,
    title: Option<String>,
    goal: Option<String>,
    expected_output: Option<String>,
    origin: String,
    expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkflowSetPriorityInput {
    workflow_id: String,
    task_id: String,
    priority: String,
    origin: String,
    expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkflowDependencyInput {
    workflow_id: String,
    task_id: String,
    depends_on: String,
    origin: String,
    expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkflowSetImpedimentInput {
    workflow_id: String,
    task_id: String,
    reason: String,
    #[serde(default = "default_workflow_impediment_kind")]
    kind: String,
    origin: String,
    expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkflowClearImpedimentInput {
    workflow_id: String,
    task_id: String,
    impediment_id: Option<String>,
    origin: String,
    expected_revision: Option<u64>,
}

fn default_workflow_impediment_kind() -> String {
    "manual".to_string()
}

#[derive(Debug, Deserialize)]
struct WorkflowUpdateNodeBrainInput {
    workflow_id: String,
    task_id: String,
    default_brain: Option<String>,
    #[serde(default)]
    allowed_brains: Vec<String>,
    #[serde(default)]
    agent_slots: Vec<String>,
    max_parallel_agents: Option<usize>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TeamworkPlanInput {
    goal: String,
    #[serde(default)]
    lanes: Vec<TeamworkLaneInput>,
    max_parallel_agents: Option<usize>,
    #[serde(default)]
    bypass_cache: bool,
}

#[derive(Debug, Deserialize)]
struct TeamworkLaneInput {
    id: String,
    brain: String,
    agent_count: usize,
    parallel_group: Option<String>,
    responsibility: Option<String>,
}

impl TeamworkLaneInput {
    fn into_config(self) -> TeamworkLaneConfig {
        let id = self.id;
        TeamworkLaneConfig {
            parallel_group: self
                .parallel_group
                .unwrap_or_else(|| "implementation-wave-001".to_string()),
            responsibility: self
                .responsibility
                .unwrap_or_else(|| format!("Deliver independent bounded work for the {id} lane.")),
            id,
            brain: self.brain,
            agent_count: self.agent_count,
        }
    }
}

fn explicit_parallel_team_from_mcp_lanes(
    lanes: Vec<TeamworkLaneInput>,
    max_parallel_agents: Option<usize>,
    source: &str,
) -> Result<Option<CoreParallelTeamSpec>> {
    if lanes.is_empty() {
        if max_parallel_agents.is_some() {
            anyhow::bail!("max_parallel_agents requires at least one lane");
        }
        return Ok(None);
    }

    let lanes = lanes
        .into_iter()
        .map(TeamworkLaneInput::into_config)
        .collect::<Vec<_>>();
    let derived_parallelism = lanes.iter().try_fold(0usize, |total, lane| {
        total
            .checked_add(lane.agent_count)
            .context("parallel team lane agent count overflow")
    })?;
    let config = TeamworkParallelConfig {
        lanes,
        max_parallel_agents: max_parallel_agents.unwrap_or(derived_parallelism),
    };
    crate::teamwork::core_parallel_team_from_teamwork(&config, source).map(Some)
}

#[derive(Debug, Deserialize)]
struct WorkflowAttachArtifactInput {
    workflow_id: String,
    path: String,
    kind: String,
    #[serde(default)]
    tags: Vec<String>,
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
    project_root: Option<String>,
    view: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorktreeGuardCheckInput {
    worktree: String,
    #[serde(default = "default_worktree_guard_operation")]
    operation: String,
    paths: Vec<String>,
    reason: String,
    workflow_id: Option<String>,
    task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorktreeGuardCreatePredecessorInput {
    worktree: String,
    workflow_id: String,
    task_id: String,
    paths: Vec<String>,
    goal: String,
    allow_workflow_mutation: bool,
    approved_by: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorktreeSandboxInput {
    worktree: String,
    purpose: String,
    workflow_id: Option<String>,
    task_id: Option<String>,
    #[serde(default)]
    command: Vec<String>,
    allow_exec: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WorktreeSandboxStatusInput {
    #[serde(alias = "sandbox")]
    sandbox_id: String,
}

#[derive(Debug, Deserialize)]
struct WorktreeSandboxStopInput {
    #[serde(alias = "sandbox")]
    sandbox_id: String,
    allow_stop: Option<bool>,
}

fn default_worktree_guard_operation() -> String {
    "modify".to_string()
}

#[derive(Debug, Deserialize)]
struct TaskHandoffInput {
    workflow_id: String,
    task_id: String,
    executor: String,
    budget: Option<usize>,
    ttl_seconds: Option<u64>,
    project_root: Option<String>,
    view: Option<String>,
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
struct InteractiveHomeInput {
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MilestoneStatusInput {
    version: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProductionReadinessInput {
    version: Option<String>,
    manifest: String,
    evidence_root: String,
}

#[derive(Debug, Deserialize)]
struct MilestoneAttachEvidenceInput {
    version: Option<String>,
    capability: Option<String>,
    capability_id: Option<String>,
    kind: String,
    summary: String,
    artifact: Option<String>,
    artifact_path: Option<String>,
    approved_by: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MilestoneEvidencePlanInput {
    version: Option<String>,
    capability: Option<String>,
    capability_id: Option<String>,
    project_root: Option<String>,
    connected_brain: Option<String>,
    connected_runtime: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MilestonePrepareEvidenceInputsInput {
    version: Option<String>,
    capability: Option<String>,
    capability_id: Option<String>,
    project_root: Option<String>,
    connected_brain: Option<String>,
    connected_runtime: Option<String>,
    provider_command: Option<String>,
    model_id: Option<String>,
    approval_ref: Option<String>,
    apply: Option<bool>,
    approved_by: Option<String>,
    force: Option<bool>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MilestoneCollectEvidenceInput {
    version: Option<String>,
    capability: Option<String>,
    capability_id: Option<String>,
    kind: Option<String>,
    project_root: Option<String>,
    connected_brain: Option<String>,
    connected_runtime: Option<String>,
    approved_by: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MilestoneCollectReadyEvidenceInput {
    version: Option<String>,
    project_root: Option<String>,
    connected_brain: Option<String>,
    connected_runtime: Option<String>,
    approved_by: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MilestoneCliDemoInput {
    origin: Option<String>,
    project_root: Option<String>,
    connected_brain: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MultimodalStatusInput {
    enable_experimental: Option<bool>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MultimodalInstallPlanInput {
    capability: Option<String>,
    capability_id: Option<String>,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MultimodalReadinessInput {
    capability: Option<String>,
    capability_id: Option<String>,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
    allow: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultimodalBenchmarkTemplateInput {
    capability: Option<String>,
    capability_id: Option<String>,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MultimodalBenchmarkResultInput {
    capability: Option<String>,
    capability_id: Option<String>,
    fixture: Option<String>,
    fixture_id: Option<String>,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
    approved_by: Option<String>,
    confirm_fixture_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultimodalRuntimeBenchmarkInput {
    capability: Option<String>,
    capability_id: Option<String>,
    fixture: Option<String>,
    fixture_id: Option<String>,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
    approved_by: Option<String>,
    confirm_runtime_execution: Option<bool>,
    allow_model: Option<bool>,
    connected_runtime: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MultimodalDemoPlanInput {
    demo: Option<String>,
    demo_id: Option<String>,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MultimodalDemoReceiptInput {
    demo: Option<String>,
    demo_id: Option<String>,
    fixture: Option<String>,
    fixture_id: Option<String>,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
    approved_by: Option<String>,
    confirm_local_fixture: Option<bool>,
    allow_model: Option<bool>,
    allow_camera: Option<bool>,
    allow_microphone: Option<bool>,
    allow_screen: Option<bool>,
    allow_input: Option<bool>,
    allow_filesystem: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultimodalGuardInput {
    capability: String,
    action: String,
    enable_experimental: Option<bool>,
    project_root: Option<String>,
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

#[derive(Debug, Deserialize)]
struct PatchPlanInput {
    workflow_id: String,
    task_id: String,
    intent: String,
    paths: Vec<String>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PatchApplyInput {
    workflow_id: String,
    task_id: String,
    paths: Vec<String>,
    origin: Option<String>,
    plan_artifact: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PatchReviewInput {
    workflow_id: String,
    task_id: String,
    paths: Vec<String>,
    origin: Option<String>,
    plan_artifact: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PatchDiffInput {
    workflow_id: String,
    task_id: String,
    paths: Vec<String>,
    file_index: Option<usize>,
    hunk_index: Option<usize>,
    context_lines: Option<usize>,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PatchRevertInput {
    workflow_id: String,
    task_id: String,
    apply_artifact: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PatchRestoreInput {
    workflow_id: String,
    task_id: String,
    revert_artifact: String,
    approved_by: String,
    confirm_restore: bool,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CredentialVaultInput {
    contract: String,
    data: String,
    vault_bin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SecuritySecretScanInput {
    input: String,
    scope: Option<String>,
    workflow_id: Option<String>,
    origin: Option<String>,
    enable_entropy: Option<bool>,
    enable_local_ai_fallback: Option<bool>,
    allow_external_ai: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AwsCheckInput {
    aws_ops_bin: Option<String>,
    vault_contract: Option<String>,
    vault_data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AwsInventoryInput {
    aws_ops_bin: Option<String>,
    vault_contract: Option<String>,
    vault_data: Option<String>,
    regions: Option<String>,
    all_regions: Option<bool>,
    full: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AwsRawInput {
    aws_ops_bin: Option<String>,
    vault_contract: Option<String>,
    vault_data: Option<String>,
    allow_mutation: Option<bool>,
    reason: Option<String>,
    aws_args: Vec<String>,
}

pub fn mcp_tools_manifest() -> McpToolsManifest {
    McpToolsManifest {
        status: "mcp_tools_loaded".to_string(),
        schema_version: MCP_TOOLS_SCHEMA_VERSION.to_string(),
        protocol: "model_context_protocol".to_string(),
        tools: vec![
            tool(
                "forge.mission.start",
                "Start Operational Mission",
                "Persist an operational mission, restricted orchestrator, task graph and real worktree binding.",
                object_schema(
                    &[
                        ("goal", "string", "mission objective"),
                        ("squad", "string", "squad id; defaults to software-factory"),
                        ("squad_version", "string", "optional immutable squad version"),
                        ("worktree", "string", "real Git worktree path"),
                    ],
                    &["goal", "worktree"],
                ),
                "forge.mission.start.v1",
                &["forge", "mission", "start", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.mission.drive",
                "Drive Operational Mission",
                "Consume one pending inbox handoff or create one bounded task assignment and harness.",
                object_schema(
                    &[("mission_id", "string", "mission id")],
                    &["mission_id"],
                ),
                "forge.mission.drive.v1",
                &["forge", "mission", "drive", "<mission-id>", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.mission.execute",
                "Execute Mission Assignment",
                "Execute an assigned command through the mission's pinned worktree sandbox with structured approval provenance and an idempotent receipt. The dry-run branch is read-only; tool metadata remains mutating because the execution branch can launch a process.",
                mission_execute_input_schema(),
                "forge.mission.execution_receipt.v3",
                &["forge", "mission", "execute", "<mission-id>", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.mission.submit",
                "Submit Mission Execution Receipt",
                "Queue a typed task delivery backed by a persisted execution receipt; caller-supplied validation claims are not accepted.",
                mission_submit_input_schema(),
                "forge.mission.submit.v1",
                &["forge", "mission", "submit", "<mission-id>", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.mission.resume",
                "Resume Operational Mission",
                "Lease and consume one persisted mission inbox item after reopening durable state.",
                object_schema(
                    &[("mission_id", "string", "mission id")],
                    &["mission_id"],
                ),
                "forge.mission.drive.v1",
                &["forge", "mission", "resume", "<mission-id>", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.mission.execution.list",
                "List Mission Execution Receipts",
                "List durable execution receipts, optionally filtered by mission.",
                object_schema(
                    &[
                        ("mission_id", "string", "optional mission id filter"),
                        ("task_id", "string", "optional task id filter"),
                    ],
                    &[],
                ),
                "forge.mission.execution_receipt_list.v1",
                &["forge", "mission", "execution", "list", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.mission.execution.inspect",
                "Inspect Mission Execution Receipt",
                "Inspect one durable mission execution receipt by receipt id.",
                object_schema(
                    &[("receipt_id", "string", "execution receipt id")],
                    &["receipt_id"],
                ),
                "forge.mission.execution_receipt.v3",
                &["forge", "mission", "execution", "inspect", "<receipt-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.mission.execution.reconcile",
                "Reconcile Mission Execution",
                "Record an explicitly approved no-effect outcome for an indeterminate, failed or timed-out execution before allowing a new idempotency key.",
                mission_execution_reconcile_input_schema(),
                "forge.mission.execution_reconciliation.v1",
                &[
                    "forge",
                    "mission",
                    "execution",
                    "reconcile",
                    "<receipt-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
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
                "forge.events.list",
                "List Workflow Events",
                "Return Forge's tenant-aware typed event stream projection for a workflow, including event severity, category, origin and correlation ids.",
                object_schema(
                    &[
                        ("workflow_id", "string", "workflow id"),
                        ("limit", "integer", "optional latest event limit"),
                    ],
                    &["workflow_id"],
                ),
                "forge.event_stream.v1",
                &[
                    "forge",
                    "events",
                    "list",
                    "--workflow",
                    "<workflow-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.events.timeline",
                "List Global Event Timeline",
                "Return a tenant-filterable global timeline of workflow events using Forge's typed event envelope.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("limit", "integer", "optional latest event limit"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        (
                            "after_sequence",
                            "integer",
                            "optional cursor; returns events with store_sequence greater than this value",
                        ),
                    ],
                    &[],
                ),
                "forge.event_timeline.v1",
                &["forge", "events", "timeline", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.ops.snapshot",
                "Read Ops Snapshot",
                "Read the operational assisted-operations snapshot, including workflows, improvement candidates, Addon views and optional project memory/context governance.",
                object_schema(
                    &[
                        (
                            "project_root",
                            "string",
                            "optional project root containing .forge/memory-governance.json",
                        ),
                        ("addon_dirs", "array", "optional Addon manifest directories"),
                    ],
                    &[],
                ),
                "forge.ops.snapshot.v1",
                &["forge", "ops", "snapshot", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.ops.addon_renderer_event",
                "Record Addon Renderer Event",
                "Record a safe client-side interaction for an Addon renderer after validating the renderer's allowed_client_events contract.",
                object_schema(
                    &[
                        ("workflow_id", "string", "workflow id"),
                        ("addon_id", "string", "optional Addon id; required when multiple Addons declare the same view id"),
                        ("view_id", "string", "Addon view id"),
                        ("event_kind", "string", "filter_changed|selection_changed|refresh_requested|hover_changed|draft_changed|submit_requested"),
                        ("actor", "string", "operator or agent id"),
                        ("payload", "object", "optional event payload"),
                        ("addon_dirs", "array", "optional Addon manifest directories"),
                    ],
                    &["workflow_id", "view_id", "event_kind"],
                ),
                "forge.ops.addon_renderer_client_event.v1",
                &[
                    "forge",
                    "ops",
                    "renderer-event",
                    "--workflow",
                    "<workflow-id>",
                    "--addon",
                    "<addon-id>",
                    "--view",
                    "<view-id>",
                    "--event-kind",
                    "<event-kind>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.events.observability",
                "List Event Observability Index",
                "Return a normalized event observability index grouped by tenant, workflow, node and Addon, with duration, retry, wait, context-pressure and memory metrics.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("node_ref", "string", "optional node/task reference filter"),
                        ("addon_id", "string", "optional Addon id filter"),
                        ("limit", "integer", "optional latest event limit"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        (
                            "after_sequence",
                            "integer",
                            "optional cursor; returns records with store_sequence greater than this value",
                        ),
                    ],
                    &[],
                ),
                "forge.event_observability_index.v1",
                &["forge", "events", "observability", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.events.observability_history",
                "List Event Observability History",
                "Return time-bucketed event observability rollups for operators and policy loops, grouped by tenant, workflow, node, Addon or all events.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("node_ref", "string", "optional node/task reference filter"),
                        ("addon_id", "string", "optional Addon id filter"),
                        ("bucket", "string", "hour or day; defaults to day"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        (
                            "group_by",
                            "string",
                            "none, tenant, workflow, node or addon; defaults to none",
                        ),
                        ("limit", "integer", "optional latest bucket limit"),
                        (
                            "after_sequence",
                            "integer",
                            "optional cursor; only includes records with store_sequence greater than this value",
                        ),
                    ],
                    &[],
                ),
                "forge.event_observability_history.v1",
                &[
                    "forge",
                    "events",
                    "observability-history",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.events.improvement_policy",
                "Recommend Event Improvement Policy",
                "Return read-only improvement recommendations derived from normalized event observability signals such as repeated execution, duration, retries, waits and context pressure.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("node_ref", "string", "optional node/task reference filter"),
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        ("min_events", "integer", "minimum event count before a scope is considered"),
                        ("min_duration_ms", "integer", "minimum total duration before repeated work is considered expensive"),
                        ("min_retries", "integer", "minimum total retries before rework is recommended"),
                        (
                            "min_context_pressure_bps",
                            "integer",
                            "minimum context pressure in basis points before context routing repair is recommended",
                        ),
                        ("min_wait_seconds", "integer", "minimum total wait seconds before wait supervision is recommended"),
                        ("limit", "integer", "optional recommendation limit"),
                        (
                            "after_sequence",
                            "integer",
                            "optional cursor; only includes records with store_sequence greater than this value",
                        ),
                    ],
                    &[],
                ),
                "forge.event_improvement_policy.v1",
                &[
                    "forge",
                    "events",
                    "improvement-policy",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.events.ingest",
                "Ingest Inbound Event",
                "Write an origin-agnostic inbound event to Forge's global event inbox without requiring an existing workflow.",
                object_schema(
                    &[
                        ("origin", "string", "event origin, for example telegram|api|cron"),
                        ("action", "string", "event action, for example start_workflow"),
                        ("data", "object", "event payload"),
                        ("project_root", "string", "project root for operating context"),
                    ],
                    &["origin", "action"],
                ),
                "forge.event_ingest.v1",
                &[
                    "forge",
                    "events",
                    "ingest",
                    "--origin",
                    "<origin>",
                    "--action",
                    "<action>",
                    "--input",
                    "<json>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.inbox",
                "List Inbound Event Inbox",
                "List pending or routed inbound events from Forge's global event inbox.",
                object_schema(
                    &[
                        ("status", "string", "optional status filter"),
                        ("limit", "integer", "maximum events"),
                        ("project_root", "string", "project root for tenant-aware inbox filtering"),
                    ],
                    &[],
                ),
                "forge.event_inbox.v1",
                &["forge", "events", "inbox", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.events.scan",
                "Scan Inbound Event Inbox",
                "Route a bounded batch of pending inbound events through Forge's event engine, marking failed events with worker error evidence.",
                object_schema(
                    &[
                        ("status", "string", "event status to scan; defaults to pending"),
                        ("limit", "integer", "maximum events to scan"),
                        ("project_root", "string", "project root for context/addons"),
                        ("dispatch_activations", "boolean", "enqueue matched Addon event workflow activations while scanning"),
                    ],
                    &[],
                ),
                "forge.event_worker.v1",
                &[
                    "forge",
                    "events",
                    "scan",
                    "--project-root",
                    ".",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.worker",
                "Run Inbound Event Worker Loop",
                "Run a bounded configurable event worker loop over Forge's inbound inbox, using the same routing and failure evidence as events scan.",
                object_schema(
                    &[
                        ("status", "string", "event status to scan; defaults to pending"),
                        ("limit", "integer", "maximum events per cycle"),
                        ("project_root", "string", "project root for context/addons"),
                        ("max_cycles", "integer", "maximum worker cycles; defaults to 1"),
                        ("interval_seconds", "integer", "sleep between cycles; defaults to 300"),
                        ("idle_exit", "boolean", "stop early when a cycle scans no events"),
                        ("dispatch_activations", "boolean", "enqueue matched Addon event workflow activations during worker scans"),
                        ("stop_file", "string", "cooperative shutdown file checked between cycles"),
                    ],
                    &[],
                ),
                "forge.event_worker_loop.v1",
                &[
                    "forge",
                    "events",
                    "worker",
                    "--project-root",
                    ".",
                    "--max-cycles",
                    "1",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.service_plan",
                "Plan Managed Event Service",
                "Create a plan-only managed service contract for an event worker or webhook ingress, including command, lease, backoff, health and shutdown policy with global timeline audit.",
                object_schema(
                    &[
                        ("kind", "string", "worker|webhook_ingress"),
                        ("project_root", "string", "project root for context/addons"),
                        ("status", "string", "worker event status; defaults to pending"),
                        ("limit", "integer", "worker event limit per cycle"),
                        ("max_cycles", "integer", "bounded cycles for the generated worker command"),
                        ("interval_seconds", "integer", "sleep between worker cycles"),
                        ("idle_exit", "boolean", "worker exits early when idle"),
                        ("dispatch_activations", "boolean", "worker command enqueues matched Addon event workflow activations"),
                        ("host", "string", "webhook host; defaults to 127.0.0.1"),
                        ("port", "integer", "webhook port"),
                        ("path", "string", "webhook path"),
                        ("origin", "string", "webhook origin"),
                        ("action", "string", "webhook action"),
                        ("schema", "string", "webhook event schema"),
                        ("route", "boolean", "route webhook events after ingest"),
                        ("hmac_secret_env", "string", "optional HMAC secret env var name"),
                        ("signature_header", "string", "optional signature header"),
                        ("lease_seconds", "integer", "planned service lease TTL"),
                        ("heartbeat_seconds", "integer", "planned heartbeat interval"),
                    ],
                    &["kind"],
                ),
                "forge.event_service_plan.v1",
                &[
                    "forge",
                    "events",
                    "service-plan",
                    "--kind",
                    "worker",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.service_run",
                "Run Managed Event Service",
                "Acquire a persistent event-service lease, execute a bounded worker or webhook ingress service, persist service health and write a global timeline audit.",
                object_schema(
                    &[
                        ("kind", "string", "worker or webhook_ingress"),
                        ("project_root", "string", "project root for context/addons"),
                        ("status", "string", "event status to scan; defaults to pending"),
                        ("limit", "integer", "maximum events per cycle"),
                        ("max_cycles", "integer", "maximum worker cycles"),
                        ("interval_seconds", "integer", "sleep between cycles"),
                        ("idle_exit", "boolean", "stop early when idle"),
                        ("dispatch_activations", "boolean", "worker service enqueues matched Addon event workflow activations"),
                        ("stop_file", "string", "cooperative shutdown file checked between worker cycles or webhook requests"),
                        ("host", "string", "webhook bind host"),
                        ("port", "integer", "webhook bind port"),
                        ("path", "string", "webhook request path"),
                        ("origin", "string", "webhook origin"),
                        ("action", "string", "webhook action"),
                        ("schema", "string", "webhook event schema"),
                        ("route", "boolean", "route webhook events after ingest"),
                        ("max_requests", "integer", "maximum webhook requests"),
                        ("max_body_bytes", "integer", "maximum webhook body size"),
                        ("hmac_secret_env", "string", "optional HMAC secret env var name"),
                        ("signature_header", "string", "optional signature header"),
                        ("lease_owner", "string", "service lease owner id"),
                        ("lease_seconds", "integer", "service lease TTL"),
                        ("heartbeat_seconds", "integer", "service heartbeat interval"),
                    ],
                    &[],
                ),
                "forge.event_service_run.v1",
                &[
                    "forge",
                    "events",
                    "service-run",
                    "--kind",
                    "worker",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.service_supervise",
                "Supervise Managed Event Service",
                "Run a bounded supervisor loop over managed event service runs, applying executable backoff, cooperative stop-file shutdown, aggregate health and global timeline audit.",
                object_schema(
                    &[
                        ("kind", "string", "worker or webhook_ingress"),
                        ("project_root", "string", "project root for context/addons"),
                        ("status", "string", "worker event status to scan"),
                        ("limit", "integer", "worker event limit per cycle"),
                        ("max_cycles", "integer", "maximum worker cycles per run"),
                        ("interval_seconds", "integer", "sleep between worker cycles"),
                        ("idle_exit", "boolean", "stop a worker run early when idle"),
                        ("dispatch_activations", "boolean", "worker service runs enqueue matched Addon event workflow activations"),
                        ("stop_file", "string", "cooperative supervisor shutdown file"),
                        ("host", "string", "webhook bind host"),
                        ("port", "integer", "webhook bind port"),
                        ("path", "string", "webhook request path"),
                        ("origin", "string", "webhook origin"),
                        ("action", "string", "webhook action"),
                        ("schema", "string", "webhook event schema"),
                        ("route", "boolean", "route webhook events after ingest"),
                        ("max_requests", "integer", "maximum webhook requests per run"),
                        ("max_body_bytes", "integer", "maximum webhook body size"),
                        ("hmac_secret_env", "string", "optional HMAC secret env var name"),
                        ("signature_header", "string", "optional signature header"),
                        ("lease_owner", "string", "service lease owner id"),
                        ("lease_seconds", "integer", "service lease TTL"),
                        ("heartbeat_seconds", "integer", "service heartbeat interval"),
                        ("max_runs", "integer", "bounded supervisor run count"),
                        ("backoff_initial_seconds", "integer", "initial backoff after failed runs"),
                        ("backoff_max_seconds", "integer", "maximum backoff after repeated failures"),
                        ("scan_schedules", "boolean", "include schedule worker status and optionally run scan-due when execute is true"),
                        ("schedule_executor", "string", "executor id used for schedule leases"),
                        ("schedule_max_workers", "integer", "maximum parallel schedule workers"),
                        ("schedule_ttl_seconds", "integer", "schedule task lease TTL"),
                    ],
                    &[],
                ),
                "forge.event_service_supervisor.v1",
                &[
                    "forge",
                    "events",
                    "service-supervise",
                    "--kind",
                    "worker",
                    "--max-runs",
                    "1",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.runtime_reconcile",
                "Reconcile Event Runtime",
                "Inspect Forge's workflow runtime registry, inbound inbox and event service leases, then recommend or optionally execute a bounded event worker supervisor.",
                object_schema(
                    &[
                        ("project_root", "string", "project root for context/addons"),
                        ("status", "string", "inbound event status to reconcile"),
                        ("limit", "integer", "inbound events sampled for the worker"),
                        ("service_limit", "integer", "event service records sampled"),
                        ("execute", "boolean", "execute the recommended bounded supervisor when no active worker lease exists"),
                        ("max_cycles", "integer", "maximum worker cycles per supervisor run"),
                        ("interval_seconds", "integer", "sleep between worker cycles"),
                        ("idle_exit", "boolean", "stop a worker run early when idle"),
                        ("dispatch_activations", "boolean", "worker supervisor enqueues matched Addon event workflow activations during runtime reconciliation"),
                        ("recover_stale_services", "boolean", "mark expired running worker leases as stale before recommending services"),
                        ("stop_file", "string", "cooperative supervisor shutdown file"),
                        ("lease_owner", "string", "service lease owner id"),
                        ("lease_seconds", "integer", "service lease TTL"),
                        ("heartbeat_seconds", "integer", "service heartbeat interval"),
                        ("max_runs", "integer", "bounded supervisor run count"),
                        ("backoff_initial_seconds", "integer", "initial backoff after failed runs"),
                        ("backoff_max_seconds", "integer", "maximum backoff after repeated failures"),
                    ],
                    &[],
                ),
                "forge.event_runtime_reconcile.v1",
                &[
                    "forge",
                    "events",
                    "runtime-reconcile",
                    "--project-root",
                    ".",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.runtime_daemon",
                "Run Event Runtime Daemon",
                "Run a bounded runtime reconciliation daemon with its own event_services lease, heartbeat, cooperative stop-file shutdown and global timeline audit.",
                object_schema(
                    &[
                        ("project_root", "string", "project root for context/addons"),
                        ("status", "string", "inbound event status to reconcile"),
                        ("limit", "integer", "inbound events sampled for the worker"),
                        ("service_limit", "integer", "event service records sampled"),
                        ("execute", "boolean", "execute the recommended bounded supervisor when no active worker lease exists"),
                        ("max_cycles", "integer", "maximum daemon reconcile cycles"),
                        ("interval_seconds", "integer", "sleep between daemon cycles"),
                        ("idle_exit", "boolean", "stop the daemon early when a cycle has no recommendation or execution"),
                        ("dispatch_activations", "boolean", "runtime daemon enqueues matched Addon event workflow activations through executed worker supervisors"),
                        ("continuous", "boolean", "ignore max_cycles and run until idle_exit or stop_file requests shutdown"),
                        ("cycle_retention", "integer", "maximum per-cycle reports retained in the daemon result"),
                        ("recover_stale_services", "boolean", "mark expired running worker leases as stale during each reconcile cycle"),
                        ("stop_file", "string", "cooperative daemon shutdown file"),
                        ("lease_owner", "string", "daemon lease owner id"),
                        ("lease_seconds", "integer", "daemon service lease TTL"),
                        ("heartbeat_seconds", "integer", "daemon heartbeat interval"),
                        ("max_runs", "integer", "bounded supervisor run count per reconcile execution"),
                        ("backoff_initial_seconds", "integer", "initial supervisor backoff after failed runs"),
                        ("backoff_max_seconds", "integer", "maximum supervisor backoff after repeated failures"),
                        ("scan_schedules", "boolean", "include schedule worker status and scan due schedules during daemon cycles"),
                        ("schedule_executor", "string", "executor id used for schedule leases"),
                        ("schedule_max_workers", "integer", "maximum parallel schedule workers"),
                        ("schedule_ttl_seconds", "integer", "schedule task lease TTL"),
                    ],
                    &[],
                ),
                "forge.event_runtime_daemon.v1",
                &[
                    "forge",
                    "events",
                    "runtime-daemon",
                    "--project-root",
                    ".",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.services",
                "List Event Services",
                "List persisted event service records with status, lease, heartbeat, health and latest data.",
                object_schema(
                    &[
                        ("project_root", "string", "project root for tenant context"),
                        ("kind", "string", "optional service kind filter"),
                        ("status", "string", "optional service status filter"),
                        ("limit", "integer", "maximum rows"),
                    ],
                    &[],
                ),
                "forge.event_services.v1",
                &[
                    "forge",
                    "events",
                    "services",
                    "--project-root",
                    ".",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.events.services_recover",
                "Recover Stale Event Services",
                "Mark running event service records with expired leases as stale while preserving their latest health and recovery evidence.",
                object_schema(
                    &[
                        ("project_root", "string", "project root for tenant context"),
                        ("kind", "string", "optional service kind filter"),
                        ("limit", "integer", "maximum running services scanned"),
                        ("origin", "string", "recovery origin"),
                    ],
                    &[],
                ),
                "forge.event_services_recovery.v1",
                &[
                    "forge",
                    "events",
                    "services-recover",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.adapters",
                "List Event Adapters",
                "Return declarative Addon event adapters plus Event Extension triggers, listeners and channels by Addon, transport or direction so agents can discover ingress/egress contracts without channel-specific code.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("transport", "string", "optional transport filter"),
                        ("direction", "string", "ingress|egress|bidirectional"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_event_adapters.v1",
                &[
                    "forge",
                    "events",
                    "adapters",
                    "--transport",
                    "<transport>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.events.emit",
                "Emit Egress Event",
                "Send a typed outbound event through a declared Addon egress adapter after direction, action, event_type, permission and endpoint allowlist checks.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id when adapter id is not unique"),
                        ("adapter_id", "string", "event adapter id"),
                        ("event_type", "string", "declared event type to emit"),
                        ("action", "string", "declared outbound action"),
                        ("origin", "string", "optional origin; defaults to forge"),
                        ("payload", "object", "event payload object"),
                        ("dry_run", "boolean", "validate and build the request without sending"),
                        (
                            "project_root",
                            "string",
                            "optional project root for operating context; defaults to .",
                        ),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["adapter_id", "event_type", "action"],
                ),
                "forge.event_egress_emit.v1",
                &[
                    "forge",
                    "events",
                    "emit",
                    "--adapter",
                    "<adapter-id>",
                    "--event-type",
                    "<event-type>",
                    "--action",
                    "<action>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.route",
                "Route Inbound Event",
                "Route a pending inbound event through Forge's event engine with declared Addon adapter policy checks for origin, action, schema and permission gate before start/continue/modify/pause/resume/complete.",
                object_schema(
                    &[
                        ("event_id", "string", "inbound event id"),
                        ("project_root", "string", "optional project root for context/addons"),
                    ],
                    &["event_id"],
                ),
                "forge.event_route.v1",
                &[
                    "forge",
                    "events",
                    "route",
                    "--event",
                    "<event-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.events.dispatch_activations",
                "Dispatch Inbound Event Activations",
                "Route an inbound event if needed, then enqueue dispatch-ready Addon Event Extension workflow activations into Forge's runtime contract dispatch ledger without executing handlers inline.",
                object_schema(
                    &[
                        ("event_id", "string", "inbound event id"),
                        ("project_root", "string", "optional project root for context/addons"),
                        ("dry_run", "boolean", "plan dispatches without persisting ledger rows"),
                    ],
                    &["event_id"],
                ),
                "forge.event_activation_dispatch.v1",
                &[
                    "forge",
                    "events",
                    "dispatch-activations",
                    "--event",
                    "<event-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.cost.ledger",
                "Inspect Cost Ledger",
                "Return estimated task cost and observed event cost grouped by workflow, node, tenant and detected Addon source.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                    ],
                    &[],
                ),
                "forge.cost_ledger.v1",
                &["forge", "cost", "ledger", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.cost.materialize",
                "Materialize Cost Ledger Index",
                "Write a normalized cost ledger index from planned task costs and observed event costs, then return the persisted rows.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("source_kind", "string", "planned_task|observed_event filter for returned rows"),
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        ("limit", "integer", "optional persisted row limit"),
                    ],
                    &[],
                ),
                "forge.cost_ledger_index.v1",
                &["forge", "cost", "materialize", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.cost.incremental",
                "Incremental Cost Ledger Materialization",
                "Scan global events after a cursor, deduplicate affected workflows and materialize cost rows only for those workflows.",
                object_schema(
                    &[
                        ("after_sequence", "integer", "optional global event cursor"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("source_kind", "string", "planned_task|observed_event filter for returned rows"),
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        ("limit", "integer", "optional scanned event and persisted row limit"),
                    ],
                    &[],
                ),
                "forge.cost_ledger_incremental.v1",
                &["forge", "cost", "incremental", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.cost.history",
                "List Cost Ledger History",
                "Read time-bucketed cost rollups from the normalized cost ledger index.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("source_kind", "string", "planned_task|observed_event filter"),
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        ("bucket", "string", "hour|day bucket, default day"),
                        ("group_by", "string", "none|tenant|workflow|source_kind|addon|executor"),
                        ("limit", "integer", "optional bucket limit"),
                    ],
                    &[],
                ),
                "forge.cost_ledger_history.v1",
                &["forge", "cost", "history", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.cost.maintain",
                "Maintain Cost Ledger",
                "Materialize the normalized cost ledger index and immediately return a time-bucketed rollup plus plan-only retention policy for scheduled Cost OS backfill.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("source_kind", "string", "planned_task|observed_event filter"),
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        ("bucket", "string", "hour|day bucket, default day"),
                        ("group_by", "string", "none|tenant|workflow|source_kind|addon|executor"),
                        ("limit", "integer", "optional persisted row and bucket limit"),
                        ("retention_days", "integer", "plan-only retention horizon"),
                    ],
                    &[],
                ),
                "forge.cost_ledger_maintenance.v1",
                &["forge", "cost", "maintain", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.cost.daemon",
                "Run Cost Ledger Daemon",
                "Run bounded dedicated Cost OS maintenance cycles, recording each cycle in the global event timeline for operational observability.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("source_kind", "string", "planned_task|observed_event filter"),
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        ("bucket", "string", "hour|day bucket, default day"),
                        ("group_by", "string", "none|tenant|workflow|source_kind|addon|executor"),
                        ("limit", "integer", "optional persisted row and bucket limit"),
                        ("retention_days", "integer", "plan-only retention horizon"),
                        ("max_cycles", "integer", "bounded daemon cycle count"),
                        ("interval_seconds", "integer", "sleep seconds between cycles"),
                        ("idle_exit", "boolean", "stop after a cycle with no indexed rows"),
                        ("origin", "string", "audit origin for daemon events"),
                    ],
                    &[],
                ),
                "forge.cost_ledger_daemon.v1",
                &["forge", "cost", "daemon", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.cost.retention",
                "Apply Cost Ledger Retention",
                "Plan or execute approval-gated physical deletion of stale normalized cost ledger rows after a retention window.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
                        ("source_kind", "string", "planned_task|observed_event filter"),
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "project_root",
                            "string",
                            "optional project root used for tenant-policy enforcement",
                        ),
                        ("retention_days", "integer", "required positive retention window"),
                        ("limit", "integer", "optional candidate row limit"),
                        ("apply", "boolean", "request physical deletion"),
                        ("approved_by", "string", "required when apply is true"),
                        ("reason", "string", "required when apply is true"),
                        ("confirm", "boolean", "required true when apply is true"),
                        ("origin", "string", "audit origin for retention event"),
                    ],
                    &[],
                ),
                "forge.cost_ledger_retention.v1",
                &["forge", "cost", "retention", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.improve.candidates",
                "Rank Improvement Candidates",
                "Rank live or degraded workflows using runs, heartbeats, workflow events, outcome evidence, parallelization opportunities and avoidable AI-cost signals.",
                object_schema(
                    &[
                        ("limit", "integer", "maximum candidates to return"),
                        ("workflow_ids", "array", "optional workflow ids to include"),
                        (
                            "goal_contains",
                            "array",
                            "optional case-insensitive goal text filters",
                        ),
                    ],
                    &[],
                ),
                "forge.orchestrator_improvement_candidates.v1",
                &["forge", "improve", "candidates", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.improve.apply_event_policy",
                "Apply Event Improvement Policy",
                "Plan or apply a selected event-improvement recommendation through a governed workflow revision with rollback details, equivalence gate and no autopromotion.",
                object_schema(
                    &[
                        ("workflow_id", "string", "workflow id"),
                        ("recommendation_id", "string", "optional exact recommendation id"),
                        ("recommended_policy", "string", "optional policy such as prefer_deterministic_node"),
                        ("apply", "boolean", "default false; true requests a revision"),
                        ("approved_by", "string", "required for apply=true"),
                        ("origin", "string", "origin label for audit events"),
                    ],
                    &["workflow_id"],
                ),
                "forge.improve.event_policy_application.v1",
                &[
                    "forge",
                    "improve",
                    "apply-event-policy",
                    "--workflow",
                    "<workflow-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.improve.benchmark_event_policy",
                "Benchmark Event Improvement Policy",
                "Validate the latest applied event-improvement policy against current workflow state, rollback readiness and validation evidence without auto-promoting.",
                object_schema(
                    &[
                        ("workflow_id", "string", "workflow id"),
                        ("recommendation_id", "string", "optional exact recommendation id"),
                        ("recommended_policy", "string", "optional policy such as prefer_deterministic_node"),
                        ("origin", "string", "origin label for audit events"),
                    ],
                    &["workflow_id"],
                ),
                "forge.improve.event_policy_benchmark.v1",
                &[
                    "forge",
                    "improve",
                    "benchmark-event-policy",
                    "--workflow",
                    "<workflow-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, false),
            ),
            tool(
                "forge.improve.promote_event_policy",
                "Promote Event Improvement Policy",
                "Accept a validated event-policy benchmark through explicit human approval, recording a workflow revision and idempotent promotion event.",
                object_schema(
                    &[
                        ("workflow_id", "string", "workflow id"),
                        ("recommendation_id", "string", "optional exact recommendation id"),
                        ("recommended_policy", "string", "optional policy such as prefer_deterministic_node"),
                        ("approved_by", "string", "required human or operator approval"),
                        ("origin", "string", "origin label for audit events"),
                    ],
                    &["workflow_id", "approved_by"],
                ),
                "forge.improve.event_policy_promotion.v1",
                &[
                    "forge",
                    "improve",
                    "promote-event-policy",
                    "--workflow",
                    "<workflow-id>",
                    "--approved-by",
                    "<operator>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.cli.create",
                "Create Workflow-backed CLI",
                "Create a Forge-owned workflow for generating an agent-native CLI, MCP server, skill and Addon runtime contract from one workflow-backed command model.",
                object_schema(
                    &[
                        ("name", "string", "required CLI/app slug such as hubspot or stripe"),
                        ("goal", "string", "optional objective for the generated CLI"),
                        ("source", "string", "optional API docs, website, OpenAPI or discovery source"),
                        ("commands", "array", "optional requested first-class commands"),
                        ("compound_commands", "array", "optional local-first compound insight commands"),
                    ],
                    &["name"],
                ),
                "forge.cli_factory.creation_plan.v1",
                &[
                    "forge",
                    "cli",
                    "create",
                    "--name",
                    "<name>",
                    "--goal",
                    "<goal>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.interactive.home",
                "Inspect Interactive Home",
                "Return the Forge interactive home dashboard for agent-visible runtime state without launching a TTY; project_root lets agents inspect project-scoped panels without relying on cwd.",
                object_schema(&[
                    ("project_root", "string", "optional project root for project-scoped dashboard panels"),
                ], &[]),
                "forge.interactive.home.v1",
                &[
                    "forge",
                    "interactive",
                    "home",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.guided_cockpit",
                "Inspect Guided Cockpit",
                "Return the Forge 0.5 guided cockpit that opens by default from `forge`, including panes, eight operator steps, previews, confirmations and rollback hints without mutating state.",
                object_schema(&[], &[]),
                "forge.interactive.guided_cockpit.v1",
                &[
                    "forge",
                    "interactive",
                    "guided-cockpit",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.ui_composition",
                "Inspect UI Composition",
                "Return the dedicated Forge UI composition contract with ordered regions, Core widgets, Addon widgets, renderer families and commands for TUI, web and agent dashboards without loading the full home dashboard.",
                object_schema(&[
                    ("project_root", "string", "optional project root for project-scoped dashboard panels"),
                ], &[]),
                "forge.interactive.ui_composition.v1",
                &[
                    "forge",
                    "interactive",
                    "ui-composition",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.readiness",
                "Inspect Interactive Readiness",
                "Return Forge interactive readiness for executors, brains, shells, Forge-controlled surfaces and harness diagnostics without loading the full home dashboard.",
                object_schema(&[], &[]),
                "forge.interactive.readiness.v1",
                &["forge", "interactive", "readiness", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.operational_cockpit",
                "Inspect Interactive Operational Cockpit",
                "Return the dedicated Forge operational cockpit for attention, handoffs, human waits, brain readiness, cost and observability without loading the full home dashboard.",
                object_schema(&[], &[]),
                "forge.interactive.operational_cockpit.v1",
                &[
                    "forge",
                    "interactive",
                    "operational-cockpit",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.improvement_loop",
                "Inspect Interactive Improvement Loop",
                "Return the Forge improvement loop panel with self-improvement candidates, log, cost, validation, context quality and outcome evidence before governed mutations.",
                object_schema(&[], &[]),
                "forge.interactive.improvement_loop.v1",
                &[
                    "forge",
                    "interactive",
                    "improvement-loop",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.architecture",
                "Inspect Interactive Architecture Compass",
                "Return the Forge architecture compass with source-of-truth tracks, project operating context, implementation evidence, open gaps, dependencies, reuse opportunities and benchmark boundaries.",
                object_schema(&[
                    ("project_root", "string", "optional project root whose .forge operating context should guide the architecture compass"),
                ], &[]),
                "forge.interactive.architecture_compass.v1",
                &[
                    "forge",
                    "interactive",
                    "architecture",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.release_gates",
                "Inspect Interactive Release Gates",
                "Return the Forge interactive release-gates panel for one milestone, including promotion decision, blocked capabilities, required evidence, current evidence, missing project manifests, secret-free manifest templates and next commands without mutating state.",
                object_schema(&[
                    ("version", "string", "milestone version, currently 0.5"),
                    ("project_root", "string", "optional project root whose .forge manifests should be inspected for evidence planning"),
                ], &[]),
                "forge.interactive.release_gates.v1",
                &[
                    "forge",
                    "interactive",
                    "release-gates",
                    "--version",
                    "0.5",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.harness",
                "Inspect Interactive Harness Center",
                "Return the Forge interactive harness center for one brain CLI, combining mode, doctor, shim status, wrap-plan, headroom plan, session lifecycle plan and token-headroom preview without installing shims or launching child processes.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("shim_dir", "string", "directory where Forge-owned shims should live"),
                    ("forge_first", "boolean", "simulate an explicit Forge-first CLI flag"),
                    ("observe_only", "boolean", "simulate an observe-only CLI override"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("task_id", "string", "optional task/node lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom readiness"),
                ], &[]),
                "forge.interactive.harness.v1",
                &["forge", "interactive", "harness", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.sessions",
                "Inspect Interactive Session Center",
                "Return the Forge interactive session center with provider/session readiness, lifecycle state, operation plans, shell history commands and next lifecycle controls without opening or attaching shells.",
                object_schema(&[
                    ("provider_id", "string", "optional provider filter such as codex, opencode, gemini or claude"),
                    ("lifecycle_state", "string", "optional lifecycle filter such as opened, attached, closed, failed or abandoned"),
                    ("readiness", "string", "optional readiness filter such as ready, native_cli_available or needs_sync_or_authorization"),
                ], &[]),
                "forge.interactive.sessions.v1",
                &["forge", "interactive", "sessions", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.command_palette",
                "Inspect Interactive Command Palette",
                "Return contextual Forge operator commands grouped by panel, including workflow, patch, Addon, permission, harness and observability actions without mutating state.",
                object_schema(&[
                    ("query", "string", "optional search query used to filter non-workflow command entries"),
                    ("project_root", "string", "optional project root used to load project-scoped .forge/addons and addons directories"),
                ], &[]),
                "forge.interactive.command_palette.v1",
                &["forge", "interactive", "command-palette", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.action_registry",
                "Inspect Interactive Action Registry",
                "Return a read-only action registry derived from the command palette for TUI, web and agent clients, including readiness counts, Addon lineage and operation plans without mutating state.",
                object_schema(&[
                    ("query", "string", "optional search query used to filter non-workflow actions"),
                    ("project_root", "string", "optional project root used to load project-scoped .forge/addons and addons directories"),
                ], &[]),
                "forge.interactive.action_registry.v1",
                &["forge", "interactive", "action-registry", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.action_invocation",
                "Plan Interactive Action Invocation",
                "Resolve one action id from the interactive action registry into a ready command or diagnostic-only operation plan without executing it.",
                object_schema(&[
                    ("action_id", "string", "action id to resolve, such as patch.diff"),
                    ("action", "string", "alias for action_id"),
                    ("project_root", "string", "optional project root used to resolve project-scoped Addon actions"),
                ], &[]),
                "forge.interactive.action_invocation.v1",
                &["forge", "interactive", "action-invocation", "--action", "<action-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.action_dispatch",
                "Dispatch Interactive Action Hooks",
                "Resolve one action id from the interactive action registry, start Forge workflow hooks through the runtime and route CLI brain hooks through Forge harness plans without executing external CLIs directly.",
                object_schema(&[
                    ("action_id", "string", "action id to dispatch, such as crm.lead.add_tag"),
                    ("action", "string", "alias for action_id"),
                    ("project_root", "string", "optional project root used to resolve project-scoped Addon actions"),
                    ("origin", "string", "dispatch origin; defaults to mcp"),
                    ("payload", "object", "structured hook payload; payload.goal becomes the workflow goal when present; requested_hook_id/requested_hook_ids and requested_hook_tag/requested_hook_tags narrow dispatch to matching hooks"),
                ], &[]),
                "forge.interactive.action_dispatch.v1",
                &["forge", "interactive", "action-dispatch", "--action", "<action-id>", "--payload", "<json>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.interactive.autocomplete",
                "Inspect Interactive Autocomplete",
                "Return read-only slash-command and command-palette suggestions for a partial operator input without launching a TTY.",
                object_schema(&[
                    ("input", "string", "partial operator input such as /patch r or patch"),
                    ("project_root", "string", "optional project root used to suggest project-scoped Addon actions"),
                ], &["input"]),
                "forge.interactive.autocomplete.v1",
                &["forge", "interactive", "autocomplete", "--input", "<input>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.patch_workbench",
                "Inspect Interactive Patch Workbench",
                "Return the Forge interactive file-editing and diff-review workbench with Git status, inline diff preview, multi-file review queue, per-file action hints, edit intake required inputs, an ordered operation plan, diff checks, approval-flow gates and permission-gated patch lifecycle commands without mutating files.",
                object_schema(&[], &[]),
                "forge.interactive.patch_workbench.v1",
                &["forge", "interactive", "patch-workbench", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.permissions",
                "Inspect Interactive Permissions",
                "Return the Forge interactive permission center with tenant memberships, Addon permission authorizations and pending human approvals without mutating state.",
                object_schema(&[], &[]),
                "forge.interactive.permissions.v1",
                &["forge", "interactive", "permissions", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.addon_capabilities",
                "Inspect Interactive Addon Capabilities",
                "Return the Forge interactive Addon capabilities surface with Addon lifecycle counts, capability registry totals, permission gates, runtime contracts, TUI views and dispatch state without mutating state.",
                object_schema(&[
                    ("project_root", "string", "optional project root used to load project-scoped .forge/addons"),
                ], &[]),
                "forge.interactive.addon_capability.v1",
                &["forge", "interactive", "addon-capabilities", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.core_boundary",
                "Inspect Core Boundary",
                "Return the Forge Core boundary audit proving universal Core responsibilities, Addon-owned domain capabilities, compatibility executors and goal3 acceptance gates without mutating state.",
                object_schema(&[
                    ("project_root", "string", "optional project root used to load project-scoped .forge/addons"),
                ], &[]),
                "forge.interactive.core_boundary.v1",
                &["forge", "interactive", "core-boundary", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.identity",
                "Inspect Interactive Identity",
                "Return the Forge interactive identity center with operating context, registry records, channel aliases, memberships and tenant audit without mutating state.",
                object_schema(&[
                    ("project_root", "string", "optional project root used to load .forge/operating-context"),
                ], &[]),
                "forge.interactive.identity.v1",
                &["forge", "interactive", "identity", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.operating_context",
                "Inspect Interactive Operating Context",
                "Return the unified Forge operating-context panel with tenant identity, memory policy, personality routing, brand/design context, prompt-packet gates, company-work checklist and handoff readiness without mutating state.",
                object_schema(&[
                    ("project_root", "string", "optional project root used to load .forge/operating-context and memory governance"),
                ], &[]),
                "forge.interactive.operating_context.v1",
                &[
                    "forge",
                    "interactive",
                    "operating-context",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.task_board",
                "Inspect Interactive Task Board",
                "Return the Forge interactive task-board lanes with ready handoffs, checkpoint resume candidates, pending human waits, artifacts and next actions without launching a TTY.",
                object_schema(&[], &[]),
                "forge.interactive.task_board.v1",
                &["forge", "interactive", "task-board", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.workflow_mutation",
                "Inspect Interactive Workflow Mutation Planner",
                "Return the Forge workflow mutation and replanning surface with DAG, task-board, modifier lane, handoffs, costs, proposals and safe mutation commands without applying mutations.",
                object_schema(&[], &[]),
                "forge.interactive.workflow_mutation.v1",
                &[
                    "forge",
                    "interactive",
                    "workflow-mutation",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.replacement_cli",
                "Inspect Replacement CLI Readiness",
                "Return the Forge replacement-grade CLI readiness panel across TUI home, workflow operations, patch UX, action discovery, harness/session controls, observability, approvals and milestone evidence without launching a TTY or mutating state; project_root lets agents inspect project-scoped harness and release-gate state without relying on cwd.",
                object_schema(&[
                    ("project_root", "string", "optional project root for project-scoped replacement CLI panels"),
                ], &[]),
                "forge.interactive.replacement_cli.v1",
                &[
                    "forge",
                    "interactive",
                    "replacement-cli",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.multimodal_runtime",
                "Inspect Multimodal Runtime Readiness",
                "Return the Addon-owned multimodal runtime readiness panel with feature-flag state, guard result, benchmark templates, demo plans and production evidence blockers without installing models, executing models, accessing devices or mutating workflows.",
                object_schema(&[
                    ("project_root", "string", "optional project root used to inspect .forge/multimodal.json and .forge/multimodal-runtimes.json"),
                    ("enable_experimental", "boolean", "optional explicit experimental feature flag for inspection only"),
                ], &[]),
                "forge.interactive.multimodal_runtime.v1",
                &[
                    "forge",
                    "interactive",
                    "multimodal-runtime",
                    "--project-root",
                    ".",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.workflow_sidebar",
                "Inspect Interactive Workflow Sidebar",
                "Return the Forge interactive workflow sidebar with grouped workflow navigation, selected workflow state, runtime/schedule summaries and drill-down commands without launching a TTY.",
                object_schema(&[], &[]),
                "forge.interactive.workflow_sidebar.v1",
                &["forge", "interactive", "workflow-sidebar", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.artifacts",
                "Inspect Interactive Artifacts",
                "Return the Forge interactive artifact evidence panel with artifact-bearing workflows, persistent paths, checksums, byte counts, lineage summaries and drill-down commands without launching a TTY.",
                object_schema(&[], &[]),
                "forge.interactive.artifacts.v1",
                &["forge", "interactive", "artifacts", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.token_usage",
                "Inspect Interactive Token Usage",
                "Return the Forge interactive token/headroom usage panel with persisted headroom receipts, saved-token totals, context-compression buckets, retrieval commands and harness drill-downs without launching a TTY.",
                object_schema(&[], &[]),
                "forge.interactive.token_usage.v1",
                &["forge", "interactive", "token-usage", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.workflow_dag",
                "Inspect Interactive Workflow DAG",
                "Return the Forge interactive workflow DAG with dependency nodes, edges, readiness, human waits and drill-down commands without launching a TTY.",
                object_schema(&[], &[]),
                "forge.interactive.workflow_dag.v1",
                &["forge", "interactive", "workflow-dag", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.schedules",
                "Inspect Interactive Schedules",
                "Return Forge interactive schedules with due workflows, scheduler worker capacity, deterministic assignment queues, sleep/backpressure/cancellation state and observed scheduled workflow rows without mutating state.",
                object_schema(&[], &[]),
                "forge.interactive.schedules.v1",
                &["forge", "interactive", "schedules", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.context_memory",
                "Inspect Interactive Context Memory",
                "Return Forge interactive context and memory governance with handoff readiness, context routing quality, project memory policy, governed memory/context commands and next actions without mutating state.",
                object_schema(&[
                    ("project_root", "string", "optional project root containing .forge/memory-governance.json"),
                ], &[]),
                "forge.interactive.context_memory.v1",
                &[
                    "forge",
                    "interactive",
                    "context-memory",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.structured_logs",
                "Inspect Interactive Structured Logs",
                "Return recent Forge structured event logs with sequence, workflow, category, severity, origin, correlation, observability and payload preview without launching a TTY.",
                object_schema(&[], &[]),
                "forge.interactive.structured_logs.v1",
                &["forge", "interactive", "structured-logs", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.brain_router",
                "Inspect Brain Router",
                "Return Forge-owned execution-brain routing boundaries so agents know Codex/OpenCode/Gemini/Claude are replaceable execution brains, while Forge controls memory, skills, MCP routing, context, shells, permissions and validation.",
                object_schema(&[], &[]),
                "forge.brain_router.v1",
                &["forge", "brains", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.sessions",
                "Inspect Brain Sessions",
                "Return Forge-owned provider/session management state for execution brains, shell specs, operation plans and recorded shell launch events without starting child processes, optionally filtered by provider, lifecycle state or readiness.",
                object_schema(
                    &[
                        ("provider_id", "string", "optional provider id such as codex, opencode, gemini, claude or forge"),
                        ("lifecycle_state", "string", "optional lifecycle state filter such as untracked, opened, attached, detached, closed, failed or abandoned"),
                        ("readiness", "string", "optional readiness filter such as ready, native_cli_available or needs_sync_or_authorization"),
                    ],
                    &[],
                ),
                "forge.brain_sessions.v1",
                &["forge", "sessions", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.session.history",
                "Inspect Brain Session History",
                "Return the Forge-owned chronological audit history for one brain shell session, including planned shell events, lifecycle transitions, current lifecycle policy and next commands without starting child processes.",
                object_schema(
                    &[("session_id", "string", "required shell session id such as codex-shell")],
                    &["session_id"],
                ),
                "forge.brain_session_history.v1",
                &["forge", "sessions", "history", "--session", "<session-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.session.lifecycle",
                "Record Brain Session Lifecycle",
                "Record an auditable Forge-owned lifecycle state for a known brain shell session without starting child processes, enforcing ordered transition policy and returning previous state, sequence and next lifecycle commands.",
                object_schema(
                    &[
                        ("session_id", "string", "required shell session id such as codex-shell"),
                        ("state", "string", "required lifecycle state: opened, attached, detached, closed, failed or abandoned"),
                        ("workflow_id", "string", "optional workflow lineage"),
                        ("task_id", "string", "optional task lineage"),
                        ("run_id", "string", "optional run lineage"),
                        ("origin", "string", "optional event origin"),
                        ("note", "string", "optional operator note"),
                    ],
                    &["session_id", "state"],
                ),
                "forge.brain_session_lifecycle.v1",
                &["forge", "sessions", "lifecycle", "--session", "<session-id>", "--state", "opened", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.shell.launch_plan",
                "Plan Brain Shell Launch",
                "Return a plan-only Forge-controlled shell launch report for one execution brain or all shell sessions, including preflight checks and handoff safety gates without starting a child process.",
                object_schema(
                    &[
                        ("executor", "string", "optional execution brain id such as codex|opencode|gemini|claude"),
                        ("brain", "string", "optional alias for executor"),
                        ("workflow_id", "string", "optional workflow id used to build concrete context and handoff commands"),
                        ("task_id", "string", "optional task id used with workflow_id for context and handoff commands"),
                        ("run_id", "string", "optional run id used to build a heartbeat command"),
                        ("context_budget", "integer", "optional context budget for context and handoff commands"),
                        ("ttl_seconds", "integer", "optional handoff lease and heartbeat TTL"),
                    ],
                    &[],
                ),
                "forge.shell_launch_plan.v1",
                &["forge", "shells", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.shell.record_plan",
                "Record Brain Shell Launch Plan",
                "Record a Forge-controlled shell launch plan in the global event ledger without starting a child process.",
                object_schema(
                    &[
                        ("executor", "string", "optional execution brain id such as codex|opencode|gemini|claude"),
                        ("brain", "string", "optional alias for executor"),
                        ("workflow_id", "string", "optional workflow id used to build concrete context and handoff commands"),
                        ("task_id", "string", "optional task id used with workflow_id for context and handoff commands"),
                        ("run_id", "string", "optional run id used to build a heartbeat command"),
                        ("context_budget", "integer", "optional context budget for context and handoff commands"),
                        ("ttl_seconds", "integer", "optional handoff lease and heartbeat TTL"),
                        ("origin", "string", "optional audit origin for the recorded event"),
                    ],
                    &[],
                ),
                "forge.shell_session_receipt.v1",
                &["forge", "shells", "--record-session", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.addons.installed",
                "List Installed Addons",
                "List Addons persisted in Forge's SQLite Addon lifecycle registry.",
                object_schema(&[], &[]),
                "forge.installed_addons.v1",
                &["forge", "addons", "installed", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.capabilities",
                "Inspect Addon Capability Index",
                "Return the SQLite-materialized capability index for installed Addons, with optional addon, capability and lifecycle filters.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional addon id filter"),
                        ("capability_id", "string", "optional capability id filter"),
                        ("lifecycle", "string", "optional lifecycle filter such as enabled or disabled"),
                    ],
                    &[],
                ),
                "forge.addon_capability_index.v1",
                &["forge", "addons", "capabilities", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.observability",
                "Inspect Addon Observability",
                "Return a consolidated operational view of Addons, capabilities, dependencies, resource permissions, event flows, UI/views, runtime contracts and dispatch usage.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional addon id filter"),
                        (
                            "lifecycle",
                            "string",
                            "optional lifecycle filter such as enabled, disabled or unauthorized",
                        ),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                        (
                            "dispatch_limit",
                            "integer",
                            "maximum dispatch rows to inspect per Addon",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_observability.v1",
                &["forge", "addons", "observability", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.permissions",
                "List Addon Permission Authorizations",
                "List human approval records for Addon permissions that gate installed Addon enablement and capability exposure.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("permission_id", "string", "optional permission id filter"),
                        ("status", "string", "optional approved|revoked status filter"),
                    ],
                    &[],
                ),
                "forge.addon_permission_authorizations.v1",
                &["forge", "addons", "permissions", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.authorize_permission",
                "Authorize Addon Permission",
                "Persist a human approval for an Addon permission before enabling high-risk or externally mutating Addons.",
                object_schema(
                    &[
                        ("addon_id", "string", "Addon id"),
                        ("permission_id", "string", "permission id"),
                        ("risk", "string", "optional permission risk"),
                        ("approved_by", "string", "optional approving human or policy id"),
                        ("source", "string", "optional approval source"),
                    ],
                    &["addon_id", "permission_id"],
                ),
                "forge.addon_permission_authorizations.v1",
                &[
                    "forge",
                    "addons",
                    "authorize-permission",
                    "--addon",
                    "<addon-id>",
                    "--permission",
                    "<permission-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.revoke_permission",
                "Revoke Addon Permission",
                "Revoke a previously approved Addon permission and remove its capabilities from the enabled capability index on the next sync.",
                object_schema(
                    &[
                        ("addon_id", "string", "Addon id"),
                        ("permission_id", "string", "permission id"),
                        ("approved_by", "string", "optional revoking human or policy id"),
                        ("source", "string", "optional revocation source"),
                    ],
                    &["addon_id", "permission_id"],
                ),
                "forge.addon_permission_authorizations.v1",
                &[
                    "forge",
                    "addons",
                    "revoke-permission",
                    "--addon",
                    "<addon-id>",
                    "--permission",
                    "<permission-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.catalog",
                "Inspect Addon Catalog",
                "Return the Forge Core + Addons catalog, including universal core capabilities, project manifests and SQLite-installed addon manifests.",
                object_schema(
                    &[(
                        "addon_dirs",
                        "array",
                        "optional addon manifest directories; defaults to .forge/addons",
                    )],
                    &[],
                ),
                "forge.addon_catalog.v1",
                &["forge", "addons", "catalog", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.resolve",
                "Resolve Goal Capabilities",
                "Resolve a goal into required Forge capabilities, active Addons, workflow extensions and missing capability dependencies before planning.",
                object_schema(
                    &[
                        ("goal", "string", "goal text to resolve"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                        (
                            "registry_source",
                            "string",
                            "optional registry index path/URL to sync before resolving",
                        ),
                        (
                            "registry_sources",
                            "array",
                            "optional registry index paths/URLs to sync before resolving",
                        ),
                        (
                            "registry_cache_dir",
                            "string",
                            "optional package cache directory for registry sync",
                        ),
                        (
                            "allow_remote_registry",
                            "boolean",
                            "required true for HTTP(S) registry sources",
                        ),
                        (
                            "registry_max_bytes",
                            "integer",
                            "maximum registry/package bytes",
                        ),
                        (
                            "registry_max_packages",
                            "integer",
                            "maximum packages per registry",
                        ),
                        (
                            "registry_lock",
                            "string",
                            "optional package lock enforced during registry sync",
                        ),
                        ("registry_lock_path", "string", "alias for registry_lock"),
                    ],
                    &["goal"],
                ),
                "forge.capability_resolution.v1",
                &[
                    "forge",
                    "addons",
                    "resolve",
                    "--goal",
                    "<goal>",
                    "--registry-source",
                    "<registry-index>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.contracts",
                "List Addon Runtime Contracts",
                "Return declarative planning, replanning, validator, executor and handoff contracts registered by Addons.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        (
                            "contract_type",
                            "string",
                            "planning_strategy|replanning_strategy|validator|executor|handoff",
                        ),
                        ("capability_id", "string", "optional capability id filter"),
                        ("lifecycle", "string", "optional Addon lifecycle filter"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_runtime_contracts.v1",
                &[
                    "forge",
                    "addons",
                    "contracts",
                    "--type",
                    "<contract-type>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.planners",
                "List Addon Planner Registrations",
                "Return planning and replanning strategy registrations declared by Addons, including first-party builders and external runtime contracts.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("capability_id", "string", "optional capability id filter"),
                        (
                            "workflow_extension_id",
                            "string",
                            "optional workflow extension id filter",
                        ),
                        ("lifecycle", "string", "optional Addon lifecycle filter"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_planner_registry.v1",
                &["forge", "addons", "planners", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.contract_policy",
                "Evaluate Addon Runtime Contract Policy",
                "Evaluate whether Addon runtime contracts are ready for safe dispatch, including lifecycle, runtime, entrypoint and permission gate checks.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "optional runtime contract id filter"),
                        (
                            "contract_type",
                            "string",
                            "planning_strategy|replanning_strategy|validator|executor|handoff",
                        ),
                        ("capability_id", "string", "optional capability id filter"),
                        ("lifecycle", "string", "optional Addon lifecycle filter"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_runtime_contract_policy.v1",
                &[
                    "forge",
                    "addons",
                    "contract-policy",
                    "--contract",
                    "<contract-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.dispatch_contract",
                "Queue Addon Runtime Contract Dispatch",
                "Validate an Addon runtime contract policy and enqueue an auditable dispatch request for an external runtime worker.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "runtime contract id"),
                        ("input", "object", "dispatch input payload"),
                        ("source", "string", "dispatch source"),
                        ("dry_run", "boolean", "evaluate without persisting dispatch"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["contract_id"],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "dispatch-contract",
                    "--contract",
                    "<contract-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.dispatch_planner",
                "Queue Addon Planner Dispatch",
                "Validate a planning_strategy or replanning_strategy contract and enqueue a standardized planner request for an external runtime worker.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "planner runtime contract id"),
                        ("goal", "string", "workflow goal to plan or replan"),
                        ("constraints", "array", "optional planning constraints"),
                        ("workflow_id", "string", "optional workflow id for replanning"),
                        ("task_id", "string", "optional task id for replanning"),
                        ("context", "object", "optional planner context payload"),
                        ("source", "string", "dispatch source"),
                        ("dry_run", "boolean", "evaluate without persisting dispatch"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["contract_id", "goal"],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "dispatch-planner",
                    "--contract",
                    "<contract-id>",
                    "--goal",
                    "<goal>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.execute_planner",
                "Execute Addon Planner With Equivalence Audit",
                "Dispatch a planning_strategy or replanning_strategy to a registered worker, validate the returned task graph and compare it with Forge Core's reference plan before any promotion.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "planner runtime contract id"),
                        ("worker_id", "string", "registered runtime worker id"),
                        ("goal", "string", "workflow goal to plan or replan"),
                        ("constraints", "array", "optional planning constraints"),
                        ("workflow_id", "string", "optional workflow id for replanning"),
                        ("task_id", "string", "optional task id for replanning"),
                        ("context", "object", "optional planner context payload"),
                        ("lease_seconds", "integer", "external worker claim lease"),
                        ("source", "string", "dispatch source"),
                        ("dry_run", "boolean", "evaluate without executing worker"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["contract_id", "worker_id", "goal"],
                ),
                "forge.addon_planning_strategy_execution.v1",
                &[
                    "forge",
                    "addons",
                    "execute-planner",
                    "--contract",
                    "<contract-id>",
                    "--worker",
                    "<worker-id>",
                    "--goal",
                    "<goal>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.execute_validator",
                "Execute Addon Validator With Result Audit",
                "Dispatch a validator runtime contract to a registered worker, validate the returned decision envelope and record the normal dispatch/claim/completion audit. Supply workflow_id and task_id together to bind the result for later explicit application.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "validator runtime contract id"),
                        ("worker_id", "string", "registered runtime worker id"),
                        ("subject", "string", "entity or artifact being validated"),
                        ("workflow_id", "string", "optional workflow binding; requires task_id"),
                        ("task_id", "string", "optional task binding; requires workflow_id"),
                        ("input", "object", "validator input payload"),
                        ("context", "object", "optional validator context payload"),
                        ("lease_seconds", "integer", "external worker claim lease"),
                        ("source", "string", "dispatch source"),
                        ("dry_run", "boolean", "evaluate without executing worker"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["contract_id", "worker_id", "subject"],
                ),
                "forge.addon_validator_execution.v1",
                &[
                    "forge",
                    "addons",
                    "execute-validator",
                    "--contract",
                    "<contract-id>",
                    "--worker",
                    "<worker-id>",
                    "--subject",
                    "<subject>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.apply_validator_outcome",
                "Apply Addon Validator Outcome",
                "Explicitly apply one completed, workflow-bound validator result. Passed records revisioned evidence without promotion; failed creates correlated rework feedback; review_required creates a blocking human interaction. Replays are idempotent.",
                object_schema(
                    &[
                        ("dispatch_id", "string", "completed validator dispatch id"),
                        ("workflow_id", "string", "bound workflow id"),
                        ("task_id", "string", "bound workflow task id"),
                        (
                            "expected_revision",
                            "integer",
                            "required optimistic concurrency revision",
                        ),
                        ("origin", "string", "application origin"),
                    ],
                    &[
                        "dispatch_id",
                        "workflow_id",
                        "task_id",
                        "expected_revision",
                    ],
                ),
                "forge.addon_validator_outcome_application.v1",
                &[
                    "forge",
                    "addons",
                    "apply-validator-outcome",
                    "--dispatch",
                    "<dispatch-id>",
                    "--workflow",
                    "<workflow-id>",
                    "--task",
                    "<task-id>",
                    "--expected-revision",
                    "<revision>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.execute_executor",
                "Execute Addon Executor With Result Audit",
                "Dispatch an executor runtime contract to a registered worker, validate the returned generic execution result and record the normal dispatch/claim/completion audit.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "executor runtime contract id"),
                        ("worker_id", "string", "registered runtime worker id"),
                        ("task_ref", "string", "workflow task or operation reference"),
                        (
                            "workflow_id",
                            "string",
                            "optional workflow id for promoting returned artifacts and events into workflow state",
                        ),
                        ("input", "object", "executor input payload"),
                        ("context", "object", "optional executor context payload"),
                        ("lease_seconds", "integer", "external worker claim lease"),
                        ("source", "string", "dispatch source"),
                        ("dry_run", "boolean", "evaluate without executing worker"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["contract_id", "worker_id", "task_ref"],
                ),
                "forge.addon_executor_execution.v1",
                &[
                    "forge",
                    "addons",
                    "execute-executor",
                    "--contract",
                    "<contract-id>",
                    "--worker",
                    "<worker-id>",
                    "--task",
                    "<task-ref>",
                    "--workflow",
                    "<workflow-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.execute_handoff",
                "Execute Addon Handoff With Result Audit",
                "Dispatch a handoff runtime contract to a registered worker, validate the returned target, receipt, artifacts and events, and record the normal dispatch/claim/completion audit.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "handoff runtime contract id"),
                        ("worker_id", "string", "registered runtime worker id"),
                        ("handoff_ref", "string", "handoff or external delivery reference"),
                        ("input", "object", "handoff input payload"),
                        ("context", "object", "optional handoff context payload"),
                        ("lease_seconds", "integer", "external worker claim lease"),
                        ("source", "string", "dispatch source"),
                        ("dry_run", "boolean", "evaluate without executing worker"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["contract_id", "worker_id", "handoff_ref"],
                ),
                "forge.addon_handoff_execution.v1",
                &[
                    "forge",
                    "addons",
                    "execute-handoff",
                    "--contract",
                    "<contract-id>",
                    "--worker",
                    "<worker-id>",
                    "--handoff",
                    "<handoff-ref>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.dispatches",
                "List Addon Runtime Contract Dispatches",
                "List queued or blocked Addon runtime contract dispatch requests from the persistent dispatch ledger.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("contract_id", "string", "optional runtime contract id filter"),
                        ("status", "string", "optional dispatch status filter"),
                        ("limit", "integer", "maximum dispatch rows"),
                    ],
                    &[],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "dispatches",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.run_dispatch",
                "Run Addon Runtime Contract Dispatch",
                "Process one queued Addon runtime dispatch with policy recheck; Forge Core only runs safe built-in runtimes and marks external runtimes for specialized workers.",
                object_schema(
                    &[
                        ("dispatch_id", "string", "runtime dispatch id"),
                        ("worker", "string", "worker identity writing the processing evidence"),
                        ("dry_run", "boolean", "inspect processing decision without updating the ledger"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["dispatch_id"],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "run-dispatch",
                    "--dispatch",
                    "<dispatch-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.dispatch_worker",
                "Run Addon Dispatch Worker",
                "Process a bounded batch of queued Addon runtime dispatches with the same policy recheck and runtime boundaries as run_dispatch.",
                object_schema(
                    &[
                        ("status", "string", "dispatch status to process; defaults to queued"),
                        ("limit", "integer", "maximum dispatch rows to process"),
                        ("worker", "string", "worker identity writing the processing evidence"),
                        ("worker_id", "string", "worker identity alias"),
                        ("dry_run", "boolean", "inspect processing decisions without updating the ledger"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "dispatch-worker",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.execute_dispatch",
                "Execute Addon Dispatch With Worker",
                "Run a registered local_process or external_api worker for one Addon runtime dispatch, using the claim/completion ledger and the same policy and signature gates as external workers.",
                object_schema(
                    &[
                        ("dispatch_id", "string", "runtime dispatch id"),
                        ("worker_id", "string", "registered runtime worker id"),
                        ("lease_seconds", "integer", "claim lease duration in seconds"),
                        ("dry_run", "boolean", "inspect execution decision without claiming or running"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["dispatch_id", "worker_id"],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "execute-dispatch",
                    "--dispatch",
                    "<dispatch-id>",
                    "--worker",
                    "<worker-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.claim_dispatch",
                "Claim Addon Runtime Dispatch",
                "Let a registered external runtime worker claim a dispatch that Forge Core marked as needing an external worker, after rechecking current runtime policy.",
                object_schema(
                    &[
                        ("dispatch_id", "string", "runtime dispatch id"),
                        ("worker_id", "string", "registered runtime worker id"),
                        ("lease_seconds", "integer", "claim lease duration in seconds"),
                        ("dry_run", "boolean", "inspect claim decision without updating the ledger"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["dispatch_id", "worker_id"],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "claim-dispatch",
                    "--dispatch",
                    "<dispatch-id>",
                    "--worker",
                    "<worker-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.complete_dispatch",
                "Complete Addon Runtime Dispatch",
                "Record a claimed external runtime dispatch result with worker ownership, current policy recheck, result hash and signature/attestation evidence.",
                object_schema(
                    &[
                        ("dispatch_id", "string", "runtime dispatch id"),
                        ("worker_id", "string", "registered runtime worker id"),
                        ("status", "string", "completed|failed"),
                        ("result", "object", "runtime result payload"),
                        ("signature", "string", "worker signature or attestation signature"),
                        ("attestation", "object", "worker attestation metadata"),
                        ("dry_run", "boolean", "inspect completion decision without updating the ledger"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &["dispatch_id", "worker_id"],
                ),
                "forge.addon_runtime_contract_dispatch.v1",
                &[
                    "forge",
                    "addons",
                    "complete-dispatch",
                    "--dispatch",
                    "<dispatch-id>",
                    "--worker",
                    "<worker-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.register_worker",
                "Register Addon Runtime Worker",
                "Register or update an external runtime worker that can consume Addon runtime dispatches for a declared runtime such as wasm or external_api.",
                object_schema(
                    &[
                        ("worker_id", "string", "runtime worker id"),
                        ("runtime", "string", "runtime handled by the worker"),
                        ("status", "string", "available|draining|disabled"),
                        ("trust_level", "string", "local|signed|trusted"),
                        ("source", "string", "registration source"),
                        ("data", "object", "worker endpoint, signer or metadata"),
                        (
                            "rotation_approved_by",
                            "string",
                            "operator approving signed/trusted worker identity rotation",
                        ),
                        (
                            "rotation_reason",
                            "string",
                            "reason recorded when rotating signed/trusted worker identity",
                        ),
                    ],
                    &["worker_id", "runtime"],
                ),
                "forge.addon_runtime_workers.v1",
                &[
                    "forge",
                    "addons",
                    "register-worker",
                    "--worker",
                    "<worker-id>",
                    "--runtime",
                    "wasm",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.workers",
                "List Addon Runtime Workers",
                "List registered external runtime workers by runtime, status or trust level.",
                object_schema(
                    &[
                        ("runtime", "string", "optional runtime filter"),
                        ("status", "string", "optional status filter"),
                        ("trust_level", "string", "optional trust filter"),
                        ("limit", "integer", "maximum worker rows"),
                    ],
                    &[],
                ),
                "forge.addon_runtime_workers.v1",
                &[
                    "forge",
                    "addons",
                    "workers",
                    "--runtime",
                    "wasm",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.views",
                "List Addon Views",
                "Return UI/TUI/ops-console views declared by Addons for dynamic interface composition.",
                object_schema(
                    &[
                        ("addon_id", "string", "optional Addon id filter"),
                        ("surface", "string", "optional view surface filter"),
                        ("lifecycle", "string", "optional Addon lifecycle filter"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories; defaults to .forge/addons",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_views.v1",
                &[
                    "forge",
                    "addons",
                    "views",
                    "--surface",
                    "<surface>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.validate",
                "Validate Addon Catalog",
                "Validate addon ids, capability ids, required addon dependencies, required capabilities and high-risk permission gates.",
                object_schema(
                    &[(
                        "addon_dirs",
                        "array",
                        "optional addon manifest directories; defaults to .forge/addons",
                    )],
                    &[],
                ),
                "forge.addon_validation.v1",
                &["forge", "addons", "validate", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.lifecycle_plan",
                "Preview Addon Lifecycle Operation",
                "Dry-run an Addon lifecycle operation before applying it, returning validation, impact, rollback, approval gates and exact apply command without mutating the installed registry.",
                object_schema(
                    &[
                        ("action", "string", "install|install_package|upgrade|downgrade|enable|disable|uninstall"),
                        ("id", "string", "addon id for enable, disable or uninstall"),
                        ("addon_id", "string", "alias for id"),
                        ("manifest", "string", "addon manifest path for install, upgrade or downgrade"),
                        ("package", "string", "Addon package JSON path for install_package"),
                        ("package_path", "string", "alias for package"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories used for dependency validation",
                        ),
                    ],
                    &["action"],
                ),
                "forge.addon_lifecycle_plan.v1",
                &[
                    "forge",
                    "addons",
                    "lifecycle-plan",
                    "--action",
                    "upgrade",
                    "--manifest",
                    "<manifest>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.install",
                "Install Addon",
                "Install or update an Addon manifest into Forge's persistent SQLite lifecycle registry after catalog validation.",
                object_schema(
                    &[
                        ("manifest", "string", "path to addon manifest"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories used for dependency validation",
                        ),
                    ],
                    &["manifest"],
                ),
                "forge.addon_lifecycle.v1",
                &[
                    "forge",
                    "addons",
                    "install",
                    "--manifest",
                    "<manifest>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.package",
                "Package Addon",
                "Create a deterministic Addon package report for marketplace distribution, including manifest hash, capability catalog, dependency summary and detached signature metadata.",
                object_schema(
                    &[
                        ("manifest", "string", "path to addon manifest"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories used for dependency validation",
                        ),
                        ("repository", "string", "optional source repository URL or id"),
                        ("channel", "string", "optional release channel, defaults to stable"),
                        ("signature", "string", "optional detached signature"),
                        ("public_key", "string", "optional detached signature public key"),
                        ("package_path", "string", "optional path to write package JSON"),
                    ],
                    &["manifest"],
                ),
                "forge.addon_package.v1",
                &[
                    "forge",
                    "addons",
                    "package",
                    "--manifest",
                    "<manifest>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.trust_key",
                "Trust Addon Package Key",
                "Add or refresh a trusted Ed25519 package signing key for one Addon repository and release channel.",
                object_schema(
                    &[
                        ("repository", "string", "Addon package repository URL or id"),
                        ("channel", "string", "release channel, defaults to stable"),
                        ("public_key", "string", "trusted Ed25519 public key hex"),
                        ("trust_level", "string", "trusted|signed|operator"),
                        ("approved_by", "string", "operator approving this trust key"),
                        ("source", "string", "approval source"),
                        ("data", "object", "approval metadata"),
                    ],
                    &["repository", "public_key"],
                ),
                "forge.addon_trust_store.v1",
                &[
                    "forge",
                    "addons",
                    "trust-key",
                    "--repository",
                    "<repo>",
                    "--public-key",
                    "<ed25519-public-key-hex>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.trust_store",
                "List Addon Trust Store",
                "List trusted package signing keys by repository, channel, public key or status.",
                object_schema(
                    &[
                        ("repository", "string", "optional repository filter"),
                        ("channel", "string", "optional release channel filter"),
                        ("public_key", "string", "optional public key filter"),
                        ("status", "string", "optional key status filter"),
                        ("limit", "integer", "maximum key rows"),
                    ],
                    &[],
                ),
                "forge.addon_trust_store.v1",
                &["forge", "addons", "trust-store", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.publish_package",
                "Publish Addon Package",
                "Index a local Addon package JSON into Forge's marketplace registry with current trust policy evidence.",
                object_schema(
                    &[
                        ("package", "string", "path to Addon package JSON"),
                        ("package_path", "string", "alias for package"),
                        ("source", "string", "publication source"),
                    ],
                    &[],
                ),
                "forge.addon_marketplace.v1",
                &[
                    "forge",
                    "addons",
                    "publish-package",
                    "--package",
                    "<package-json>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.fetch_package",
                "Fetch Addon Package",
                "Fetch or copy an Addon package into Forge's local package cache, validate optional SHA-256, and index it through the marketplace trust policy.",
                object_schema(
                    &[
                        ("source", "string", "local path, file:// URI, or HTTP(S) URL"),
                        ("cache_dir", "string", "optional package cache directory"),
                        ("expected_sha256", "string", "optional expected package SHA-256"),
                        ("lock", "string", "optional package lock path to enforce"),
                        ("lock_path", "string", "alias for lock"),
                        ("allow_remote", "boolean", "required true for HTTP(S) sources"),
                        ("max_bytes", "integer", "maximum package size in bytes"),
                    ],
                    &["source"],
                ),
                "forge.addon_package_fetch.v1",
                &[
                    "forge",
                    "addons",
                    "fetch-package",
                    "--source",
                    "<package-source>",
                    "--lock",
                    "<package-lock>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.sync_registry",
                "Sync Addon Registry",
                "Read a package registry index, fetch listed packages into Forge's local cache, and index trusted packages through the marketplace policy.",
                object_schema(
                    &[
                        ("source", "string", "local path, file:// URI, or HTTP(S) registry index URL"),
                        ("cache_dir", "string", "optional package cache directory"),
                        ("lock", "string", "optional package lock path to enforce for every fetched package"),
                        ("lock_path", "string", "alias for lock"),
                        ("allow_remote", "boolean", "required true for HTTP(S) index or package sources"),
                        ("max_bytes", "integer", "maximum index/package size in bytes"),
                        ("max_packages", "integer", "maximum packages to fetch from the index"),
                    ],
                    &["source"],
                ),
                "forge.addon_registry_sync.v1",
                &[
                    "forge",
                    "addons",
                    "sync-registry",
                    "--source",
                    "<registry-index>",
                    "--lock",
                    "<package-lock>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.package_lock",
                "Create Addon Package Lock",
                "Create a reproducible lock snapshot of indexed Addon packages with repository, channel, package hashes, manifest hashes and current trust-policy status.",
                object_schema(
                    &[
                        ("repository", "string", "optional repository filter"),
                        ("channel", "string", "optional release channel filter"),
                        ("addon", "string", "optional Addon id filter"),
                        ("addon_id", "string", "alias for addon"),
                        ("status", "string", "optional package status filter"),
                        ("write", "string", "optional lockfile path to write"),
                        ("write_path", "string", "alias for write"),
                        ("limit", "integer", "maximum package rows"),
                    ],
                    &[],
                ),
                "forge.addon_package_lock.v1",
                &["forge", "addons", "package-lock", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.marketplace",
                "List Addon Marketplace",
                "List indexed Addon marketplace packages with current signature and trust-policy status.",
                object_schema(
                    &[
                        ("repository", "string", "optional repository filter"),
                        ("channel", "string", "optional release channel filter"),
                        ("addon", "string", "optional Addon id filter"),
                        ("addon_id", "string", "alias for addon"),
                        ("status", "string", "optional package status filter"),
                        ("limit", "integer", "maximum package rows"),
                    ],
                    &[],
                ),
                "forge.addon_marketplace.v1",
                &["forge", "addons", "marketplace", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.addons.install_package",
                "Install Trusted Addon Package",
                "Install an Addon package only after manifest hash, detached Ed25519 signature and trust-store policy are verified.",
                object_schema(
                    &[
                        ("package", "string", "path to Addon package JSON"),
                        ("package_path", "string", "alias for package"),
                        ("lock", "string", "optional package lock path to enforce"),
                        ("lock_path", "string", "alias for lock"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories used for dependency validation",
                        ),
                    ],
                    &[],
                ),
                "forge.addon_package_install.v1",
                &[
                    "forge",
                    "addons",
                    "install-package",
                    "--package",
                    "<package-json>",
                    "--lock",
                    "<package-lock>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.migration_workflow",
                "Create Addon Migration Workflow",
                "Create and persist an auditable Forge workflow for an Addon migration or rollback path using the candidate manifest compatibility.migrations contract.",
                object_schema(
                    &[
                        ("from_manifest", "string", "path to currently installed or source Addon manifest"),
                        ("to_manifest", "string", "path to target Addon manifest with compatibility.migrations entry"),
                        ("action", "string", "upgrade|downgrade|install|install_package"),
                        ("origin", "string", "operator or agent creating the workflow"),
                    ],
                    &["from_manifest", "to_manifest"],
                ),
                "forge.addon_migration_workflow.v1",
                &[
                    "forge",
                    "addons",
                    "migration-workflow",
                    "--from-manifest",
                    "<current-manifest>",
                    "--to-manifest",
                    "<target-manifest>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.upgrade",
                "Upgrade Addon",
                "Replace an installed Addon manifest with a higher version after catalog compatibility validation, preserving the existing lifecycle state.",
                object_schema(
                    &[
                        ("manifest", "string", "path to addon manifest"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories used for dependency validation",
                        ),
                    ],
                    &["manifest"],
                ),
                "forge.addon_lifecycle.v1",
                &[
                    "forge",
                    "addons",
                    "upgrade",
                    "--manifest",
                    "<manifest>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.downgrade",
                "Downgrade Addon",
                "Replace an installed Addon manifest with a lower version after catalog compatibility validation, preserving the existing lifecycle state.",
                object_schema(
                    &[
                        ("manifest", "string", "path to addon manifest"),
                        (
                            "addon_dirs",
                            "array",
                            "optional addon manifest directories used for dependency validation",
                        ),
                    ],
                    &["manifest"],
                ),
                "forge.addon_lifecycle.v1",
                &[
                    "forge",
                    "addons",
                    "downgrade",
                    "--manifest",
                    "<manifest>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.enable",
                "Enable Addon",
                "Enable a SQLite-installed Addon and expose its capabilities to planning.",
                object_schema(
                    &[
                        ("id", "string", "addon id"),
                        ("addon_dirs", "array", "optional addon manifest directories"),
                    ],
                    &["id"],
                ),
                "forge.addon_lifecycle.v1",
                &["forge", "addons", "enable", "<addon-id>", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.disable",
                "Disable Addon",
                "Disable a SQLite-installed Addon without removing its manifest from the registry.",
                object_schema(
                    &[
                        ("id", "string", "addon id"),
                        ("addon_dirs", "array", "optional addon manifest directories"),
                    ],
                    &["id"],
                ),
                "forge.addon_lifecycle.v1",
                &[
                    "forge",
                    "addons",
                    "disable",
                    "<addon-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.addons.uninstall",
                "Uninstall Addon",
                "Remove a SQLite-installed Addon from Forge's persistent lifecycle registry.",
                object_schema(
                    &[
                        ("id", "string", "addon id"),
                        ("addon_dirs", "array", "optional addon manifest directories"),
                    ],
                    &["id"],
                ),
                "forge.addon_lifecycle.v1",
                &[
                    "forge",
                    "addons",
                    "uninstall",
                    "<addon-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.identity.context",
                "Inspect Operating Context",
                "Return the project operating context used for organization, brand, product, user, channel, memory scope and personality scope.",
                object_schema(
                    &[(
                        "project_root",
                        "string",
                        "optional project root; defaults to current directory",
                    )],
                    &[],
                ),
                "forge.operating_context_load.v1",
                &[
                    "forge",
                    "identity",
                    "context",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.registry",
                "List Identity Registry",
                "List Forge's persisted organization, brand, product, user and channel identity registry.",
                object_schema(
                    &[
                        ("scope", "string", "optional scope filter"),
                        ("id", "string", "optional identity id filter"),
                    ],
                    &[],
                ),
                "forge.identity_registry.v1",
                &["forge", "identity", "registry", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.memberships",
                "List Identity Memberships",
                "List persisted user-to-organization/brand/product memberships used by Forge's multi-tenant policy gate.",
                object_schema(
                    &[
                        ("subject_scope", "string", "optional subject scope filter, usually user"),
                        ("subject_id", "string", "optional subject id filter"),
                        ("organization_id", "string", "optional organization id filter"),
                        ("brand_id", "string", "optional brand id filter"),
                        ("product_id", "string", "optional product id filter"),
                        ("status", "string", "optional membership status filter"),
                    ],
                    &[],
                ),
                "forge.identity_memberships.v1",
                &["forge", "identity", "memberships", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.membership_update",
                "Update Identity Membership",
                "Update a tenant membership role/status, custom grants/denies and validity window without editing raw data_json.",
                object_schema(
                    &[
                        ("subject_scope", "string", "subject scope, usually user"),
                        ("subject_id", "string", "subject id"),
                        ("organization_id", "string", "organization id"),
                        ("brand_id", "string", "brand id"),
                        ("product_id", "string", "product id"),
                        ("role", "string", "optional role update"),
                        ("status", "string", "optional status update"),
                        ("grant_permissions", "array", "permissions to add to permission_grants"),
                        ("revoke_grants", "array", "permissions to remove from permission_grants"),
                        ("deny_permissions", "array", "permissions to add to permission_denies"),
                        ("remove_denies", "array", "permissions to remove from permission_denies"),
                        ("expires_at", "string", "optional RFC3339 expiry timestamp"),
                        ("clear_expires_at", "boolean", "clear expires_at"),
                        ("not_before", "string", "optional RFC3339 validity start"),
                        ("clear_not_before", "boolean", "clear not_before/valid_from"),
                        ("source", "string", "update origin"),
                    ],
                    &["subject_id", "organization_id", "brand_id", "product_id"],
                ),
                "forge.identity_membership_update.v1",
                &[
                    "forge",
                    "identity",
                    "membership-update",
                    "--subject",
                    "<user>",
                    "--organization",
                    "<org>",
                    "--brand",
                    "<brand>",
                    "--product",
                    "<product>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.identity.link",
                "Link Cross-Channel Identities",
                "Persist an active equivalence link between two identity records so Forge can resolve Telegram, Discord, Web or other channel ids to the same subject.",
                object_schema(
                    &[
                        ("left_scope", "string", "left identity scope, for example telegram"),
                        ("left_id", "string", "left identity id"),
                        ("right_scope", "string", "right identity scope, for example user"),
                        ("right_id", "string", "right identity id"),
                        ("link_type", "string", "optional link type, default same_person"),
                        ("source", "string", "link origin"),
                        ("reason", "string", "optional audit reason"),
                    ],
                    &["left_scope", "left_id", "right_scope", "right_id"],
                ),
                "forge.identity_link.v1",
                &[
                    "forge",
                    "identity",
                    "link",
                    "--left-scope",
                    "<scope>",
                    "--left-id",
                    "<id>",
                    "--right-scope",
                    "<scope>",
                    "--right-id",
                    "<id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.identity.unlink",
                "Unlink Cross-Channel Identities",
                "Mark a persisted identity equivalence link as unlinked without deleting the audit trail.",
                object_schema(
                    &[
                        ("left_scope", "string", "left identity scope"),
                        ("left_id", "string", "left identity id"),
                        ("right_scope", "string", "right identity scope"),
                        ("right_id", "string", "right identity id"),
                        ("link_type", "string", "optional link type"),
                        ("source", "string", "unlink origin"),
                        ("reason", "string", "optional audit reason"),
                    ],
                    &["left_scope", "left_id", "right_scope", "right_id"],
                ),
                "forge.identity_link.v1",
                &[
                    "forge",
                    "identity",
                    "unlink",
                    "--left-scope",
                    "<scope>",
                    "--left-id",
                    "<id>",
                    "--right-scope",
                    "<scope>",
                    "--right-id",
                    "<id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.identity.links",
                "List Identity Links",
                "List persisted cross-channel identity links with optional identity and status filters.",
                object_schema(
                    &[
                        ("scope", "string", "optional identity scope filter"),
                        ("id", "string", "optional identity id filter"),
                        ("status", "string", "optional link status filter"),
                    ],
                    &[],
                ),
                "forge.identity_links.v1",
                &["forge", "identity", "links", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.resolve",
                "Resolve Unified Identity",
                "Resolve a cross-channel identity into its connected aliases and canonical subject.",
                object_schema(
                    &[
                        ("scope", "string", "identity scope"),
                        ("id", "string", "identity id"),
                    ],
                    &["scope", "id"],
                ),
                "forge.identity_resolve.v1",
                &[
                    "forge",
                    "identity",
                    "resolve",
                    "--scope",
                    "<scope>",
                    "--id",
                    "<id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.tenant_index",
                "List Tenant Resource Index",
                "List Forge's physical tenant index for workflows, runs, artifacts and events with organization, brand, product and workflow filters.",
                object_schema(
                    &[
                        ("resource_type", "string", "optional resource type filter: workflow|run|artifact|event"),
                        ("organization_id", "string", "optional organization id filter"),
                        ("brand_id", "string", "optional brand id filter"),
                        ("product_id", "string", "optional product id filter"),
                        ("workflow_id", "string", "optional workflow id filter"),
                    ],
                    &[],
                ),
                "forge.tenant_index.v1",
                &["forge", "identity", "tenant-index", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.tenant_audit",
                "Audit Tenant Resource Index",
                "Audit whether workflows, runs, artifacts and events have corresponding physical tenant-index rows before multi-tenant enforcement is enabled.",
                object_schema(&[], &[]),
                "forge.tenant_audit.v1",
                &["forge", "identity", "tenant-audit", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.tenant_policy",
                "Evaluate Tenant Policy",
                "Evaluate whether a workflow has explicit operating context, active membership and tenant-index coverage before multi-tenant enforcement.",
                object_schema(
                    &[
                        ("workflow_id", "string", "workflow id to evaluate"),
                        ("mode", "string", "optional audit|enforce mode"),
                        ("action", "string", "optional action name to map to a required membership permission"),
                    ],
                    &["workflow_id"],
                ),
                "forge.tenant_policy.v1",
                &[
                    "forge",
                    "identity",
                    "tenant-policy",
                    "--workflow",
                    "<workflow-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.identity.sync",
                "Sync Project Identity Context",
                "Materialize the project operating context into Forge's persisted identity registry.",
                object_schema(
                    &[(
                        "project_root",
                        "string",
                        "optional project root; defaults to current directory",
                    )],
                    &[],
                ),
                "forge.identity_sync.v1",
                &[
                    "forge",
                    "identity",
                    "sync",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.memory.policy",
                "Inspect Memory Policy",
                "Return Forge's file-first memory governance model: global, organization, project and processing scopes; public, internal and private visibility; manager-shared customer suggestions; and company-level request handling.",
                object_schema(
                    &[(
                        "project_root",
                        "string",
                        "optional project root used to resolve .forge/memory-governance.json",
                    )],
                    &[],
                ),
                "forge.memory_policy.v1",
                &["forge", "memory", "policy", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.memory.configure",
                "Configure Project Memory Governance",
                "Write the project's .forge/memory-governance.json file with memory level, default scopes, default audience, privacy mode, retention mode and explicit approval.",
                object_schema(
                    &[
                        ("project_root", "string", "project root containing the .forge directory"),
                        ("memory_level", "string", "MEMORY_NONE|MEMORY_SESSION|MEMORY_SHORT_TERM|MEMORY_STANDARD|MEMORY_FULL|MEMORY_ADMIN"),
                        ("default_scopes", "array", "default memory scopes allowed by the selected memory level"),
                        ("default_audience", "string", "public|internal|manager|operator|private"),
                        ("privacy_mode", "string", "privacy posture such as private_by_default"),
                        ("retention_mode", "string", "retention posture such as processing_auto_archive"),
                        ("approved_by", "string", "operator or human approving the governance update"),
                        ("reason", "string", "why this memory governance policy is being applied"),
                    ],
                    &[
                        "project_root",
                        "memory_level",
                        "default_audience",
                        "privacy_mode",
                        "retention_mode",
                        "approved_by",
                        "reason",
                    ],
                ),
                "forge.memory_governance_config.v1",
                &[
                    "forge",
                    "memory",
                    "configure",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.memory.search",
                "Search File Memory",
                "Search Markdown memory snippets across global, organization, project and processing scopes with visibility/shareability filtering, project governance defaults and line-range results.",
                object_schema(
                    &[
                        ("query", "string", "search query"),
                        ("workflow_id", "string", "optional workflow id used to derive and enforce organization and allowed memory scopes"),
                        ("scopes", "array", "optional memory scopes: global|organization|project|processing"),
                        ("audience", "string", "public|internal|manager|private"),
                        ("visibility", "string", "optional visibility filter: public|internal|private"),
                        ("memory_level", "string", "none|session|short_term|standard|full|admin"),
                        ("run_id", "string", "optional run id for processing memory"),
                        ("organization_id", "string", "optional organization id for default organization memory root"),
                        ("limit", "integer", "maximum results"),
                        ("global_root", "string", "optional global memory root override"),
                        ("organization_root", "string", "optional organization memory root override"),
                        ("project_root", "string", "optional project memory root override"),
                        ("processing_root", "string", "optional processing memory root override"),
                    ],
                    &["query"],
                ),
                "forge.memory_search.v1",
                &["forge", "memory", "search", "--query", "<query>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.memory.promote",
                "Promote Curated Memory",
                "Promote a curated summary from processing/project/organization memory into project, organization or global memory with approval, classification and source lineage. Raw source content is not copied.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id used to derive and enforce organization and allowed memory scopes"),
                        ("from_scope", "string", "processing|project|organization"),
                        ("to_scope", "string", "project|organization|global"),
                        ("source_path", "string", "source memory file path used as evidence"),
                        ("source_start_line", "integer", "optional source start line"),
                        ("source_end_line", "integer", "optional source end line"),
                        ("summary", "string", "curated promoted memory summary"),
                        ("approved_by", "string", "human or operator approving the promotion"),
                        ("reason", "string", "why this memory can be promoted"),
                        ("visibility", "string", "public|internal|private"),
                        ("shareability", "string", "global_shared|organization_shared|project_shared|manager_shared"),
                        ("organization_id", "string", "organization id for default organization memory root"),
                        ("global_root", "string", "optional global memory root override"),
                        ("organization_root", "string", "optional organization memory root override"),
                        ("project_root", "string", "optional project memory root override"),
                        ("dry_run", "boolean", "plan without writing"),
                    ],
                    &[
                        "from_scope",
                        "to_scope",
                        "source_path",
                        "summary",
                        "approved_by",
                        "reason",
                    ],
                ),
                "forge.memory_promotion.v1",
                &[
                    "forge",
                    "memory",
                    "promote",
                    "--from-scope",
                    "<scope>",
                    "--to-scope",
                    "<scope>",
                    "--source-path",
                    "<path>",
                    "--summary",
                    "<summary>",
                    "--approved-by",
                    "<operator>",
                    "--reason",
                    "<reason>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.memory.promotions",
                "List Memory Promotions",
                "List the SQLite-backed memory promotion index with filters by source scope, target scope and approver.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id used to enforce tenant policy before reading the promotion index"),
                        ("from_scope", "string", "optional source scope filter"),
                        ("to_scope", "string", "optional target scope filter"),
                        ("approved_by", "string", "optional approver filter"),
                    ],
                    &[],
                ),
                "forge.memory_promotion_index.v1",
                &[
                    "forge",
                    "memory",
                    "promotions",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.memory.retention",
                "Evaluate Memory Retention",
                "Evaluate memory retention/expiration posture across configured roots without deleting files.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id used to derive and enforce organization and allowed memory scopes"),
                        ("scopes", "array", "optional memory scopes: global|organization|project|processing"),
                        ("run_id", "string", "optional run id for processing memory"),
                        ("organization_id", "string", "optional organization id for default organization memory root"),
                        ("global_root", "string", "optional global memory root override"),
                        ("organization_root", "string", "optional organization memory root override"),
                        ("project_root", "string", "optional project memory root override"),
                        ("processing_root", "string", "optional processing memory root override"),
                    ],
                    &[],
                ),
                "forge.memory_retention.v1",
                &["forge", "memory", "retention", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.memory.cleanup",
                "Cleanup Processing Memory",
                "Archive or delete processing memory that retention classified as delete_after_final_packaging. Non-dry-run execution requires approval, reason and confirm.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id used to derive and enforce organization and allowed memory scopes"),
                        ("scopes", "array", "optional memory scopes; cleanup only acts on processing delete candidates"),
                        ("run_id", "string", "optional run id for processing memory"),
                        ("organization_id", "string", "optional organization id for default organization memory root"),
                        ("global_root", "string", "optional global memory root override"),
                        ("organization_root", "string", "optional organization memory root override"),
                        ("project_root", "string", "optional project memory root override"),
                        ("processing_root", "string", "optional processing memory root override"),
                        ("mode", "string", "archive|delete; defaults to archive"),
                        ("archive_root", "string", "optional archive root for archive mode"),
                        ("approved_by", "string", "operator approving non-dry-run cleanup"),
                        ("reason", "string", "approval reason for non-dry-run cleanup"),
                        ("dry_run", "boolean", "plan without moving or deleting files"),
                        ("confirm", "boolean", "required for non-dry-run cleanup"),
                    ],
                    &[],
                ),
                "forge.memory_cleanup.v1",
                &["forge", "memory", "cleanup", "--dry-run", "--output", "json"],
                ToolFlags::new(false, true),
            ),
            tool(
                "forge.interactive.slash_commands",
                "List Interactive Slash Commands",
                "Return slash-command metadata so agents can map interactive operations to scriptable Forge commands.",
                object_schema(&[], &[]),
                "forge.interactive.slash_commands.v1",
                &[
                    "forge",
                    "interactive",
                    "slash-commands",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.interactive.route",
                "Route Interactive Input",
                "Classify a chat or slash-command input through Forge's interactive routing model. Complex chat input may create a durable workflow/run with retention policy evidence.",
                object_schema(&[
                    ("input", "string", "chat text or slash command"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["input"]),
                "forge.interactive.route.v1",
                &[
                    "forge",
                    "interactive",
                    "route",
                    "--input",
                    "<input>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
            ),
        tool(
            "forge.security.secret_scan",
            "Sanitize Secret Values",
            "Detect and replace secret values with Forge Secret Vault references before prompt construction. The result never serializes raw secret values.",
            object_schema(
                &[
                    ("input", "string", "text to scan and sanitize"),
                    ("scope", "string", "optional vault scope; defaults to project"),
                    ("workflow_id", "string", "optional workflow id for tenant-scoped vault storage"),
                    ("origin", "string", "optional audit origin"),
                    ("enable_entropy", "boolean", "optional entropy scanner toggle"),
                    (
                        "enable_local_ai_fallback",
                        "boolean",
                        "optional local-only AI fallback toggle",
                    ),
                    (
                        "allow_external_ai",
                        "boolean",
                        "must remain false for secret guardrail safety",
                    ),
                ],
                &["input"],
            ),
            "forge.runtime.secret_guardrail.v1",
            &[
                "forge",
                "security",
                "secret-scan",
                "--input",
                "<input>",
                "--output",
                "json",
            ],
            ToolFlags::new(true, false),
        ),
        tool(
            "forge.credential_vault.describe",
                "Describe Credential Vault Contract",
                "Inspect credential-vault contract metadata without resolving or printing secret values.",
                object_schema(
                    &[
                        ("contract", "string", "path to visible credential-vault contract YAML"),
                        ("data", "string", "path to encrypted credential-vault data YAML"),
                        ("vault_bin", "string", "optional credential-vault binary path"),
                    ],
                    &["contract", "data"],
                ),
                CREDENTIAL_VAULT_COMMAND_SCHEMA,
                &[
                    "forge",
                    "credential-vault",
                    "describe",
                    "--contract",
                    "<contract>",
                    "--data",
                    "<data>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.credential_vault.records",
                "List Credential Vault Records",
                "List credential-vault records and fields with secret markers, without resolving secret values.",
                object_schema(
                    &[
                        ("contract", "string", "path to visible credential-vault contract YAML"),
                        ("data", "string", "path to encrypted credential-vault data YAML"),
                        ("vault_bin", "string", "optional credential-vault binary path"),
                    ],
                    &["contract", "data"],
                ),
                CREDENTIAL_VAULT_COMMAND_SCHEMA,
                &[
                    "forge",
                    "credential-vault",
                    "records",
                    "--contract",
                    "<contract>",
                    "--data",
                    "<data>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.aws.check",
                "Check AWS Identity",
                "Validate the configured AWS Ops vault by running aws sts get-caller-identity through the guarded aws-ops wrapper without printing secrets.",
                object_schema(
                    &[
                        ("aws_ops_bin", "string", "optional aws-ops wrapper path"),
                        ("vault_contract", "string", "optional AWS credential-vault contract path"),
                        ("vault_data", "string", "optional encrypted AWS credential-vault data path"),
                    ],
                    &[],
                ),
                AWS_OPS_COMMAND_SCHEMA,
                &["forge", "aws", "check", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.aws.inventory",
                "Inventory AWS Account",
                "Run a read-only inventory for common AWS services through aws-ops and the AWS credential vault.",
                object_schema(
                    &[
                        ("aws_ops_bin", "string", "optional aws-ops wrapper path"),
                        ("vault_contract", "string", "optional AWS credential-vault contract path"),
                        ("vault_data", "string", "optional encrypted AWS credential-vault data path"),
                        ("regions", "string", "comma-separated regions such as us-east-1,sa-east-1"),
                        ("all_regions", "boolean", "discover and inventory all enabled EC2 regions"),
                        ("full", "boolean", "include full AWS JSON payloads instead of compact previews"),
                    ],
                    &[],
                ),
                AWS_OPS_COMMAND_SCHEMA,
                &["forge", "aws", "inventory", "--regions", "<regions>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.aws.raw",
                "Run Guarded AWS Command",
                "Run a direct AWS CLI command through aws-ops. Read-only commands are allowed; non-read-only commands require allow_mutation=true and a concrete reason.",
                object_schema(
                    &[
                        ("aws_ops_bin", "string", "optional aws-ops wrapper path"),
                        ("vault_contract", "string", "optional AWS credential-vault contract path"),
                        ("vault_data", "string", "optional encrypted AWS credential-vault data path"),
                        ("allow_mutation", "boolean", "must be true for non-read-only AWS commands"),
                        ("reason", "string", "required by aws-ops for mutation"),
                        ("aws_args", "array", "AWS CLI arguments, for example [\"sts\",\"get-caller-identity\"]"),
                    ],
                    &["aws_args"],
                ),
                AWS_OPS_COMMAND_SCHEMA,
                &["forge", "aws", "raw", "--", "<aws-args>"],
                ToolFlags::new(true, true),
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
                "Aggregate cron, wait_until and delay schedule state across all Forge-owned workflows for agent runtime visibility.",
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
                "Execute a scheduled workflow that has due cron or one-shot wait nodes (next_run_at <= now).",
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
                "Scan Forge-owned cron/wait scheduled workflows, lease due schedule nodes locally, run due work and report idle scale-to-zero decisions. Supports bounded parallel dispatch with max_workers and returns WorkerPool evidence when parallel.",
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
                        (
                            "idempotency_key",
                            "string",
                            "optional retry key scoped to origin",
                        ),
                        (
                            "lanes",
                            "array",
                            "optional explicit independent lane objects with id, brain and agent_count",
                        ),
                        (
                            "max_parallel_agents",
                            "integer",
                            "optional admission ceiling across all declared lane agents",
                        ),
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
                "forge.run.drive",
                "Drive Async Forge Run",
                "Refresh the run heartbeat and return the next safe executor action, prioritizing accepted needs_retry responses before blind handoff.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("executor", "string", "codex|opencode|skill|mcp|custom executor id"),
                    ("ttl_seconds", "integer", "heartbeat freshness TTL"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id"]),
                "forge.request_drive.v1",
                &["forge", "request", "drive", "--run", "<run-id>", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.step",
                "Step Ready Deterministic Task",
                "Drive a run and auto-promote one ready deterministic task through Forge's normal executor-response validation path; AI and external-command tasks still return handoff_required.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("executor", "string", "codex|opencode|skill|mcp|custom executor id"),
                    ("ttl_seconds", "integer", "heartbeat freshness TTL"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id"]),
                "forge.request_step.v1",
                &["forge", "request", "step", "--run", "<run-id>", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.execute_wave",
                "Execute Admitted Executor Wave",
                "Drive a run, admit a bounded dependency-ready frontier and execute its task-worktree-bound Codex/Agy processes in parallel. Requires explicit allow_exec, approved_by and reason authorization. Process receipts never complete or promote Forge tasks; reviewed validation must still use forge.run.complete_task.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("executor", "string", "executor router id; defaults to auto"),
                    ("ttl_seconds", "integer", "dispatch lease TTL; bounded default is 300 seconds"),
                    ("timeout_seconds", "integer", "per-process timeout; bounded default is 1800 seconds"),
                    ("context_budget", "integer", "per-task bounded context budget; default is 4096 bytes"),
                    ("max_parallel", "integer", "optional execution ceiling; defaults to at most 8 and worker count is capped by admitted assignments"),
                    ("allow_exec", "boolean", "explicit process-launch authorization; must be true"),
                    ("approved_by", "string", "non-empty operator approval provenance"),
                    ("reason", "string", "non-empty process execution authorization reason"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id", "allow_exec", "approved_by", "reason"]),
                "forge.request_executor_wave.v1",
                &["forge", "request", "execute-wave", "--run", "<run-id>", "--allow-exec", "--approved-by", "<operator>", "--reason", "<reason>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.complete_task",
                "Complete Ready Task With Evidence",
                "Record executor evidence for the current ready handoff task, generate a replayable execution trace, validate the executor response, promote the task and drive the next action.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("task_id", "string", "ready task id to complete"),
                    ("executor", "string", "codex|opencode|skill|mcp|custom executor id"),
                    ("summary", "string", "executor result summary without secrets"),
                ("artifacts", "array", "optional local artifact paths to attach"),
                ("evidence_command", "string", "command or gate that produced the submitted evidence"),
                ("evidence_exit_code", "integer", "real exit code produced by evidence_command"),
                ("evidence_summary", "string", "optional passing evidence summary"),
                    ("estimated_usd", "number", "non-negative estimated executor cost"),
                    ("tokens_in", "integer", "non-negative input token count"),
                    ("tokens_out", "integer", "non-negative output token count"),
                    ("ttl_seconds", "integer", "heartbeat freshness TTL"),
                    ("context_budget", "integer", "context budget used when re-driving the ready handoff before completion"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id", "task_id", "summary", "evidence_command", "evidence_exit_code"]),
                "forge.request_task_completion.v1",
                &["forge", "request", "complete-task", "--run", "<run-id>", "--task", "<task-id>", "--summary", "<summary>", "--evidence-command", "<passing-gate>", "--evidence-exit-code", "<observed-exit-code>", "--budget", "<bytes>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.final_package",
                "Create Final Delivery Package",
                "Create a user-facing final delivery package for a run, attaching Markdown and JSON artifacts that summarize readiness, deliverables, evidence, tasks and remaining gaps.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id"]),
                "forge.request_final_delivery_package.v1",
                &["forge", "request", "final-package", "--run", "<run-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.workflow.ensure_final_audit",
                "Ensure Final Completion Audit",
                "Create or surface the final completion audit task for a workflow so user-facing deliverables cannot be mistaken for complete without audited evidence.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("executor", "string", "codex|opencode|skill|mcp|custom executor id"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id"]),
                "forge.request_final_audit.v1",
                &["forge", "request", "ensure-final-audit", "--workflow", "<workflow-id>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.run.switch_executor",
                "Switch Async Run Executor",
                "Hot-swap the active executor for an async run without cancelling the run, changing workflow id, dropping checkpoints or weakening explicit user directives.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("executor", "string", "new executor id such as opencode|codex|custom"),
                    ("fallback_executors", "array", "ordered fallback executor ids, for example [\"codex\"]"),
                    ("summary", "string", "short takeover summary without secrets"),
                    ("ttl_seconds", "integer", "heartbeat freshness TTL for the new executor"),
                    ("pid", "integer", "optional new executor process id"),
                    ("reason", "string", "why the executor is being switched"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id", "executor"]),
                "forge.request_executor_switch.v1",
                &["forge", "request", "switch-executor", "--run", "<run-id>", "--executor", "<executor>", "--fallback-executor", "<fallback-executor>", "--output", "json"],
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
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "goal", "origin"]),
                "forge.workflow_goal_update.v1",
            &["forge", "workflow", "update-goal", "--workflow", "<workflow-id>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
        tool(
            "forge.workflow.add_task",
            "Add Workflow Task",
            "Add a task to a live Forge workflow with an atomic revisioned graph mutation.",
            object_schema(&[
                ("workflow_id", "string", "workflow id"),
                ("task_id", "string", "optional explicit task id"),
                ("description", "string", "task description and goal"),
                ("priority", "string", "high|medium|low"),
                ("origin", "string", "codex|opencode|skill|mcp"),
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "description", "priority", "origin"]),
            "forge.workflow_mutation.v1",
            &["forge", "workflow", "add-task", "--workflow", "<workflow-id>", "--description", "<description>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
        tool(
            "forge.workflow.update_task",
            "Update Workflow Task",
            "Update a mutable task's title, goal or expected output with revision tracking.",
            object_schema(&[
                ("workflow_id", "string", "workflow id"),
                ("task_id", "string", "task id"),
                ("title", "string", "optional new title"),
                ("goal", "string", "optional new task goal"),
                ("expected_output", "string", "optional new expected output"),
                ("origin", "string", "codex|opencode|skill|mcp"),
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "task_id", "origin"]),
            "forge.workflow_task_update.v1",
            &["forge", "workflow", "update-task", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
        tool(
            "forge.workflow.set_priority",
            "Set Workflow Task Priority",
            "Set a task priority through an atomic revisioned Forge graph mutation.",
            object_schema(&[
                ("workflow_id", "string", "workflow id"),
                ("task_id", "string", "task id"),
                ("priority", "string", "high|medium|low"),
                ("origin", "string", "codex|opencode|skill|mcp"),
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "task_id", "priority", "origin"]),
            "forge.workflow_mutation.v1",
            &["forge", "workflow", "set-priority", "--workflow", "<workflow-id>", "--task", "<task-id>", "--priority", "<priority>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
        tool(
            "forge.workflow.add_dependency",
            "Add Workflow Task Dependency",
            "Add a dependency edge after validating that the live workflow graph remains acyclic.",
            object_schema(&[
                ("workflow_id", "string", "workflow id"),
                ("task_id", "string", "task id"),
                ("depends_on", "string", "predecessor task id"),
                ("origin", "string", "codex|opencode|skill|mcp"),
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "task_id", "depends_on", "origin"]),
            "forge.workflow_mutation.v1",
            &["forge", "workflow", "add-dependency", "--workflow", "<workflow-id>", "--task", "<task-id>", "--depends-on", "<predecessor-task-id>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
        tool(
            "forge.workflow.remove_dependency",
            "Remove Workflow Task Dependency",
            "Remove a dependency edge through an atomic revisioned Forge graph mutation.",
            object_schema(&[
                ("workflow_id", "string", "workflow id"),
                ("task_id", "string", "task id"),
                ("depends_on", "string", "predecessor task id"),
                ("origin", "string", "codex|opencode|skill|mcp"),
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "task_id", "depends_on", "origin"]),
            "forge.workflow_mutation.v1",
            &["forge", "workflow", "remove-dependency", "--workflow", "<workflow-id>", "--task", "<task-id>", "--depends-on", "<predecessor-task-id>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
        tool(
            "forge.workflow.set_impediment",
            "Set Workflow Task Impediment",
            "Block a task with a structured active impediment and revisioned audit event.",
            object_schema(&[
                ("workflow_id", "string", "workflow id"),
                ("task_id", "string", "task id"),
                ("reason", "string", "human-readable blocking reason"),
                ("kind", "string", "optional impediment kind; defaults to manual"),
                ("origin", "string", "codex|opencode|skill|mcp"),
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "task_id", "reason", "origin"]),
            "forge.workflow_mutation.v1",
            &["forge", "workflow", "set-impediment", "--workflow", "<workflow-id>", "--task", "<task-id>", "--reason", "<reason>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
        tool(
            "forge.workflow.clear_impediment",
            "Clear Workflow Task Impediment",
            "Clear one impediment by id, or clear only manual impediments when no id is provided.",
            object_schema(&[
                ("workflow_id", "string", "workflow id"),
                ("task_id", "string", "task id"),
                ("impediment_id", "string", "optional impediment id; omit to clear manual impediments only"),
                ("origin", "string", "codex|opencode|skill|mcp"),
                (
                    "expected_revision",
                    "integer",
                    "optional optimistic concurrency revision",
                ),
            ], &["workflow_id", "task_id", "origin"]),
            "forge.workflow_mutation.v1",
            &["forge", "workflow", "clear-impediment", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
            ToolFlags::new(true, true),
        ),
            tool(
                "forge.teamwork.plan",
                "Plan Elastic Teamwork",
                "Create an auditable fan-out/fan-in workflow with one or more independent lanes. Each lane chooses its executor brain and agent count, such as three Agy frontend agents and five Codex backend agents.",
                object_schema(&[
                    ("goal", "string", "teamwork objective"),
                    (
                        "lanes",
                        "array",
                        "optional lane objects with id, brain, agent_count, optional parallel_group and optional responsibility",
                    ),
                    (
                        "max_parallel_agents",
                        "integer",
                        "optional admission ceiling across all lane agents",
                    ),
                    (
                        "bypass_cache",
                        "boolean",
                        "optionally bypass advisory benchmark cache",
                    ),
                ], &["goal"]),
                "forge.teamwork.plan.v1",
                &["forge", "teamwork", "--goal", "<goal>", "--lane", "frontend=agy:3", "--lane", "backend=codex:5", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.workflow.update_node_brain",
                "Update Node Brain Routing",
                "Mutate one AI or mixed workflow node's Forge-owned brain routing without stopping the workflow run. Supports default brain, allowed brains, multiple agent slots and parallel-agent limits.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task/node id"),
                    ("default_brain", "string", "optional default execution brain for this node"),
                    ("allowed_brains", "array", "optional allowed brain ids"),
                    ("agent_slots", "array", "optional slot specs: slot_id=brain_id:role:parallel_group"),
                    ("max_parallel_agents", "integer", "optional node-level parallel agent limit"),
                    ("origin", "string", "codex|opencode|gemini|claude|skill|mcp"),
                ], &["workflow_id", "task_id"]),
                "forge.workflow_node_brain_routing_update.v1",
                &["forge", "workflow", "update-node-brain", "--workflow", "<workflow-id>", "--task", "<task-id>", "--output", "json"],
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
                    ("tags", "array", "optional artifact search tags"),
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
                "Build the minimum correct task-local context package before executor handoff, optionally resolving project memory governance.",
                context_request_input_schema(),
            CONTEXT_COMPACT_VIEW_SCHEMA_VERSION,
            &[
                "forge",
                "context",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--view",
                "compact",
                "--output",
                "json",
            ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.worktree.guard.check",
                "Check Worktree Modification Guard",
                "Evaluate file and directory modification scopes for a registered worktree and return a fail-closed agent action when a protected path needs a predecessor task.",
                object_schema(&[
                    ("worktree", "string", "registered worktree id or root"),
                    ("operation", "string", "modify"),
                    ("paths", "array", "repository-relative files or directory scopes"),
                    ("reason", "string", "objective reason for the requested modification"),
                    ("workflow_id", "string", "optional workflow binding"),
                    ("task_id", "string", "optional current task binding"),
                ], &["worktree", "paths", "reason"]),
                "forge.worktree.modification_guard.v1",
                &["forge", "worktree", "guard", "check", "--worktree", "<worktree-id>", "--path", "<path>", "--reason", "<reason>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.worktree.guard.create_predecessor",
                "Create Worktree Guard Predecessor",
                "With explicit workflow-mutation approval, create a path-specific predecessor task and block the current task through a revisioned DAG dependency.",
                object_schema(&[
                    ("worktree", "string", "registered worktree id or root"),
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "current task id"),
                    ("paths", "array", "blocked repository-relative paths"),
                    ("goal", "string", "objective and path-specific predecessor goal"),
                    ("allow_workflow_mutation", "boolean", "explicit mutation authorization"),
                    ("approved_by", "string", "approver identity"),
                    ("origin", "string", "mutation origin"),
                ], &["worktree", "workflow_id", "task_id", "paths", "goal", "allow_workflow_mutation", "approved_by"]),
                "forge.worktree.predecessor_task.v1",
                &["forge", "worktree", "guard", "create-predecessor", "--worktree", "<worktree-id>", "--workflow", "<workflow-id>", "--task", "<task-id>", "--path", "<path>", "--goal", "<goal>", "--allow-workflow-mutation", "--approved-by", "<approver>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.worktree.sandbox.plan",
                "Plan Worktree Sandbox",
                "Evaluate the approved worktree, binding, command, runtime, filesystem and network guardrails without executing the preview or test payload.",
                object_schema(&[
                    ("worktree", "string", "registered worktree id or root"),
                    ("purpose", "string", "preview|test"),
                    ("workflow_id", "string", "optional workflow binding"),
                    ("task_id", "string", "optional task binding; requires workflow_id"),
                    ("command", "array", "optional command argv override"),
                ], &["worktree", "purpose"]),
                "forge.worktree.sandbox_plan.v1",
                &["forge", "worktree", "sandbox", "plan", "--worktree", "<worktree-id>", "--purpose", "<preview|test>", "--output", "json", "--", "<command>"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.worktree.sandbox.run",
                "Run Worktree Sandbox",
                "Run one bounded preview or test command after explicit execution authorization and persist a structured, redacted receipt.",
                object_schema(&[
                    ("worktree", "string", "registered worktree id or root"),
                    ("purpose", "string", "preview|test"),
                    ("workflow_id", "string", "optional workflow binding"),
                    ("task_id", "string", "optional task binding; requires workflow_id"),
                    ("command", "array", "optional command argv override"),
                    ("allow_exec", "boolean", "explicit payload execution authorization"),
                ], &["worktree", "purpose", "allow_exec"]),
                "forge.worktree.sandbox_receipt.v1",
                &["forge", "worktree", "sandbox", "run", "--worktree", "<worktree-id>", "--purpose", "<preview|test>", "--allow-exec", "--output", "json", "--", "<command>"],
                ToolFlags::new(false, false),
            ),
            tool(
                "forge.worktree.sandbox.start",
                "Start Worktree Sandbox",
                "Start a detached, persistent preview or test sandbox under a Forge supervisor after explicit execution authorization.",
                object_schema(&[
                    ("worktree", "string", "registered worktree id or root"),
                    ("purpose", "string", "preview|test"),
                    ("workflow_id", "string", "optional workflow binding"),
                    ("task_id", "string", "optional task binding; requires workflow_id"),
                    ("command", "array", "optional command argv override"),
                    ("allow_exec", "boolean", "explicit payload execution authorization"),
                ], &["worktree", "purpose", "allow_exec"]),
                "forge.worktree.sandbox_lifecycle.v1",
                &["forge", "worktree", "sandbox", "start", "--worktree", "<worktree-id>", "--purpose", "<preview|test>", "--allow-exec", "--output", "json", "--", "<command>"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.worktree.sandbox.status",
                "Inspect Worktree Sandbox",
                "Inspect the durable lifecycle state for a persistent preview or test sandbox.",
                object_schema(&[("sandbox_id", "string", "persistent sandbox id")], &["sandbox_id"]),
                "forge.worktree.sandbox_lifecycle.v1",
                &["forge", "worktree", "sandbox", "status", "--sandbox", "<sandbox-id>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.worktree.sandbox.stop",
                "Stop Worktree Sandbox",
                "Stop the persistent sandbox payload process group after explicit stop authorization and persist the terminal lifecycle state.",
                object_schema(&[
                    ("sandbox_id", "string", "persistent sandbox id"),
                    ("allow_stop", "boolean", "explicit process-group termination authorization"),
                ], &["sandbox_id", "allow_stop"]),
                "forge.worktree.sandbox_lifecycle.v1",
                &["forge", "worktree", "sandbox", "stop", "--sandbox", "<sandbox-id>", "--allow-stop", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.token_headroom",
                "Analyze Token Headroom",
                "Apply Forge-native local token-headroom routing to a context or tool-output payload and report estimated savings plus reversible retrieval metadata.",
                object_schema(&[
                    ("content", "string", "context or tool-output payload"),
                    ("content_kind", "string", "optional json|log|search|code|text hint"),
                    ("budget_tokens", "integer", "optional target token budget"),
                    ("source", "string", "caller/source label"),
                    ("reversible", "boolean", "whether to include retrieval metadata"),
                    ("persist", "boolean", "whether to store reversible content locally"),
                ], &["content"]),
                "forge.harness.token_headroom.v1",
                &["forge", "harness", "token-headroom", "--content", "<payload>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.harness.retrieve_headroom",
                "Retrieve Headroom Blob",
                "Retrieve metadata or content for a persisted Forge headroom blob by retrieval ref.",
                object_schema(&[
                    ("retrieval_ref", "string", "forge://harness/headroom/<sha256> or raw sha256"),
                    ("include_content", "boolean", "include original and compressed content"),
                ], &["retrieval_ref"]),
                "forge.harness.headroom_retrieval.v1",
                &["forge", "harness", "retrieve-headroom", "--ref", "<retrieval-ref>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.headroom_stats",
                "Inspect Headroom Stats",
                "Aggregate persisted Forge headroom blobs by source and content kind, including top reversible retrieval refs for token-savings observability.",
                object_schema(&[
                    ("source", "string", "optional persisted headroom source filter"),
                    ("content_kind", "string", "optional json|log|search|code|text filter"),
                    ("limit", "integer", "maximum top saved blobs to return"),
                ], &[]),
                "forge.harness.headroom_stats.v1",
                &["forge", "harness", "headroom-stats", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.mode",
                "Inspect Harness Mode",
                "Report the effective Forge-first harness mode, source, project config, exec policy status and precedence before wrapper, shim or exec use.",
                object_schema(&[
                    ("forge_first", "boolean", "simulate an explicit Forge-first CLI flag"),
                    ("observe_only", "boolean", "simulate an observe-only CLI override"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                ], &[]),
                "forge.harness.mode.v1",
                &["forge", "harness", "mode", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.doctor",
                "Audit Forge Harness Readiness",
                "Return a read-only consolidated harness readiness report for one brain CLI, combining Forge-first mode, project policy, shim status, wrapper/session lifecycle plan and token-headroom readiness without installing shims or launching child processes.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("shim_dir", "string", "directory where Forge-owned shims should live"),
                    ("forge_first", "boolean", "simulate an explicit Forge-first CLI flag"),
                    ("observe_only", "boolean", "simulate an observe-only CLI override"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("workflow_id", "string", "optional workflow lineage for readiness"),
                    ("task_id", "string", "optional task/node lineage for readiness"),
                    ("run_id", "string", "optional async run lineage for readiness"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom readiness"),
                ], &["executor", "shim_dir"]),
                "forge.harness.doctor.v1",
                &["forge", "harness", "doctor", "--executor", "<executor>", "--shim-dir", "<dir>", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.headroom_plan",
                "Plan Harness Headroom",
                "Return a read-only Forge harness headroom plan for one brain CLI, including effective context budget, token-headroom source, wrapper env, session lifecycle plan, compression pipeline, reserve strategy and next commands.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("command", "array", "optional command argv to launch under the harness"),
                    ("forge_first", "boolean", "prefer Forge context routing before native CLI defaults"),
                    ("observe_only", "boolean", "force one read-only observe-only planning pass"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("task_id", "string", "optional task/node lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom env"),
                ], &["executor"]),
                "forge.harness.headroom_plan.v1",
                &["forge", "harness", "headroom-plan", "--executor", "<executor>", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.adoption_plan",
                "Plan Forge-First Harness Adoption",
                "Return a read-only ordered adoption plan for making one project use Forge-first CLI harness defaults, token headroom, shims, executor sync and lineage-required execution without writing config, installing shims or launching child processes.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("shim_dir", "string", "directory where Forge-owned shims should live"),
                    ("forge_first", "boolean", "simulate an explicit Forge-first CLI flag"),
                    ("observe_only", "boolean", "simulate an observe-only CLI override"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("task_id", "string", "optional task/node lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom readiness"),
                ], &["executor", "shim_dir"]),
                "forge.harness.adoption_plan.v1",
                &["forge", "harness", "adoption-plan", "--executor", "<executor>", "--shim-dir", "<dir>", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.activation_profile",
                "Project Forge-First Shell Activation",
                "Return a shell activation/deactivation profile that puts Forge-owned CLI shims first in PATH and exports Forge-first harness defaults. Dry-run by default; with apply, shell_rc and approved_by it writes a reversible Forge-managed shell startup block without launching child CLIs.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("shim_dir", "string", "directory where Forge-owned shims should live"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("shell_rc", "string", "optional shell startup file to update when apply=true"),
                    ("apply", "boolean", "write the managed shell startup block when true"),
                    ("approved_by", "string", "required approver when apply=true"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom defaults"),
                ], &["executor", "shim_dir"]),
                "forge.harness.activation_profile.v1",
                &["forge", "harness", "activation-profile", "--executor", "<executor>", "--shim-dir", "<dir>", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(false, false),
            ),
            tool(
                "forge.harness.bootstrap",
                "Bootstrap Forge-First Harness",
                "Plan by default, or with explicit approval write project .forge/harness.json and install Forge-owned CLI shims for one executor. It keeps dry-run safe by default and reports the adoption plan, config write and shim install evidence.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("shim_dir", "string", "directory where Forge-owned shims should live"),
                    ("project_root", "string", "project root where .forge/harness.json should be written"),
                    ("context_budget", "integer", "default context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom defaults"),
                    ("apply", "boolean", "write project config and install shim when true"),
                    ("approved_by", "string", "required approver when apply=true"),
                    ("force", "boolean", "allow replacing an existing Forge-owned or disposable shim"),
                ], &["executor", "shim_dir", "project_root"]),
                "forge.harness.bootstrap.v1",
                &["forge", "harness", "bootstrap", "--executor", "<executor>", "--shim-dir", "<dir>", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(false, false),
            ),
            tool(
                "forge.harness.wrap_plan",
                "Plan Forge-First CLI Wrapper",
                "Return a non-destructive Forge-first wrapper plan for Codex, Claude, Gemini or OpenCode with context budget, token-headroom environment shaping and session lifecycle gates.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("command", "array", "command argv to launch under the harness"),
                    ("forge_first", "boolean", "prefer Forge context routing before native CLI defaults"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("task_id", "string", "optional task/node lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom env"),
                ], &["executor"]),
                "forge.harness.cli_wrapper_plan.v1",
                &["forge", "harness", "wrap-plan", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.install_shims",
                "Install Forge-First CLI Shims",
                "Install a Forge-owned PATH shim for a brain CLI without overwriting an existing non-Forge file unless explicitly forced.",
                object_schema(&[
                    ("shim_dir", "string", "directory where Forge-owned shims will be written"),
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("real_cmd", "string", "optional resolved native CLI command/path; omitted values are discovered from PATH outside shim_dir"),
                    ("forge_first", "boolean", "prefer Forge context routing before native CLI defaults"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("task_id", "string", "optional task/node lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom env"),
                    ("force", "boolean", "allow replacing an existing file"),
                ], &["shim_dir", "executor"]),
                "forge.harness.shim_install.v1",
                &["forge", "harness", "install-shims", "--shim-dir", "<dir>", "--executor", "<executor>", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.harness.install_provider_adapter",
                "Install Connected-Brain Provider Adapter",
                "Install a Forge-owned provider adapter command that is separate from daily PATH shims and emits forge.connected_external_brain.provider_output.v1 only when later executed by milestone collection.",
                object_schema(&[
                    ("shim_dir", "string", "directory where Forge-owned provider adapters will be written"),
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("real_cmd", "string", "optional approved provider CLI command/path; omitted values are discovered from PATH outside shim_dir"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("token_headroom", "boolean", "record token-headroom intent in adapter output"),
                    ("force", "boolean", "allow replacing an existing Forge-owned adapter file"),
                ], &["shim_dir", "executor"]),
                "forge.harness.provider_adapter_install.v1",
                &["forge", "harness", "install-provider-adapter", "--shim-dir", "<dir>", "--executor", "<executor>", "--real-cmd", "<provider-cli>", "--project-root", "<project-root>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.harness.shim_status",
                "Inspect Forge-First CLI Shim Status",
                "Audit whether a brain CLI shim exists, is Forge-owned, has PATH precedence and avoids recursion before using Forge-first shells.",
                object_schema(&[
                    ("shim_dir", "string", "directory where Forge-owned shims should live"),
                    ("executor", "string", "codex|claude|gemini|opencode"),
                ], &["shim_dir", "executor"]),
                "forge.harness.shim_status.v1",
                &["forge", "harness", "shim-status", "--shim-dir", "<dir>", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.exec",
                "Execute Forge Harness Receipt",
                "Return a dry-run or explicitly guarded execution receipt for a Forge-first brain CLI invocation, including executable resolution, env overlay, bounded output hashes and workflow timeline event evidence when lineage is present.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("command", "array", "command argv to launch under the harness"),
                    ("forge_first", "boolean", "prefer Forge context routing before native CLI defaults"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("task_id", "string", "optional task/node lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom env"),
                    ("dry_run", "boolean", "default true; false requests guarded execution"),
                    ("allow_exec", "boolean", "must be true together with dry_run=false before executing"),
                    ("secret_env", "array", "optional ENV=vault.reference mappings resolved only with secret permissions"),
                    ("secret_permissions", "array", "must include credential_secret_permissions and tool_usage_permissions for injection"),
                    ("project_root", "string", "optional project root containing .forge/harness.json"),
                    ("cwd", "string", "optional child working directory"),
                ], &["executor"]),
                "forge.harness.exec_receipt.v1",
                &["forge", "harness", "exec", "--executor", "<executor>", "--project-root", "<project-root>", "--", "<cmd>"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.task.handoff",
                "Acquire Task Handoff",
                "Acquire a bounded executor handoff packet for an authorized task executor, optionally resolving project memory governance.",
                task_handoff_input_schema(),
                EXECUTOR_HANDOFF_COMPACT_SCHEMA_VERSION,
                &["forge", "task", "handoff", "--workflow", "<workflow-id>", "--task", "<task-id>", "--executor", "<executor>", "--view", "compact", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.patch.plan",
                "Plan File Patch",
                "Create a Forge-owned, bounded file patch plan with snapshots, permission gates and diff review without applying changes.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("intent", "string", "patch intent"),
                    ("paths", "array", "repo-relative file paths"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id", "intent", "paths"]),
                "forge.patch_plan.v1",
                &["forge", "patch", "plan", "--workflow", "<workflow-id>", "--task", "<task-id>", "--intent", "<intent>", "--path", "<path>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.patch.apply",
                "Apply File Patch",
                "Record a file patch as applied: snapshot current file state, run validation, and persist an apply artifact with rollback support.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("paths", "array", "repo-relative file paths that were modified"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                    ("plan_artifact", "string", "optional path to patch plan artifact for lineage"),
                ], &["workflow_id", "task_id", "paths"]),
                "forge.patch_apply.v1",
                &["forge", "patch", "apply", "--workflow", "<workflow-id>", "--task", "<task-id>", "--path", "<path>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.patch.review",
                "Review File Patch",
                "Review current repo diffs for a bounded file patch, persist review evidence, and keep source files unchanged.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("paths", "array", "repo-relative file paths to review"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                    ("plan_artifact", "string", "optional patch plan artifact for lineage"),
                ], &["workflow_id", "task_id", "paths"]),
                "forge.patch_review.v1",
                &["forge", "patch", "review", "--workflow", "<workflow-id>", "--task", "<task-id>", "--path", "<path>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.patch.diff",
                "Navigate File Patch Diff",
                "Build a read-only multi-file diff navigation model with selectable file and hunk indexes.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("paths", "array", "repo-relative file paths to include"),
                    ("file_index", "number", "selected file index"),
                    ("hunk_index", "number", "selected hunk index"),
                    ("context_lines", "number", "git diff context line count"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id", "paths"]),
                "forge.patch_diff.v1",
                &["forge", "patch", "diff", "--workflow", "<workflow-id>", "--task", "<task-id>", "--path", "<path>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.patch.revert",
                "Revert File Patch",
                "Record a guarded revert proposal for a previously applied file patch without restoring files automatically.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("apply_artifact", "string", "path to the apply artifact to revert"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id", "apply_artifact"]),
                "forge.patch_revert.v1",
                &["forge", "patch", "revert", "--workflow", "<workflow-id>", "--task", "<task-id>", "--apply-artifact", "<path>", "--output", "json"],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.patch.restore",
                "Restore File Patch",
                "Execute an explicitly approved repo-local file restore from a patch revert artifact and persist restore evidence.",
                object_schema(&[
                    ("workflow_id", "string", "workflow id"),
                    ("task_id", "string", "task id"),
                    ("revert_artifact", "string", "path to patch revert artifact"),
                    ("approved_by", "string", "human or operator identity approving restore"),
                    ("confirm_restore", "boolean", "must be true to execute restore"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["workflow_id", "task_id", "revert_artifact", "approved_by", "confirm_restore"]),
                "forge.patch_restore.v1",
                &["forge", "patch", "restore", "--workflow", "<workflow-id>", "--task", "<task-id>", "--revert-artifact", "<path>", "--approved-by", "<operator>", "--confirm-restore", "--output", "json"],
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
                "Generate the Forge 0.5 promotion manifest with requirements, completed and missing capabilities, validation evidence, attached evidence, demos, gaps and decision.",
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
                "forge.milestone.production_plan",
                "Plan Production Readiness Evidence",
                "Return the fail-closed production-readiness evidence requirements without executing commands, collecting evidence or mutating state.",
                object_schema(&[("version", "string", "milestone version, currently 0.5")], &[]),
                "forge.milestone.production_readiness_plan.v1",
                &[
                    "forge",
                    "milestone",
                    "production-plan",
                    "--version",
                    "0.5",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.milestone.production_readiness",
                "Evaluate Production Readiness",
                "Evaluate an explicit secret-free manifest and its evidence root read-only, returning the existing fail-closed production-readiness report without executing commands or inventing evidence.",
                object_schema(&[
                    ("version", "string", "milestone version, currently 0.5"),
                    ("manifest", "string", "manifest path inside evidence_root"),
                    ("evidence_root", "string", "root directory containing the manifest and referenced evidence"),
                ], &["manifest", "evidence_root"]),
                "forge.milestone.production_readiness.v1",
                &[
                    "forge",
                    "milestone",
                    "production-readiness",
                    "--version",
                    "0.5",
                    "--manifest",
                    "<manifest>",
                    "--evidence-root",
                    "<evidence-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.milestone.attach_evidence",
                "Attach Milestone Evidence",
                "Attach an operator-approved milestone evidence artifact into the Forge store and global event timeline without auto-promoting the milestone.",
                object_schema(&[
                    ("version", "string", "milestone version, currently 0.5"),
                    ("capability_id", "string", "milestone capability id"),
                    ("kind", "string", "evidence kind, for example production_runtime_benchmark"),
                    ("summary", "string", "short evidence summary"),
                    ("artifact_path", "string", "local path to the evidence artifact"),
                    ("approved_by", "string", "operator approving this evidence attachment"),
                    ("origin", "string", "codex|opencode|gemini|forge_cli|skill|mcp"),
                ], &["capability_id", "kind", "summary", "artifact_path", "approved_by"]),
                "forge.milestone.attached_evidence.v1",
                &[
                    "forge",
                    "milestone",
                    "attach-evidence",
                    "--version",
                    "0.5",
                    "--capability",
                    "<capability-id>",
                    "--artifact",
                    "<artifact-path>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.milestone.evidence_plan",
                "Plan Milestone Evidence Collection",
                "Inspect project manifests, secret-free manifest_templates, attached evidence and collection commands before collecting real milestone receipts.",
                object_schema(&[
                    ("version", "string", "milestone version, currently 0.5"),
                    ("capability_id", "string", "milestone capability id"),
                    ("kind", "string", "optional evidence kind; defaults to the capability collector default"),
                    ("project_root", "string", "project root containing .forge manifests"),
                    ("connected_brain", "string", "optional connected brain provider id"),
                    ("connected_runtime", "string", "optional connected multimodal runtime id"),
                ], &["capability_id"]),
                "forge.milestone.evidence_plan.v1",
                &[
                    "forge",
                    "milestone",
                    "evidence-plan",
                    "--version",
                    "0.5",
                    "--capability",
                    "<capability-id>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.milestone.prepare_evidence_inputs",
                "Prepare Milestone Evidence Inputs",
                "Materialize secret-free milestone evidence manifest templates under a project .forge directory. Dry-run by default; apply requires approved_by and refuses overwrite unless force is true.",
                object_schema(&[
                    ("version", "string", "milestone version, currently 0.5"),
                    ("capability_id", "string", "milestone capability id"),
                    ("project_root", "string", "project root where .forge manifests are prepared"),
                    ("connected_brain", "string", "optional connected brain provider id"),
                    ("connected_runtime", "string", "optional connected multimodal runtime id"),
                    ("provider_command", "string", "optional approved absolute connected-brain provider adapter command for replacement_grade_cli"),
                    ("model_id", "string", "approved model or provider profile id for connected-brain manifests"),
                    ("approval_ref", "string", "approval/change reference for connected-brain manifests"),
                    ("apply", "boolean", "write files when true; dry-run when false or omitted"),
                    ("approved_by", "string", "operator approving file writes when apply is true"),
                    ("force", "boolean", "allow overwriting existing template targets after review"),
                    ("origin", "string", "codex|opencode|gemini|forge_cli|skill|mcp"),
                ], &["capability_id"]),
                "forge.milestone.prepare_evidence_inputs.v1",
                &[
                    "forge",
                    "milestone",
                    "prepare-evidence-inputs",
                    "--version",
                    "0.5",
                    "--capability",
                    "<capability-id>",
                    "--project-root",
                    "<project-root>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.milestone.collect_evidence",
                "Collect and Attach Milestone Evidence",
                "Run a ready, approval-gated milestone evidence source, persist the generated receipt artifact and attach it to the milestone without auto-promoting.",
                object_schema(&[
                    ("version", "string", "milestone version, currently 0.5"),
                    ("capability_id", "string", "milestone capability id"),
                    ("project_root", "string", "project root containing .forge manifests"),
                    ("connected_brain", "string", "optional connected brain provider id"),
                    ("connected_runtime", "string", "optional connected multimodal runtime id"),
                    ("approved_by", "string", "operator approving evidence collection and attachment"),
                    ("origin", "string", "codex|opencode|gemini|forge_cli|skill|mcp"),
                ], &["capability_id", "approved_by"]),
                "forge.milestone.collect_evidence.v1",
                &[
                    "forge",
                    "milestone",
                    "collect-evidence",
                    "--version",
                    "0.5",
                    "--capability",
                    "<capability-id>",
                    "--kind",
                    "<evidence-kind>",
                    "--approved-by",
                    "<operator>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
            ),
            tool(
                "forge.milestone.collect_ready_evidence",
                "Collect Ready Milestone Evidence",
                "Attempt every required Forge 0.5 milestone evidence kind, attach receipts whose inputs are ready and report skipped or failed kinds without auto-promoting.",
                object_schema(&[
                    ("version", "string", "milestone version, currently 0.5"),
                    ("project_root", "string", "project root containing .forge manifests"),
                    ("connected_brain", "string", "optional connected brain provider id"),
                    ("connected_runtime", "string", "optional connected multimodal runtime id"),
                    ("approved_by", "string", "operator approving ready evidence collection and attachment"),
                    ("origin", "string", "codex|opencode|gemini|forge_cli|skill|mcp"),
                ], &["approved_by"]),
                "forge.milestone.collect_ready_evidence.v1",
                &[
                    "forge",
                    "milestone",
                    "collect-ready-evidence",
                    "--version",
                    "0.5",
                    "--approved-by",
                    "<operator>",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, true),
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
                "Generate deterministic Forge-first replacement-grade CLI demo evidence for coding, patch lifecycle artifacts, connected external brain providers, research/artifact and long-running async workflows without mutating external resources.",
                object_schema(&[
                    ("origin", "string", "codex|opencode|skill|mcp"),
                    ("project_root", "string", "optional project root containing .forge/connected-brain-runtimes.json"),
                    ("connected_brain", "string", "optional provider id from the connected brain runtime manifest"),
                ], &[]),
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
                    ("project_root", "string", "optional project root containing .forge/multimodal.json"),
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
                    ("project_root", "string", "optional project root containing .forge/multimodal.json"),
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
                "forge.multimodal.readiness",
                "Inspect Multimodal Readiness",
                "Inspect runtime PATH candidates and Forge model manifests for one multimodal capability without installs, model execution, device access, network access or automation.",
                object_schema(&[
                    ("capability_id", "string", "capability id from forge.multimodal.status"),
                    ("enable_experimental", "boolean", "optional explicit experimental flag for readiness output only"),
                    ("project_root", "string", "optional project root containing .forge/multimodal.json and .forge/multimodal-models"),
                    ("allow", "boolean", "optional explicit runtime guard allow for readiness reporting only; no execution is performed"),
                ], &["capability_id"]),
                "forge.multimodal.readiness.v1",
                &[
                    "forge",
                    "multimodal",
                    "readiness",
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
                    ("project_root", "string", "optional project root containing .forge/multimodal.json"),
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
                "forge.multimodal.benchmark_result",
                "Record Multimodal Fixture Benchmark Result",
                "Generate an approval-gated fixture-only benchmark result artifact for one multimodal capability without installs, model execution, device access, network access or automation.",
                object_schema(
                    &[
                        ("capability_id", "string", "capability id from forge.multimodal.status"),
                        ("fixture_id", "string", "fixture id from forge.multimodal.benchmark_template"),
                        ("approved_by", "string", "required human/operator approval identity"),
                        ("confirm_fixture_only", "boolean", "must be true to confirm no model/device/network execution"),
                        ("enable_experimental", "boolean", "optional explicit experimental flag for evidence output only"),
                        ("project_root", "string", "optional project root containing .forge/multimodal.json"),
                    ],
                    &["capability_id", "fixture_id", "approved_by", "confirm_fixture_only"],
                ),
                "forge.multimodal.benchmark_result.v1",
                &[
                    "forge",
                    "multimodal",
                    "benchmark-result",
                    "--capability",
                    "<capability-id>",
                    "--fixture",
                    "<fixture-id>",
                    "--approved-by",
                    "<operator>",
                    "--confirm-fixture-only",
                    "--output",
                    "json",
                ],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.multimodal.runtime_benchmark",
                "Run Guarded Multimodal Runtime Benchmark",
                "Run an approval-gated, guard-approved deterministic or project-connected runtime benchmark after experimental opt-in. This performs no installs, device access or network access; connected runtimes are loaded from a project manifest and require an explicit connected_runtime id. A connected runtime can declare production evidence with approval, model manifest hash, artifacts and quality/latency thresholds; Forge marks that benchmark promotion_ready only when the probe measurements satisfy the contract.",
                object_schema(
                    &[
                        ("capability_id", "string", "capability id from forge.multimodal.status"),
                        ("fixture_id", "string", "fixture id from forge.multimodal.benchmark_template"),
                        ("approved_by", "string", "required human/operator approval identity"),
                        ("confirm_runtime_execution", "boolean", "must be true to confirm runtime execution is approved"),
                        ("allow_model", "boolean", "must be true after reviewing the model runtime guard"),
                        ("enable_experimental", "boolean", "optional explicit experimental flag"),
                        ("project_root", "string", "optional project root containing .forge/multimodal.json"),
                        ("connected_runtime", "string", "optional runtime id from .forge/multimodal-runtimes.json to probe under the same guard"),
                    ],
                    &[
                        "capability_id",
                        "fixture_id",
                        "approved_by",
                        "confirm_runtime_execution",
                        "allow_model",
                    ],
                ),
                "forge.multimodal.runtime_benchmark.v1",
                &[
                    "forge",
                    "multimodal",
                    "runtime-benchmark",
                    "--capability",
                    "<capability-id>",
                    "--fixture",
                    "<fixture-id>",
                    "--approved-by",
                    "<operator>",
                    "--confirm-runtime-execution",
                    "--allow-model",
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
                    ("project_root", "string", "optional project root containing .forge/multimodal.json"),
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
                "forge.multimodal.demo_receipt",
                "Record Multimodal Guarded Demo Receipt",
                "Record approval-gated guarded demo evidence after experimental opt-in using a local fixture receipt and explicit guard matrix. This tool performs no installs, real model execution, device access, filesystem access or network access unless separate guard approvals are supplied and recorded.",
                object_schema(
                    &[
                        ("demo_id", "string", "local_image_recognition|audio_transcription_synthesis|blender_avatar_preparation"),
                        ("fixture_id", "string", "fixture id from forge.multimodal.benchmark_template"),
                        ("approved_by", "string", "required human/operator approval identity"),
                        ("confirm_local_fixture", "boolean", "must be true to confirm the receipt uses a secret-free local fixture"),
                        ("enable_experimental", "boolean", "optional explicit experimental flag for evidence output"),
                        ("project_root", "string", "optional project root containing .forge/multimodal.json"),
                        ("allow_model", "boolean", "record model runtime guard approval without executing a real model"),
                        ("allow_camera", "boolean", "record camera guard approval; omitted means blocked/no access"),
                        ("allow_microphone", "boolean", "record microphone guard approval; omitted means blocked/no access"),
                        ("allow_screen", "boolean", "record screen guard approval; omitted means blocked/no access"),
                        ("allow_input", "boolean", "record input guard approval; omitted means blocked/no access"),
                        ("allow_filesystem", "boolean", "record filesystem guard approval; omitted means blocked/no access"),
                    ],
                    &[
                        "demo_id",
                        "fixture_id",
                        "approved_by",
                        "confirm_local_fixture",
                    ],
                ),
                "forge.multimodal.demo_receipt.v1",
                &[
                    "forge",
                    "multimodal",
                    "demo-receipt",
                    "--demo",
                    "<demo-id>",
                    "--fixture",
                    "<fixture-id>",
                    "--approved-by",
                    "<operator>",
                    "--confirm-local-fixture",
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
                    ("project_root", "string", "optional project root containing .forge/multimodal.json"),
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
        "forge.teamwork.plan" => {
            let input: TeamworkPlanInput = parse_input(input)?;
            if input.lanes.is_empty() && input.max_parallel_agents.is_none() {
                serde_json::to_value(plan_teamwork_workflow(
                    store,
                    &input.goal,
                    false,
                    input.bypass_cache,
                )?)?
            } else {
                let lanes = if input.lanes.is_empty() {
                    TeamworkParallelConfig::default().lanes
                } else {
                    input
                        .lanes
                        .into_iter()
                        .map(TeamworkLaneInput::into_config)
                        .collect()
                };
                let derived_parallelism = lanes.iter().try_fold(0usize, |total, lane| {
                    total
                        .checked_add(lane.agent_count)
                        .context("teamwork lane agent count overflow")
                })?;
                serde_json::to_value(plan_teamwork_workflow_with_config(
                    store,
                    &input.goal,
                    false,
                    input.bypass_cache,
                    TeamworkParallelConfig {
                        lanes,
                        max_parallel_agents: input
                            .max_parallel_agents
                            .unwrap_or(derived_parallelism),
                    },
                )?)?
            }
        }
        "forge.mission.start" => {
            let input: MissionStartInput = parse_input(input)?;
            serde_json::to_value(start_mission(
                store,
                &input.goal,
                input.squad.as_deref().unwrap_or("software-factory"),
                input.squad_version.as_deref(),
                PathBuf::from(input.worktree).as_path(),
            )?)?
        }
        "forge.mission.drive" => {
            let input: MissionIdInput = parse_input(input)?;
            serde_json::to_value(drive_mission(store, &input.mission_id)?)?
        }
        "forge.mission.execute" => {
            let input: MissionExecuteInput = parse_input(input)?;
            let mission = load_mission(store, &input.mission_id)?;
            let agent = mission
                .agents
                .iter()
                .find(|agent| agent.instance_id == input.agent_id)
                .with_context(|| format!("mission agent not found: {}", input.agent_id))?;
            let harness = mission
                .harnesses
                .iter()
                .rev()
                .find(|harness| {
                    harness.task_id == input.task_id && harness.agent_id == agent.definition_id
                })
                .with_context(|| {
                    format!(
                        "mission harness not found for task {} and agent {}",
                        input.task_id, input.agent_id
                    )
                })?;
            let dry_run = input.dry_run.unwrap_or(false);
            let mut request = MissionExecutionRequest {
                idempotency_key: input.idempotency_key,
                mission_id: mission.id.clone(),
                workflow_id: mission.workflow_id.clone(),
                expected_mission_revision: mission.revision,
                task_id: input.task_id,
                agent_id: input.agent_id,
                executor_id: harness.runtime.clone(),
                worktree: mission.worktree.clone(),
                purpose: input.purpose,
                command: input.command,
                requested_evidence: input.evidence.unwrap_or_default(),
                approval: None,
                dry_run,
                allow_trusted_process_runtime: input.allow_trusted_process_runtime.unwrap_or(false),
            };
            let plan = plan_mission_execution(store, &request)?;
            if !dry_run {
                let approved_by = input
                    .approved_by
                    .as_deref()
                    .context("mission execution requires approved_by unless dry_run is true")?;
                request.approval = Some(build_mission_execution_approval(
                    &plan,
                    approved_by,
                    input.approval_ttl_seconds.unwrap_or(300),
                )?);
            }
            serde_json::to_value(execute_mission_command(store, request)?)?
        }
        "forge.mission.submit" => {
            let input: MissionSubmitInput = parse_input(input)?;
            serde_json::to_value(submit_mission(
                store,
                &input.mission_id,
                MissionSubmission {
                    idempotency_key: input.idempotency_key,
                    execution_receipt_id: input.receipt_id,
                    task_id: input.task_id,
                    agent_id: input.agent_id,
                    status: input.status.unwrap_or_else(|| "completed".to_string()),
                    summary: input.summary,
                    artifacts: input.artifacts.unwrap_or_default(),
                    validations: Vec::new(),
                    risks: input.risks.unwrap_or_default(),
                    followups: input.followups.unwrap_or_default(),
                    tests_passed: 0,
                    tests_failed: 0,
                },
            )?)?
        }
        "forge.mission.resume" => {
            let input: MissionIdInput = parse_input(input)?;
            serde_json::to_value(resume_mission(store, &input.mission_id)?)?
        }
        "forge.mission.execution.list" => {
            let input: MissionExecutionListInput = parse_input(input)?;
            serde_json::to_value(list_mission_execution_receipts(
                store,
                input.mission_id.as_deref(),
                input.task_id.as_deref(),
            )?)?
        }
        "forge.mission.execution.inspect" => {
            let input: MissionExecutionInspectInput = parse_input(input)?;
            serde_json::to_value(inspect_mission_execution_receipt(store, &input.receipt_id)?)?
        }
        "forge.mission.execution.reconcile" => {
            let input: MissionExecutionReconcileInput = parse_input(input)?;
            serde_json::to_value(reconcile_mission_execution(
                store,
                MissionExecutionReconcileRequest {
                    receipt_id: input.receipt_id,
                    outcome: input.outcome,
                    approved_by: input.approved_by,
                    reason: input.reason,
                    confirm_no_effect_retry: input.confirm_no_effect_retry,
                },
            )?)?
        }
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
        "forge.events.list" => {
            let input: WorkflowEventsInput = parse_input(input)?;
            serde_json::to_value(build_workflow_event_stream(
                store,
                &input.workflow_id,
                input.limit,
            )?)?
        }
        "forge.events.timeline" => {
            let input: EventTimelineInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(build_global_event_timeline_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.limit,
                input.after_sequence,
                &operating_context,
            )?)?
        }
        "forge.ops.snapshot" => {
            let input: OpsSnapshotInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let project_root = input.project_root.as_deref().map(PathBuf::from);
            serde_json::to_value(build_ops_snapshot_with_addon_dirs_and_project(
                store,
                &addon_dirs,
                project_root.as_deref(),
            )?)?
        }
        "forge.ops.addon_renderer_event" => {
            let input: OpsAddonRendererEventInput = parse_input(input)?;
            let workflow_id = input
                .workflow_id
                .or(input.workflow)
                .ok_or_else(|| anyhow::anyhow!("workflow_id is required"))?;
            let view_id = input
                .view_id
                .or(input.view)
                .ok_or_else(|| anyhow::anyhow!("view_id is required"))?;
            let addon_id = input.addon_id.or(input.addon);
            let actor = input.actor.unwrap_or_else(|| "mcp".to_string());
            let payload = input.payload.as_ref().map(Value::to_string);
            let addon_dirs = input
                .addon_dirs
                .unwrap_or_else(|| {
                    default_addon_dirs()
                        .into_iter()
                        .map(|path| path.to_string_lossy().to_string())
                        .collect()
                })
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            serde_json::to_value(record_addon_renderer_client_event(
                store,
                &addon_dirs,
                OpsAddonRendererClientEventInput {
                    workflow_id: &workflow_id,
                    addon_id: addon_id.as_deref(),
                    view_id: &view_id,
                    event_kind: &input.event_kind,
                    actor: &actor,
                    payload: payload.as_deref(),
                },
            )?)?
        }
        "forge.events.observability" => {
            let input: EventObservabilityInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let node_ref = input.node_ref.or(input.node);
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(build_event_observability_index_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                node_ref.as_deref(),
                addon_id.as_deref(),
                input.limit,
                input.after_sequence,
                &operating_context,
            )?)?
        }
        "forge.events.observability_history" => {
            let input: EventObservabilityHistoryInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let node_ref = input.node_ref.or(input.node);
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(build_event_observability_history_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                node_ref.as_deref(),
                addon_id.as_deref(),
                input.bucket.as_deref(),
                input.group_by.as_deref(),
                input.limit,
                input.after_sequence,
                &operating_context,
            )?)?
        }
        "forge.events.improvement_policy" => {
            let input: EventImprovementPolicyInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let node_ref = input.node_ref.or(input.node);
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(build_event_improvement_policy_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                node_ref.as_deref(),
                addon_id.as_deref(),
                input.min_events.or(input.min_event_count),
                input.min_duration_ms.or(input.min_total_duration_ms),
                input.min_retries.or(input.min_total_retry_count),
                input.min_context_pressure_bps,
                input.min_wait_seconds.or(input.min_total_wait_seconds),
                input.limit,
                input.after_sequence,
                &operating_context,
            )?)?
        }
        "forge.events.ingest" => {
            let input: InboundEventMcpIngestInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(ingest_inbound_event_with_context(
                store,
                InboundEventIngestInput {
                    origin: input.origin,
                    action: input.action,
                    data: input.data,
                },
                &operating_context,
            )?)?
        }
        "forge.events.inbox" => {
            let input: InboundEventInboxInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(list_inbound_event_inbox_for_context(
                store,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                &operating_context,
            )?)?
        }
        "forge.events.scan" => {
            let input: InboundEventScanInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(scan_inbound_event_inbox(
                store,
                &project_root,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                input.dispatch_activations.unwrap_or(false),
            )?)?
        }
        "forge.events.worker" => {
            let input: InboundEventWorkerLoopInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let stop_file = input.stop_file.map(PathBuf::from);
            serde_json::to_value(run_inbound_event_worker_loop(
                store,
                &project_root,
                InboundEventWorkerLoopOptions {
                    status: input.status.as_deref(),
                    limit: input.limit.unwrap_or(20),
                    max_cycles: input.max_cycles.unwrap_or(1),
                    interval_seconds: input.interval_seconds.unwrap_or(300),
                    idle_exit: input.idle_exit.unwrap_or(false),
                    dispatch_activations: input.dispatch_activations.unwrap_or(false),
                    stop_file: stop_file.as_deref(),
                },
            )?)?
        }
        "forge.events.service_plan" => {
            let input: EventServicePlanInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let service_kind = input
                .kind
                .or(input.service_kind)
                .unwrap_or_else(|| "worker".to_string());
            serde_json::to_value(build_event_service_plan(
                store,
                &project_root,
                &service_kind,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                input.max_cycles.unwrap_or(1),
                input.interval_seconds.unwrap_or(300),
                input.idle_exit.unwrap_or(false),
                input.dispatch_activations.unwrap_or(false),
                input.host.as_deref().unwrap_or("127.0.0.1"),
                input.port.unwrap_or(8787),
                input.path.as_deref().unwrap_or("/webhook"),
                input.origin.as_deref(),
                input.action.as_deref(),
                input.schema.as_deref(),
                input.route.unwrap_or(false),
                input.max_requests.unwrap_or(1),
                input.max_body_bytes.unwrap_or(65_536),
                input.hmac_secret_env.as_deref(),
                input
                    .signature_header
                    .as_deref()
                    .unwrap_or("X-Forge-Signature"),
                input.lease_seconds.unwrap_or(300),
                input.heartbeat_seconds.unwrap_or(60),
                input.backoff_initial_seconds.unwrap_or(5),
                input.backoff_max_seconds.unwrap_or(300),
                input.shutdown_grace_seconds.unwrap_or(30),
            )?)?
        }
        "forge.events.service_run" => {
            let input: EventServiceRunInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let service_kind = input
                .kind
                .or(input.service_kind)
                .unwrap_or_else(|| "worker".to_string());
            let normalized_kind = service_kind.trim();
            let stop_file = input.stop_file.map(PathBuf::from);
            if normalized_kind == "worker" {
                serde_json::to_value(run_event_worker_service(
                    store,
                    &project_root,
                    input.status.as_deref(),
                    input.limit.unwrap_or(20),
                    input.max_cycles.unwrap_or(1),
                    input.interval_seconds.unwrap_or(300),
                    input.idle_exit.unwrap_or(false),
                    input.dispatch_activations.unwrap_or(false),
                    stop_file.as_deref(),
                    input
                        .lease_owner
                        .as_deref()
                        .unwrap_or("forge.event_service_manager"),
                    input.lease_seconds.unwrap_or(300),
                    input.heartbeat_seconds.unwrap_or(60),
                )?)?
            } else if matches!(
                normalized_kind,
                "webhook_ingress" | "webhook-ingress" | "webhook"
            ) {
                serde_json::to_value(run_event_webhook_ingress_service(
                    store,
                    &project_root,
                    input.host.as_deref().unwrap_or("127.0.0.1"),
                    input.port.unwrap_or(8787),
                    input.path.as_deref().unwrap_or("/webhook"),
                    input.origin.as_deref(),
                    input.action.as_deref(),
                    input.schema.as_deref(),
                    input.route.unwrap_or(false),
                    input.max_requests.unwrap_or(1),
                    input.max_body_bytes.unwrap_or(65_536),
                    input.hmac_secret_env.as_deref(),
                    input
                        .signature_header
                        .as_deref()
                        .unwrap_or("X-Forge-Signature"),
                    stop_file.as_deref(),
                    input
                        .lease_owner
                        .as_deref()
                        .unwrap_or("forge.event_service_manager"),
                    input.lease_seconds.unwrap_or(300),
                    input.heartbeat_seconds.unwrap_or(60),
                )?)?
            } else {
                anyhow::bail!("unsupported event service kind for service_run: {normalized_kind}");
            }
        }
        "forge.events.service_supervise" => {
            let input: EventServiceSuperviseInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let service_kind = input
                .kind
                .or(input.service_kind)
                .unwrap_or_else(|| "worker".to_string());
            let stop_file = input.stop_file.map(PathBuf::from);
            serde_json::to_value(run_event_service_supervisor(
                store,
                &project_root,
                &service_kind,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                input.max_cycles.unwrap_or(1),
                input.interval_seconds.unwrap_or(300),
                input.idle_exit.unwrap_or(false),
                input.dispatch_activations.unwrap_or(false),
                input.host.as_deref().unwrap_or("127.0.0.1"),
                input.port.unwrap_or(8787),
                input.path.as_deref().unwrap_or("/webhook"),
                input.origin.as_deref(),
                input.action.as_deref(),
                input.schema.as_deref(),
                input.route.unwrap_or(false),
                input.max_requests.unwrap_or(1),
                input.max_body_bytes.unwrap_or(65_536),
                input.hmac_secret_env.as_deref(),
                input
                    .signature_header
                    .as_deref()
                    .unwrap_or("X-Forge-Signature"),
                stop_file.as_deref(),
                input
                    .lease_owner
                    .as_deref()
                    .unwrap_or("forge.event_service_supervisor"),
                input.lease_seconds.unwrap_or(300),
                input.heartbeat_seconds.unwrap_or(60),
                input.max_runs.unwrap_or(1),
                input.backoff_initial_seconds.unwrap_or(5),
                input.backoff_max_seconds.unwrap_or(300),
            )?)?
        }
        "forge.events.runtime_reconcile" => {
            let input: EventRuntimeReconcileInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let stop_file = input.stop_file.map(PathBuf::from);
            serde_json::to_value(run_event_runtime_reconcile(
                store,
                &project_root,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                input.service_limit.unwrap_or(20),
                input.execute.unwrap_or(false),
                input.max_cycles.unwrap_or(1),
                input.interval_seconds.unwrap_or(300),
                input.idle_exit.unwrap_or(false),
                input.dispatch_activations.unwrap_or(false),
                input.recover_stale_services.unwrap_or(false),
                stop_file.as_deref(),
                input
                    .lease_owner
                    .as_deref()
                    .unwrap_or("forge.event_runtime_reconcile"),
                input.lease_seconds.unwrap_or(300),
                input.heartbeat_seconds.unwrap_or(60),
                input.max_runs.unwrap_or(1),
                input.backoff_initial_seconds.unwrap_or(5),
                input.backoff_max_seconds.unwrap_or(300),
                input.scan_schedules.unwrap_or(false),
                input
                    .schedule_executor
                    .as_deref()
                    .unwrap_or("forge-runtime-scheduler"),
                input.schedule_max_workers.unwrap_or(1),
                input.schedule_ttl_seconds.unwrap_or(300),
            )?)?
        }
        "forge.events.runtime_daemon" => {
            let input: EventRuntimeReconcileInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let stop_file = input.stop_file.map(PathBuf::from);
            serde_json::to_value(run_event_runtime_daemon(
                store,
                &project_root,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                input.service_limit.unwrap_or(20),
                input.execute.unwrap_or(false),
                input.max_cycles.unwrap_or(1),
                input.interval_seconds.unwrap_or(300),
                input.idle_exit.unwrap_or(false),
                input.dispatch_activations.unwrap_or(false),
                input.continuous.unwrap_or(false),
                input.cycle_retention.unwrap_or(100),
                input.recover_stale_services.unwrap_or(false),
                stop_file.as_deref(),
                input
                    .lease_owner
                    .as_deref()
                    .unwrap_or("forge.event_runtime_daemon"),
                input.lease_seconds.unwrap_or(300),
                input.heartbeat_seconds.unwrap_or(60),
                input.max_runs.unwrap_or(1),
                input.backoff_initial_seconds.unwrap_or(5),
                input.backoff_max_seconds.unwrap_or(300),
                input.scan_schedules.unwrap_or(false),
                input
                    .schedule_executor
                    .as_deref()
                    .unwrap_or("forge-runtime-scheduler"),
                input.schedule_max_workers.unwrap_or(1),
                input.schedule_ttl_seconds.unwrap_or(300),
            )?)?
        }
        "forge.events.services" => {
            let input: EventServicesInput = parse_input(input)?;
            let service_kind = input.kind.or(input.service_kind);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(list_event_services(
                store,
                &project_root,
                service_kind.as_deref(),
                input.status.as_deref(),
                input.limit.unwrap_or(20),
            )?)?
        }
        "forge.events.services_recover" => {
            let input: EventServicesInput = parse_input(input)?;
            let service_kind = input.kind.or(input.service_kind);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(recover_stale_event_services(
                store,
                &project_root,
                service_kind.as_deref(),
                input.limit.unwrap_or(20),
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.events.adapters" => {
            let input: EventAdaptersInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            serde_json::to_value(list_addon_event_adapters(
                &catalog,
                addon_id.as_deref(),
                input.transport.as_deref(),
                input.direction.as_deref(),
            ))?
        }
        "forge.events.emit" => {
            let input: EventEmitInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(emit_event_egress(
                store,
                &catalog,
                EventEgressEmitInput {
                    adapter_id: input.adapter_id,
                    addon_id,
                    event_type: input.event_type,
                    action: input.action,
                    origin: input.origin.unwrap_or_else(|| "forge".to_string()),
                    payload: input.payload.unwrap_or_else(|| json!({})),
                    dry_run: input.dry_run.unwrap_or(false),
                },
                &operating_context,
            )?)?
        }
        "forge.events.route" => {
            let input: InboundEventRouteInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(route_inbound_event(store, &input.event_id, &project_root)?)?
        }
        "forge.events.dispatch_activations" => {
            let input: InboundEventActivationDispatchInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(dispatch_inbound_event_activations(
                store,
                &input.event_id,
                &project_root,
                input.dry_run.unwrap_or(false),
            )?)?
        }
        "forge.cost.ledger" => {
            let input: CostLedgerInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(build_cost_ledger_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                &operating_context,
            )?)?
        }
        "forge.cost.materialize" => {
            let input: CostLedgerMaterializeInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(materialize_cost_ledger_index_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.source_kind.as_deref(),
                addon_id.as_deref(),
                input.limit,
                &operating_context,
            )?)?
        }
        "forge.cost.incremental" => {
            let input: CostLedgerIncrementalInput = parse_input(input)?;
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(materialize_cost_ledger_incremental_for_context(
                store,
                input.after_sequence,
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.source_kind.as_deref(),
                addon_id.as_deref(),
                input.limit,
                &operating_context,
            )?)?
        }
        "forge.cost.history" => {
            let input: CostLedgerHistoryInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(build_cost_ledger_history_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.source_kind.as_deref(),
                addon_id.as_deref(),
                input.bucket.as_deref(),
                input.group_by.as_deref(),
                input.limit,
                &operating_context,
            )?)?
        }
        "forge.cost.maintain" => {
            let input: CostLedgerMaintainInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(maintain_cost_ledger_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.source_kind.as_deref(),
                addon_id.as_deref(),
                input.bucket.as_deref(),
                input.group_by.as_deref(),
                input.limit,
                input.retention_days,
                &operating_context,
            )?)?
        }
        "forge.cost.daemon" => {
            let input: CostLedgerDaemonInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(run_cost_ledger_daemon_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.source_kind.as_deref(),
                addon_id.as_deref(),
                input.bucket.as_deref(),
                input.group_by.as_deref(),
                input.limit,
                input.retention_days,
                input.max_cycles.unwrap_or(1),
                input.interval_seconds.unwrap_or(300),
                input.idle_exit.unwrap_or(false),
                &origin,
                &operating_context,
            )?)?
        }
        "forge.cost.retention" => {
            let input: CostLedgerRetentionInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let operating_context = load_project_operating_context(&project_root)?;
            serde_json::to_value(apply_cost_ledger_retention_for_context(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.source_kind.as_deref(),
                addon_id.as_deref(),
                input.retention_days,
                input.limit,
                input.apply.unwrap_or(false),
                input.approved_by.as_deref(),
                input.reason.as_deref(),
                input.confirm.unwrap_or(false),
                &origin,
                &operating_context,
            )?)?
        }
        "forge.improve.candidates" => {
            let input: ImprovementCandidatesInput = parse_input(input)?;
            serde_json::to_value(rank_improvement_candidates_with_filter(
                store,
                input.limit.unwrap_or(20),
                ImprovementCandidateFilter {
                    workflow_ids: input.workflow_ids.unwrap_or_default(),
                    goal_contains: input.goal_contains.unwrap_or_default(),
                },
            )?)?
        }
        "forge.improve.apply_event_policy" => {
            let input: ImproveApplyEventPolicyInput = parse_input(input)?;
            let workflow_id = input
                .workflow_id
                .or(input.workflow)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("forge.improve.apply_event_policy requires workflow_id")
                })?;
            let recommendation_id = input.recommendation_id.or(input.recommendation);
            let recommended_policy = input.recommended_policy.or(input.policy);
            serde_json::to_value(apply_event_improvement_policy(
                store,
                &workflow_id,
                recommendation_id.as_deref(),
                recommended_policy.as_deref(),
                input.apply.unwrap_or(false),
                input.approved_by.as_deref(),
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.improve.benchmark_event_policy" => {
            let input: ImproveBenchmarkEventPolicyInput = parse_input(input)?;
            let workflow_id = input
                .workflow_id
                .or(input.workflow)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("forge.improve.benchmark_event_policy requires workflow_id")
                })?;
            let recommendation_id = input.recommendation_id.or(input.recommendation);
            let recommended_policy = input.recommended_policy.or(input.policy);
            serde_json::to_value(benchmark_event_improvement_policy(
                store,
                &workflow_id,
                recommendation_id.as_deref(),
                recommended_policy.as_deref(),
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.improve.promote_event_policy" => {
            let input: ImprovePromoteEventPolicyInput = parse_input(input)?;
            let workflow_id = input
                .workflow_id
                .or(input.workflow)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("forge.improve.promote_event_policy requires workflow_id")
                })?;
            let recommendation_id = input.recommendation_id.or(input.recommendation);
            let recommended_policy = input.recommended_policy.or(input.policy);
            serde_json::to_value(promote_event_improvement_policy(
                store,
                &workflow_id,
                recommendation_id.as_deref(),
                recommended_policy.as_deref(),
                input.approved_by.as_deref(),
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.cli.create" => {
            let input: CliFactoryCreateMcpInput = parse_input(input)?;
            let commands = input.commands.or(input.command).unwrap_or_default();
            let compound_commands = input
                .compound_commands
                .or(input.compound_command)
                .unwrap_or_default();
            serde_json::to_value(create_cli_factory_plan(
                store,
                CliFactoryCreateInput {
                    name: input.name,
                    goal: input.goal.unwrap_or_default(),
                    source: input.source,
                    commands,
                    compound_commands,
                },
            )?)?
        }
        "forge.interactive.home" => {
            let input: InteractiveHomeInput = if input.is_null() {
                InteractiveHomeInput { project_root: None }
            } else {
                parse_input(input)?
            };
            serde_json::to_value(build_interactive_home_with_options(
                store,
                InteractiveHomeOptions {
                    project_root: input.project_root.map(PathBuf::from),
                },
            )?)?
        }
        "forge.interactive.guided_cockpit" => {
            serde_json::to_value(build_interactive_guided_cockpit(store)?)?
        }
        "forge.interactive.ui_composition" => {
            let input: InteractiveHomeInput = if input.is_null() {
                InteractiveHomeInput { project_root: None }
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_interactive_ui_composition(store, project_root)?)?
        }
        "forge.interactive.readiness" => serde_json::to_value(build_interactive_readiness(store)?)?,
        "forge.interactive.operational_cockpit" => {
            serde_json::to_value(build_interactive_operational_cockpit(store)?)?
        }
        "forge.interactive.improvement_loop" => {
            serde_json::to_value(build_interactive_improvement_loop(store)?)?
        }
        "forge.interactive.architecture" => {
            let input: InteractiveHomeInput = if input.is_null() {
                InteractiveHomeInput { project_root: None }
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_interactive_architecture_compass(store, project_root)?)?
        }
        "forge.interactive.release_gates" => {
            let input: MilestoneStatusInput = if input.is_null() {
                MilestoneStatusInput {
                    version: None,
                    project_root: None,
                }
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_interactive_release_gates(
                store,
                input.version.as_deref().unwrap_or("0.5"),
                project_root.as_deref(),
            )?)?
        }
        "forge.interactive.harness" => {
            let input: InteractiveHarnessInput = if input.is_null() {
                InteractiveHarnessInput::default()
            } else {
                parse_input(input)?
            };
            let mut options = InteractiveHarnessOptions::default_for_current_dir();
            if let Some(executor) = input.executor {
                options.executor = executor;
            }
            if let Some(shim_dir) = input.shim_dir {
                options.shim_dir = PathBuf::from(shim_dir);
            }
            if let Some(project_root) = input.project_root {
                options.project_root = Some(PathBuf::from(project_root));
            }
            options.forge_first = input.forge_first.unwrap_or(false);
            options.observe_only = input.observe_only.unwrap_or(false);
            options.workflow_id = input.workflow_id.or(input.workflow);
            options.task_id = input.task_id.or(input.task);
            options.run_id = input.run_id.or(input.run);
            options.context_budget = input.context_budget;
            options.token_headroom = input.token_headroom;
            serde_json::to_value(build_interactive_harness(store, options)?)?
        }
        "forge.interactive.sessions" => {
            let input: BrainSessionsInput = if input.is_null() {
                BrainSessionsInput::default()
            } else {
                parse_input(input)?
            };
            serde_json::to_value(build_interactive_sessions(
                store,
                InteractiveSessionsOptions {
                    provider_id: input.provider_id.or(input.provider),
                    lifecycle_state: input.lifecycle_state.or(input.state),
                    readiness: input.readiness,
                },
            )?)?
        }
        "forge.interactive.command_palette" => {
            let input: InteractiveCommandPaletteInput = if input.is_null() {
                InteractiveCommandPaletteInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_interactive_command_palette_for_project(
                store,
                input.query.as_deref(),
                project_root.as_deref(),
            )?)?
        }
        "forge.interactive.action_registry" => {
            let input: InteractiveCommandPaletteInput = if input.is_null() {
                InteractiveCommandPaletteInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_interactive_action_registry_for_project(
                store,
                input.query.as_deref(),
                project_root.as_deref(),
            )?)?
        }
        "forge.interactive.action_invocation" => {
            let input: InteractiveActionInvocationInput = if input.is_null() {
                InteractiveActionInvocationInput::default()
            } else {
                parse_input(input)?
            };
            let action_id = input.action_id.or(input.action).ok_or_else(|| {
                anyhow::anyhow!("forge.interactive.action_invocation requires action_id")
            })?;
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_interactive_action_invocation_for_project(
                store,
                &action_id,
                project_root.as_deref(),
            )?)?
        }
        "forge.interactive.action_dispatch" => {
            let input: InteractiveActionDispatchInput = if input.is_null() {
                InteractiveActionDispatchInput::default()
            } else {
                parse_input(input)?
            };
            let action_id = input.action_id.or(input.action).ok_or_else(|| {
                anyhow::anyhow!("forge.interactive.action_dispatch requires action_id")
            })?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(dispatch_interactive_action_hooks_for_project(
                store,
                &action_id,
                project_root.as_deref(),
                &origin,
                input.payload,
            )?)?
        }
        "forge.interactive.autocomplete" => {
            let input: InteractiveAutocompleteInput = if input.is_null() {
                InteractiveAutocompleteInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_interactive_autocomplete_for_project(
                store,
                input.input.as_deref().unwrap_or_default(),
                project_root.as_deref(),
            )?)?
        }
        "forge.interactive.patch_workbench" => {
            serde_json::to_value(build_interactive_patch_workbench(store)?)?
        }
        "forge.interactive.permissions" => {
            serde_json::to_value(build_interactive_permissions(store)?)?
        }
        "forge.interactive.addon_capabilities" => {
            let input: InteractiveProjectRootInput = if input.is_null() {
                InteractiveProjectRootInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.as_deref().map(PathBuf::from);
            serde_json::to_value(build_interactive_addon_capabilities_for_project(
                store,
                project_root.as_deref(),
            ))?
        }
        "forge.interactive.core_boundary" => {
            let input: InteractiveProjectRootInput = if input.is_null() {
                InteractiveProjectRootInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.as_deref().map(PathBuf::from);
            serde_json::to_value(build_interactive_core_boundary_for_project(
                store,
                project_root.as_deref(),
            ))?
        }
        "forge.interactive.identity" => {
            let input: InteractiveIdentityInput = if input.is_null() {
                InteractiveIdentityInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(build_interactive_identity(store, &project_root)?)?
        }
        "forge.interactive.operating_context" => {
            let input: InteractiveIdentityInput = if input.is_null() {
                InteractiveIdentityInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(build_interactive_operating_context(store, &project_root)?)?
        }
        "forge.interactive.task_board" => {
            serde_json::to_value(build_interactive_task_board(store)?)?
        }
        "forge.interactive.workflow_mutation" => {
            serde_json::to_value(build_interactive_workflow_mutation(store)?)?
        }
        "forge.interactive.replacement_cli" => {
            let input: InteractiveHomeInput = if input.is_null() {
                InteractiveHomeInput { project_root: None }
            } else {
                parse_input(input)?
            };
            serde_json::to_value(build_interactive_replacement_cli_with_options(
                store,
                InteractiveReplacementCliOptions {
                    project_root: input.project_root.map(PathBuf::from),
                },
            )?)?
        }
        "forge.interactive.multimodal_runtime" => {
            let input: InteractiveMultimodalRuntimeInput = if input.is_null() {
                InteractiveMultimodalRuntimeInput::default()
            } else {
                parse_input(input)?
            };
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            serde_json::to_value(build_interactive_multimodal_runtime(
                store,
                &project_root,
                input.enable_experimental.unwrap_or(false),
            )?)?
        }
        "forge.interactive.workflow_sidebar" => {
            serde_json::to_value(build_interactive_workflow_sidebar(store)?)?
        }
        "forge.interactive.artifacts" => serde_json::to_value(build_interactive_artifacts(store)?)?,
        "forge.interactive.token_usage" => {
            serde_json::to_value(build_interactive_token_usage(store)?)?
        }
        "forge.interactive.workflow_dag" => {
            serde_json::to_value(build_interactive_workflow_dag(store)?)?
        }
        "forge.interactive.schedules" => serde_json::to_value(build_interactive_schedules(store))?,
        "forge.interactive.context_memory" => {
            let input: MemoryPolicyInput = if input.is_null() {
                MemoryPolicyInput { project_root: None }
            } else {
                parse_input(input)?
            };
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            serde_json::to_value(build_interactive_context_memory(store, &project_root)?)?
        }
        "forge.interactive.structured_logs" => {
            serde_json::to_value(build_interactive_structured_logs(store)?)?
        }
        "forge.brain_router" => serde_json::to_value(load_executors(store)?.brain_router)?,
        "forge.sessions" => {
            let input: BrainSessionsInput = if input.is_null() {
                BrainSessionsInput::default()
            } else {
                parse_input(input)?
            };
            let report = load_executors(store)?;
            serde_json::to_value(build_brain_sessions_report_with_options(
                store,
                &report.brain_router,
                BrainSessionsReportOptions {
                    provider_id: input.provider_id.or(input.provider),
                    lifecycle_state: input.lifecycle_state.or(input.state),
                    readiness: input.readiness,
                },
            )?)?
        }
        "forge.session.history" => {
            let input: BrainSessionHistoryInput = parse_input(input)?;
            let session_id = input
                .session_id
                .or(input.session)
                .ok_or_else(|| anyhow::anyhow!("forge.session.history requires session_id"))?;
            let report = load_executors(store)?;
            serde_json::to_value(build_brain_session_history_report(
                store,
                &report.brain_router,
                &session_id,
            )?)?
        }
        "forge.session.lifecycle" => {
            let input: BrainSessionLifecycleInput = parse_input(input)?;
            let session_id = input
                .session_id
                .or(input.session)
                .ok_or_else(|| anyhow::anyhow!("forge.session.lifecycle requires session_id"))?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let report = load_executors(store)?;
            serde_json::to_value(record_brain_session_lifecycle(
                store,
                &report.brain_router,
                BrainSessionLifecycleOptions {
                    session_id: &session_id,
                    state: &input.state,
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    run_id: run_id.as_deref(),
                    origin: input.origin.as_deref().unwrap_or("mcp"),
                    note: input.note.as_deref(),
                },
            )?)?
        }
        "forge.shell.launch_plan" => {
            let input: ShellLaunchPlanInput = parse_input(input)?;
            let executor = input.executor.or(input.brain);
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let report = load_executors(store)?;
            serde_json::to_value(build_shell_launch_plan(
                &report.brain_router,
                ShellLaunchPlanOptions {
                    executor_filter: executor,
                    workflow_id,
                    task_id,
                    run_id,
                    context_budget: input.context_budget,
                    ttl_seconds: input.ttl_seconds,
                },
            ))?
        }
        "forge.shell.record_plan" => {
            let input: ShellLaunchPlanInput = parse_input(input)?;
            let executor = input.executor.or(input.brain);
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let report = load_executors(store)?;
            serde_json::to_value(record_shell_session_plan(
                store,
                &report.brain_router,
                ShellLaunchPlanOptions {
                    executor_filter: executor,
                    workflow_id,
                    task_id,
                    run_id,
                    context_budget: input.context_budget,
                    ttl_seconds: input.ttl_seconds,
                },
                &origin,
            )?)?
        }
        "forge.addons.installed" => serde_json::to_value(list_installed_addons(store)?)?,
        "forge.addons.capabilities" => {
            let input: AddonCapabilityIndexInput = parse_input(input)?;
            let addon_id = input.addon_id.or(input.addon);
            let capability_id = input.capability_id.or(input.capability);
            serde_json::to_value(list_addon_capability_index(
                store,
                addon_id.as_deref(),
                capability_id.as_deref(),
                input.lifecycle.as_deref(),
            )?)?
        }
        "forge.addons.observability" => {
            let input: AddonObservabilityInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            serde_json::to_value(addon_observability_report(
                store,
                &catalog,
                addon_id.as_deref(),
                input.lifecycle.as_deref(),
                input.dispatch_limit.unwrap_or(1000),
            )?)?
        }
        "forge.addons.contracts" => {
            let input: AddonRuntimeContractInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let capability_id = input.capability_id.or(input.capability);
            serde_json::to_value(list_addon_runtime_contracts(
                &catalog,
                addon_id.as_deref(),
                input.contract_type.as_deref(),
                capability_id.as_deref(),
                input.lifecycle.as_deref(),
            ))?
        }
        "forge.addons.planners" => {
            let input: AddonPlannerRegistryInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let capability_id = input.capability_id.or(input.capability);
            let workflow_extension_id = input.workflow_extension_id.or(input.workflow_extension);
            serde_json::to_value(list_addon_planner_registry(
                &catalog,
                addon_id.as_deref(),
                capability_id.as_deref(),
                workflow_extension_id.as_deref(),
                input.lifecycle.as_deref(),
            ))?
        }
        "forge.addons.contract_policy" => {
            let input: AddonRuntimeContractInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract);
            let capability_id = input.capability_id.or(input.capability);
            serde_json::to_value(evaluate_addon_runtime_contract_policy(
                &catalog,
                addon_id.as_deref(),
                contract_id.as_deref(),
                input.contract_type.as_deref(),
                capability_id.as_deref(),
                input.lifecycle.as_deref(),
            ))?
        }
        "forge.addons.dispatch_contract" => {
            let input: AddonRuntimeDispatchInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.dispatch_contract requires contract_id")
            })?;
            serde_json::to_value(enqueue_addon_runtime_contract_dispatch(
                store,
                &catalog,
                addon_id.as_deref(),
                &contract_id,
                input.input.unwrap_or_else(|| serde_json::json!({})),
                input.source.as_deref().unwrap_or("mcp"),
                input.dry_run.unwrap_or(false),
            )?)?
        }
        "forge.addons.dispatch_planner" => {
            let input: AddonPlannerDispatchInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.dispatch_planner requires contract_id")
            })?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let constraints = input.constraints.unwrap_or_default();
            serde_json::to_value(enqueue_addon_planner_dispatch(
                store,
                &catalog,
                AddonPlannerDispatchRequest {
                    addon_id: addon_id.as_deref(),
                    contract_id: &contract_id,
                    goal: &input.goal,
                    constraints: &constraints,
                    workflow_id: workflow_id.as_deref(),
                    task_id: task_id.as_deref(),
                    context: input.context.unwrap_or_else(|| serde_json::json!({})),
                    source: input.source.as_deref().unwrap_or("mcp"),
                    dry_run: input.dry_run.unwrap_or(false),
                },
            )?)?
        }
        "forge.addons.execute_planner" => {
            let input: AddonPlannerDispatchInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_planner requires contract_id")
            })?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_planner requires worker_id")
            })?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let constraints = input.constraints.unwrap_or_default();
            serde_json::to_value(execute_addon_planning_strategy(
                store,
                &catalog,
                AddonPlanningStrategyRequest {
                    dispatch: AddonPlannerDispatchRequest {
                        addon_id: addon_id.as_deref(),
                        contract_id: &contract_id,
                        goal: &input.goal,
                        constraints: &constraints,
                        workflow_id: workflow_id.as_deref(),
                        task_id: task_id.as_deref(),
                        context: input.context.unwrap_or_else(|| serde_json::json!({})),
                        source: input.source.as_deref().unwrap_or("mcp"),
                        dry_run: input.dry_run.unwrap_or(false),
                    },
                    worker_id: &worker_id,
                    lease_seconds: input.lease_seconds.unwrap_or(300),
                },
            )?)?
        }
        "forge.addons.execute_validator" => {
            let input: AddonValidatorExecutionInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_validator requires contract_id")
            })?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_validator requires worker_id")
            })?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            serde_json::to_value(execute_addon_validator(
                store,
                &catalog,
                AddonValidatorExecutionRequest {
                    dispatch: AddonValidatorDispatchRequest {
                        addon_id: addon_id.as_deref(),
                        contract_id: &contract_id,
                        subject: &input.subject,
                        workflow_id: workflow_id.as_deref(),
                        task_id: task_id.as_deref(),
                        input: input.input.unwrap_or_else(|| serde_json::json!({})),
                        context: input.context.unwrap_or_else(|| serde_json::json!({})),
                        source: input.source.as_deref().unwrap_or("mcp"),
                        dry_run: input.dry_run.unwrap_or(false),
                    },
                    worker_id: &worker_id,
                    lease_seconds: input.lease_seconds.unwrap_or(300),
                },
            )?)?
        }
        "forge.addons.apply_validator_outcome" => {
            let input: AddonValidatorOutcomeApplyInput = parse_input(input)?;
            let dispatch_id = input.dispatch_id.or(input.dispatch).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.apply_validator_outcome requires dispatch_id")
            })?;
            let workflow_id = input.workflow_id.or(input.workflow).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.apply_validator_outcome requires workflow_id")
            })?;
            let task_id = input.task_id.or(input.task).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.apply_validator_outcome requires task_id")
            })?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(apply_addon_validator_outcome(
                store,
                AddonValidatorOutcomeApplyRequest {
                    dispatch_id: &dispatch_id,
                    workflow_id: &workflow_id,
                    task_id: &task_id,
                    expected_revision: input.expected_revision,
                    origin: &origin,
                },
            )?)?
        }
        "forge.addons.execute_executor" => {
            let input: AddonExecutorExecutionInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_executor requires contract_id")
            })?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_executor requires worker_id")
            })?;
            let task_ref = input.task_ref.or(input.task).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_executor requires task_ref")
            })?;
            let workflow_id = input.workflow_id.or(input.workflow);
            serde_json::to_value(execute_addon_executor(
                store,
                &catalog,
                AddonExecutorExecutionRequest {
                    dispatch: AddonExecutorDispatchRequest {
                        addon_id: addon_id.as_deref(),
                        contract_id: &contract_id,
                        task_ref: &task_ref,
                        input: input.input.unwrap_or_else(|| serde_json::json!({})),
                        context: input.context.unwrap_or_else(|| serde_json::json!({})),
                        source: input.source.as_deref().unwrap_or("mcp"),
                        dry_run: input.dry_run.unwrap_or(false),
                    },
                    worker_id: &worker_id,
                    lease_seconds: input.lease_seconds.unwrap_or(300),
                    workflow_id: workflow_id.as_deref(),
                },
            )?)?
        }
        "forge.addons.execute_handoff" => {
            let input: AddonHandoffExecutionInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_handoff requires contract_id")
            })?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_handoff requires worker_id")
            })?;
            let handoff_ref = input.handoff_ref.or(input.handoff).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_handoff requires handoff_ref")
            })?;
            serde_json::to_value(execute_addon_handoff(
                store,
                &catalog,
                AddonHandoffExecutionRequest {
                    dispatch: AddonHandoffDispatchRequest {
                        addon_id: addon_id.as_deref(),
                        contract_id: &contract_id,
                        handoff_ref: &handoff_ref,
                        input: input.input.unwrap_or_else(|| serde_json::json!({})),
                        context: input.context.unwrap_or_else(|| serde_json::json!({})),
                        source: input.source.as_deref().unwrap_or("mcp"),
                        dry_run: input.dry_run.unwrap_or(false),
                    },
                    worker_id: &worker_id,
                    lease_seconds: input.lease_seconds.unwrap_or(300),
                },
            )?)?
        }
        "forge.addons.dispatches" => {
            let input: AddonRuntimeDispatchListInput = parse_input(input)?;
            let addon_id = input.addon_id.or(input.addon);
            let contract_id = input.contract_id.or(input.contract);
            serde_json::to_value(list_addon_runtime_contract_dispatches(
                store,
                addon_id.as_deref(),
                contract_id.as_deref(),
                input.status.as_deref(),
                input.limit.unwrap_or(20),
            )?)?
        }
        "forge.addons.run_dispatch" => {
            let input: AddonRuntimeDispatchRunInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let dispatch_id = input
                .dispatch_id
                .or(input.dispatch)
                .ok_or_else(|| anyhow::anyhow!("forge.addons.run_dispatch requires dispatch_id"))?;
            serde_json::to_value(run_addon_runtime_contract_dispatch(
                store,
                &catalog,
                &dispatch_id,
                input.worker_id.or(input.worker).as_deref().unwrap_or("mcp"),
                input.dry_run.unwrap_or(false),
            )?)?
        }
        "forge.addons.dispatch_worker" => {
            let input: AddonRuntimeDispatchWorkerInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            serde_json::to_value(run_addon_runtime_contract_dispatch_worker(
                store,
                &catalog,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                input.worker_id.or(input.worker).as_deref().unwrap_or("mcp"),
                input.dry_run.unwrap_or(false),
            )?)?
        }
        "forge.addons.execute_dispatch" => {
            let input: AddonRuntimeDispatchRunInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let dispatch_id = input.dispatch_id.or(input.dispatch).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_dispatch requires dispatch_id")
            })?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.execute_dispatch requires worker_id")
            })?;
            serde_json::to_value(execute_addon_runtime_contract_dispatch(
                store,
                &catalog,
                &dispatch_id,
                &worker_id,
                input.lease_seconds.unwrap_or(300),
                input.dry_run.unwrap_or(false),
            )?)?
        }
        "forge.addons.claim_dispatch" => {
            let input: AddonRuntimeDispatchClaimInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let dispatch_id = input.dispatch_id.or(input.dispatch).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.claim_dispatch requires dispatch_id")
            })?;
            let worker_id = input
                .worker_id
                .or(input.worker)
                .ok_or_else(|| anyhow::anyhow!("forge.addons.claim_dispatch requires worker_id"))?;
            serde_json::to_value(claim_addon_runtime_contract_dispatch(
                store,
                &catalog,
                &dispatch_id,
                &worker_id,
                input.lease_seconds.unwrap_or(300),
                input.dry_run.unwrap_or(false),
            )?)?
        }
        "forge.addons.complete_dispatch" => {
            let input: AddonRuntimeDispatchCompleteInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let dispatch_id = input.dispatch_id.or(input.dispatch).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.complete_dispatch requires dispatch_id")
            })?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.complete_dispatch requires worker_id")
            })?;
            serde_json::to_value(complete_addon_runtime_contract_dispatch(
                store,
                &catalog,
                AddonRuntimeContractCompletionRequest {
                    dispatch_id: &dispatch_id,
                    worker_id: &worker_id,
                    completion_status: input.status.as_deref().unwrap_or("completed"),
                    result: input.result.unwrap_or_else(|| serde_json::json!({})),
                    signature: input.signature.as_deref(),
                    attestation: input.attestation.unwrap_or_else(|| serde_json::json!({})),
                    dry_run: input.dry_run.unwrap_or(false),
                },
            )?)?
        }
        "forge.addons.register_worker" => {
            let input: AddonRuntimeWorkerRegisterInput = parse_input(input)?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.register_worker requires worker_id")
            })?;
            serde_json::to_value(register_addon_runtime_worker(
                store,
                AddonRuntimeWorkerRegistrationRequest {
                    worker_id: &worker_id,
                    runtime: &input.runtime,
                    status: input.status.as_deref().unwrap_or("available"),
                    trust_level: input.trust_level.as_deref().unwrap_or("local"),
                    source: input.source.as_deref().unwrap_or("mcp"),
                    data: input.data.unwrap_or_else(|| serde_json::json!({})),
                    rotation_approved_by: input.rotation_approved_by.as_deref(),
                    rotation_reason: input.rotation_reason.as_deref(),
                },
            )?)?
        }
        "forge.addons.workers" => {
            let input: AddonRuntimeWorkerListInput = parse_input(input)?;
            serde_json::to_value(list_addon_runtime_workers(
                store,
                input.runtime.as_deref(),
                input.status.as_deref(),
                input.trust_level.as_deref(),
                input.limit.unwrap_or(20),
            )?)?
        }
        "forge.addons.views" => {
            let input: AddonViewsInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let addon_id = input.addon_id.or(input.addon);
            serde_json::to_value(list_addon_views(
                &catalog,
                addon_id.as_deref(),
                input.surface.as_deref(),
                input.lifecycle.as_deref(),
            ))?
        }
        "forge.addons.permissions" => {
            let input: AddonPermissionAuthorizationInput = parse_input(input)?;
            let addon_id = input.addon_id.or(input.addon);
            let permission_id = input.permission_id.or(input.permission);
            serde_json::to_value(list_addon_permission_authorizations(
                store,
                addon_id.as_deref(),
                permission_id.as_deref(),
                input.status.as_deref(),
            )?)?
        }
        "forge.addons.authorize_permission" => {
            let input: AddonPermissionMutationInput = parse_input(input)?;
            let addon_id = input.addon_id.or(input.addon).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.authorize_permission requires addon_id")
            })?;
            let permission_id = input.permission_id.or(input.permission).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.authorize_permission requires permission_id")
            })?;
            serde_json::to_value(authorize_addon_permission(
                store,
                &addon_id,
                &permission_id,
                input.risk.as_deref().unwrap_or("medium"),
                input.approved_by.as_deref().unwrap_or("human"),
                input.source.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.addons.revoke_permission" => {
            let input: AddonPermissionMutationInput = parse_input(input)?;
            let addon_id = input.addon_id.or(input.addon).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.revoke_permission requires addon_id")
            })?;
            let permission_id = input.permission_id.or(input.permission).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.revoke_permission requires permission_id")
            })?;
            serde_json::to_value(revoke_addon_permission(
                store,
                &addon_id,
                &permission_id,
                input.approved_by.as_deref().unwrap_or("human"),
                input.source.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.addons.catalog" => {
            let input: AddonCatalogInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            serde_json::to_value(load_addon_catalog_from_store(store, &addon_dirs)?)?
        }
        "forge.addons.resolve" => {
            let input: AddonResolveInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let mut registry_sources = input.registry_sources.unwrap_or_default();
            if let Some(source) = input.registry_source {
                registry_sources.push(source);
            }
            let registry_cache_dir = input.registry_cache_dir.as_deref().map(PathBuf::from);
            let registry_lock_path = input
                .registry_lock
                .or(input.registry_lock_path)
                .map(PathBuf::from);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            let report = if registry_sources.is_empty() {
                resolve_goal_capabilities_with_store(store, &input.goal, &catalog)?
            } else {
                resolve_goal_capabilities_with_registry_sync(
                    store,
                    &input.goal,
                    &catalog,
                    CapabilityRegistrySyncInput {
                        registry_sources: &registry_sources,
                        cache_dir: registry_cache_dir.as_deref(),
                        allow_remote: input.allow_remote_registry.unwrap_or(false),
                        max_bytes: input.registry_max_bytes.unwrap_or(10 * 1024 * 1024),
                        max_packages: input.registry_max_packages.unwrap_or(50),
                        lock_path: registry_lock_path.as_deref(),
                    },
                )?
            };
            serde_json::to_value(report)?
        }
        "forge.addons.validate" => {
            let input: AddonCatalogInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
            serde_json::to_value(validate_addon_catalog(&catalog))?
        }
        "forge.addons.lifecycle_plan" => {
            let input: AddonLifecyclePlanInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let addon_id = input.id.or(input.addon_id);
            let manifest = input.manifest.map(PathBuf::from);
            let package_path = input.package.or(input.package_path).map(PathBuf::from);
            serde_json::to_value(plan_addon_lifecycle(
                store,
                &input.action,
                addon_id.as_deref(),
                manifest.as_deref(),
                package_path.as_deref(),
                &addon_dirs,
            )?)?
        }
        "forge.addons.install" => {
            let input: AddonInstallInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            serde_json::to_value(install_addon(
                store,
                &PathBuf::from(input.manifest),
                &addon_dirs,
            )?)?
        }
        "forge.addons.package" => {
            let input: AddonPackageInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let package_path = input.package_path.as_deref().map(PathBuf::from);
            let manifest_path = PathBuf::from(input.manifest);
            serde_json::to_value(package_addon(
                store,
                AddonPackageRequest {
                    manifest_path: &manifest_path,
                    addon_dirs: &addon_dirs,
                    repository: input.repository.as_deref(),
                    channel: input.channel.as_deref().unwrap_or("stable"),
                    signature: input.signature.as_deref(),
                    public_key: input.public_key.as_deref(),
                    package_path: package_path.as_deref(),
                },
            )?)?
        }
        "forge.addons.trust_key" => {
            let input: AddonTrustKeyInput = parse_input(input)?;
            serde_json::to_value(trust_addon_package_key(
                store,
                AddonTrustKeyRequest {
                    repository: &input.repository,
                    channel: input.channel.as_deref().unwrap_or("stable"),
                    public_key: &input.public_key,
                    trust_level: input.trust_level.as_deref().unwrap_or("trusted"),
                    approved_by: input.approved_by.as_deref().unwrap_or("human"),
                    source: input.source.as_deref().unwrap_or("mcp"),
                    data: input.data.unwrap_or_else(|| serde_json::json!({})),
                },
            )?)?
        }
        "forge.addons.trust_store" => {
            let input: AddonTrustStoreInput = parse_input(input)?;
            serde_json::to_value(list_addon_trust_store(
                store,
                input.repository.as_deref(),
                input.channel.as_deref(),
                input.public_key.as_deref(),
                input.status.as_deref(),
                input.limit.unwrap_or(20),
            )?)?
        }
        "forge.addons.publish_package" => {
            let input: AddonPackagePathInput = parse_input(input)?;
            let package = input
                .package
                .or(input.package_path)
                .with_context(|| "package or package_path is required")?;
            serde_json::to_value(publish_addon_package(
                store,
                &PathBuf::from(package),
                input.source.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.addons.fetch_package" => {
            let input: AddonPackageFetchInput = parse_input(input)?;
            let cache_dir = input.cache_dir.as_deref().map(PathBuf::from);
            let lock_path = input.lock.or(input.lock_path).map(PathBuf::from);
            serde_json::to_value(fetch_addon_package(
                store,
                &input.source,
                cache_dir.as_deref(),
                input.expected_sha256.as_deref(),
                input.allow_remote.unwrap_or(false),
                input.max_bytes.unwrap_or(10 * 1024 * 1024),
                lock_path.as_deref(),
            )?)?
        }
        "forge.addons.sync_registry" => {
            let input: AddonRegistrySyncInput = parse_input(input)?;
            let cache_dir = input.cache_dir.as_deref().map(PathBuf::from);
            let lock_path = input.lock.or(input.lock_path).map(PathBuf::from);
            serde_json::to_value(sync_addon_package_registry(
                store,
                &input.source,
                cache_dir.as_deref(),
                input.allow_remote.unwrap_or(false),
                input.max_bytes.unwrap_or(10 * 1024 * 1024),
                input.max_packages.unwrap_or(50),
                lock_path.as_deref(),
            )?)?
        }
        "forge.addons.package_lock" => {
            let input: AddonPackageLockInput = parse_input(input)?;
            let write_path = input.write.or(input.write_path).map(PathBuf::from);
            serde_json::to_value(create_addon_package_lock(
                store,
                input.repository.as_deref(),
                input.channel.as_deref(),
                input.addon.as_deref().or(input.addon_id.as_deref()),
                input.status.as_deref(),
                write_path.as_deref(),
                input.limit.unwrap_or(200),
            )?)?
        }
        "forge.addons.marketplace" => {
            let input: AddonMarketplaceInput = parse_input(input)?;
            serde_json::to_value(list_addon_marketplace(
                store,
                input.repository.as_deref(),
                input.channel.as_deref(),
                input.addon.as_deref().or(input.addon_id.as_deref()),
                input.status.as_deref(),
                input.limit.unwrap_or(20),
            )?)?
        }
        "forge.addons.install_package" => {
            let input: AddonPackagePathInput = parse_input(input)?;
            let package = input
                .package
                .or(input.package_path)
                .with_context(|| "package or package_path is required")?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            let lock_path = input.lock.or(input.lock_path).map(PathBuf::from);
            serde_json::to_value(install_addon_package(
                store,
                &PathBuf::from(package),
                &addon_dirs,
                lock_path.as_deref(),
            )?)?
        }
        "forge.addons.migration_workflow" => {
            let input: AddonMigrationWorkflowInput = parse_input(input)?;
            serde_json::to_value(create_addon_migration_workflow(
                store,
                &PathBuf::from(input.from_manifest),
                &PathBuf::from(input.to_manifest),
                input.action.as_deref().unwrap_or("upgrade"),
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.addons.upgrade" => {
            let input: AddonInstallInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            serde_json::to_value(upgrade_addon(
                store,
                &PathBuf::from(input.manifest),
                &addon_dirs,
            )?)?
        }
        "forge.addons.downgrade" => {
            let input: AddonInstallInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            serde_json::to_value(downgrade_addon(
                store,
                &PathBuf::from(input.manifest),
                &addon_dirs,
            )?)?
        }
        "forge.addons.enable" => {
            let input: AddonLifecycleInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            serde_json::to_value(enable_addon(store, &input.id, &addon_dirs)?)?
        }
        "forge.addons.disable" => {
            let input: AddonLifecycleInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            serde_json::to_value(disable_addon(store, &input.id, &addon_dirs)?)?
        }
        "forge.addons.uninstall" => {
            let input: AddonLifecycleInput = parse_input(input)?;
            let addon_dirs = addon_dirs_from_input(input.addon_dirs);
            serde_json::to_value(uninstall_addon(store, &input.id, &addon_dirs)?)?
        }
        "forge.identity.context" => {
            let input: IdentityContextInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(inspect_project_operating_context(&project_root)?)?
        }
        "forge.identity.registry" => {
            let input: IdentityRegistryInput = parse_input(input)?;
            serde_json::to_value(list_identity_registry(
                store,
                input.scope.as_deref(),
                input.id.as_deref(),
            )?)?
        }
        "forge.identity.memberships" => {
            let input: IdentityMembershipInput = parse_input(input)?;
            let subject_id = input.subject_id.or(input.subject);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            serde_json::to_value(list_identity_memberships(
                store,
                input.subject_scope.as_deref(),
                subject_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.status.as_deref(),
            )?)?
        }
        "forge.identity.membership_update" => {
            let input: IdentityMembershipUpdateMcpInput = parse_input(input)?;
            let subject_id = input
                .subject_id
                .or(input.subject)
                .filter(|value| !value.trim().is_empty())
                .context("subject_id is required")?;
            let organization_id = input
                .organization_id
                .or(input.organization)
                .filter(|value| !value.trim().is_empty())
                .context("organization_id is required")?;
            let brand_id = input
                .brand_id
                .or(input.brand)
                .filter(|value| !value.trim().is_empty())
                .context("brand_id is required")?;
            let product_id = input
                .product_id
                .or(input.product)
                .filter(|value| !value.trim().is_empty())
                .context("product_id is required")?;
            serde_json::to_value(update_identity_membership(
                store,
                IdentityMembershipUpdateInput {
                    subject_scope: input.subject_scope.unwrap_or_else(|| "user".to_string()),
                    subject_id,
                    organization_id,
                    brand_id,
                    product_id,
                    role: input.role,
                    status: input.status,
                    grant_permissions: input.grant_permissions,
                    revoke_grants: input.revoke_grants,
                    deny_permissions: input.deny_permissions,
                    remove_denies: input.remove_denies,
                    expires_at: input.expires_at,
                    clear_expires_at: input.clear_expires_at.unwrap_or(false),
                    not_before: input.not_before,
                    clear_not_before: input.clear_not_before.unwrap_or(false),
                    source: input.source.unwrap_or_else(|| "mcp".to_string()),
                },
            )?)?
        }
        "forge.identity.link" => {
            let input: IdentityLinkMcpInput = parse_input(input)?;
            serde_json::to_value(link_identity(
                store,
                IdentityLinkInput {
                    left_scope: input.left_scope,
                    left_id: input.left_id,
                    right_scope: input.right_scope,
                    right_id: input.right_id,
                    link_type: input.link_type.unwrap_or_else(|| "same_person".to_string()),
                    source: input.source.unwrap_or_else(|| "mcp".to_string()),
                    reason: input.reason,
                },
            )?)?
        }
        "forge.identity.unlink" => {
            let input: IdentityLinkMcpInput = parse_input(input)?;
            serde_json::to_value(unlink_identity(
                store,
                IdentityLinkInput {
                    left_scope: input.left_scope,
                    left_id: input.left_id,
                    right_scope: input.right_scope,
                    right_id: input.right_id,
                    link_type: input.link_type.unwrap_or_else(|| "same_person".to_string()),
                    source: input.source.unwrap_or_else(|| "mcp".to_string()),
                    reason: input.reason,
                },
            )?)?
        }
        "forge.identity.links" => {
            let input: IdentityLinksInput = parse_input(input)?;
            serde_json::to_value(list_identity_links(
                store,
                input.scope.as_deref(),
                input.id.as_deref(),
                input.status.as_deref(),
            )?)?
        }
        "forge.identity.resolve" => {
            let input: IdentityResolveInput = parse_input(input)?;
            serde_json::to_value(resolve_identity(store, &input.scope, &input.id)?)?
        }
        "forge.identity.tenant_index" => {
            let input: TenantIndexInput = parse_input(input)?;
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let workflow_id = input.workflow_id.or(input.workflow);
            serde_json::to_value(list_tenant_index(
                store,
                input.resource_type.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                workflow_id.as_deref(),
            )?)?
        }
        "forge.identity.tenant_audit" => serde_json::to_value(audit_tenant_index(store)?)?,
        "forge.identity.tenant_policy" => {
            let input: TenantPolicyInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow).ok_or_else(|| {
                anyhow::anyhow!("forge.identity.tenant_policy requires workflow_id")
            })?;
            serde_json::to_value(evaluate_tenant_policy_for_action(
                store,
                &workflow_id,
                input.mode.as_deref().unwrap_or("audit"),
                input.action.as_deref().unwrap_or("tenant policy"),
            )?)?
        }
        "forge.identity.sync" => {
            let input: IdentityContextInput = parse_input(input)?;
            let project_root = input
                .project_root
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            serde_json::to_value(sync_project_operating_context(store, &project_root)?)?
        }
        "forge.memory.policy" => {
            let input = if input.is_null() {
                MemoryPolicyInput { project_root: None }
            } else {
                parse_input(input)?
            };
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(memory_policy_report_for_project(
                store,
                project_root.as_deref(),
            ))?
        }
        "forge.memory.configure" => {
            let input: MemoryConfigureInput = parse_input(input)?;
            serde_json::to_value(configure_memory_governance(
                MemoryGovernanceConfigOptions {
                    project_root: PathBuf::from(input.project_root),
                    memory_level: input.memory_level,
                    default_scopes: input.default_scopes,
                    default_audience: input.default_audience,
                    privacy_mode: input.privacy_mode,
                    retention_mode: input.retention_mode,
                    approved_by: input.approved_by,
                    reason: input.reason,
                },
            )?)?
        }
        "forge.memory.search" => {
            let input: MemorySearchInput = parse_input(input)?;
            serde_json::to_value(search_memory(
                store,
                MemorySearchOptions {
                    query: input.query,
                    workflow_id: input.workflow_id,
                    scopes: input.scopes,
                    audience: input.audience,
                    visibility: input.visibility,
                    memory_level: input.memory_level,
                    run_id: input.run_id,
                    organization_id: input.organization_id,
                    limit: input.limit.unwrap_or(10),
                    global_root: input.global_root.map(PathBuf::from),
                    organization_root: input.organization_root.map(PathBuf::from),
                    project_root: input.project_root.map(PathBuf::from),
                    processing_root: input.processing_root.map(PathBuf::from),
                },
            )?)?
        }
        "forge.memory.promote" => {
            let input: MemoryPromotionInput = parse_input(input)?;
            serde_json::to_value(promote_memory(
                store,
                MemoryPromotionOptions {
                    workflow_id: input.workflow_id,
                    from_scope: input.from_scope,
                    to_scope: input.to_scope,
                    source_path: PathBuf::from(input.source_path),
                    source_start_line: input.source_start_line,
                    source_end_line: input.source_end_line,
                    summary: input.summary,
                    approved_by: input.approved_by,
                    reason: input.reason,
                    visibility: input.visibility.unwrap_or_else(|| "internal".to_string()),
                    shareability: input.shareability,
                    organization_id: input.organization_id,
                    global_root: input.global_root.map(PathBuf::from),
                    organization_root: input.organization_root.map(PathBuf::from),
                    project_root: input.project_root.map(PathBuf::from),
                    dry_run: input.dry_run.unwrap_or(false),
                },
            )?)?
        }
        "forge.memory.promotions" => {
            let input: MemoryPromotionIndexInput = parse_input(input)?;
            serde_json::to_value(list_memory_promotions(
                store,
                input.from_scope,
                input.to_scope,
                input.approved_by,
                input.workflow_id,
            )?)?
        }
        "forge.memory.retention" => {
            let input: MemoryRetentionInput = parse_input(input)?;
            serde_json::to_value(memory_retention_report(
                store,
                MemoryRetentionOptions {
                    workflow_id: input.workflow_id,
                    scopes: input.scopes,
                    run_id: input.run_id,
                    organization_id: input.organization_id,
                    global_root: input.global_root.map(PathBuf::from),
                    organization_root: input.organization_root.map(PathBuf::from),
                    project_root: input.project_root.map(PathBuf::from),
                    processing_root: input.processing_root.map(PathBuf::from),
                },
            )?)?
        }
        "forge.memory.cleanup" => {
            let input: MemoryCleanupInput = parse_input(input)?;
            serde_json::to_value(memory_cleanup_report(
                store,
                MemoryCleanupOptions {
                    workflow_id: input.workflow_id,
                    scopes: input.scopes,
                    run_id: input.run_id,
                    organization_id: input.organization_id,
                    global_root: input.global_root.map(PathBuf::from),
                    organization_root: input.organization_root.map(PathBuf::from),
                    project_root: input.project_root.map(PathBuf::from),
                    processing_root: input.processing_root.map(PathBuf::from),
                    mode: input.mode.unwrap_or_else(|| "archive".to_string()),
                    archive_root: input.archive_root.map(PathBuf::from),
                    approved_by: input.approved_by,
                    reason: input.reason,
                    dry_run: input.dry_run.unwrap_or(false),
                    confirm: input.confirm.unwrap_or(false),
                },
            )?)?
        }
        "forge.interactive.slash_commands" => serde_json::to_value(slash_command_catalog())?,
        "forge.interactive.route" => {
            let input: InteractiveRouteInput = parse_input(input)?;
            serde_json::to_value(route_interactive_input(
                store,
                &input.input,
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.security.secret_scan" => {
            let input: SecuritySecretScanInput = parse_input(input)?;
            let tenant_context = input
                .workflow_id
                .as_deref()
                .and_then(|workflow_id| store.load_workflow(workflow_id).ok())
                .map(|workflow| serde_json::to_value(&workflow.intent.operating_context))
                .transpose()?
                .unwrap_or_else(|| {
                    serde_json::to_value(crate::intent::OperatingContextSpec::default())
                        .unwrap_or_else(|_| serde_json::json!({}))
                });
            serde_json::to_value(sanitize_prompt_secrets_with_vault(
                &input.input,
                SecretSanitizationOptions {
                    scope: input.scope.unwrap_or_else(|| "project".to_string()),
                    enable_entropy: input.enable_entropy.unwrap_or(true),
                    enable_local_ai_fallback: input.enable_local_ai_fallback.unwrap_or(true),
                    allow_external_ai: input.allow_external_ai.unwrap_or(false),
                    ..SecretSanitizationOptions::default()
                },
                SecretVaultPersistOptions {
                    store,
                    workflow_id: input.workflow_id.as_deref(),
                    origin: input.origin.as_deref().unwrap_or("mcp"),
                    tenant_context: &tenant_context,
                },
            )?)?
        }
        "forge.credential_vault.describe" => {
            let input: CredentialVaultInput = parse_input(input)?;
            serde_json::to_value(run_credential_vault_describe(
                input.vault_bin.as_deref().map(PathBuf::from).as_deref(),
                &PathBuf::from(input.contract),
                &PathBuf::from(input.data),
            )?)?
        }
        "forge.credential_vault.records" => {
            let input: CredentialVaultInput = parse_input(input)?;
            serde_json::to_value(run_credential_vault_records(
                input.vault_bin.as_deref().map(PathBuf::from).as_deref(),
                &PathBuf::from(input.contract),
                &PathBuf::from(input.data),
            )?)?
        }
        "forge.aws.check" => {
            let input: AwsCheckInput = parse_input(input)?;
            serde_json::to_value(run_aws_ops_check(
                input.aws_ops_bin.as_deref().map(PathBuf::from).as_deref(),
                input
                    .vault_contract
                    .as_deref()
                    .map(PathBuf::from)
                    .as_deref(),
                input.vault_data.as_deref().map(PathBuf::from).as_deref(),
            )?)?
        }
        "forge.aws.inventory" => {
            let input: AwsInventoryInput = parse_input(input)?;
            serde_json::to_value(run_aws_ops_inventory(
                input.aws_ops_bin.as_deref().map(PathBuf::from).as_deref(),
                input
                    .vault_contract
                    .as_deref()
                    .map(PathBuf::from)
                    .as_deref(),
                input.vault_data.as_deref().map(PathBuf::from).as_deref(),
                input.regions.as_deref(),
                input.all_regions.unwrap_or(false),
                input.full.unwrap_or(false),
            )?)?
        }
        "forge.aws.raw" => {
            let input: AwsRawInput = parse_input(input)?;
            serde_json::to_value(run_aws_ops_raw(
                input.aws_ops_bin.as_deref().map(PathBuf::from).as_deref(),
                input
                    .vault_contract
                    .as_deref()
                    .map(PathBuf::from)
                    .as_deref(),
                input.vault_data.as_deref().map(PathBuf::from).as_deref(),
                input.allow_mutation.unwrap_or(false),
                input.reason.as_deref(),
                &input.aws_args,
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
            let parallel_team = explicit_parallel_team_from_mcp_lanes(
                input.lanes,
                input.max_parallel_agents,
                "forge.run.start.mcp",
            )?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(start_async_request_with_idempotency_and_parallel_team(
                store,
                &input.goal,
                &origin,
                input.idempotency_key.as_deref(),
                parallel_team,
            )?)?
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
        "forge.run.drive" => {
            let input: RunDriveInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(drive_request(
                store,
                &input.run_id,
                input.executor.as_deref().unwrap_or("mcp"),
                input.ttl_seconds.unwrap_or(300),
                &origin,
            )?)?
        }
        "forge.run.step" => {
            let input: RunStepInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(step_request(
                store,
                &input.run_id,
                input.executor.as_deref().unwrap_or("mcp"),
                input.ttl_seconds.unwrap_or(300),
                &origin,
            )?)?
        }
        "forge.run.execute_wave" => {
            let input: RunExecuteWaveInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let executor = input
                .executor
                .unwrap_or_else(|| DEFAULT_REQUEST_EXECUTOR_WAVE_EXECUTOR.to_string());
            serde_json::to_value(execute_request_executor_wave(
                store,
                &RequestExecutorWaveOptions {
                    run_id: &input.run_id,
                    requested_executor: &executor,
                    ttl_seconds: input
                        .ttl_seconds
                        .unwrap_or(DEFAULT_REQUEST_EXECUTOR_WAVE_TTL_SECONDS),
                    timeout_seconds: input
                        .timeout_seconds
                        .unwrap_or(DEFAULT_REQUEST_EXECUTOR_WAVE_TIMEOUT_SECONDS),
                    context_budget: input
                        .context_budget
                        .unwrap_or(DEFAULT_REQUEST_EXECUTOR_WAVE_CONTEXT_BUDGET),
                    max_parallel: input.max_parallel,
                    allow_exec: input.allow_exec,
                    approved_by: &input.approved_by,
                    reason: &input.reason,
                    origin: &origin,
                },
            )?)?
        }
        "forge.run.complete_task" => {
            let input: RunCompleteTaskInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let executor = input.executor.unwrap_or_else(|| "mcp".to_string());
            let artifacts = input
                .artifacts
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            serde_json::to_value(complete_ready_task(
                store,
                &input.run_id,
                RequestTaskCompletionInput {
                    task_id: &input.task_id,
                    executor: &executor,
                    summary: &input.summary,
                    artifact_paths: &artifacts,
                    evidence_command: input.evidence_command.as_deref(),
                    evidence_exit_code: input.evidence_exit_code,
                    evidence_summary: input.evidence_summary.as_deref(),
                    estimated_usd: input.estimated_usd.unwrap_or(0.0),
                    tokens_in: input.tokens_in.unwrap_or(0),
                    tokens_out: input.tokens_out.unwrap_or(0),
                    ttl_seconds: input.ttl_seconds.unwrap_or(300),
                    context_budget: input.context_budget,
                    origin: &origin,
                },
            )?)?
        }
        "forge.run.final_package" => {
            let input: RunIdInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(create_final_delivery_package(
                store,
                &input.run_id,
                &origin,
            )?)?
        }
        "forge.workflow.ensure_final_audit" => {
            let input: EnsureFinalAuditInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let executor = input.executor.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(ensure_final_audit(
                store,
                &input.workflow_id,
                &executor,
                &origin,
            )?)?
        }
        "forge.run.switch_executor" => {
            let input: RunSwitchExecutorInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(switch_request_executor(
                store,
                &input.run_id,
                RequestExecutorSwitchInput {
                    executor: input.executor,
                    fallback_executors: input.fallback_executors,
                    summary: input
                        .summary
                        .unwrap_or_else(|| "executor hot swap through MCP".to_string()),
                    ttl_seconds: input.ttl_seconds.unwrap_or(300),
                    pid: input.pid,
                    origin,
                    reason: input
                        .reason
                        .unwrap_or_else(|| "executor limit or availability changed".to_string()),
                },
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
            serde_json::to_value(update_workflow_goal_with_expected_revision(
                store,
                &input.workflow_id,
                &input.goal,
                &input.origin,
                input.expected_revision,
            )?)?
        }
        "forge.workflow.add_task" => {
            let input: WorkflowAddTaskInput = parse_input(input)?;
            serde_json::to_value(add_workflow_task(
                store,
                &input.workflow_id,
                WorkflowTaskAddInput {
                    task_id: input.task_id,
                    description: input.description,
                    priority: input.priority,
                    origin: input.origin,
                    expected_revision: input.expected_revision,
                },
            )?)?
        }
        "forge.workflow.update_task" => {
            let input: WorkflowUpdateTaskInput = parse_input(input)?;
            serde_json::to_value(update_workflow_task_with_expected_revision(
                store,
                &input.workflow_id,
                WorkflowTaskUpdateInput {
                    task_id: &input.task_id,
                    title: input.title.as_deref(),
                    goal: input.goal.as_deref(),
                    expected_output: input.expected_output.as_deref(),
                    origin: &input.origin,
                },
                input.expected_revision,
            )?)?
        }
        "forge.workflow.set_priority" => {
            let input: WorkflowSetPriorityInput = parse_input(input)?;
            serde_json::to_value(set_workflow_task_priority(
                store,
                &input.workflow_id,
                WorkflowTaskPriorityInput {
                    task_id: input.task_id,
                    priority: input.priority,
                    origin: input.origin,
                    expected_revision: input.expected_revision,
                },
            )?)?
        }
        "forge.workflow.add_dependency" => {
            let input: WorkflowDependencyInput = parse_input(input)?;
            serde_json::to_value(add_workflow_task_dependency(
                store,
                &input.workflow_id,
                WorkflowTaskDependencyInput {
                    task_id: input.task_id,
                    dependency_task_id: input.depends_on,
                    origin: input.origin,
                    expected_revision: input.expected_revision,
                },
            )?)?
        }
        "forge.workflow.remove_dependency" => {
            let input: WorkflowDependencyInput = parse_input(input)?;
            serde_json::to_value(remove_workflow_task_dependency(
                store,
                &input.workflow_id,
                WorkflowTaskDependencyInput {
                    task_id: input.task_id,
                    dependency_task_id: input.depends_on,
                    origin: input.origin,
                    expected_revision: input.expected_revision,
                },
            )?)?
        }
        "forge.workflow.set_impediment" => {
            let input: WorkflowSetImpedimentInput = parse_input(input)?;
            serde_json::to_value(set_workflow_task_impediment(
                store,
                &input.workflow_id,
                WorkflowTaskImpedimentInput {
                    task_id: input.task_id,
                    reason: input.reason,
                    kind: input.kind,
                    origin: input.origin,
                    expected_revision: input.expected_revision,
                },
            )?)?
        }
        "forge.workflow.clear_impediment" => {
            let input: WorkflowClearImpedimentInput = parse_input(input)?;
            serde_json::to_value(clear_workflow_task_impediment(
                store,
                &input.workflow_id,
                WorkflowTaskImpedimentClearInput {
                    task_id: input.task_id,
                    impediment_id: input.impediment_id,
                    origin: input.origin,
                    expected_revision: input.expected_revision,
                },
            )?)?
        }
        "forge.workflow.update_node_brain" => {
            let input: WorkflowUpdateNodeBrainInput = parse_input(input)?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            let agent_slots = input
                .agent_slots
                .iter()
                .map(|slot| parse_node_brain_agent_slot(slot))
                .collect::<Result<Vec<_>>>()?;
            serde_json::to_value(update_workflow_node_brain_routing(
                store,
                &input.workflow_id,
                WorkflowNodeBrainRoutingUpdateInput {
                    task_id: input.task_id,
                    default_brain: input.default_brain,
                    allowed_brains: input.allowed_brains,
                    agent_slots,
                    max_parallel_agents: input.max_parallel_agents,
                    origin,
                },
            )?)?
        }
        "forge.workflow.attach_artifact" => {
            let input: WorkflowAttachArtifactInput = parse_input(input)?;
            serde_json::to_value(attach_workflow_artifact_with_tags(
                store,
                &input.workflow_id,
                &PathBuf::from(input.path),
                &input.kind,
                &input.origin,
                &input.tags,
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
                    expected_revision: None,
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
            ensure_workflow_policy(store, &input.workflow_id, "context request")?;
            let workflow = store.load_workflow(&input.workflow_id)?;
            let latest_checkpoint =
                load_latest_task_checkpoint(store, &input.workflow_id, &input.task_id)?;
            let budget = input.budget.unwrap_or(DEFAULT_CONTEXT_BUDGET);
            let explicit_project_root = input.project_root.as_deref().map(PathBuf::from);
            let project_root = resolve_effective_project_root(
                store,
                &input.workflow_id,
                Some(&input.task_id),
                explicit_project_root.as_deref(),
            )?;
            let bound_worktree =
                bound_worktree_context(store, &input.workflow_id, Some(&input.task_id))?;
            let workflow_secret_scan_input = serde_json::to_string(&workflow)?;
            let tenant_context = serde_json::to_value(&workflow.intent.operating_context)?;
            let _ = sanitize_prompt_secrets_with_vault(
                &workflow_secret_scan_input,
                SecretSanitizationOptions::default(),
                SecretVaultPersistOptions {
                    store,
                    workflow_id: Some(&workflow.id),
                    origin: "forge_mcp_context",
                    tenant_context: &tenant_context,
                },
            )?;
            let package = if bound_worktree.is_some() {
                build_context_package_with_checkpoint_project_and_worktree(
                    &workflow,
                    &input.task_id,
                    budget,
                    latest_checkpoint,
                    project_root.as_deref(),
                    bound_worktree,
                )?
            } else {
                build_context_package_with_checkpoint_and_project(
                    &workflow,
                    &input.task_id,
                    budget,
                    latest_checkpoint,
                    project_root.as_deref(),
                )?
            };
            match input.view.as_deref().unwrap_or("compact") {
                "full" => serde_json::to_value(package)?,
                "compact" => {
                    let predecessor_plans = build_predecessor_handoff_plans(
                        store,
                        &workflow,
                        &input.task_id,
                        budget,
                        explicit_project_root.as_deref(),
                    )?;
                    serde_json::to_value(build_compact_context_view_with_predecessor_plans(
                        &package,
                        &workflow,
                        store.path(),
                        project_root.as_deref(),
                        &predecessor_plans,
                    ))?
                }
                view => bail!("unsupported context view `{view}`; expected full or compact"),
            }
        }
        "forge.worktree.guard.check" => {
            let input: WorktreeGuardCheckInput = parse_input(input)?;
            serde_json::to_value(evaluate_worktree_modification_guard(
                store,
                WorktreeModificationGuardRequest {
                    worktree: input.worktree,
                    operation: input.operation,
                    paths: input.paths,
                    reason: input.reason,
                    workflow_id: input.workflow_id,
                    task_id: input.task_id,
                },
            )?)?
        }
        "forge.worktree.guard.create_predecessor" => {
            let input: WorktreeGuardCreatePredecessorInput = parse_input(input)?;
            serde_json::to_value(create_worktree_guard_predecessor_task(
                store,
                &input.worktree,
                &input.workflow_id,
                &input.task_id,
                input.paths,
                &input.goal,
                input.allow_workflow_mutation,
                &input.approved_by,
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.worktree.sandbox.plan" => {
            let input: WorktreeSandboxInput = parse_input(input)?;
            serde_json::to_value(plan_worktree_sandbox(
                store,
                WorktreeSandboxRequest {
                    worktree: input.worktree,
                    purpose: input.purpose,
                    workflow_id: input.workflow_id,
                    task_id: input.task_id,
                    command: input.command,
                },
            )?)?
        }
        "forge.worktree.sandbox.run" => {
            let input: WorktreeSandboxInput = parse_input(input)?;
            serde_json::to_value(run_worktree_sandbox(
                store,
                WorktreeSandboxRequest {
                    worktree: input.worktree,
                    purpose: input.purpose,
                    workflow_id: input.workflow_id,
                    task_id: input.task_id,
                    command: input.command,
                },
                input.allow_exec.unwrap_or(false),
            )?)?
        }
        "forge.worktree.sandbox.start" => {
            let input: WorktreeSandboxInput = parse_input(input)?;
            serde_json::to_value(start_worktree_sandbox(
                store,
                WorktreeSandboxRequest {
                    worktree: input.worktree,
                    purpose: input.purpose,
                    workflow_id: input.workflow_id,
                    task_id: input.task_id,
                    command: input.command,
                },
                input.allow_exec.unwrap_or(false),
            )?)?
        }
        "forge.worktree.sandbox.status" => {
            let input: WorktreeSandboxStatusInput = parse_input(input)?;
            serde_json::to_value(inspect_worktree_sandbox_lifecycle(
                store,
                &input.sandbox_id,
            )?)?
        }
        "forge.worktree.sandbox.stop" => {
            let input: WorktreeSandboxStopInput = parse_input(input)?;
            serde_json::to_value(stop_worktree_sandbox(
                store,
                &input.sandbox_id,
                input.allow_stop.unwrap_or(false),
            )?)?
        }
        "forge.harness.token_headroom" => {
            let input: HarnessTokenHeadroomInput = parse_input(input)?;
            let content_kind = input.content_kind.or(input.kind);
            let report = analyze_token_headroom(
                &input.content,
                content_kind.as_deref(),
                input.budget_tokens.unwrap_or(0),
                input.source.as_deref().unwrap_or("mcp"),
                input.reversible.unwrap_or(true),
            );
            let report = if input.persist.unwrap_or(false) {
                persist_token_headroom_report(store, report, &input.content)?
            } else {
                report
            };
            serde_json::to_value(report)?
        }
        "forge.harness.retrieve_headroom" => {
            let input: HarnessRetrieveHeadroomInput = parse_input(input)?;
            serde_json::to_value(retrieve_headroom_blob(
                store,
                &input.retrieval_ref,
                input.include_content.unwrap_or(false),
            )?)?
        }
        "forge.harness.headroom_stats" => {
            let input: HarnessHeadroomStatsInput = if input.is_null() {
                HarnessHeadroomStatsInput {
                    source: None,
                    content_kind: None,
                    kind: None,
                    limit: None,
                }
            } else {
                parse_input(input)?
            };
            let content_kind = input.content_kind.or(input.kind);
            serde_json::to_value(build_headroom_stats_report(
                store,
                HeadroomStatsOptions {
                    source: input.source.as_deref(),
                    content_kind: content_kind.as_deref(),
                    limit: input.limit.unwrap_or(10),
                },
            )?)?
        }
        "forge.harness.mode" => {
            let input: HarnessModeInput = parse_input(input)?;
            serde_json::to_value(build_harness_mode_report(HarnessModeOptions {
                forge_first: input.forge_first.unwrap_or(false),
                observe_only: input.observe_only.unwrap_or(false),
                project_root: input.project_root.as_deref().map(std::path::Path::new),
            }))?
        }
        "forge.harness.doctor" => {
            let input: HarnessDoctorInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            let (effective_forge_first, _) = if let Some(forge_first) = input.forge_first {
                (forge_first, "mcp_input")
            } else if let Some(project_root) = project_root {
                resolve_harness_forge_first_source_for_project(false, false, Some(project_root))
            } else {
                (true, "mcp_default")
            };
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root,
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first: effective_forge_first,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(build_harness_doctor_report(HarnessDoctorOptions {
                shim_dir: std::path::Path::new(&input.shim_dir),
                executor: &input.executor,
                forge_first: input.forge_first.unwrap_or(false),
                observe_only: input.observe_only.unwrap_or(false),
                project_root,
                workflow_id: workflow_id.as_deref(),
                task_id: task_id.as_deref(),
                run_id: run_id.as_deref(),
                context_budget: runtime_policy.context_budget,
                context_budget_source: &runtime_policy.context_budget_source,
                token_headroom: runtime_policy.token_headroom,
                token_headroom_source: &runtime_policy.token_headroom_source,
                require_token_headroom_for_forge_first: runtime_policy
                    .require_token_headroom_for_forge_first,
            })?)?
        }
        "forge.harness.headroom_plan" => {
            let input: HarnessHeadroomPlanInput = parse_input(input)?;
            let command = input.command.or(input.cmd).unwrap_or_default();
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            let observe_only = input.observe_only.unwrap_or(false);
            let (forge_first, forge_first_source) = if observe_only {
                (false, "observe_only_flag")
            } else if let Some(forge_first) = input.forge_first {
                (forge_first, "mcp_input")
            } else if let Some(project_root) = project_root {
                resolve_harness_forge_first_source_for_project(false, false, Some(project_root))
            } else {
                (true, "mcp_default")
            };
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root,
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(build_harness_headroom_plan(HarnessHeadroomPlanOptions {
                executor: &input.executor,
                command: &command,
                forge_first,
                forge_first_source,
                project_root,
                workflow_id: workflow_id.as_deref(),
                task_id: task_id.as_deref(),
                run_id: run_id.as_deref(),
                context_budget: runtime_policy.context_budget,
                context_budget_source: &runtime_policy.context_budget_source,
                token_headroom: runtime_policy.token_headroom,
                token_headroom_source: &runtime_policy.token_headroom_source,
                require_token_headroom_for_forge_first: runtime_policy
                    .require_token_headroom_for_forge_first,
            }))?
        }
        "forge.harness.adoption_plan" => {
            let input: HarnessAdoptionPlanInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            let observe_only = input.observe_only.unwrap_or(false);
            let effective_forge_first = if observe_only {
                false
            } else if let Some(forge_first) = input.forge_first {
                forge_first
            } else if let Some(project_root) = project_root {
                resolve_harness_forge_first_source_for_project(false, false, Some(project_root)).0
            } else {
                true
            };
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root,
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first: effective_forge_first,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(build_harness_adoption_plan(HarnessAdoptionPlanOptions {
                shim_dir: std::path::Path::new(&input.shim_dir),
                executor: &input.executor,
                forge_first: input.forge_first.unwrap_or(false),
                observe_only,
                project_root,
                workflow_id: workflow_id.as_deref(),
                task_id: task_id.as_deref(),
                run_id: run_id.as_deref(),
                context_budget: runtime_policy.context_budget,
                context_budget_source: &runtime_policy.context_budget_source,
                token_headroom: runtime_policy.token_headroom,
                token_headroom_source: &runtime_policy.token_headroom_source,
                require_token_headroom_for_forge_first: runtime_policy
                    .require_token_headroom_for_forge_first,
            })?)?
        }
        "forge.harness.activation_profile" => {
            let input: HarnessActivationProfileInput = parse_input(input)?;
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root,
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first: true,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(build_harness_activation_profile(
                HarnessActivationProfileOptions {
                    shim_dir: std::path::Path::new(&input.shim_dir),
                    executor: &input.executor,
                    project_root,
                    context_budget: runtime_policy.context_budget,
                    context_budget_source: &runtime_policy.context_budget_source,
                    token_headroom: runtime_policy.token_headroom,
                    token_headroom_source: &runtime_policy.token_headroom_source,
                    apply: input.apply.unwrap_or(false),
                    shell_rc: input.shell_rc.as_deref().map(std::path::Path::new),
                    approved_by: input.approved_by.as_deref(),
                },
            )?)?
        }
        "forge.harness.bootstrap" => {
            let input: HarnessBootstrapInput = parse_input(input)?;
            let project_root = std::path::Path::new(&input.project_root);
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root: Some(project_root),
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first: true,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(build_harness_bootstrap_report(HarnessBootstrapOptions {
                shim_dir: std::path::Path::new(&input.shim_dir),
                executor: &input.executor,
                project_root,
                store_path: Some(store.path()),
                context_budget: runtime_policy.context_budget,
                context_budget_source: &runtime_policy.context_budget_source,
                token_headroom: runtime_policy.token_headroom,
                token_headroom_source: &runtime_policy.token_headroom_source,
                apply: input.apply.unwrap_or(false),
                approved_by: input.approved_by.as_deref(),
                force: input.force.unwrap_or(false),
            })?)?
        }
        "forge.harness.wrap_plan" => {
            let input: HarnessWrapPlanInput = parse_input(input)?;
            let command = input.command.or(input.cmd).unwrap_or_default();
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            let (forge_first, forge_first_source) = if let Some(forge_first) = input.forge_first {
                (forge_first, "mcp_input")
            } else if let Some(project_root) = project_root {
                resolve_harness_forge_first_source_for_project(false, false, Some(project_root))
            } else {
                (true, "mcp_default")
            };
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root,
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(build_cli_wrapper_plan(CliWrapperPlanOptions {
                executor: &input.executor,
                command: &command,
                forge_first,
                forge_first_source,
                project_root,
                workflow_id: workflow_id.as_deref(),
                task_id: task_id.as_deref(),
                run_id: run_id.as_deref(),
                context_budget: runtime_policy.context_budget,
                context_budget_source: &runtime_policy.context_budget_source,
                token_headroom: runtime_policy.token_headroom,
                token_headroom_source: &runtime_policy.token_headroom_source,
                require_token_headroom_for_forge_first: runtime_policy
                    .require_token_headroom_for_forge_first,
            }))?
        }
        "forge.harness.install_shims" => {
            let input: HarnessInstallShimsInput = parse_input(input)?;
            let real_cmd = input
                .real_cmd
                .or(input.real_command)
                .filter(|value| !value.trim().is_empty());
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            let (forge_first, forge_first_source) = if let Some(forge_first) = input.forge_first {
                (forge_first, "mcp_input")
            } else if let Some(project_root) = project_root {
                resolve_harness_forge_first_source_for_project(false, false, Some(project_root))
            } else {
                (true, "mcp_default")
            };
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root,
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(install_cli_harness_shim(CliShimInstallOptions {
                shim_dir: std::path::Path::new(&input.shim_dir),
                executor: &input.executor,
                real_cmd: real_cmd.as_deref(),
                store_path: Some(store.path()),
                forge_first,
                forge_first_source,
                workflow_id: workflow_id.as_deref(),
                task_id: task_id.as_deref(),
                run_id: run_id.as_deref(),
                context_budget: runtime_policy.context_budget,
                token_headroom: runtime_policy.token_headroom,
                force: input.force.unwrap_or(false),
            })?)?
        }
        "forge.harness.install_provider_adapter" => {
            let input: HarnessInstallProviderAdapterInput = parse_input(input)?;
            let real_cmd = input
                .real_cmd
                .or(input.real_command)
                .filter(|value| !value.trim().is_empty());
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root,
                context_budget: None,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first: true,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            serde_json::to_value(install_cli_provider_adapter(
                ProviderAdapterInstallOptions {
                    shim_dir: std::path::Path::new(&input.shim_dir),
                    executor: &input.executor,
                    real_cmd: real_cmd.as_deref(),
                    project_root,
                    token_headroom: runtime_policy.token_headroom,
                    force: input.force.unwrap_or(false),
                },
            )?)?
        }
        "forge.harness.shim_status" => {
            let input: HarnessShimStatusInput = parse_input(input)?;
            serde_json::to_value(inspect_cli_harness_shim_status(CliShimStatusOptions {
                shim_dir: std::path::Path::new(&input.shim_dir),
                executor: &input.executor,
            })?)?
        }
        "forge.harness.exec" => {
            let input: HarnessExecInput = parse_input(input)?;
            let command = input.command.or(input.cmd).unwrap_or_default();
            let workflow_id = input.workflow_id.or(input.workflow);
            let task_id = input.task_id.or(input.task);
            let run_id = input.run_id.or(input.run);
            let explicit_cwd = input.cwd.as_deref().map(PathBuf::from);
            let explicit_project_root = input.project_root.as_deref().map(PathBuf::from);
            let project_root = if let Some(workflow_id) = workflow_id.as_deref() {
                resolve_effective_project_root(
                    store,
                    workflow_id,
                    task_id.as_deref(),
                    explicit_project_root.as_deref(),
                )?
            } else {
                explicit_project_root
            };
            let bound_root = workflow_id
                .as_deref()
                .map(|workflow_id| {
                    resolve_effective_project_root(store, workflow_id, task_id.as_deref(), None)
                })
                .transpose()?
                .flatten();
            let cwd = if let Some(bound_root) = bound_root.as_ref() {
                if let Some(explicit_cwd) = explicit_cwd.as_ref() {
                    let explicit_cwd = std::fs::canonicalize(explicit_cwd)?;
                    let canonical_bound = std::fs::canonicalize(bound_root)?;
                    if explicit_cwd != canonical_bound {
                        bail!(
                            "explicit cwd {} conflicts with bound worktree {}",
                            explicit_cwd.display(),
                            canonical_bound.display()
                        );
                    }
                }
                Some(bound_root.clone())
            } else {
                explicit_cwd
            };
            let (forge_first, forge_first_source) = if let Some(forge_first) = input.forge_first {
                (forge_first, "mcp_input")
            } else if let Some(project_root) = project_root.as_deref() {
                resolve_harness_forge_first_source_for_project(false, false, Some(project_root))
            } else {
                (true, "mcp_default")
            };
            let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
                project_root: project_root.as_deref(),
                context_budget: input.context_budget,
                context_budget_source: "mcp_input",
                token_headroom: input.token_headroom,
                token_headroom_source: "mcp_input",
                forge_first,
                default_context_budget: DEFAULT_CONTEXT_BUDGET,
            });
            let secret_env = input.secret_env.unwrap_or_default();
            let secret_permissions = input.secret_permissions.unwrap_or_default();
            serde_json::to_value(run_cli_harness_exec(CliHarnessExecOptions {
                store: Some(store),
                executor: &input.executor,
                command: &command,
                forge_first,
                forge_first_source,
                workflow_id: workflow_id.as_deref(),
                task_id: task_id.as_deref(),
                run_id: run_id.as_deref(),
                context_budget: runtime_policy.context_budget,
                context_budget_source: &runtime_policy.context_budget_source,
                token_headroom: runtime_policy.token_headroom,
                token_headroom_source: &runtime_policy.token_headroom_source,
                require_token_headroom_for_forge_first: runtime_policy
                    .require_token_headroom_for_forge_first,
                dry_run: input.dry_run.unwrap_or(true),
                allow_exec: input.allow_exec.unwrap_or(false),
                secret_env: &secret_env,
                secret_permissions: &secret_permissions,
                project_root: project_root.as_deref(),
                cwd: cwd.as_deref(),
            })?)?
        }
        "forge.task.handoff" => {
            let input: TaskHandoffInput = parse_input(input)?;
            let project_root = input.project_root.as_deref().map(PathBuf::from);
            let view = match input.view.as_deref().unwrap_or("compact") {
                "compact" => TaskHandoffView::Compact,
                "full" => TaskHandoffView::Full,
                view => bail!("unsupported task handoff view `{view}`; expected compact or full"),
            };
            serde_json::to_value(build_task_handoff_response_with_project(
                store,
                &input.workflow_id,
                &input.task_id,
                &input.executor,
                input.budget.unwrap_or(DEFAULT_CONTEXT_BUDGET),
                input.ttl_seconds.unwrap_or(900),
                project_root.as_deref(),
                view,
            )?)?
        }
        "forge.patch.plan" => {
            let input: PatchPlanInput = parse_input(input)?;
            serde_json::to_value(build_patch_plan(
                store,
                &input.workflow_id,
                &input.task_id,
                input.paths,
                &input.intent,
                input.origin.as_deref().unwrap_or("mcp"),
            )?)?
        }
        "forge.patch.apply" => {
            let input: PatchApplyInput = parse_input(input)?;
            serde_json::to_value(build_patch_apply(
                store,
                &input.workflow_id,
                &input.task_id,
                input.paths,
                input.origin.as_deref().unwrap_or("mcp"),
                input.plan_artifact.as_deref(),
                None,
            )?)?
        }
        "forge.patch.review" => {
            let input: PatchReviewInput = parse_input(input)?;
            serde_json::to_value(build_patch_review(
                store,
                &input.workflow_id,
                &input.task_id,
                input.paths,
                input.origin.as_deref().unwrap_or("mcp"),
                input.plan_artifact.as_deref(),
            )?)?
        }
        "forge.patch.diff" => {
            let input: PatchDiffInput = parse_input(input)?;
            serde_json::to_value(build_patch_diff(
                store,
                &input.workflow_id,
                &input.task_id,
                input.paths,
                PatchDiffOptions {
                    file_index: input.file_index.unwrap_or(0),
                    hunk_index: input.hunk_index.unwrap_or(0),
                    context_lines: input.context_lines.unwrap_or(3),
                    origin: input.origin.as_deref().unwrap_or("mcp"),
                },
            )?)?
        }
        "forge.patch.revert" => {
            let input: PatchRevertInput = parse_input(input)?;
            serde_json::to_value(build_patch_revert(
                store,
                &input.workflow_id,
                &input.task_id,
                &input.apply_artifact,
                input.origin.as_deref().unwrap_or("mcp"),
                None,
            )?)?
        }
        "forge.patch.restore" => {
            let input: PatchRestoreInput = parse_input(input)?;
            serde_json::to_value(build_patch_restore(
                store,
                &input.workflow_id,
                &input.task_id,
                &input.revert_artifact,
                &input.approved_by,
                input.confirm_restore,
                input.origin.as_deref().unwrap_or("mcp"),
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
            serde_json::to_value(build_milestone_manifest_with_store(&version, Some(store))?)?
        }
        "forge.milestone.production_plan" => {
            let input: MilestoneStatusInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            serde_json::to_value(build_production_readiness_plan(&version)?)?
        }
        "forge.milestone.production_readiness" => {
            let input: ProductionReadinessInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            let manifest = PathBuf::from(input.manifest);
            let evidence_root = PathBuf::from(input.evidence_root);
            serde_json::to_value(evaluate_production_readiness(ProductionReadinessOptions {
                version: &version,
                manifest_path: &manifest,
                evidence_root: &evidence_root,
                store_path: store.path(),
            })?)?
        }
        "forge.milestone.attach_evidence" => {
            let input: MilestoneAttachEvidenceInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            let capability_id = input
                .capability_id
                .or(input.capability)
                .context("capability_id is required")?;
            let artifact_path = input
                .artifact_path
                .or(input.artifact)
                .context("artifact_path is required")?;
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(attach_milestone_evidence(
                store,
                MilestoneAttachEvidenceOptions {
                    version: &version,
                    capability_id: &capability_id,
                    kind: &input.kind,
                    summary: &input.summary,
                    artifact_path: &PathBuf::from(artifact_path),
                    approved_by: &input.approved_by,
                    origin: &origin,
                },
            )?)?
        }
        "forge.milestone.evidence_plan" => {
            let input: MilestoneEvidencePlanInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            let capability_id = input
                .capability_id
                .or(input.capability)
                .context("capability_id is required")?;
            let project_root = input.project_root.map(PathBuf::from);
            serde_json::to_value(build_milestone_evidence_plan(
                store,
                MilestoneEvidencePlanOptions {
                    version: &version,
                    capability_id: &capability_id,
                    project_root: project_root.as_deref(),
                    connected_brain: input.connected_brain.as_deref(),
                    connected_runtime: input.connected_runtime.as_deref(),
                },
            )?)?
        }
        "forge.milestone.prepare_evidence_inputs" => {
            let input: MilestonePrepareEvidenceInputsInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            let capability_id = input
                .capability_id
                .or(input.capability)
                .context("capability_id is required")?;
            let project_root = input.project_root.map(PathBuf::from);
            let provider_command = input.provider_command.map(PathBuf::from);
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(prepare_milestone_evidence_inputs(
                store,
                MilestonePrepareEvidenceInputsOptions {
                    version: &version,
                    capability_id: &capability_id,
                    project_root: project_root.as_deref(),
                    connected_brain: input.connected_brain.as_deref(),
                    connected_runtime: input.connected_runtime.as_deref(),
                    provider_command: provider_command.as_deref(),
                    model_id: input.model_id.as_deref(),
                    approval_ref: input.approval_ref.as_deref(),
                    apply: input.apply.unwrap_or(false),
                    approved_by: input.approved_by.as_deref(),
                    force: input.force.unwrap_or(false),
                    origin: &origin,
                },
            )?)?
        }
        "forge.milestone.collect_evidence" => {
            let input: MilestoneCollectEvidenceInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            let capability_id = input
                .capability_id
                .or(input.capability)
                .context("capability_id is required")?;
            let project_root = input.project_root.map(PathBuf::from);
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(collect_milestone_evidence(
                store,
                MilestoneCollectEvidenceOptions {
                    version: &version,
                    capability_id: &capability_id,
                    kind: input.kind.as_deref(),
                    project_root: project_root.as_deref(),
                    connected_brain: input.connected_brain.as_deref(),
                    connected_runtime: input.connected_runtime.as_deref(),
                    approved_by: &input.approved_by,
                    origin: &origin,
                },
            )?)?
        }
        "forge.milestone.collect_ready_evidence" => {
            let input: MilestoneCollectReadyEvidenceInput = parse_input(input)?;
            let version = input.version.unwrap_or_else(|| "0.5".to_string());
            let project_root = input.project_root.map(PathBuf::from);
            let origin = input.origin.unwrap_or_else(|| "mcp".to_string());
            serde_json::to_value(collect_ready_milestone_evidence(
                store,
                MilestoneCollectReadyEvidenceOptions {
                    version: &version,
                    project_root: project_root.as_deref(),
                    connected_brain: input.connected_brain.as_deref(),
                    connected_runtime: input.connected_runtime.as_deref(),
                    approved_by: &input.approved_by,
                    origin: &origin,
                },
            )?)?
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
            let project_root = input.project_root.as_deref().map(std::path::Path::new);
            serde_json::to_value(build_replacement_cli_demo_with_options(
                store,
                input.origin.as_deref().unwrap_or("mcp"),
                MilestoneCliDemoOptions {
                    project_root,
                    connected_brain: input.connected_brain.as_deref(),
                },
            )?)?
        }
        "forge.multimodal.status" => {
            let input: MultimodalStatusInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            serde_json::to_value(build_multimodal_status_with_feature_flag(feature_flag))?
        }
        "forge.multimodal.install_plan" => {
            let input: MultimodalInstallPlanInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            let capability = input
                .capability_id
                .or(input.capability)
                .ok_or_else(|| anyhow::anyhow!("capability_id is required"))?;
            serde_json::to_value(build_multimodal_install_plan(
                &capability,
                feature_flag.enabled,
            )?)?
        }
        "forge.multimodal.readiness" => {
            let input: MultimodalReadinessInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            let capability = input
                .capability_id
                .or(input.capability)
                .ok_or_else(|| anyhow::anyhow!("capability_id is required"))?;
            serde_json::to_value(build_multimodal_readiness(MultimodalReadinessOptions {
                capability_id: &capability,
                enable_experimental: feature_flag.enabled,
                explicit_allow: input.allow.unwrap_or(false),
                project_root: input.project_root.as_deref().map(std::path::Path::new),
            })?)?
        }
        "forge.multimodal.benchmark_template" => {
            let input: MultimodalBenchmarkTemplateInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            let capability = input
                .capability_id
                .or(input.capability)
                .ok_or_else(|| anyhow::anyhow!("capability_id is required"))?;
            serde_json::to_value(build_multimodal_benchmark_template(
                &capability,
                feature_flag.enabled,
            )?)?
        }
        "forge.multimodal.benchmark_result" => {
            let input: MultimodalBenchmarkResultInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            let capability = input
                .capability_id
                .or(input.capability)
                .ok_or_else(|| anyhow::anyhow!("capability_id is required"))?;
            let fixture = input
                .fixture_id
                .or(input.fixture)
                .ok_or_else(|| anyhow::anyhow!("fixture_id is required"))?;
            serde_json::to_value(build_multimodal_benchmark_result(
                MultimodalBenchmarkResultOptions {
                    capability_id: &capability,
                    fixture_id: &fixture,
                    enable_experimental: feature_flag.enabled,
                    approved_by: input.approved_by.as_deref(),
                    confirm_fixture_only: input.confirm_fixture_only.unwrap_or(false),
                },
            )?)?
        }
        "forge.multimodal.runtime_benchmark" => {
            let input: MultimodalRuntimeBenchmarkInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            let capability = input
                .capability_id
                .or(input.capability)
                .ok_or_else(|| anyhow::anyhow!("capability_id is required"))?;
            let fixture = input
                .fixture_id
                .or(input.fixture)
                .ok_or_else(|| anyhow::anyhow!("fixture_id is required"))?;
            serde_json::to_value(build_multimodal_runtime_benchmark(
                MultimodalRuntimeBenchmarkOptions {
                    capability_id: &capability,
                    fixture_id: &fixture,
                    enable_experimental: feature_flag.enabled,
                    project_root: input.project_root.as_deref().map(std::path::Path::new),
                    approved_by: input.approved_by.as_deref(),
                    confirm_runtime_execution: input.confirm_runtime_execution.unwrap_or(false),
                    allow_model: input.allow_model.unwrap_or(false),
                    connected_runtime: input.connected_runtime.as_deref(),
                },
            )?)?
        }
        "forge.multimodal.demo_plan" => {
            let input: MultimodalDemoPlanInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            let demo = input
                .demo_id
                .or(input.demo)
                .ok_or_else(|| anyhow::anyhow!("demo_id is required"))?;
            serde_json::to_value(build_multimodal_demo_plan(&demo, feature_flag.enabled)?)?
        }
        "forge.multimodal.demo_receipt" => {
            let input: MultimodalDemoReceiptInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            let demo = input
                .demo_id
                .or(input.demo)
                .ok_or_else(|| anyhow::anyhow!("demo_id is required"))?;
            let fixture = input
                .fixture_id
                .or(input.fixture)
                .ok_or_else(|| anyhow::anyhow!("fixture_id is required"))?;
            serde_json::to_value(build_multimodal_demo_receipt(
                MultimodalDemoReceiptOptions {
                    demo_id: &demo,
                    fixture_id: &fixture,
                    enable_experimental: feature_flag.enabled,
                    approved_by: input.approved_by.as_deref(),
                    confirm_local_fixture: input.confirm_local_fixture.unwrap_or(false),
                    allow_model: input.allow_model.unwrap_or(false),
                    allow_camera: input.allow_camera.unwrap_or(false),
                    allow_microphone: input.allow_microphone.unwrap_or(false),
                    allow_screen: input.allow_screen.unwrap_or(false),
                    allow_input: input.allow_input.unwrap_or(false),
                    allow_filesystem: input.allow_filesystem.unwrap_or(false),
                },
            )?)?
        }
        "forge.multimodal.guard" => {
            let input: MultimodalGuardInput = parse_input(input)?;
            let feature_flag = resolve_multimodal_feature_flag(
                input.enable_experimental.unwrap_or(false),
                input.project_root.as_deref().map(std::path::Path::new),
            );
            serde_json::to_value(evaluate_multimodal_guard(
                &input.capability,
                &input.action,
                feature_flag.enabled,
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

fn output_schema_selector(tool_name: &str) -> Option<Value> {
    match tool_name {
        "forge.context.request" => Some(json!({
            "input_field": "view",
            "default_value": "compact",
            "mapping": {
                "full": CONTEXT_SCHEMA_VERSION,
                "compact": CONTEXT_COMPACT_VIEW_SCHEMA_VERSION,
            }
        })),
        "forge.task.handoff" => Some(json!({
            "input_field": "view",
            "default_value": "compact",
            "mapping": {
                "full": EXECUTOR_HANDOFF_SCHEMA_VERSION,
                "compact": EXECUTOR_HANDOFF_COMPACT_SCHEMA_VERSION,
            }
        })),
        _ => None,
    }
}

fn context_request_input_schema() -> Value {
    let mut schema = object_schema(
        &[
            ("workflow_id", "string", "workflow id"),
            ("task_id", "string", "task id"),
            ("budget", "integer", "context byte budget"),
            (
                "project_root",
                "string",
                "optional project root containing .forge/memory-governance.json",
            ),
            (
                "view",
                "string",
                "full|compact response view; compact is optimized for executor context",
            ),
        ],
        &["workflow_id", "task_id"],
    );
    schema["properties"]["view"]["enum"] = json!(["full", "compact"]);
    schema["properties"]["view"]["default"] = json!("compact");
    schema
}

fn mission_execute_input_schema() -> Value {
    object_schema(
        &[
            ("mission_id", "string", "mission id"),
            ("task_id", "string", "assigned mission task id"),
            ("agent_id", "string", "assigned mission agent instance id"),
            (
                "idempotency_key",
                "string",
                "caller-stable execution idempotency key",
            ),
            ("purpose", "string", "bounded command purpose"),
            (
                "command",
                "array",
                "command argv; no shell interpolation is added",
            ),
            (
                "evidence",
                "array",
                "explicit semantic evidence kinds from the assigned task gate contract",
            ),
            (
                "approved_by",
                "string",
                "human or policy authority granting execution approval; required unless dry_run is true",
            ),
            (
                "approval_ttl_seconds",
                "integer",
                "bounded approval lifetime; defaults to 300 seconds",
            ),
            (
                "dry_run",
                "boolean",
                "return a non-persisted planned or blocked preview without spawning a command",
            ),
            (
                "allow_trusted_process_runtime",
                "boolean",
                "explicitly authorize the approved non-isolating process fallback",
            ),
        ],
        &[
            "mission_id",
            "task_id",
            "agent_id",
            "idempotency_key",
            "purpose",
            "command",
        ],
    )
}

fn mission_execution_reconcile_input_schema() -> Value {
    let mut schema = object_schema(
        &[
            ("receipt_id", "string", "execution receipt id"),
            (
                "outcome",
                "string",
                "safe reconciliation outcome; only no_effect_retry is accepted",
            ),
            (
                "approved_by",
                "string",
                "human or policy authority approving the reconciliation",
            ),
            (
                "reason",
                "string",
                "non-empty evidence explaining why the execution had no effect",
            ),
            (
                "confirm_no_effect_retry",
                "boolean",
                "explicit confirmation that retrying cannot duplicate an effect",
            ),
        ],
        &[
            "receipt_id",
            "outcome",
            "approved_by",
            "reason",
            "confirm_no_effect_retry",
        ],
    );
    schema["properties"]["outcome"]["enum"] = json!(["no_effect_retry"]);
    schema["properties"]["confirm_no_effect_retry"]["const"] = json!(true);
    schema
}

fn mission_submit_input_schema() -> Value {
    object_schema(
        &[
            ("mission_id", "string", "mission id"),
            ("task_id", "string", "assigned mission task id"),
            ("agent_id", "string", "assigned mission agent instance id"),
            (
                "idempotency_key",
                "string",
                "caller-stable submission idempotency key",
            ),
            (
                "receipt_id",
                "string",
                "persisted execution receipt proving the delivery",
            ),
            ("status", "string", "completed|repaired|failed|blocked"),
            ("summary", "string", "delivery summary"),
            (
                "artifacts",
                "array",
                "optional delivery artifact references",
            ),
            ("risks", "array", "optional unresolved risks"),
            ("followups", "array", "optional follow-up actions"),
        ],
        &[
            "mission_id",
            "task_id",
            "agent_id",
            "idempotency_key",
            "receipt_id",
            "summary",
        ],
    )
}

fn task_handoff_input_schema() -> Value {
    let mut schema = object_schema(
        &[
            ("workflow_id", "string", "workflow id"),
            ("task_id", "string", "task id"),
            ("executor", "string", "selected executor id"),
            ("budget", "integer", "context byte budget"),
            ("ttl_seconds", "integer", "lease TTL in seconds"),
            (
                "project_root",
                "string",
                "optional project root containing .forge/memory-governance.json",
            ),
            (
                "view",
                "string",
                "compact by default for executor use; full is reserved for audit and replay",
            ),
        ],
        &["workflow_id", "task_id", "executor"],
    );
    schema["properties"]["view"]["enum"] = json!(["compact", "full"]);
    schema["properties"]["view"]["default"] = json!("compact");
    schema
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
        let property_schema = if *name == "lanes" && *value_type == "array" {
            parallel_lane_array_schema(description)
        } else {
            json!({
                "type": value_type,
                "description": description
            })
        };
        props.insert((*name).to_string(), property_schema);
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": props,
        "required": required,
    })
}

fn parallel_lane_array_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "description": description,
        "minItems": 1,
        "maxItems": 64,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "id": {
                    "type": "string",
                    "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$",
                    "description": "canonical lane id such as frontend or backend"
                },
                "brain": {
                    "type": "string",
                    "description": "explicit executor id or supported alias; auto is rejected"
                },
                "agent_count": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 64,
                    "description": "maximum independent agents in this lane"
                },
                "parallel_group": {
                    "type": "string",
                    "description": "optional shared parallel wave id"
                },
                "responsibility": {
                    "type": "string",
                    "description": "optional bounded lane responsibility"
                }
            },
            "required": ["id", "brain", "agent_count"]
        }
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

fn addon_dirs_from_input(addon_dirs: Option<Vec<String>>) -> Vec<PathBuf> {
    let dirs = addon_dirs
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.trim().to_string())
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if dirs.is_empty() {
        default_addon_dirs()
    } else {
        dirs
    }
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
