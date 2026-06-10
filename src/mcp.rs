use crate::addon::{
    addon_observability_report, authorize_addon_permission, claim_addon_runtime_contract_dispatch,
    complete_addon_runtime_contract_dispatch, create_addon_migration_workflow,
    create_addon_package_lock, default_addon_dirs, disable_addon, downgrade_addon, enable_addon,
    enqueue_addon_planner_dispatch, enqueue_addon_runtime_contract_dispatch,
    evaluate_addon_runtime_contract_policy, execute_addon_planning_strategy,
    execute_addon_runtime_contract_dispatch, fetch_addon_package, install_addon,
    install_addon_package, list_addon_capability_index, list_addon_event_adapters,
    list_addon_marketplace, list_addon_permission_authorizations, list_addon_planner_registry,
    list_addon_runtime_contract_dispatches, list_addon_runtime_contracts,
    list_addon_runtime_workers, list_addon_trust_store, list_addon_views, list_installed_addons,
    load_addon_catalog_from_store, package_addon, publish_addon_package,
    register_addon_runtime_worker, resolve_goal_capabilities_with_registry_sync,
    resolve_goal_capabilities_with_store, revoke_addon_permission,
    run_addon_runtime_contract_dispatch, sync_addon_package_registry, trust_addon_package_key,
    uninstall_addon, upgrade_addon, validate_addon_catalog,
};
use crate::artifact::{hex_sha256, list_workflow_artifacts, ListedArtifact};
use crate::aws_ops::{
    run_check as run_aws_ops_check, run_inventory as run_aws_ops_inventory,
    run_raw as run_aws_ops_raw, AWS_OPS_COMMAND_SCHEMA,
};
use crate::checkpoint::load_latest_task_checkpoint;
use crate::context::{build_context_package_with_checkpoint, DEFAULT_CONTEXT_BUDGET};
use crate::cost::{
    build_cost_ledger, build_cost_ledger_history, maintain_cost_ledger,
    materialize_cost_ledger_index,
};
use crate::credential_vault::{
    run_describe as run_credential_vault_describe, run_records as run_credential_vault_records,
    CREDENTIAL_VAULT_COMMAND_SCHEMA,
};
use crate::event::{
    build_event_improvement_policy, build_event_observability_history,
    build_event_observability_index, build_event_service_plan, build_global_event_timeline,
    build_workflow_event_stream, emit_event_egress, ingest_inbound_event, list_event_services,
    list_inbound_event_inbox, recover_stale_event_services, route_inbound_event,
    run_event_runtime_daemon, run_event_runtime_reconcile, run_event_service_supervisor,
    run_event_webhook_ingress_service, run_event_worker_service, run_inbound_event_worker_loop,
    scan_inbound_event_inbox, EventEgressEmitInput, InboundEventIngestInput,
};
use crate::executor::load_executors;
use crate::handoff::build_task_handoff;
use crate::harness::{
    analyze_token_headroom, build_cli_wrapper_plan, persist_token_headroom_report,
    retrieve_headroom_blob, run_cli_harness_exec,
};
use crate::identity::{
    audit_tenant_index, ensure_workflow_policy, evaluate_tenant_policy_for_action,
    inspect_project_operating_context, link_identity, list_identity_links,
    list_identity_memberships, list_identity_registry, list_tenant_index,
    load_project_operating_context, resolve_identity, sync_project_operating_context,
    unlink_identity, update_identity_membership, IdentityLinkInput, IdentityMembershipUpdateInput,
};
use crate::improve::{rank_improvement_candidates_with_filter, ImprovementCandidateFilter};
use crate::inspection::inspect_workflow_with_focus;
use crate::interaction::{
    answer_human_interaction, create_choice_interaction, create_form_interaction,
    expire_human_interaction, list_human_interactions, CreateChoiceInteractionRequest,
};
use crate::interactive::{build_interactive_home, route_interactive_input, slash_command_catalog};
use crate::ir::{CreativeArtifact, TokenCollection};
use crate::memory::{
    list_memory_promotions, memory_cleanup_report, memory_policy_report, memory_retention_report,
    promote_memory, search_memory, MemoryCleanupOptions, MemoryPromotionOptions,
    MemoryRetentionOptions, MemorySearchOptions,
};
use crate::milestone::{
    build_milestone_export_demo, build_milestone_manifest, build_milestone_research,
    build_milestone_status, build_replacement_cli_demo,
};
use crate::multimodal::{
    build_multimodal_benchmark_template, build_multimodal_demo_plan, build_multimodal_install_plan,
    build_multimodal_status, evaluate_multimodal_guard,
};
use crate::patch::{build_patch_apply, build_patch_plan, build_patch_revert};
use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
};
use crate::request::{
    cancel_request, complete_ready_task, create_final_delivery_package, drive_request,
    ensure_final_audit, heartbeat_request, list_requests, load_request_status,
    recover_stale_request, resume_async_request, start_async_request, step_request,
    switch_request_executor, RequestExecutorSwitchInput, RequestTaskCompletionInput,
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
    parse_node_brain_agent_slot, patch_workflow_token, record_creative_collaboration_event,
    resolve_workflow_tokens, set_workflow_token_collection, update_workflow_goal,
    update_workflow_node_brain_routing, CreativeCollaborationEventRequest,
    WorkflowNodeBrainRoutingUpdateInput,
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
struct ImprovementCandidatesInput {
    limit: Option<usize>,
    workflow_ids: Option<Vec<String>>,
    goal_contains: Option<Vec<String>>,
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
struct HarnessWrapPlanInput {
    executor: String,
    command: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    forge_first: Option<bool>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HarnessExecInput {
    executor: String,
    command: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    forge_first: Option<bool>,
    workflow: Option<String>,
    workflow_id: Option<String>,
    run: Option<String>,
    run_id: Option<String>,
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
    dry_run: Option<bool>,
    allow_exec: Option<bool>,
    cwd: Option<String>,
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
}

#[derive(Debug, Deserialize)]
struct InboundEventInboxInput {
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct InboundEventScanInput {
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboundEventWorkerLoopInput {
    status: Option<String>,
    limit: Option<usize>,
    project_root: Option<String>,
    max_cycles: Option<usize>,
    interval_seconds: Option<u64>,
    idle_exit: Option<bool>,
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
struct RunCompleteTaskInput {
    run_id: String,
    task_id: String,
    executor: Option<String>,
    summary: String,
    #[serde(default)]
    artifacts: Vec<String>,
    evidence_command: Option<String>,
    evidence_summary: Option<String>,
    estimated_usd: Option<f64>,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    ttl_seconds: Option<u64>,
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
struct PatchRevertInput {
    workflow_id: String,
    task_id: String,
    apply_artifact: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CredentialVaultInput {
    contract: String,
    data: String,
    vault_bin: Option<String>,
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
                "Return declarative Addon event adapters by Addon, transport or direction so agents can discover ingress/egress contracts without channel-specific code.",
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
                "forge.cost.ledger",
                "Inspect Cost Ledger",
                "Return estimated task cost and observed event cost grouped by workflow, node, tenant and detected Addon source.",
                object_schema(
                    &[
                        ("workflow_id", "string", "optional workflow id filter"),
                        ("organization_id", "string", "optional organization filter"),
                        ("brand_id", "string", "optional brand filter"),
                        ("product_id", "string", "optional product filter"),
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
                        ("limit", "integer", "optional persisted row limit"),
                    ],
                    &[],
                ),
                "forge.cost_ledger_index.v1",
                &["forge", "cost", "materialize", "--output", "json"],
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
                "forge.interactive.home",
                "Inspect Interactive Home",
                "Return the no-argument Forge interactive home dashboard for agent-visible runtime state without launching a TTY.",
                object_schema(&[], &[]),
                "forge.interactive.home.v1",
                &["forge", "interactive", "home", "--output", "json"],
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
                object_schema(&[], &[]),
                "forge.memory_policy.v1",
                &["forge", "memory", "policy", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.memory.search",
                "Search File Memory",
                "Search Markdown memory snippets across global, organization, project and processing scopes with visibility/shareability filtering and line-range results.",
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
                "forge.run.complete_task",
                "Complete Ready Task With Evidence",
                "Record executor evidence for the current ready handoff task, generate a replayable execution trace, validate the executor response, promote the task and drive the next action.",
                object_schema(&[
                    ("run_id", "string", "run id"),
                    ("task_id", "string", "ready task id to complete"),
                    ("executor", "string", "codex|opencode|skill|mcp|custom executor id"),
                    ("summary", "string", "executor result summary without secrets"),
                    ("artifacts", "array", "optional local artifact paths to attach"),
                    ("evidence_command", "string", "optional command or gate that produced passing evidence"),
                    ("evidence_summary", "string", "optional passing evidence summary"),
                    ("estimated_usd", "number", "non-negative estimated executor cost"),
                    ("tokens_in", "integer", "non-negative input token count"),
                    ("tokens_out", "integer", "non-negative output token count"),
                    ("ttl_seconds", "integer", "heartbeat freshness TTL"),
                    ("origin", "string", "codex|opencode|skill|mcp"),
                ], &["run_id", "task_id", "summary"]),
                "forge.request_task_completion.v1",
                &["forge", "request", "complete-task", "--run", "<run-id>", "--task", "<task-id>", "--summary", "<summary>", "--output", "json"],
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
                ], &["workflow_id", "goal", "origin"]),
                "forge.workflow_goal_update.v1",
                &["forge", "workflow", "update-goal", "--workflow", "<workflow-id>", "--output", "json"],
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
                "forge.harness.wrap_plan",
                "Plan Forge-First CLI Wrapper",
                "Return a non-destructive Forge-first wrapper plan for Codex, Claude, Gemini or OpenCode with context budget and token-headroom environment shaping.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("command", "array", "command argv to launch under the harness"),
                    ("forge_first", "boolean", "prefer Forge context routing before native CLI defaults"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom env"),
                ], &["executor"]),
                "forge.harness.cli_wrapper_plan.v1",
                &["forge", "harness", "wrap-plan", "--executor", "<executor>", "--output", "json"],
                ToolFlags::new(true, false),
            ),
            tool(
                "forge.harness.exec",
                "Execute Forge Harness Receipt",
                "Return a dry-run or explicitly guarded execution receipt for a Forge-first brain CLI invocation, including executable resolution, env overlay and bounded output hashes.",
                object_schema(&[
                    ("executor", "string", "codex|claude|gemini|opencode"),
                    ("command", "array", "command argv to launch under the harness"),
                    ("forge_first", "boolean", "prefer Forge context routing before native CLI defaults"),
                    ("workflow_id", "string", "optional workflow lineage"),
                    ("run_id", "string", "optional async run lineage"),
                    ("context_budget", "integer", "context byte budget"),
                    ("token_headroom", "boolean", "enable token-headroom env"),
                    ("dry_run", "boolean", "default true; false requests guarded execution"),
                    ("allow_exec", "boolean", "must be true together with dry_run=false before executing"),
                    ("cwd", "string", "optional child working directory"),
                ], &["executor"]),
                "forge.harness.exec_receipt.v1",
                &["forge", "harness", "exec", "--executor", "<executor>", "--", "<cmd>"],
                ToolFlags::new(true, true),
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
            serde_json::to_value(build_global_event_timeline(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.limit,
                input.after_sequence,
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
            serde_json::to_value(build_event_observability_index(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                node_ref.as_deref(),
                addon_id.as_deref(),
                input.limit,
                input.after_sequence,
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
            serde_json::to_value(build_event_observability_history(
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
            serde_json::to_value(build_event_improvement_policy(
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
            )?)?
        }
        "forge.events.ingest" => {
            let input: InboundEventIngestInput = parse_input(input)?;
            serde_json::to_value(ingest_inbound_event(store, input)?)?
        }
        "forge.events.inbox" => {
            let input: InboundEventInboxInput = parse_input(input)?;
            serde_json::to_value(list_inbound_event_inbox(
                store,
                input.status.as_deref(),
                input.limit.unwrap_or(20),
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
                input.status.as_deref(),
                input.limit.unwrap_or(20),
                input.max_cycles.unwrap_or(1),
                input.interval_seconds.unwrap_or(300),
                input.idle_exit.unwrap_or(false),
                stop_file.as_deref(),
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
            serde_json::to_value(list_event_services(
                store,
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
        "forge.cost.ledger" => {
            let input: CostLedgerInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            serde_json::to_value(build_cost_ledger(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
            )?)?
        }
        "forge.cost.materialize" => {
            let input: CostLedgerMaterializeInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            serde_json::to_value(materialize_cost_ledger_index(
                store,
                workflow_id.as_deref(),
                organization_id.as_deref(),
                brand_id.as_deref(),
                product_id.as_deref(),
                input.source_kind.as_deref(),
                addon_id.as_deref(),
                input.limit,
            )?)?
        }
        "forge.cost.history" => {
            let input: CostLedgerHistoryInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            serde_json::to_value(build_cost_ledger_history(
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
            )?)?
        }
        "forge.cost.maintain" => {
            let input: CostLedgerMaintainInput = parse_input(input)?;
            let workflow_id = input.workflow_id.or(input.workflow);
            let organization_id = input.organization_id.or(input.organization);
            let brand_id = input.brand_id.or(input.brand);
            let product_id = input.product_id.or(input.product);
            let addon_id = input.addon_id.or(input.addon);
            serde_json::to_value(maintain_cost_ledger(
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
        "forge.interactive.home" => serde_json::to_value(build_interactive_home(store)?)?,
        "forge.brain_router" => serde_json::to_value(load_executors(store)?.brain_router)?,
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
                addon_id.as_deref(),
                &contract_id,
                &input.goal,
                &constraints,
                workflow_id.as_deref(),
                task_id.as_deref(),
                input.context.unwrap_or_else(|| serde_json::json!({})),
                input.source.as_deref().unwrap_or("mcp"),
                input.dry_run.unwrap_or(false),
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
                addon_id.as_deref(),
                &contract_id,
                &input.goal,
                &constraints,
                workflow_id.as_deref(),
                task_id.as_deref(),
                input.context.unwrap_or_else(|| serde_json::json!({})),
                &worker_id,
                input.lease_seconds.unwrap_or(300),
                input.source.as_deref().unwrap_or("mcp"),
                input.dry_run.unwrap_or(false),
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
                &dispatch_id,
                &worker_id,
                input.status.as_deref().unwrap_or("completed"),
                input.result.unwrap_or_else(|| serde_json::json!({})),
                input.signature.as_deref(),
                input.attestation.unwrap_or_else(|| serde_json::json!({})),
                input.dry_run.unwrap_or(false),
            )?)?
        }
        "forge.addons.register_worker" => {
            let input: AddonRuntimeWorkerRegisterInput = parse_input(input)?;
            let worker_id = input.worker_id.or(input.worker).ok_or_else(|| {
                anyhow::anyhow!("forge.addons.register_worker requires worker_id")
            })?;
            serde_json::to_value(register_addon_runtime_worker(
                store,
                &worker_id,
                &input.runtime,
                input.status.as_deref().unwrap_or("available"),
                input.trust_level.as_deref().unwrap_or("local"),
                input.source.as_deref().unwrap_or("mcp"),
                input.data.unwrap_or_else(|| serde_json::json!({})),
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
                    &registry_sources,
                    registry_cache_dir.as_deref(),
                    input.allow_remote_registry.unwrap_or(false),
                    input.registry_max_bytes.unwrap_or(10 * 1024 * 1024),
                    input.registry_max_packages.unwrap_or(50),
                    registry_lock_path.as_deref(),
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
            serde_json::to_value(package_addon(
                store,
                &PathBuf::from(input.manifest),
                &addon_dirs,
                input.repository.as_deref(),
                input.channel.as_deref().unwrap_or("stable"),
                input.signature.as_deref(),
                input.public_key.as_deref(),
                package_path.as_deref(),
            )?)?
        }
        "forge.addons.trust_key" => {
            let input: AddonTrustKeyInput = parse_input(input)?;
            serde_json::to_value(trust_addon_package_key(
                store,
                &input.repository,
                input.channel.as_deref().unwrap_or("stable"),
                &input.public_key,
                input.trust_level.as_deref().unwrap_or("trusted"),
                input.approved_by.as_deref().unwrap_or("human"),
                input.source.as_deref().unwrap_or("mcp"),
                input.data.unwrap_or_else(|| serde_json::json!({})),
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
        "forge.memory.policy" => serde_json::to_value(memory_policy_report(store))?,
        "forge.memory.search" => {
            let input: MemorySearchInput = parse_input(input)?;
            serde_json::to_value(search_memory(
                store,
                MemorySearchOptions {
                    query: input.query,
                    workflow_id: input.workflow_id,
                    scopes: input.scopes,
                    audience: input.audience.unwrap_or_else(|| "private".to_string()),
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
                    evidence_summary: input.evidence_summary.as_deref(),
                    estimated_usd: input.estimated_usd.unwrap_or(0.0),
                    tokens_in: input.tokens_in.unwrap_or(0),
                    tokens_out: input.tokens_out.unwrap_or(0),
                    ttl_seconds: input.ttl_seconds.unwrap_or(300),
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
            serde_json::to_value(update_workflow_goal(
                store,
                &input.workflow_id,
                &input.goal,
                &input.origin,
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
            ensure_workflow_policy(store, &input.workflow_id, "context request")?;
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
        "forge.harness.wrap_plan" => {
            let input: HarnessWrapPlanInput = parse_input(input)?;
            let command = input.command.or(input.cmd).unwrap_or_default();
            let workflow_id = input.workflow_id.or(input.workflow);
            let run_id = input.run_id.or(input.run);
            serde_json::to_value(build_cli_wrapper_plan(
                &input.executor,
                &command,
                input.forge_first.unwrap_or(true),
                workflow_id.as_deref(),
                run_id.as_deref(),
                input.context_budget.unwrap_or(DEFAULT_CONTEXT_BUDGET),
                input.token_headroom.unwrap_or(true),
            ))?
        }
        "forge.harness.exec" => {
            let input: HarnessExecInput = parse_input(input)?;
            let command = input.command.or(input.cmd).unwrap_or_default();
            let workflow_id = input.workflow_id.or(input.workflow);
            let run_id = input.run_id.or(input.run);
            let cwd = input.cwd.as_deref().map(std::path::Path::new);
            serde_json::to_value(run_cli_harness_exec(
                &input.executor,
                &command,
                input.forge_first.unwrap_or(true),
                workflow_id.as_deref(),
                run_id.as_deref(),
                input.context_budget.unwrap_or(DEFAULT_CONTEXT_BUDGET),
                input.token_headroom.unwrap_or(true),
                input.dry_run.unwrap_or(true),
                input.allow_exec.unwrap_or(false),
                cwd,
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
