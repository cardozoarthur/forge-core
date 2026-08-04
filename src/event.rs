use crate::addon::{
    enqueue_addon_runtime_contract_dispatch, evaluate_addon_runtime_contract_policy,
    list_addon_event_adapters, load_addon_catalog_from_store, AddonCatalog, AddonEventAdapterView,
    AddonEventChannelView, AddonEventExtensionRegistry, AddonEventListenerView,
    AddonEventTriggerView, AddonPermissionGate, AddonRuntimeContractDispatchReport,
    AddonRuntimeContractPolicyEntry, EventAdapterCredentialVaultRef, EventAdapterDeclaration,
};
use crate::artifact::hex_sha256;
use crate::checkpoint::{record_task_checkpoint, TaskCheckpointRequest};
use crate::credential_vault::resolve_credential_vault_bin;
use crate::graph::{create_workflow, Workflow};
use crate::identity::{
    ensure_operating_context_policy, ensure_workflow_policy, load_project_operating_context,
    resolve_identity, IdentityAliasView,
};
use crate::intent::parse_intent_with_catalog_and_context;
use crate::intent::{
    BrandIdentitySpec, ContextIdentityRef, DesignSystemSpec, OperatingContextSpec,
    OperatingPolicySpec,
};
use crate::interaction::answer_human_interaction;
use crate::registry::list_workflows;
use crate::request::{
    complete_ready_task, drive_request, load_run_record, RequestTaskCompletionInput,
};
use crate::schedule::{
    build_schedule_worker_status, scan_due_workflows_parallel, ScheduleScanDueReport,
    ScheduleWorkerStatusReport,
};
use crate::storage::{
    EventServiceWrite, ForgeStore, GlobalEventWrite, InboundEventRecord, StoreEvent,
    StoredEventObservabilityRecord, StoredEventServiceRecord, StoredGlobalEventRecord,
};
use crate::workflow::{
    add_workflow_task, add_workflow_task_dependency, attach_workflow_artifact,
    clear_workflow_task_impediment, complete_workflow, pause_workflow,
    remove_workflow_task_dependency, resume_workflow, set_workflow_task_impediment,
    set_workflow_task_priority, update_workflow_goal_with_expected_revision,
    update_workflow_task_with_expected_revision, ArtifactAttachReport, WorkflowTaskAddInput,
    WorkflowTaskDependencyInput, WorkflowTaskImpedimentClearInput, WorkflowTaskImpedimentInput,
    WorkflowTaskPriorityInput, WorkflowTaskUpdateInput,
};
use anyhow::{bail, Context, Result};
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, NaiveDate, NaiveDateTime, Timelike, Utc,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const EVENT_STREAM_SCHEMA_VERSION: &str = "forge.event_stream.v1";
pub const EVENT_ENVELOPE_SCHEMA_VERSION: &str = "forge.event_envelope.v1";
pub const EVENT_TIMELINE_SCHEMA_VERSION: &str = "forge.event_timeline.v1";
pub const EVENT_OBSERVABILITY_INDEX_SCHEMA_VERSION: &str = "forge.event_observability_index.v1";
pub const EVENT_OBSERVABILITY_HISTORY_SCHEMA_VERSION: &str = "forge.event_observability_history.v1";
pub const EVENT_IMPROVEMENT_POLICY_SCHEMA_VERSION: &str = "forge.event_improvement_policy.v1";
pub const EVENT_INBOX_SCHEMA_VERSION: &str = "forge.event_inbox.v1";
pub const EVENT_INGEST_SCHEMA_VERSION: &str = "forge.event_ingest.v1";
pub const EVENT_ROUTE_SCHEMA_VERSION: &str = "forge.event_route.v1";
pub const EVENT_IDENTITY_CONTEXT_SCHEMA_VERSION: &str = "forge.event_identity_context.v1";
pub const EVENT_WORKER_SCHEMA_VERSION: &str = "forge.event_worker.v1";
pub const EVENT_WORKER_LOOP_SCHEMA_VERSION: &str = "forge.event_worker_loop.v1";
pub const EVENT_SERVICE_PLAN_SCHEMA_VERSION: &str = "forge.event_service_plan.v1";
pub const EVENT_SERVICE_RUN_SCHEMA_VERSION: &str = "forge.event_service_run.v1";
pub const EVENT_SERVICE_SUPERVISOR_SCHEMA_VERSION: &str = "forge.event_service_supervisor.v1";
pub const EVENT_RUNTIME_RECONCILE_SCHEMA_VERSION: &str = "forge.event_runtime_reconcile.v1";
pub const EVENT_RUNTIME_DAEMON_SCHEMA_VERSION: &str = "forge.event_runtime_daemon.v1";
pub const EVENT_SERVICES_SCHEMA_VERSION: &str = "forge.event_services.v1";
pub const EVENT_SERVICES_RECOVERY_SCHEMA_VERSION: &str = "forge.event_services_recovery.v1";
pub const EVENT_WEBHOOK_INGRESS_SCHEMA_VERSION: &str = "forge.event_webhook_ingress.v1";
pub const EVENT_WEBHOOK_INGRESS_RESPONSE_SCHEMA_VERSION: &str =
    "forge.event_webhook_ingress.response.v1";
pub const EVENT_ADAPTER_POLICY_SCHEMA_VERSION: &str = "forge.event_adapter_policy.v1";
pub const EVENT_ADDON_ADAPTER_PLAN_SCHEMA_VERSION: &str = "forge.event_addon_adapter_plan.v1";
pub const EVENT_EXTENSION_MATCHES_SCHEMA_VERSION: &str = "forge.event_extension_matches.v1";
pub const EVENT_WORKFLOW_ACTIVATION_PLAN_SCHEMA_VERSION: &str =
    "forge.event_workflow_activation_plan.v1";
pub const EVENT_ACTIVATION_DISPATCH_SCHEMA_VERSION: &str = "forge.event_activation_dispatch.v1";
pub const EVENT_EGRESS_EMIT_SCHEMA_VERSION: &str = "forge.event_egress_emit.v1";
pub const EVENT_EGRESS_REQUEST_SCHEMA_VERSION: &str = "forge.event_egress_request.v1";
pub const EVENT_EGRESS_DELIVERY_EVIDENCE_SCHEMA_VERSION: &str =
    "forge.event_egress_delivery_evidence.v1";
const WEBHOOK_TIMESTAMP_HEADER: &str = "x-forge-timestamp";
const WEBHOOK_NONCE_HEADER: &str = "x-forge-nonce";
const WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS: i64 = 300;
const WEBHOOK_RATE_LIMIT_PER_MINUTE_DEFAULT: usize = 60;
const WEBHOOK_ALLOW_INSECURE_LOCAL_ENV: &str = "FORGE_ALLOW_INSECURE_LOCAL_WEBHOOK";
const WEBHOOK_RATE_LIMIT_ENV: &str = "FORGE_WEBHOOK_RATE_LIMIT_PER_MINUTE";
const MIN_WEBHOOK_HMAC_SECRET_BYTES: usize = 32;

type WebhookIngressProgressCallback<'a> =
    Option<&'a mut dyn FnMut(&[EventWebhookIngressEntry], &str) -> Result<()>>;

#[derive(Debug, Clone, Copy, Default)]
pub struct EventObservabilityQuery<'a> {
    pub workflow_id: Option<&'a str>,
    pub organization_id: Option<&'a str>,
    pub brand_id: Option<&'a str>,
    pub product_id: Option<&'a str>,
    pub node_ref: Option<&'a str>,
    pub addon_id: Option<&'a str>,
    pub limit: Option<usize>,
    pub after_sequence: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EventObservabilityHistoryQuery<'a> {
    pub observability: EventObservabilityQuery<'a>,
    pub bucket: Option<&'a str>,
    pub group_by: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EventImprovementPolicyQuery<'a> {
    pub observability: EventObservabilityQuery<'a>,
    pub min_event_count: Option<usize>,
    pub min_total_duration_ms: Option<i64>,
    pub min_total_retry_count: Option<i64>,
    pub min_context_pressure_bps: Option<i64>,
    pub min_total_wait_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
pub struct InboundEventWorkerLoopOptions<'a> {
    pub status: Option<&'a str>,
    pub limit: usize,
    pub max_cycles: usize,
    pub interval_seconds: u64,
    pub idle_exit: bool,
    pub dispatch_activations: bool,
    pub stop_file: Option<&'a Path>,
}

struct BlockedAdapterPolicyInput<'a> {
    normalized_action: String,
    transport: Option<String>,
    schema: Option<String>,
    auth_verified: Option<bool>,
    status: &'a str,
    issue: &'a str,
    matched_adapter: Option<AddonEventAdapterView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowEventStreamReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub workflow_goal: String,
    pub workflow_mode: String,
    pub tenant_context: EventTenantContext,
    pub total_event_count: usize,
    pub event_count: usize,
    pub events: Vec<WorkflowEventEnvelope>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalEventTimelineReport {
    pub schema_version: String,
    pub status: String,
    pub filters: EventTimelineFilters,
    pub page: EventTimelinePage,
    pub total_event_count: usize,
    pub event_count: usize,
    pub events: Vec<WorkflowEventEnvelope>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventTimelineFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventTimelinePage {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityIndexReport {
    pub schema_version: String,
    pub status: String,
    pub index_source: String,
    pub filters: EventObservabilityIndexFilters,
    pub page: EventTimelinePage,
    pub summary: EventObservabilitySummary,
    pub tenants: Vec<EventObservabilityTenantSummary>,
    pub workflows: Vec<EventObservabilityWorkflowSummary>,
    pub nodes: Vec<EventObservabilityNodeSummary>,
    pub addons: Vec<EventObservabilityAddonSummary>,
    pub event_count: usize,
    pub events: Vec<EventObservabilityRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityHistoryReport {
    pub schema_version: String,
    pub status: String,
    pub index_source: String,
    pub filters: EventObservabilityHistoryFilters,
    pub summary: EventObservabilitySummary,
    pub bucket_count: usize,
    pub buckets: Vec<EventObservabilityHistoryBucket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventImprovementPolicyReport {
    pub schema_version: String,
    pub status: String,
    pub index_source: String,
    pub filters: EventImprovementPolicyFilters,
    pub thresholds: EventImprovementPolicyThresholds,
    pub summary: EventObservabilitySummary,
    pub recommendation_count: usize,
    pub recommendations: Vec<EventImprovementRecommendation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityIndexFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityHistoryFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    pub bucket: String,
    pub group_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventImprovementPolicyFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventImprovementPolicyThresholds {
    pub min_event_count: usize,
    pub min_total_duration_ms: i64,
    pub min_total_retry_count: i64,
    pub min_context_pressure_bps: i64,
    pub min_total_wait_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventImprovementRecommendation {
    pub id: String,
    pub kind: String,
    pub priority: String,
    pub scope: String,
    pub workflow_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    pub event_count: usize,
    pub ai_signal_count: usize,
    pub total_duration_ms: i64,
    pub total_retry_count: i64,
    pub total_wait_seconds: i64,
    pub total_selected_context_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_pressure_bps: Option<i64>,
    pub first_event_sequence: i64,
    pub last_event_sequence: i64,
    pub recommended_policy: String,
    pub recommended_action: String,
    pub reason: String,
    pub suggested_commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityHistoryBucket {
    pub bucket: String,
    pub bucket_start: String,
    pub bucket_end: String,
    pub group_by: String,
    pub group_id: String,
    pub group: EventObservabilityHistoryGroup,
    pub summary: EventObservabilitySummary,
    pub first_event_sequence: i64,
    pub last_event_sequence: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityHistoryGroup {
    pub group_by: String,
    pub group_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EventObservabilitySummary {
    pub total_event_count: usize,
    pub node_event_count: usize,
    pub addon_event_count: usize,
    pub duration_event_count: usize,
    pub total_duration_ms: i64,
    pub retry_event_count: usize,
    pub total_retry_count: i64,
    pub wait_event_count: usize,
    pub total_wait_seconds: i64,
    pub context_event_count: usize,
    pub context_pressure_event_count: usize,
    pub total_context_budget_bytes: i64,
    pub total_selected_context_bytes: i64,
    pub total_context_remaining_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_pressure_bps: Option<i64>,
    pub memory_event_count: usize,
    pub severity_counts: Vec<EventObservabilityCount>,
    pub category_counts: Vec<EventObservabilityCount>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityCount {
    pub id: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityTenantSummary {
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub event_count: usize,
    pub workflow_count: usize,
    pub node_count: usize,
    pub addon_count: usize,
    pub total_duration_ms: i64,
    pub total_retry_count: i64,
    pub total_wait_seconds: i64,
    pub context_event_count: usize,
    pub context_pressure_event_count: usize,
    pub total_context_budget_bytes: i64,
    pub total_selected_context_bytes: i64,
    pub total_context_remaining_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_pressure_bps: Option<i64>,
    pub memory_event_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityWorkflowSummary {
    pub workflow_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub event_count: usize,
    pub node_count: usize,
    pub addon_count: usize,
    pub total_duration_ms: i64,
    pub total_retry_count: i64,
    pub total_wait_seconds: i64,
    pub context_event_count: usize,
    pub context_pressure_event_count: usize,
    pub total_context_budget_bytes: i64,
    pub total_selected_context_bytes: i64,
    pub total_context_remaining_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_pressure_bps: Option<i64>,
    pub memory_event_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityNodeSummary {
    pub workflow_id: String,
    pub node_ref: String,
    pub addon_id: Option<String>,
    pub event_count: usize,
    pub kinds: Vec<String>,
    pub total_duration_ms: i64,
    pub total_retry_count: i64,
    pub total_wait_seconds: i64,
    pub context_event_count: usize,
    pub context_pressure_event_count: usize,
    pub total_context_budget_bytes: i64,
    pub total_selected_context_bytes: i64,
    pub total_context_remaining_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_pressure_bps: Option<i64>,
    pub memory_event_count: usize,
    pub last_event_sequence: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityAddonSummary {
    pub addon_id: String,
    pub event_count: usize,
    pub workflow_count: usize,
    pub node_count: usize,
    pub total_duration_ms: i64,
    pub total_retry_count: i64,
    pub total_wait_seconds: i64,
    pub context_event_count: usize,
    pub context_pressure_event_count: usize,
    pub total_context_budget_bytes: i64,
    pub total_selected_context_bytes: i64,
    pub total_context_remaining_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_pressure_bps: Option<i64>,
    pub memory_event_count: usize,
    pub last_event_sequence: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservabilityRecord {
    pub event_id: String,
    pub store_sequence: i64,
    pub workflow_id: String,
    pub kind: String,
    pub category: String,
    pub severity: String,
    pub origin: String,
    pub source: String,
    pub occurred_at: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub node_ref: Option<String>,
    pub addon_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub retry_count: Option<i64>,
    pub wait_state: Option<String>,
    pub wait_seconds: Option<i64>,
    pub context_budget_bytes: Option<i64>,
    pub selected_context_bytes: Option<i64>,
    pub context_remaining_bytes: Option<i64>,
    pub context_pressure_bps: Option<i64>,
    pub context_pressure_state: Option<String>,
    pub memory_level: Option<String>,
    pub memory_scope: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InboundEventIngestInput {
    pub origin: String,
    pub action: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventIngestReport {
    pub schema_version: String,
    pub status: String,
    pub event: InboundEventView,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventInboxReport {
    pub schema_version: String,
    pub status: String,
    pub filters: InboundEventInboxFilters,
    pub event_count: usize,
    pub events: Vec<InboundEventView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventInboxFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventWorkerReport {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub requested_status: String,
    pub limit: usize,
    pub scanned_count: usize,
    pub routed_count: usize,
    pub failed_count: usize,
    pub events: Vec<InboundEventWorkerEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventWorkerEntry {
    pub event_id: String,
    pub origin: String,
    pub action: String,
    pub before_status: String,
    pub after_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_context: Option<EventIdentityContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_event_adapter_plan: Option<InboundEventAddonAdapterPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_dispatch: Option<InboundEventActivationDispatchReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventWorkerLoopReport {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub requested_status: String,
    pub limit: usize,
    pub max_cycles: usize,
    pub interval_seconds: u64,
    pub idle_exit: bool,
    pub cycle_count: usize,
    pub scanned_count: usize,
    pub routed_count: usize,
    pub failed_count: usize,
    pub idle_cycle_count: usize,
    pub stopped_reason: String,
    pub stop_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_file: Option<String>,
    pub cycles: Vec<InboundEventWorkerLoopCycle>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventWorkerLoopCycle {
    pub cycle: usize,
    pub slept_after_seconds: u64,
    pub report: InboundEventWorkerReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventServicePlanReport {
    pub schema_version: String,
    pub status: String,
    pub service_id: String,
    pub service_kind: String,
    pub mode: String,
    pub project_root: String,
    pub command: Vec<String>,
    pub settings: Value,
    pub lease: Value,
    pub backoff: Value,
    pub shutdown: Value,
    pub health: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventServiceRunReport {
    pub schema_version: String,
    pub status: String,
    pub service: EventServiceView,
    pub lease: Value,
    pub heartbeat: Value,
    pub plan: EventServicePlanReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_report: Option<InboundEventWorkerLoopReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_report: Option<EventWebhookIngressReport>,
    pub health: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventServiceSupervisorReport {
    pub schema_version: String,
    pub status: String,
    pub supervisor_id: String,
    pub service_kind: String,
    pub project_root: String,
    pub max_runs: usize,
    pub run_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub stopped_count: usize,
    pub backoff_initial_seconds: u64,
    pub backoff_max_seconds: u64,
    pub stopped_reason: String,
    pub stop_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_file: Option<String>,
    pub health: Value,
    pub runs: Vec<EventServiceSupervisorRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventServiceSupervisorRun {
    pub run: usize,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
    pub backoff_after_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<EventServiceRunReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventServiceListReport {
    pub schema_version: String,
    pub status: String,
    pub filters: Value,
    pub service_count: usize,
    pub services: Vec<EventServiceView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventServiceRecoveryReport {
    pub schema_version: String,
    pub status: String,
    pub recovery_id: String,
    pub project_root: String,
    pub origin: String,
    pub filters: Value,
    pub scanned_count: usize,
    pub stale_running_count: usize,
    pub recovered_count: usize,
    pub services: Vec<EventServiceView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeReconcileReport {
    pub schema_version: String,
    pub status: String,
    pub reconcile_id: String,
    pub project_root: String,
    pub execute: bool,
    pub dispatch_activations: bool,
    pub registry: EventRuntimeRegistrySnapshot,
    pub inbox: EventRuntimeInboxSnapshot,
    pub services: EventRuntimeServiceSnapshot,
    pub recover_stale_services: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_recovery: Option<EventServiceRecoveryReport>,
    pub recommendation_count: usize,
    pub recommendations: Vec<EventRuntimeServiceRecommendation>,
    pub execution_count: usize,
    pub executions: Vec<EventServiceSupervisorReport>,
    pub schedule_execution_count: usize,
    pub schedule_scale_to_zero_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<EventRuntimeScheduleSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeScheduleSnapshot {
    pub schema_version: String,
    pub scan_schedules: bool,
    pub executor: String,
    pub max_workers: usize,
    pub ttl_seconds: u64,
    pub worker_status: ScheduleWorkerStatusReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_due: Option<ScheduleScanDueReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeRegistrySnapshot {
    pub schema_version: String,
    pub workflow_count: usize,
    pub persistent_workflows: usize,
    pub idle_waiting_for_events: usize,
    pub scaled_to_zero: usize,
    pub operator_actions: Vec<EventRuntimeOperatorActionSummary>,
    pub actionable_workflows: Vec<EventRuntimeWorkflowTarget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeOperatorActionSummary {
    pub action: String,
    pub workflow_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeWorkflowTarget {
    pub workflow_id: String,
    pub current_goal: String,
    pub lifecycle_kind: String,
    pub operational_state: String,
    pub operator_action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeInboxSnapshot {
    pub schema_version: String,
    pub status_filter: String,
    pub pending_event_count: usize,
    pub sampled_limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeServiceSnapshot {
    pub schema_version: String,
    pub service_kind: String,
    pub service_count: usize,
    pub running_count: usize,
    pub active_lease_count: usize,
    pub stale_running_count: usize,
    pub terminal_count: usize,
    pub services: Vec<EventServiceView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeServiceRecommendation {
    pub service_kind: String,
    pub action: String,
    pub required: bool,
    pub reason: String,
    pub workflow_count: usize,
    pub pending_event_count: usize,
    pub active_service_count: usize,
    pub workflow_ids: Vec<String>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeDaemonReport {
    pub schema_version: String,
    pub status: String,
    pub service: EventServiceView,
    pub project_root: String,
    pub execute: bool,
    pub requested_status: String,
    pub limit: usize,
    pub service_limit: usize,
    pub max_cycles: usize,
    pub interval_seconds: u64,
    pub idle_exit: bool,
    pub dispatch_activations: bool,
    pub continuous: bool,
    pub cycle_retention: usize,
    pub recover_stale_services: bool,
    pub scan_schedules: bool,
    pub schedule_executor: String,
    pub schedule_max_workers: usize,
    pub schedule_ttl_seconds: u64,
    pub cycle_count: usize,
    pub retained_cycle_count: usize,
    pub dropped_cycle_count: usize,
    pub recommendation_count: usize,
    pub execution_count: usize,
    pub schedule_execution_count: usize,
    pub schedule_scale_to_zero_count: usize,
    pub failed_count: usize,
    pub stopped_reason: String,
    pub stop_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_file: Option<String>,
    pub health: Value,
    pub cycles: Vec<EventRuntimeDaemonCycle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRuntimeDaemonCycle {
    pub cycle: usize,
    pub slept_after_seconds: u64,
    pub report: EventRuntimeReconcileReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventServiceView {
    pub id: String,
    pub service_kind: String,
    pub status: String,
    pub tenant_context: EventTenantContext,
    pub lease_owner: String,
    pub lease_id: String,
    pub lease_acquired_at: String,
    pub lease_expires_at: String,
    pub last_heartbeat_at: String,
    pub heartbeat_ttl_seconds: u64,
    pub data: Value,
    pub created_at: String,
    pub updated_at: String,
}

impl From<StoredEventServiceRecord> for EventServiceView {
    fn from(record: StoredEventServiceRecord) -> Self {
        let operating_context =
            serde_json::from_value::<OperatingContextSpec>(record.tenant_context)
                .unwrap_or_default();
        Self {
            id: record.id,
            service_kind: record.service_kind,
            status: record.status,
            tenant_context: EventTenantContext::from(&operating_context),
            lease_owner: record.lease_owner,
            lease_id: record.lease_id,
            lease_acquired_at: record.lease_acquired_at,
            lease_expires_at: record.lease_expires_at,
            last_heartbeat_at: record.last_heartbeat_at,
            heartbeat_ttl_seconds: record.heartbeat_ttl_seconds,
            data: record.data,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EventWebhookIngressReport {
    pub schema_version: String,
    pub status: String,
    pub bind_address: String,
    pub path: String,
    pub default_origin: String,
    pub default_action: String,
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub route_after_ingest: bool,
    pub auth: EventWebhookIngressAuthReport,
    pub max_requests: usize,
    pub max_body_bytes: usize,
    pub request_count: usize,
    pub ingested_count: usize,
    pub routed_count: usize,
    pub failed_count: usize,
    pub stopped_reason: String,
    pub stop_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_file: Option<String>,
    pub events: Vec<EventWebhookIngressEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventWebhookIngressEntry {
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub http_status: u16,
    pub status: String,
    pub origin: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<InboundEventView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<InboundEventRouteReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventWebhookIngressAuthReport {
    pub required: bool,
    pub scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_clock_skew_seconds: Option<i64>,
    pub replay_protection: bool,
    pub rate_limit_per_minute: usize,
}

struct WebhookHttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

struct WebhookHmacVerifier {
    secret_env: String,
    signature_header: String,
    secret: Vec<u8>,
}

#[derive(Debug)]
struct WebhookIngressSecurityState {
    seen_nonces: BTreeMap<String, Instant>,
    requests_by_peer: BTreeMap<IpAddr, VecDeque<Instant>>,
    rate_limit_per_minute: usize,
    allow_unsigned_mutations: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventRouteReport {
    pub schema_version: String,
    pub status: String,
    pub event_id: String,
    pub action: String,
    pub origin: String,
    pub adapter_policy: InboundEventAdapterPolicyReport,
    pub addon_event_adapter_plan: InboundEventAddonAdapterPlan,
    pub route_decision: String,
    pub workflow_id: Option<String>,
    pub workflow_goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_workflow: Option<Workflow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_result: Option<Value>,
    pub event: InboundEventView,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventActivationDispatchReport {
    pub schema_version: String,
    pub status: String,
    pub event_id: String,
    pub dry_run: bool,
    pub activation_count: usize,
    pub dispatch_attempt_count: usize,
    pub queued_count: usize,
    pub dry_run_count: usize,
    pub blocked_count: usize,
    pub skipped_count: usize,
    pub route: InboundEventRouteReport,
    pub dispatch_reports: Vec<AddonRuntimeContractDispatchReport>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventAdapterPolicyReport {
    pub schema_version: String,
    pub status: String,
    pub allowed: bool,
    pub enforced: bool,
    pub origin: String,
    pub action: String,
    pub normalized_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_verified: Option<bool>,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_adapter: Option<AddonEventAdapterView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventAddonAdapterPlan {
    pub schema_version: String,
    pub status: String,
    pub enforced: bool,
    pub origin: String,
    pub action: String,
    pub normalized_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_verified: Option<bool>,
    pub source_candidate_count: usize,
    pub matched_count: usize,
    pub allowed_count: usize,
    pub blocked_count: usize,
    pub event_extension_matches: InboundEventExtensionMatches,
    pub event_workflow_activation_plan: InboundEventWorkflowActivationPlan,
    pub adapters: Vec<InboundEventAddonAdapterPlanEntry>,
    pub next_commands: Vec<Vec<String>>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventExtensionMatches {
    pub schema_version: String,
    pub status: String,
    pub matched_trigger_count: usize,
    pub matched_listener_count: usize,
    pub matched_channel_count: usize,
    pub triggers: Vec<AddonEventTriggerView>,
    pub listeners: Vec<AddonEventListenerView>,
    pub channels: Vec<AddonEventChannelView>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventWorkflowActivationPlan {
    pub schema_version: String,
    pub status: String,
    pub activation_count: usize,
    pub dispatch_ready_count: usize,
    pub blocked_count: usize,
    pub activations: Vec<InboundEventWorkflowActivation>,
    pub next_commands: Vec<Vec<String>>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventWorkflowActivation {
    pub id: String,
    pub source_kind: String,
    pub source_id: String,
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub capability_id: String,
    pub workflow_extension_id: String,
    pub event_type: String,
    pub channel: String,
    pub adapter_id: String,
    pub normalized_action: String,
    pub operation: String,
    pub permission_gate: AddonPermissionGate,
    pub runtime_contract_count: usize,
    pub dispatch_allowed: bool,
    pub runtime_contracts: Vec<AddonRuntimeContractPolicyEntry>,
    pub dispatch_commands: Vec<Vec<String>>,
    pub issues: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventAddonAdapterPlanEntry {
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub adapter_id: String,
    pub adapter_title: String,
    pub transport: String,
    pub direction: String,
    pub status: String,
    pub allowed: bool,
    pub source_matched: bool,
    pub action_matched: bool,
    pub schema_matched: bool,
    pub auth_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_verified: Option<bool>,
    pub mutates_workflow: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_decision: Option<String>,
    pub origins: Vec<String>,
    pub actions: Vec<String>,
    pub event_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub permission_gate: AddonPermissionGate,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventEgressEmitInput {
    pub adapter_id: String,
    #[serde(default)]
    pub addon_id: Option<String>,
    pub event_type: String,
    pub action: String,
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventEgressEmitReport {
    pub schema_version: String,
    pub status: String,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
    pub adapter_policy: EventEgressAdapterPolicyReport,
    pub request: EventEgressRequestEnvelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<EventEgressDeliveryReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_artifact: Option<ArtifactAttachReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventEgressAdapterPolicyReport {
    pub schema_version: String,
    pub status: String,
    pub allowed: bool,
    pub enforced: bool,
    pub adapter_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    pub origin: String,
    pub action: String,
    pub normalized_action: String,
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_adapter: Option<AddonEventAdapterView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventEgressRequestEnvelope {
    pub schema_version: String,
    pub request_id: String,
    pub addon_id: String,
    pub adapter_id: String,
    pub transport: String,
    pub direction: String,
    pub auth: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_vault: Option<EventAdapterCredentialVaultRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_header: Option<String>,
    pub event_type: String,
    pub action: String,
    pub origin: String,
    pub schema: String,
    pub issued_at: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventEgressDeliveryReport {
    pub transport: String,
    pub endpoint: String,
    pub auth_scheme: String,
    pub signed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_env: Option<String>,
    pub secret_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_vault: Option<EventAdapterCredentialVaultRef>,
    pub success: bool,
    pub status_code: u16,
    pub response_bytes: usize,
    pub response_sha256: String,
    pub response_truncated: bool,
}

struct ParsedHttpEndpoint {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

struct EventEgressHttpResponse {
    status_code: u16,
    body: Vec<u8>,
    truncated: bool,
}

struct EventEgressSignatureHeaders {
    auth_scheme: String,
    signed: bool,
    signature_header: Option<String>,
    secret_env: Option<String>,
    secret_source: String,
    credential_vault: Option<EventAdapterCredentialVaultRef>,
    headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboundEventView {
    pub id: String,
    pub origin: String,
    pub action: String,
    pub status: String,
    pub tenant_context: EventTenantContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_context: Option<EventIdentityContext>,
    pub data: Value,
    pub created_at: String,
    pub processed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventIdentityContext {
    pub schema_version: String,
    pub resolution_status: String,
    pub source_identity: ContextIdentityRef,
    pub canonical_identity: IdentityAliasView,
    pub identity_count: usize,
    pub link_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowEventEnvelope {
    pub schema_version: String,
    pub event_id: String,
    pub store_sequence: i64,
    pub workflow_id: String,
    pub kind: String,
    pub category: String,
    pub severity: String,
    pub origin: String,
    pub source: String,
    pub occurred_at: String,
    pub correlation: EventCorrelation,
    pub tenant_context: EventTenantContext,
    pub observability: EventObservability,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventTenantContext {
    pub organization: ContextIdentityRef,
    pub brand: ContextIdentityRef,
    pub product: ContextIdentityRef,
    pub user: ContextIdentityRef,
    pub channel: ContextIdentityRef,
    pub memory_scope: String,
    pub personality_scope: String,
    pub brand_identity: BrandIdentitySpec,
    pub design_system: DesignSystemSpec,
    pub operating_policy: OperatingPolicySpec,
}

impl From<&OperatingContextSpec> for EventTenantContext {
    fn from(context: &OperatingContextSpec) -> Self {
        Self {
            organization: context.organization.clone(),
            brand: context.brand.clone(),
            product: context.product.clone(),
            user: context.user.clone(),
            channel: context.channel.clone(),
            memory_scope: context.memory_scope.clone(),
            personality_scope: context.personality_scope.clone(),
            brand_identity: context.brand_identity.clone(),
            design_system: context.design_system.clone(),
            operating_policy: context.operating_policy.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EventCorrelation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventObservability {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_budget_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_context_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_remaining_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_pressure_bps: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_pressure_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_scope: Option<String>,
}

pub fn build_workflow_event_stream(
    store: &ForgeStore,
    workflow_id: &str,
    limit: Option<usize>,
) -> Result<WorkflowEventStreamReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let tenant_context = EventTenantContext::from(&workflow.intent.operating_context);
    let events = store.load_workflow_events(workflow_id)?;
    let total_event_count = events.len();
    let selected_events = select_tail(events, limit);
    let envelopes = selected_events
        .into_iter()
        .map(|event| envelope_event(event, &tenant_context))
        .collect::<Vec<_>>();

    Ok(WorkflowEventStreamReport {
        schema_version: EVENT_STREAM_SCHEMA_VERSION.to_string(),
        status: "event_stream_loaded".to_string(),
        workflow_id: workflow.id,
        workflow_goal: workflow.goal,
        workflow_mode: workflow.intent.workflow_mode.kind,
        tenant_context,
        total_event_count,
        event_count: envelopes.len(),
        events: envelopes,
    })
}

pub fn build_global_event_timeline(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
) -> Result<GlobalEventTimelineReport> {
    let global_events = store.load_global_events()?;
    if !global_events.is_empty() {
        let mut events = global_events
            .into_iter()
            .filter(|event| {
                global_event_matches_filters(
                    event,
                    workflow_id,
                    organization_id,
                    brand_id,
                    product_id,
                )
            })
            .map(global_event_envelope)
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.store_sequence);
        let total_event_count = events.len();
        let (events, page) = select_timeline_page(events, limit, after_sequence);
        return Ok(GlobalEventTimelineReport {
            schema_version: EVENT_TIMELINE_SCHEMA_VERSION.to_string(),
            status: "event_timeline_loaded".to_string(),
            filters: EventTimelineFilters {
                workflow_id: normalize_text(workflow_id),
                organization_id: normalize_text(organization_id),
                brand_id: normalize_text(brand_id),
                product_id: normalize_text(product_id),
                limit,
                after_sequence,
            },
            page,
            total_event_count,
            event_count: events.len(),
            events,
        });
    }

    let workflows = if let Some(workflow_id) = normalize_text(workflow_id) {
        vec![store.load_workflow(&workflow_id)?]
    } else {
        store.load_workflows()?
    };
    let mut events = Vec::new();
    for workflow in workflows {
        if !workflow_matches_tenant(&workflow, organization_id, brand_id, product_id) {
            continue;
        }
        let tenant_context = EventTenantContext::from(&workflow.intent.operating_context);
        for event in store.load_workflow_events(&workflow.id)? {
            events.push(envelope_event(event, &tenant_context));
        }
    }
    events.sort_by_key(|event| event.store_sequence);
    let total_event_count = events.len();
    let (events, page) = select_timeline_page(events, limit, after_sequence);
    Ok(GlobalEventTimelineReport {
        schema_version: EVENT_TIMELINE_SCHEMA_VERSION.to_string(),
        status: "event_timeline_loaded".to_string(),
        filters: EventTimelineFilters {
            workflow_id: normalize_text(workflow_id),
            organization_id: normalize_text(organization_id),
            brand_id: normalize_text(brand_id),
            product_id: normalize_text(product_id),
            limit,
            after_sequence,
        },
        page,
        total_event_count,
        event_count: events.len(),
        events,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_global_event_timeline_for_context(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    operating_context: &OperatingContextSpec,
) -> Result<GlobalEventTimelineReport> {
    if operating_context.tenant_policy_mode != "enforce" {
        return build_global_event_timeline(
            store,
            workflow_id,
            organization_id,
            brand_id,
            product_id,
            limit,
            after_sequence,
        );
    }
    ensure_operating_context_policy(store, operating_context, "events timeline list")?;
    let organization_id = enforce_timeline_tenant_filter(
        "events timeline list",
        "organization",
        organization_id,
        &operating_context.organization.id,
    )?;
    let brand_id = enforce_timeline_tenant_filter(
        "events timeline list",
        "brand",
        brand_id,
        &operating_context.brand.id,
    )?;
    let product_id = enforce_timeline_tenant_filter(
        "events timeline list",
        "product",
        product_id,
        &operating_context.product.id,
    )?;
    build_global_event_timeline(
        store,
        workflow_id,
        Some(&organization_id),
        Some(&brand_id),
        Some(&product_id),
        limit,
        after_sequence,
    )
}

pub fn build_event_observability_index(
    store: &ForgeStore,
    query: EventObservabilityQuery<'_>,
) -> Result<EventObservabilityIndexReport> {
    let (mut records, index_source) = load_event_observability_records(store, query)?;
    records.sort_by_key(|event| event.store_sequence);
    let summary = summarize_event_observability(&records);
    let tenants = summarize_event_observability_tenants(&records);
    let workflows = summarize_event_observability_workflows(&records);
    let nodes = summarize_event_observability_nodes(&records);
    let addons = summarize_event_observability_addons(&records);
    let (events, page) = select_observability_page(records, query.limit, query.after_sequence);
    Ok(EventObservabilityIndexReport {
        schema_version: EVENT_OBSERVABILITY_INDEX_SCHEMA_VERSION.to_string(),
        status: "event_observability_index_loaded".to_string(),
        index_source,
        filters: EventObservabilityIndexFilters {
            workflow_id: normalize_text(query.workflow_id),
            organization_id: normalize_text(query.organization_id),
            brand_id: normalize_text(query.brand_id),
            product_id: normalize_text(query.product_id),
            node_ref: normalize_text(query.node_ref),
            addon_id: normalize_text(query.addon_id),
            limit: query.limit,
            after_sequence: query.after_sequence,
        },
        page,
        summary,
        tenants,
        workflows,
        nodes,
        addons,
        event_count: events.len(),
        events,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_event_observability_index_for_context(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    node_ref: Option<&str>,
    addon_id: Option<&str>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    operating_context: &OperatingContextSpec,
) -> Result<EventObservabilityIndexReport> {
    if operating_context.tenant_policy_mode != "enforce" {
        return build_event_observability_index(
            store,
            EventObservabilityQuery {
                workflow_id,
                organization_id,
                brand_id,
                product_id,
                node_ref,
                addon_id,
                limit,
                after_sequence,
            },
        );
    }
    ensure_operating_context_policy(store, operating_context, "events observability list")?;
    let organization_id = enforce_timeline_tenant_filter(
        "events observability list",
        "organization",
        organization_id,
        &operating_context.organization.id,
    )?;
    let brand_id = enforce_timeline_tenant_filter(
        "events observability list",
        "brand",
        brand_id,
        &operating_context.brand.id,
    )?;
    let product_id = enforce_timeline_tenant_filter(
        "events observability list",
        "product",
        product_id,
        &operating_context.product.id,
    )?;
    build_event_observability_index(
        store,
        EventObservabilityQuery {
            workflow_id,
            organization_id: Some(&organization_id),
            brand_id: Some(&brand_id),
            product_id: Some(&product_id),
            node_ref,
            addon_id,
            limit,
            after_sequence,
        },
    )
}

pub fn build_event_observability_history(
    store: &ForgeStore,
    query: EventObservabilityHistoryQuery<'_>,
) -> Result<EventObservabilityHistoryReport> {
    let bucket = normalize_history_bucket(query.bucket)?;
    let group_by = normalize_history_group_by(query.group_by)?;
    let (mut records, index_source) = load_event_observability_records(store, query.observability)?;
    records.sort_by_key(|event| event.store_sequence);
    if let Some(after_sequence) = query.observability.after_sequence {
        records.retain(|event| event.store_sequence > after_sequence);
    }
    let summary = summarize_event_observability(&records);
    let buckets = build_event_observability_history_buckets(
        &records,
        bucket.as_str(),
        group_by.as_str(),
        query.observability.limit,
    )?;

    Ok(EventObservabilityHistoryReport {
        schema_version: EVENT_OBSERVABILITY_HISTORY_SCHEMA_VERSION.to_string(),
        status: "event_observability_history_loaded".to_string(),
        index_source,
        filters: EventObservabilityHistoryFilters {
            workflow_id: normalize_text(query.observability.workflow_id),
            organization_id: normalize_text(query.observability.organization_id),
            brand_id: normalize_text(query.observability.brand_id),
            product_id: normalize_text(query.observability.product_id),
            node_ref: normalize_text(query.observability.node_ref),
            addon_id: normalize_text(query.observability.addon_id),
            bucket,
            group_by,
            limit: query.observability.limit.filter(|limit| *limit > 0),
            after_sequence: query.observability.after_sequence,
        },
        summary,
        bucket_count: buckets.len(),
        buckets,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_event_observability_history_for_context(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    node_ref: Option<&str>,
    addon_id: Option<&str>,
    bucket: Option<&str>,
    group_by: Option<&str>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    operating_context: &OperatingContextSpec,
) -> Result<EventObservabilityHistoryReport> {
    if operating_context.tenant_policy_mode != "enforce" {
        return build_event_observability_history(
            store,
            EventObservabilityHistoryQuery {
                observability: EventObservabilityQuery {
                    workflow_id,
                    organization_id,
                    brand_id,
                    product_id,
                    node_ref,
                    addon_id,
                    limit,
                    after_sequence,
                },
                bucket,
                group_by,
            },
        );
    }
    ensure_operating_context_policy(
        store,
        operating_context,
        "events observability history list",
    )?;
    let organization_id = enforce_timeline_tenant_filter(
        "events observability history list",
        "organization",
        organization_id,
        &operating_context.organization.id,
    )?;
    let brand_id = enforce_timeline_tenant_filter(
        "events observability history list",
        "brand",
        brand_id,
        &operating_context.brand.id,
    )?;
    let product_id = enforce_timeline_tenant_filter(
        "events observability history list",
        "product",
        product_id,
        &operating_context.product.id,
    )?;
    build_event_observability_history(
        store,
        EventObservabilityHistoryQuery {
            observability: EventObservabilityQuery {
                workflow_id,
                organization_id: Some(&organization_id),
                brand_id: Some(&brand_id),
                product_id: Some(&product_id),
                node_ref,
                addon_id,
                limit,
                after_sequence,
            },
            bucket,
            group_by,
        },
    )
}

pub fn build_event_improvement_policy(
    store: &ForgeStore,
    query: EventImprovementPolicyQuery<'_>,
) -> Result<EventImprovementPolicyReport> {
    let thresholds = EventImprovementPolicyThresholds {
        min_event_count: query.min_event_count.unwrap_or(3).max(1),
        min_total_duration_ms: query.min_total_duration_ms.unwrap_or(1_000).max(0),
        min_total_retry_count: query.min_total_retry_count.unwrap_or(2).max(0),
        min_context_pressure_bps: query
            .min_context_pressure_bps
            .unwrap_or(8_500)
            .clamp(0, 10_000),
        min_total_wait_seconds: query.min_total_wait_seconds.unwrap_or(60).max(0),
    };
    let (mut records, index_source) = load_event_observability_records(store, query.observability)?;
    records.sort_by_key(|event| event.store_sequence);
    if let Some(after_sequence) = query.observability.after_sequence {
        records.retain(|event| event.store_sequence > after_sequence);
    }
    let summary = summarize_event_observability(&records);
    let mut recommendations = build_event_improvement_recommendations(&records, &thresholds);
    recommendations.sort_by(|left, right| {
        event_improvement_priority_rank(&right.priority)
            .cmp(&event_improvement_priority_rank(&left.priority))
            .then_with(|| right.total_duration_ms.cmp(&left.total_duration_ms))
            .then_with(|| right.total_retry_count.cmp(&left.total_retry_count))
            .then_with(|| right.event_count.cmp(&left.event_count))
            .then_with(|| left.id.cmp(&right.id))
    });
    if let Some(limit) = query.observability.limit.filter(|limit| *limit > 0) {
        recommendations.truncate(limit);
    }
    let status = if recommendations.is_empty() {
        "event_improvement_policy_clear"
    } else {
        "event_improvement_policy_recommended"
    };
    Ok(EventImprovementPolicyReport {
        schema_version: EVENT_IMPROVEMENT_POLICY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        index_source,
        filters: EventImprovementPolicyFilters {
            workflow_id: normalize_text(query.observability.workflow_id),
            organization_id: normalize_text(query.observability.organization_id),
            brand_id: normalize_text(query.observability.brand_id),
            product_id: normalize_text(query.observability.product_id),
            node_ref: normalize_text(query.observability.node_ref),
            addon_id: normalize_text(query.observability.addon_id),
            limit: query.observability.limit.filter(|limit| *limit > 0),
            after_sequence: query.observability.after_sequence,
        },
        thresholds,
        summary,
        recommendation_count: recommendations.len(),
        recommendations,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_event_improvement_policy_for_context(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    node_ref: Option<&str>,
    addon_id: Option<&str>,
    min_event_count: Option<usize>,
    min_total_duration_ms: Option<i64>,
    min_total_retry_count: Option<i64>,
    min_context_pressure_bps: Option<i64>,
    min_total_wait_seconds: Option<i64>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
    operating_context: &OperatingContextSpec,
) -> Result<EventImprovementPolicyReport> {
    if operating_context.tenant_policy_mode != "enforce" {
        return build_event_improvement_policy(
            store,
            EventImprovementPolicyQuery {
                observability: EventObservabilityQuery {
                    workflow_id,
                    organization_id,
                    brand_id,
                    product_id,
                    node_ref,
                    addon_id,
                    limit,
                    after_sequence,
                },
                min_event_count,
                min_total_duration_ms,
                min_total_retry_count,
                min_context_pressure_bps,
                min_total_wait_seconds,
            },
        );
    }
    ensure_operating_context_policy(store, operating_context, "events improvement policy list")?;
    let organization_id = enforce_timeline_tenant_filter(
        "events improvement policy list",
        "organization",
        organization_id,
        &operating_context.organization.id,
    )?;
    let brand_id = enforce_timeline_tenant_filter(
        "events improvement policy list",
        "brand",
        brand_id,
        &operating_context.brand.id,
    )?;
    let product_id = enforce_timeline_tenant_filter(
        "events improvement policy list",
        "product",
        product_id,
        &operating_context.product.id,
    )?;
    build_event_improvement_policy(
        store,
        EventImprovementPolicyQuery {
            observability: EventObservabilityQuery {
                workflow_id,
                organization_id: Some(&organization_id),
                brand_id: Some(&brand_id),
                product_id: Some(&product_id),
                node_ref,
                addon_id,
                limit,
                after_sequence,
            },
            min_event_count,
            min_total_duration_ms,
            min_total_retry_count,
            min_context_pressure_bps,
            min_total_wait_seconds,
        },
    )
}

fn load_event_observability_records(
    store: &ForgeStore,
    query: EventObservabilityQuery<'_>,
) -> Result<(Vec<EventObservabilityRecord>, String)> {
    let mut records = store
        .load_event_observability_index(
            query.workflow_id,
            query.organization_id,
            query.brand_id,
            query.product_id,
            query.node_ref,
            query.addon_id,
        )?
        .into_iter()
        .map(event_observability_record_from_store)
        .collect::<Vec<_>>();
    let index_source = if records.is_empty() {
        let timeline = build_global_event_timeline(
            store,
            query.workflow_id,
            query.organization_id,
            query.brand_id,
            query.product_id,
            None,
            None,
        )?;
        records = timeline
            .events
            .into_iter()
            .filter(|event| {
                filter_matches_optional(query.node_ref, event.observability.node_ref.as_deref())
            })
            .filter(|event| {
                filter_matches_optional(query.addon_id, event.observability.addon_id.as_deref())
            })
            .map(event_observability_record)
            .collect::<Vec<_>>();
        "derived_timeline".to_string()
    } else {
        "sqlite_materialized".to_string()
    };
    Ok((records, index_source))
}

fn event_observability_record(event: WorkflowEventEnvelope) -> EventObservabilityRecord {
    EventObservabilityRecord {
        event_id: event.event_id,
        store_sequence: event.store_sequence,
        workflow_id: event.workflow_id,
        kind: event.kind,
        category: event.category,
        severity: event.severity,
        origin: event.origin,
        source: event.source,
        occurred_at: event.occurred_at,
        organization_id: event.tenant_context.organization.id,
        brand_id: event.tenant_context.brand.id,
        product_id: event.tenant_context.product.id,
        node_ref: event.observability.node_ref,
        addon_id: event.observability.addon_id,
        duration_ms: event.observability.duration_ms,
        retry_count: event.observability.retry_count,
        wait_state: event.observability.wait_state,
        wait_seconds: event.observability.wait_seconds,
        context_budget_bytes: event.observability.context_budget_bytes,
        selected_context_bytes: event.observability.selected_context_bytes,
        context_remaining_bytes: event.observability.context_remaining_bytes,
        context_pressure_bps: event.observability.context_pressure_bps,
        context_pressure_state: event.observability.context_pressure_state,
        memory_level: event.observability.memory_level,
        memory_scope: event.observability.memory_scope,
    }
}

fn event_observability_record_from_store(
    record: StoredEventObservabilityRecord,
) -> EventObservabilityRecord {
    EventObservabilityRecord {
        event_id: format!("evtg_{}", record.global_event_id),
        store_sequence: record.global_event_id,
        workflow_id: record.workflow_id,
        kind: record.kind,
        category: record.category,
        severity: record.severity,
        origin: record.origin,
        source: record.source,
        occurred_at: record.created_at,
        organization_id: record.organization_id,
        brand_id: record.brand_id,
        product_id: record.product_id,
        node_ref: record.node_ref,
        addon_id: record.addon_id,
        duration_ms: record.duration_ms,
        retry_count: record.retry_count,
        wait_state: record.wait_state,
        wait_seconds: record.wait_seconds,
        context_budget_bytes: record.context_budget_bytes,
        selected_context_bytes: record.selected_context_bytes,
        context_remaining_bytes: record.context_remaining_bytes,
        context_pressure_bps: record.context_pressure_bps,
        context_pressure_state: record.context_pressure_state,
        memory_level: record.memory_level,
        memory_scope: record.memory_scope,
    }
}

fn summarize_event_observability(
    records: &[EventObservabilityRecord],
) -> EventObservabilitySummary {
    let mut severity_counts = BTreeMap::new();
    let mut category_counts = BTreeMap::new();
    let mut summary = EventObservabilitySummary {
        total_event_count: records.len(),
        ..Default::default()
    };
    for record in records {
        if record.node_ref.is_some() {
            summary.node_event_count += 1;
        }
        if record.addon_id.is_some() {
            summary.addon_event_count += 1;
        }
        if let Some(duration_ms) = record.duration_ms {
            summary.duration_event_count += 1;
            summary.total_duration_ms += duration_ms;
        }
        if let Some(retry_count) = record.retry_count {
            summary.retry_event_count += 1;
            summary.total_retry_count += retry_count;
        }
        if record.wait_state.is_some() || record.wait_seconds.is_some() {
            summary.wait_event_count += 1;
        }
        if let Some(wait_seconds) = record.wait_seconds {
            summary.total_wait_seconds += wait_seconds;
        }
        if record.context_budget_bytes.is_some()
            || record.selected_context_bytes.is_some()
            || record.context_remaining_bytes.is_some()
            || record.context_pressure_bps.is_some()
            || record.context_pressure_state.is_some()
        {
            summary.context_event_count += 1;
        }
        if let Some(context_budget_bytes) = record.context_budget_bytes {
            summary.total_context_budget_bytes += context_budget_bytes;
        }
        if let Some(selected_context_bytes) = record.selected_context_bytes {
            summary.total_selected_context_bytes += selected_context_bytes;
        }
        if let Some(context_remaining_bytes) = record.context_remaining_bytes {
            summary.total_context_remaining_bytes += context_remaining_bytes;
        }
        if let Some(context_pressure_bps) = record.context_pressure_bps {
            summary.context_pressure_event_count += 1;
            max_i64_option(&mut summary.max_context_pressure_bps, context_pressure_bps);
        }
        if record.memory_level.is_some() || record.memory_scope.is_some() {
            summary.memory_event_count += 1;
        }
        *severity_counts.entry(record.severity.clone()).or_insert(0) += 1;
        *category_counts.entry(record.category.clone()).or_insert(0) += 1;
    }
    summary.severity_counts = count_entries(severity_counts);
    summary.category_counts = count_entries(category_counts);
    summary
}

#[derive(Default)]
struct EventObservabilityBucket {
    event_count: usize,
    workflows: Vec<String>,
    nodes: Vec<String>,
    addons: Vec<String>,
    total_duration_ms: i64,
    total_retry_count: i64,
    total_wait_seconds: i64,
    context_event_count: usize,
    context_pressure_event_count: usize,
    total_context_budget_bytes: i64,
    total_selected_context_bytes: i64,
    total_context_remaining_bytes: i64,
    max_context_pressure_bps: Option<i64>,
    memory_event_count: usize,
    kinds: Vec<String>,
    last_event_sequence: i64,
    organization_id: String,
    brand_id: String,
    product_id: String,
    workflow_id: String,
    node_ref: String,
    addon_id: Option<String>,
}

fn summarize_event_observability_tenants(
    records: &[EventObservabilityRecord],
) -> Vec<EventObservabilityTenantSummary> {
    let mut buckets: BTreeMap<String, EventObservabilityBucket> = BTreeMap::new();
    for record in records {
        let key = format!(
            "{}|{}|{}",
            record.organization_id, record.brand_id, record.product_id
        );
        let bucket = buckets.entry(key).or_default();
        bucket.organization_id = record.organization_id.clone();
        bucket.brand_id = record.brand_id.clone();
        bucket.product_id = record.product_id.clone();
        accumulate_observability_bucket(bucket, record);
    }
    buckets
        .into_values()
        .map(|mut bucket| {
            bucket.workflows.sort();
            bucket.workflows.dedup();
            bucket.nodes.sort();
            bucket.nodes.dedup();
            bucket.addons.sort();
            bucket.addons.dedup();
            EventObservabilityTenantSummary {
                organization_id: bucket.organization_id,
                brand_id: bucket.brand_id,
                product_id: bucket.product_id,
                event_count: bucket.event_count,
                workflow_count: bucket.workflows.len(),
                node_count: bucket.nodes.len(),
                addon_count: bucket.addons.len(),
                total_duration_ms: bucket.total_duration_ms,
                total_retry_count: bucket.total_retry_count,
                total_wait_seconds: bucket.total_wait_seconds,
                context_event_count: bucket.context_event_count,
                context_pressure_event_count: bucket.context_pressure_event_count,
                total_context_budget_bytes: bucket.total_context_budget_bytes,
                total_selected_context_bytes: bucket.total_selected_context_bytes,
                total_context_remaining_bytes: bucket.total_context_remaining_bytes,
                max_context_pressure_bps: bucket.max_context_pressure_bps,
                memory_event_count: bucket.memory_event_count,
            }
        })
        .collect()
}

fn summarize_event_observability_workflows(
    records: &[EventObservabilityRecord],
) -> Vec<EventObservabilityWorkflowSummary> {
    let mut buckets: BTreeMap<String, EventObservabilityBucket> = BTreeMap::new();
    for record in records {
        let bucket = buckets.entry(record.workflow_id.clone()).or_default();
        bucket.workflow_id = record.workflow_id.clone();
        bucket.organization_id = record.organization_id.clone();
        bucket.brand_id = record.brand_id.clone();
        bucket.product_id = record.product_id.clone();
        accumulate_observability_bucket(bucket, record);
    }
    buckets
        .into_values()
        .map(|mut bucket| {
            bucket.nodes.sort();
            bucket.nodes.dedup();
            bucket.addons.sort();
            bucket.addons.dedup();
            EventObservabilityWorkflowSummary {
                workflow_id: bucket.workflow_id,
                organization_id: bucket.organization_id,
                brand_id: bucket.brand_id,
                product_id: bucket.product_id,
                event_count: bucket.event_count,
                node_count: bucket.nodes.len(),
                addon_count: bucket.addons.len(),
                total_duration_ms: bucket.total_duration_ms,
                total_retry_count: bucket.total_retry_count,
                total_wait_seconds: bucket.total_wait_seconds,
                context_event_count: bucket.context_event_count,
                context_pressure_event_count: bucket.context_pressure_event_count,
                total_context_budget_bytes: bucket.total_context_budget_bytes,
                total_selected_context_bytes: bucket.total_selected_context_bytes,
                total_context_remaining_bytes: bucket.total_context_remaining_bytes,
                max_context_pressure_bps: bucket.max_context_pressure_bps,
                memory_event_count: bucket.memory_event_count,
            }
        })
        .collect()
}

fn summarize_event_observability_nodes(
    records: &[EventObservabilityRecord],
) -> Vec<EventObservabilityNodeSummary> {
    let mut buckets: BTreeMap<String, EventObservabilityBucket> = BTreeMap::new();
    for record in records {
        let Some(node_ref) = record.node_ref.as_deref() else {
            continue;
        };
        let key = format!("{}|{}", record.workflow_id, node_ref);
        let bucket = buckets.entry(key).or_default();
        bucket.workflow_id = record.workflow_id.clone();
        bucket.node_ref = node_ref.to_string();
        if bucket.addon_id.is_none() {
            bucket.addon_id = record.addon_id.clone();
        }
        accumulate_observability_bucket(bucket, record);
    }
    buckets
        .into_values()
        .map(|mut bucket| {
            bucket.kinds.sort();
            bucket.kinds.dedup();
            EventObservabilityNodeSummary {
                workflow_id: bucket.workflow_id,
                node_ref: bucket.node_ref,
                addon_id: bucket.addon_id,
                event_count: bucket.event_count,
                kinds: bucket.kinds,
                total_duration_ms: bucket.total_duration_ms,
                total_retry_count: bucket.total_retry_count,
                total_wait_seconds: bucket.total_wait_seconds,
                context_event_count: bucket.context_event_count,
                context_pressure_event_count: bucket.context_pressure_event_count,
                total_context_budget_bytes: bucket.total_context_budget_bytes,
                total_selected_context_bytes: bucket.total_selected_context_bytes,
                total_context_remaining_bytes: bucket.total_context_remaining_bytes,
                max_context_pressure_bps: bucket.max_context_pressure_bps,
                memory_event_count: bucket.memory_event_count,
                last_event_sequence: bucket.last_event_sequence,
            }
        })
        .collect()
}

fn summarize_event_observability_addons(
    records: &[EventObservabilityRecord],
) -> Vec<EventObservabilityAddonSummary> {
    let mut buckets: BTreeMap<String, EventObservabilityBucket> = BTreeMap::new();
    for record in records {
        let Some(addon_id) = record.addon_id.as_deref() else {
            continue;
        };
        let bucket = buckets.entry(addon_id.to_string()).or_default();
        bucket.addon_id = Some(addon_id.to_string());
        accumulate_observability_bucket(bucket, record);
    }
    buckets
        .into_values()
        .map(|mut bucket| {
            bucket.workflows.sort();
            bucket.workflows.dedup();
            bucket.nodes.sort();
            bucket.nodes.dedup();
            EventObservabilityAddonSummary {
                addon_id: bucket.addon_id.unwrap_or_default(),
                event_count: bucket.event_count,
                workflow_count: bucket.workflows.len(),
                node_count: bucket.nodes.len(),
                total_duration_ms: bucket.total_duration_ms,
                total_retry_count: bucket.total_retry_count,
                total_wait_seconds: bucket.total_wait_seconds,
                context_event_count: bucket.context_event_count,
                context_pressure_event_count: bucket.context_pressure_event_count,
                total_context_budget_bytes: bucket.total_context_budget_bytes,
                total_selected_context_bytes: bucket.total_selected_context_bytes,
                total_context_remaining_bytes: bucket.total_context_remaining_bytes,
                max_context_pressure_bps: bucket.max_context_pressure_bps,
                memory_event_count: bucket.memory_event_count,
                last_event_sequence: bucket.last_event_sequence,
            }
        })
        .collect()
}

fn accumulate_observability_bucket(
    bucket: &mut EventObservabilityBucket,
    record: &EventObservabilityRecord,
) {
    bucket.event_count += 1;
    bucket.workflows.push(record.workflow_id.clone());
    if let Some(node_ref) = &record.node_ref {
        bucket.nodes.push(node_ref.clone());
    }
    if let Some(addon_id) = &record.addon_id {
        bucket.addons.push(addon_id.clone());
    }
    if let Some(duration_ms) = record.duration_ms {
        bucket.total_duration_ms += duration_ms;
    }
    if let Some(retry_count) = record.retry_count {
        bucket.total_retry_count += retry_count;
    }
    if let Some(wait_seconds) = record.wait_seconds {
        bucket.total_wait_seconds += wait_seconds;
    }
    if record.context_budget_bytes.is_some()
        || record.selected_context_bytes.is_some()
        || record.context_remaining_bytes.is_some()
        || record.context_pressure_bps.is_some()
        || record.context_pressure_state.is_some()
    {
        bucket.context_event_count += 1;
    }
    if let Some(context_budget_bytes) = record.context_budget_bytes {
        bucket.total_context_budget_bytes += context_budget_bytes;
    }
    if let Some(selected_context_bytes) = record.selected_context_bytes {
        bucket.total_selected_context_bytes += selected_context_bytes;
    }
    if let Some(context_remaining_bytes) = record.context_remaining_bytes {
        bucket.total_context_remaining_bytes += context_remaining_bytes;
    }
    if let Some(context_pressure_bps) = record.context_pressure_bps {
        bucket.context_pressure_event_count += 1;
        max_i64_option(&mut bucket.max_context_pressure_bps, context_pressure_bps);
    }
    if record.memory_level.is_some() || record.memory_scope.is_some() {
        bucket.memory_event_count += 1;
    }
    bucket.kinds.push(record.kind.clone());
    bucket.last_event_sequence = bucket.last_event_sequence.max(record.store_sequence);
}

fn max_i64_option(target: &mut Option<i64>, value: i64) {
    *target = Some(target.map_or(value, |current| current.max(value)));
}

fn build_event_observability_history_buckets(
    records: &[EventObservabilityRecord],
    bucket: &str,
    group_by: &str,
    limit: Option<usize>,
) -> Result<Vec<EventObservabilityHistoryBucket>> {
    let mut buckets: BTreeMap<String, EventObservabilityHistoryAccumulator> = BTreeMap::new();
    for record in records {
        let occurred_at = parse_observability_time(&record.occurred_at)?;
        let (bucket_start, bucket_end) = observability_bucket_bounds(occurred_at, bucket)?;
        let group_id = event_observability_history_group_id(record, group_by);
        let key = format!("{bucket_start}|{group_by}|{group_id}");
        let accumulator =
            buckets
                .entry(key)
                .or_insert_with(|| EventObservabilityHistoryAccumulator {
                    bucket: bucket.to_string(),
                    bucket_start: bucket_start.clone(),
                    bucket_end: bucket_end.clone(),
                    group_by: group_by.to_string(),
                    group_id: group_id.clone(),
                    records: Vec::new(),
                });
        accumulator.records.push(record.clone());
    }

    let mut history = buckets
        .into_values()
        .map(|mut bucket| {
            bucket.records.sort_by_key(|record| record.store_sequence);
            let first_event_sequence = bucket
                .records
                .first()
                .map(|record| record.store_sequence)
                .unwrap_or_default();
            let last_event_sequence = bucket
                .records
                .last()
                .map(|record| record.store_sequence)
                .unwrap_or_default();
            let group = event_observability_history_group(
                bucket.group_by.as_str(),
                bucket.group_id.as_str(),
                &bucket.records,
            );
            EventObservabilityHistoryBucket {
                bucket: bucket.bucket,
                bucket_start: bucket.bucket_start,
                bucket_end: bucket.bucket_end,
                group_by: bucket.group_by,
                group_id: bucket.group_id,
                group,
                summary: summarize_event_observability(&bucket.records),
                first_event_sequence,
                last_event_sequence,
            }
        })
        .collect::<Vec<_>>();
    history.sort_by(|left, right| {
        left.bucket_start
            .cmp(&right.bucket_start)
            .then_with(|| left.group_by.cmp(&right.group_by))
            .then_with(|| left.group_id.cmp(&right.group_id))
    });
    if let Some(limit) = limit.filter(|limit| *limit > 0) {
        let start = history.len().saturating_sub(limit);
        history = history.into_iter().skip(start).collect();
    }
    Ok(history)
}

struct EventObservabilityHistoryAccumulator {
    bucket: String,
    bucket_start: String,
    bucket_end: String,
    group_by: String,
    group_id: String,
    records: Vec<EventObservabilityRecord>,
}

fn normalize_history_bucket(bucket: Option<&str>) -> Result<String> {
    let bucket = normalize_text(bucket).unwrap_or_else(|| "day".to_string());
    match bucket.as_str() {
        "hour" | "day" => Ok(bucket),
        _ => bail!("observability history bucket must be `hour` or `day`"),
    }
}

fn normalize_history_group_by(group_by: Option<&str>) -> Result<String> {
    let group_by = normalize_text(group_by).unwrap_or_else(|| "none".to_string());
    match group_by.as_str() {
        "none" | "tenant" | "workflow" | "node" | "addon" => Ok(group_by),
        _ => bail!(
            "observability history group_by must be one of `none`, `tenant`, `workflow`, `node` or `addon`"
        ),
    }
}

fn parse_observability_time(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(parsed.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("event timestamp `{value}` must be RFC3339 or SQLite UTC"))?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn observability_bucket_bounds(
    occurred_at: DateTime<Utc>,
    bucket: &str,
) -> Result<(String, String)> {
    let date = NaiveDate::from_ymd_opt(occurred_at.year(), occurred_at.month(), occurred_at.day())
        .with_context(|| "failed to derive event bucket date")?;
    let start_naive = match bucket {
        "hour" => date
            .and_hms_opt(occurred_at.hour(), 0, 0)
            .with_context(|| "failed to derive hourly event bucket")?,
        "day" => date
            .and_hms_opt(0, 0, 0)
            .with_context(|| "failed to derive daily event bucket")?,
        _ => bail!("observability history bucket must be `hour` or `day`"),
    };
    let start = DateTime::<Utc>::from_naive_utc_and_offset(start_naive, Utc);
    let end = match bucket {
        "hour" => start + ChronoDuration::hours(1),
        "day" => start + ChronoDuration::days(1),
        _ => bail!("observability history bucket must be `hour` or `day`"),
    };
    Ok((start.to_rfc3339(), end.to_rfc3339()))
}

fn event_observability_history_group_id(
    record: &EventObservabilityRecord,
    group_by: &str,
) -> String {
    match group_by {
        "tenant" => format!(
            "{}|{}|{}",
            record.organization_id, record.brand_id, record.product_id
        ),
        "workflow" => record.workflow_id.clone(),
        "node" => record
            .node_ref
            .clone()
            .unwrap_or_else(|| "_unassigned_node".to_string()),
        "addon" => record
            .addon_id
            .clone()
            .unwrap_or_else(|| "_no_addon".to_string()),
        _ => "_all".to_string(),
    }
}

fn event_observability_history_group(
    group_by: &str,
    group_id: &str,
    records: &[EventObservabilityRecord],
) -> EventObservabilityHistoryGroup {
    let first = records.first();
    EventObservabilityHistoryGroup {
        group_by: group_by.to_string(),
        group_id: group_id.to_string(),
        organization_id: (group_by == "tenant")
            .then(|| first.map(|record| record.organization_id.clone()))
            .flatten(),
        brand_id: (group_by == "tenant")
            .then(|| first.map(|record| record.brand_id.clone()))
            .flatten(),
        product_id: (group_by == "tenant")
            .then(|| first.map(|record| record.product_id.clone()))
            .flatten(),
        workflow_id: (group_by == "workflow")
            .then(|| first.map(|record| record.workflow_id.clone()))
            .flatten(),
        node_ref: (group_by == "node")
            .then(|| first.and_then(|record| record.node_ref.clone()))
            .flatten(),
        addon_id: (group_by == "addon")
            .then(|| first.and_then(|record| record.addon_id.clone()))
            .flatten(),
    }
}

#[derive(Default)]
struct EventImprovementPolicyBucket {
    scope: String,
    workflow_id: String,
    organization_id: String,
    brand_id: String,
    product_id: String,
    node_ref: Option<String>,
    addon_id: Option<String>,
    event_count: usize,
    ai_signal_count: usize,
    total_duration_ms: i64,
    total_retry_count: i64,
    total_wait_seconds: i64,
    total_selected_context_bytes: i64,
    max_context_pressure_bps: Option<i64>,
    first_event_sequence: i64,
    last_event_sequence: i64,
    kinds: Vec<String>,
}

fn build_event_improvement_recommendations(
    records: &[EventObservabilityRecord],
    thresholds: &EventImprovementPolicyThresholds,
) -> Vec<EventImprovementRecommendation> {
    let mut recommendations = Vec::new();
    recommendations.extend(build_scoped_event_improvement_recommendations(
        records, thresholds, "node",
    ));
    recommendations.extend(build_scoped_event_improvement_recommendations(
        records, thresholds, "addon",
    ));
    recommendations.extend(build_scoped_event_improvement_recommendations(
        records, thresholds, "workflow",
    ));
    recommendations
}

fn build_scoped_event_improvement_recommendations(
    records: &[EventObservabilityRecord],
    thresholds: &EventImprovementPolicyThresholds,
    scope: &str,
) -> Vec<EventImprovementRecommendation> {
    let mut buckets: BTreeMap<String, EventImprovementPolicyBucket> = BTreeMap::new();
    for record in records {
        let key = match scope {
            "node" => {
                let Some(node_ref) = record.node_ref.as_deref() else {
                    continue;
                };
                format!("{}|{}", record.workflow_id, node_ref)
            }
            "addon" => {
                let Some(addon_id) = record.addon_id.as_deref() else {
                    continue;
                };
                format!("{}|{}", record.workflow_id, addon_id)
            }
            "workflow" => record.workflow_id.clone(),
            _ => continue,
        };
        let bucket = buckets.entry(key).or_default();
        if bucket.scope.is_empty() {
            bucket.scope = scope.to_string();
        }
        bucket.workflow_id = record.workflow_id.clone();
        bucket.organization_id = record.organization_id.clone();
        bucket.brand_id = record.brand_id.clone();
        bucket.product_id = record.product_id.clone();
        if bucket.node_ref.is_none() {
            bucket.node_ref = record.node_ref.clone();
        }
        if bucket.addon_id.is_none() {
            bucket.addon_id = record.addon_id.clone();
        }
        accumulate_event_improvement_bucket(bucket, record);
    }

    let mut recommendations = Vec::new();
    for mut bucket in buckets.into_values() {
        bucket.kinds.sort();
        bucket.kinds.dedup();
        if bucket.event_count < thresholds.min_event_count {
            continue;
        }
        if scope == "node" && bucket.total_duration_ms >= thresholds.min_total_duration_ms {
            recommendations.push(event_improvement_recommendation(
                &bucket,
                thresholds,
                "deterministic_node_candidate",
                "prefer_deterministic_node",
                "Substituir trabalho repetitivo por command node, worker de Addon ou subworkflow determinístico quando a equivalência puder ser validada.",
            ));
        }
        if bucket.total_retry_count >= thresholds.min_total_retry_count
            && thresholds.min_total_retry_count > 0
        {
            recommendations.push(event_improvement_recommendation(
                &bucket,
                thresholds,
                "retry_hotspot",
                "add_validation_or_rework_gate",
                "Adicionar validação/rework antes da execução ou revisar o contrato do node/Addon que está repetindo tentativas.",
            ));
        }
        if bucket
            .max_context_pressure_bps
            .is_some_and(|pressure| pressure >= thresholds.min_context_pressure_bps)
        {
            recommendations.push(event_improvement_recommendation(
                &bucket,
                thresholds,
                "context_pressure_hotspot",
                "tighten_context_routing",
                "Reduzir contexto enviado ao executor com shard mais específico, compressão, cache ou memória governada por busca explícita.",
            ));
        }
        if bucket.total_wait_seconds >= thresholds.min_total_wait_seconds
            && thresholds.min_total_wait_seconds > 0
        {
            recommendations.push(event_improvement_recommendation(
                &bucket,
                thresholds,
                "wait_hotspot",
                "supervise_wait_or_external_dependency",
                "Mover espera recorrente para worker/schedule supervisionado e registrar recovery quando a dependência externa degradar.",
            ));
        }
    }
    recommendations
}

fn accumulate_event_improvement_bucket(
    bucket: &mut EventImprovementPolicyBucket,
    record: &EventObservabilityRecord,
) {
    bucket.event_count += 1;
    if event_has_ai_signal(record) {
        bucket.ai_signal_count += 1;
    }
    if let Some(duration_ms) = record.duration_ms {
        bucket.total_duration_ms += duration_ms;
    }
    if let Some(retry_count) = record.retry_count {
        bucket.total_retry_count += retry_count;
    }
    if let Some(wait_seconds) = record.wait_seconds {
        bucket.total_wait_seconds += wait_seconds;
    }
    if let Some(selected_context_bytes) = record.selected_context_bytes {
        bucket.total_selected_context_bytes += selected_context_bytes;
    }
    if let Some(context_pressure_bps) = record.context_pressure_bps {
        max_i64_option(&mut bucket.max_context_pressure_bps, context_pressure_bps);
    }
    if bucket.first_event_sequence == 0 {
        bucket.first_event_sequence = record.store_sequence;
    } else {
        bucket.first_event_sequence = bucket.first_event_sequence.min(record.store_sequence);
    }
    bucket.last_event_sequence = bucket.last_event_sequence.max(record.store_sequence);
    bucket.kinds.push(record.kind.clone());
}

fn event_improvement_recommendation(
    bucket: &EventImprovementPolicyBucket,
    thresholds: &EventImprovementPolicyThresholds,
    kind: &str,
    recommended_policy: &str,
    recommended_action: &str,
) -> EventImprovementRecommendation {
    let priority = event_improvement_priority(bucket, thresholds, kind);
    let target = event_improvement_bucket_target(bucket);
    EventImprovementRecommendation {
        id: format!(
            "event_policy:{}:{}:{}:{}",
            kind, bucket.workflow_id, bucket.scope, target
        ),
        kind: kind.to_string(),
        priority,
        scope: bucket.scope.clone(),
        workflow_id: bucket.workflow_id.clone(),
        organization_id: bucket.organization_id.clone(),
        brand_id: bucket.brand_id.clone(),
        product_id: bucket.product_id.clone(),
        node_ref: bucket.node_ref.clone(),
        addon_id: bucket.addon_id.clone(),
        event_count: bucket.event_count,
        ai_signal_count: bucket.ai_signal_count,
        total_duration_ms: bucket.total_duration_ms,
        total_retry_count: bucket.total_retry_count,
        total_wait_seconds: bucket.total_wait_seconds,
        total_selected_context_bytes: bucket.total_selected_context_bytes,
        max_context_pressure_bps: bucket.max_context_pressure_bps,
        first_event_sequence: bucket.first_event_sequence,
        last_event_sequence: bucket.last_event_sequence,
        recommended_policy: recommended_policy.to_string(),
        recommended_action: recommended_action.to_string(),
        reason: event_improvement_reason(bucket, thresholds, kind),
        suggested_commands: event_improvement_suggested_commands(bucket, recommended_policy),
    }
}

fn event_improvement_priority(
    bucket: &EventImprovementPolicyBucket,
    thresholds: &EventImprovementPolicyThresholds,
    kind: &str,
) -> String {
    let critical_context = bucket
        .max_context_pressure_bps
        .is_some_and(|pressure| pressure >= 9_500);
    let high_retry = thresholds.min_total_retry_count > 0
        && bucket.total_retry_count >= thresholds.min_total_retry_count.saturating_mul(2);
    let high_duration = thresholds.min_total_duration_ms > 0
        && bucket.total_duration_ms >= thresholds.min_total_duration_ms.saturating_mul(2);
    let high_wait = thresholds.min_total_wait_seconds > 0
        && bucket.total_wait_seconds >= thresholds.min_total_wait_seconds.saturating_mul(2);
    if critical_context || high_retry || high_duration || high_wait {
        "critical".to_string()
    } else if matches!(
        kind,
        "retry_hotspot" | "context_pressure_hotspot" | "deterministic_node_candidate"
    ) {
        "high".to_string()
    } else {
        "medium".to_string()
    }
}

fn event_improvement_priority_rank(priority: &str) -> usize {
    match priority {
        "critical" => 3,
        "high" => 2,
        "medium" => 1,
        _ => 0,
    }
}

fn event_improvement_reason(
    bucket: &EventImprovementPolicyBucket,
    thresholds: &EventImprovementPolicyThresholds,
    kind: &str,
) -> String {
    let target = event_improvement_bucket_target(bucket);
    match kind {
        "deterministic_node_candidate" => format!(
            "{target} teve {} eventos, {} ms acumulados e {} sinais de executor/IA; limite mínimo: {} eventos e {} ms.",
            bucket.event_count,
            bucket.total_duration_ms,
            bucket.ai_signal_count,
            thresholds.min_event_count,
            thresholds.min_total_duration_ms
        ),
        "retry_hotspot" => format!(
            "{target} acumulou {} retries; limite mínimo: {}.",
            bucket.total_retry_count, thresholds.min_total_retry_count
        ),
        "context_pressure_hotspot" => format!(
            "{target} atingiu pressão máxima de contexto de {} bps; limite mínimo: {} bps.",
            bucket.max_context_pressure_bps.unwrap_or_default(),
            thresholds.min_context_pressure_bps
        ),
        "wait_hotspot" => format!(
            "{target} acumulou {} segundos de espera; limite mínimo: {} segundos.",
            bucket.total_wait_seconds, thresholds.min_total_wait_seconds
        ),
        _ => format!("{target} excedeu uma política de melhoria baseada em eventos."),
    }
}

fn event_improvement_bucket_target(bucket: &EventImprovementPolicyBucket) -> &str {
    match bucket.scope.as_str() {
        "node" => bucket.node_ref.as_deref().unwrap_or("_node"),
        "addon" => bucket.addon_id.as_deref().unwrap_or("_addon"),
        "workflow" => "_workflow",
        _ => bucket
            .node_ref
            .as_deref()
            .or(bucket.addon_id.as_deref())
            .unwrap_or("_workflow"),
    }
}

fn event_improvement_suggested_commands(
    bucket: &EventImprovementPolicyBucket,
    recommended_policy: &str,
) -> Vec<Vec<String>> {
    let mut commands = vec![vec![
        "forge".to_string(),
        "events".to_string(),
        "observability".to_string(),
        "--workflow".to_string(),
        bucket.workflow_id.clone(),
        "--output".to_string(),
        "json".to_string(),
    ]];
    if bucket.scope == "node" {
        if let Some(node_ref) = bucket.node_ref.as_deref() {
            commands[0].push("--node".to_string());
            commands[0].push(node_ref.to_string());
        }
    }
    if matches!(bucket.scope.as_str(), "node" | "addon") {
        if let Some(addon_id) = bucket.addon_id.as_deref() {
            commands[0].push("--addon".to_string());
            commands[0].push(addon_id.to_string());
        }
    }
    if recommended_policy == "prefer_deterministic_node" && bucket.scope == "node" {
        let Some(node_ref) = bucket.node_ref.as_deref() else {
            return commands;
        };
        commands.push(vec![
            "forge".to_string(),
            "workflow".to_string(),
            "update-node-brain".to_string(),
            "--workflow".to_string(),
            bucket.workflow_id.clone(),
            "--task".to_string(),
            node_ref.to_string(),
            "--default-brain".to_string(),
            "command".to_string(),
            "--origin".to_string(),
            "event_improvement_policy".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ]);
    }
    commands
}

fn event_has_ai_signal(record: &EventObservabilityRecord) -> bool {
    let haystack = [
        record.kind.as_str(),
        record.category.as_str(),
        record.origin.as_str(),
        record.source.as_str(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    [
        "ai", "codex", "opencode", "gemini", "claude", "executor", "llm",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

pub fn ingest_inbound_event(
    store: &ForgeStore,
    input: InboundEventIngestInput,
) -> Result<InboundEventIngestReport> {
    ingest_inbound_event_with_context(store, input, &OperatingContextSpec::default())
}

pub fn ingest_inbound_event_with_context(
    store: &ForgeStore,
    input: InboundEventIngestInput,
    operating_context: &OperatingContextSpec,
) -> Result<InboundEventIngestReport> {
    let origin = required_text("origin", &input.origin)?;
    let action = required_text("action", &input.action)?;
    let event_id = format!("evtin_{}", Uuid::new_v4().to_string().replace('-', ""));
    let tenant_context = serde_json::to_value(operating_context)?;
    store.save_inbound_event(
        &event_id,
        &origin,
        &action,
        "pending",
        &input.data,
        &tenant_context,
    )?;
    let event = store.load_inbound_event(&event_id)?;
    Ok(InboundEventIngestReport {
        schema_version: EVENT_INGEST_SCHEMA_VERSION.to_string(),
        status: "event_ingested".to_string(),
        event: inbound_event_view(store, event),
    })
}

pub fn list_inbound_event_inbox(
    store: &ForgeStore,
    status: Option<&str>,
    limit: usize,
) -> Result<InboundEventInboxReport> {
    list_inbound_event_inbox_for_context(store, status, limit, &OperatingContextSpec::default())
}

pub fn list_inbound_event_inbox_for_context(
    store: &ForgeStore,
    status: Option<&str>,
    limit: usize,
    operating_context: &OperatingContextSpec,
) -> Result<InboundEventInboxReport> {
    let status = status.map(str::trim).filter(|status| !status.is_empty());
    let (organization_id, brand_id, product_id) =
        event_inbox_tenant_filters_for_context(store, operating_context, "events inbox list")?;
    let events = store
        .list_inbound_events(
            status,
            limit,
            organization_id.as_deref(),
            brand_id.as_deref(),
            product_id.as_deref(),
        )?
        .into_iter()
        .map(|event| inbound_event_view(store, event))
        .collect::<Vec<_>>();
    Ok(InboundEventInboxReport {
        schema_version: EVENT_INBOX_SCHEMA_VERSION.to_string(),
        status: "event_inbox_loaded".to_string(),
        filters: InboundEventInboxFilters {
            status: status.map(ToString::to_string),
            limit,
            organization_id,
            brand_id,
            product_id,
        },
        event_count: events.len(),
        events,
    })
}

pub fn scan_inbound_event_inbox(
    store: &ForgeStore,
    project_root: &Path,
    status: Option<&str>,
    limit: usize,
    dispatch_activations: bool,
) -> Result<InboundEventWorkerReport> {
    let requested_status = normalize_text(status).unwrap_or_else(|| "pending".to_string());
    let limit = limit.max(1);
    let operating_context = load_project_operating_context(project_root)?;
    let (organization_id, brand_id, product_id) =
        event_inbox_tenant_filters_for_context(store, &operating_context, "events inbox scan")?;
    let records = store.list_inbound_events(
        Some(&requested_status),
        limit,
        organization_id.as_deref(),
        brand_id.as_deref(),
        product_id.as_deref(),
    )?;
    let mut entries = Vec::new();
    for record in records {
        let before_status = record.status.clone();
        match route_inbound_event(store, &record.id, project_root) {
            Ok(route) => {
                let (activation_dispatch, error) = if dispatch_activations {
                    match dispatch_inbound_event_activations_for_route(
                        store,
                        &route,
                        project_root,
                        false,
                    ) {
                        Ok(report) => (Some(report), None),
                        Err(error) => (None, Some(format!("activation dispatch failed: {error}"))),
                    }
                } else {
                    (None, None)
                };
                entries.push(InboundEventWorkerEntry {
                    event_id: record.id,
                    origin: record.origin,
                    action: record.action,
                    before_status,
                    after_status: route.event.status.clone(),
                    identity_context: route.event.identity_context.clone(),
                    workflow_id: route.workflow_id,
                    route_decision: Some(route.route_decision),
                    addon_event_adapter_plan: Some(route.addon_event_adapter_plan),
                    activation_dispatch,
                    error,
                })
            }
            Err(error) => {
                let error_message = error.to_string();
                let identity_context = inbound_event_identity_context(store, &record);
                let event_id = record.id.clone();
                let origin = record.origin.clone();
                let action = record.action.clone();
                let failed_data = json!({
                    "event_id": event_id,
                    "origin": origin,
                    "action": action,
                    "previous_status": before_status.clone(),
                    "worker_error": error_message.clone(),
                    "worker_project_root": project_root.display().to_string(),
                });
                store.update_inbound_event_status(&record.id, "failed", &failed_data)?;
                entries.push(InboundEventWorkerEntry {
                    event_id: record.id,
                    origin: record.origin,
                    action: record.action,
                    before_status,
                    after_status: "failed".to_string(),
                    identity_context,
                    workflow_id: None,
                    route_decision: None,
                    addon_event_adapter_plan: None,
                    activation_dispatch: None,
                    error: Some(error_message),
                });
            }
        }
    }
    let routed_count = entries
        .iter()
        .filter(|entry| entry.after_status == "routed")
        .count();
    let failed_count = entries
        .iter()
        .filter(|entry| entry.after_status == "failed" || entry.error.is_some())
        .count();
    Ok(InboundEventWorkerReport {
        schema_version: EVENT_WORKER_SCHEMA_VERSION.to_string(),
        status: "event_worker_scanned".to_string(),
        project_root: project_root.display().to_string(),
        requested_status,
        limit,
        scanned_count: entries.len(),
        routed_count,
        failed_count,
        events: entries,
    })
}

pub fn run_inbound_event_worker_loop(
    store: &ForgeStore,
    project_root: &Path,
    options: InboundEventWorkerLoopOptions<'_>,
) -> Result<InboundEventWorkerLoopReport> {
    let requested_status = normalize_text(options.status).unwrap_or_else(|| "pending".to_string());
    let limit = options.limit.max(1);
    let max_cycles = options.max_cycles.max(1);
    let stop_file_display = options.stop_file.map(|path| path.display().to_string());
    let mut cycles = Vec::new();
    let mut scanned_count = 0usize;
    let mut routed_count = 0usize;
    let mut failed_count = 0usize;
    let mut idle_cycle_count = 0usize;
    let mut stopped_reason = "max_cycles_reached".to_string();
    let mut stop_requested = false;

    for cycle in 1..=max_cycles {
        if event_stop_file_requested(options.stop_file) {
            stop_requested = true;
            stopped_reason = "stop_file_requested".to_string();
            break;
        }
        let report = scan_inbound_event_inbox(
            store,
            project_root,
            Some(&requested_status),
            limit,
            options.dispatch_activations,
        )?;
        scanned_count += report.scanned_count;
        routed_count += report.routed_count;
        failed_count += report.failed_count;
        let idle = report.scanned_count == 0;
        if idle {
            idle_cycle_count += 1;
        }
        let should_stop_for_idle = idle && options.idle_exit;
        let should_sleep =
            cycle < max_cycles && !should_stop_for_idle && options.interval_seconds > 0;
        let slept_after_seconds = if should_sleep {
            options.interval_seconds
        } else {
            0
        };
        cycles.push(InboundEventWorkerLoopCycle {
            cycle,
            slept_after_seconds,
            report,
        });
        if should_stop_for_idle {
            stopped_reason = "idle_exit".to_string();
            break;
        }
        if event_stop_file_requested(options.stop_file) {
            stop_requested = true;
            stopped_reason = "stop_file_requested".to_string();
            break;
        }
        if should_sleep {
            sleep(Duration::from_secs(options.interval_seconds));
        }
    }

    let status = if stop_requested {
        "event_worker_loop_stopped"
    } else if failed_count > 0 {
        "event_worker_loop_completed_with_failures"
    } else if scanned_count == 0 {
        "event_worker_loop_idle"
    } else {
        "event_worker_loop_completed"
    };
    Ok(InboundEventWorkerLoopReport {
        schema_version: EVENT_WORKER_LOOP_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: project_root.display().to_string(),
        requested_status,
        limit,
        max_cycles,
        interval_seconds: options.interval_seconds,
        idle_exit: options.idle_exit,
        cycle_count: cycles.len(),
        scanned_count,
        routed_count,
        failed_count,
        idle_cycle_count,
        stopped_reason,
        stop_requested,
        stop_file: stop_file_display,
        cycles,
    })
}

fn event_stop_file_requested(stop_file: Option<&Path>) -> bool {
    stop_file.is_some_and(|path| path.exists())
}

#[allow(clippy::too_many_arguments)]
pub fn build_event_service_plan(
    store: &ForgeStore,
    project_root: &Path,
    service_kind: &str,
    status: Option<&str>,
    limit: usize,
    max_cycles: usize,
    interval_seconds: u64,
    idle_exit: bool,
    dispatch_activations: bool,
    host: &str,
    port: u16,
    path: &str,
    origin: Option<&str>,
    action: Option<&str>,
    schema: Option<&str>,
    route: bool,
    max_requests: usize,
    max_body_bytes: usize,
    hmac_secret_env: Option<&str>,
    signature_header: &str,
    lease_seconds: u64,
    heartbeat_seconds: u64,
    backoff_initial_seconds: u64,
    backoff_max_seconds: u64,
    shutdown_grace_seconds: u64,
) -> Result<EventServicePlanReport> {
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let service_kind = normalize_event_service_kind(service_kind)?;
    let service_id = format!("evtservice_{}", Uuid::new_v4().to_string().replace('-', ""));
    let lease_seconds = lease_seconds.clamp(30, 86_400);
    let heartbeat_seconds = heartbeat_seconds.clamp(5, lease_seconds);
    let backoff_initial_seconds = backoff_initial_seconds.clamp(1, 3600);
    let backoff_max_seconds = backoff_max_seconds.clamp(backoff_initial_seconds, 86_400);
    let shutdown_grace_seconds = shutdown_grace_seconds.clamp(1, 3600);
    let requested_status = normalize_text(status).unwrap_or_else(|| "pending".to_string());
    let limit = limit.max(1);
    let max_cycles = max_cycles.max(1);
    let mut notes = vec![
        "plan_only: no daemon or external supervisor is started by this command".to_string(),
        "execution: use service-run for one bounded service execution or service-supervise for bounded restart/backoff supervision".to_string(),
    ];

    let (command, settings) = if service_kind == "worker" {
        let mut command = vec![
            "forge".to_string(),
            "events".to_string(),
            "worker".to_string(),
            "--project-root".to_string(),
            project_root.display().to_string(),
            "--status".to_string(),
            requested_status.clone(),
            "--limit".to_string(),
            limit.to_string(),
            "--max-cycles".to_string(),
            max_cycles.to_string(),
            "--interval-seconds".to_string(),
            interval_seconds.to_string(),
        ];
        if idle_exit {
            command.push("--idle-exit".to_string());
        }
        if dispatch_activations {
            command.push("--dispatch-activations".to_string());
        }
        command.extend(["--output".to_string(), "json".to_string()]);
        (
            command,
            json!({
                "status": requested_status,
                "limit": limit,
                "max_cycles": max_cycles,
                "interval_seconds": interval_seconds,
                "idle_exit": idle_exit,
                "dispatch_activations": dispatch_activations,
            }),
        )
    } else {
        let host = required_text("host", host)?;
        let path = normalize_webhook_path(path)?;
        let origin = required_text("origin", origin.unwrap_or("external_webhook"))?;
        let action = required_text("action", action.unwrap_or("start_workflow"))?;
        let signature_header = required_text("signature_header", signature_header)?;
        let max_requests = max_requests.max(1);
        let max_body_bytes = max_body_bytes.clamp(256, 1_048_576);
        let hmac_secret_env = normalize_text(hmac_secret_env);
        if hmac_secret_env.is_none() {
            notes.push(
                "webhook_ingress_without_hmac: declare --hmac-secret-env for signed production ingress"
                    .to_string(),
            );
        }
        let mut command = vec![
            "forge".to_string(),
            "events".to_string(),
            "webhook-ingress".to_string(),
            "--host".to_string(),
            host.clone(),
            "--port".to_string(),
            port.to_string(),
            "--path".to_string(),
            path.clone(),
            "--origin".to_string(),
            origin.clone(),
            "--action".to_string(),
            action.clone(),
            "--project-root".to_string(),
            project_root.display().to_string(),
            "--max-requests".to_string(),
            max_requests.to_string(),
            "--max-body-bytes".to_string(),
            max_body_bytes.to_string(),
        ];
        if let Some(schema) = normalize_text(schema) {
            command.extend(["--schema".to_string(), schema.clone()]);
        }
        if route {
            command.push("--route".to_string());
        }
        if let Some(secret_env) = &hmac_secret_env {
            command.extend(["--hmac-secret-env".to_string(), secret_env.clone()]);
            command.extend(["--signature-header".to_string(), signature_header.clone()]);
        }
        command.extend(["--output".to_string(), "json".to_string()]);
        (
            command,
            json!({
                "host": host,
                "port": port,
                "path": path,
                "origin": origin,
                "action": action,
                "schema": normalize_text(schema),
                "route": route,
                "max_requests": max_requests,
                "max_body_bytes": max_body_bytes,
                "hmac_secret_env": hmac_secret_env,
                "signature_header": signature_header,
            }),
        )
    };

    let lease = json!({
        "enabled": true,
        "owner": "forge.event_service_manager",
        "ttl_seconds": lease_seconds,
        "heartbeat_interval_seconds": heartbeat_seconds,
        "stale_policy": "recover_or_restart_after_ttl",
    });
    let backoff = json!({
        "enabled": true,
        "initial_seconds": backoff_initial_seconds,
        "max_seconds": backoff_max_seconds,
        "multiplier": 2.0,
        "jitter": "bounded",
    });
    let shutdown = json!({
        "mode": "cooperative",
        "grace_seconds": shutdown_grace_seconds,
        "safe_points": ["between_worker_cycles", "between_webhook_requests"],
        "signal_policy": "finish_current_request_then_stop",
    });
    let health = json!({
        "schema_version": "forge.event_service_health_plan.v1",
        "checks": ["process_alive", "lease_fresh", "last_cycle_or_request_recent", "failed_count_threshold"],
        "interval_seconds": heartbeat_seconds,
        "report_command": ["forge", "events", "timeline", "--limit", "20", "--output", "json"],
    });

    let mut report = EventServicePlanReport {
        schema_version: EVENT_SERVICE_PLAN_SCHEMA_VERSION.to_string(),
        status: "event_service_plan_created".to_string(),
        service_id,
        service_kind,
        mode: "plan_only".to_string(),
        project_root: project_root.display().to_string(),
        command,
        settings,
        lease,
        backoff,
        shutdown,
        health,
        global_event_id: None,
        notes,
    };
    let operating_context = load_project_operating_context(&project_root)?;
    let tenant_context = serde_json::to_value(&operating_context)?;
    let event_data = serde_json::to_value(&report)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "event_service_plan",
        source_id: &report.service_id,
        workflow_id: None,
        kind: "event_service_plan_created",
        origin: "forge",
        status: &report.status,
        data: &event_data,
        tenant_context: &tenant_context,
    })?;
    report.global_event_id = Some(global_event_id);
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
pub fn run_event_worker_service(
    store: &ForgeStore,
    project_root: &Path,
    status: Option<&str>,
    limit: usize,
    max_cycles: usize,
    interval_seconds: u64,
    idle_exit: bool,
    dispatch_activations: bool,
    stop_file: Option<&Path>,
    lease_owner: &str,
    lease_seconds: u64,
    heartbeat_seconds: u64,
) -> Result<EventServiceRunReport> {
    let lease_owner = required_text("lease_owner", lease_owner)?;
    let lease_seconds = lease_seconds.clamp(30, 86_400);
    let heartbeat_seconds = heartbeat_seconds.clamp(5, lease_seconds);
    let plan = build_event_service_plan(
        store,
        project_root,
        "worker",
        status,
        limit,
        max_cycles,
        interval_seconds,
        idle_exit,
        dispatch_activations,
        "127.0.0.1",
        8787,
        "/webhook",
        None,
        None,
        None,
        false,
        1,
        65_536,
        None,
        "X-Forge-Signature",
        lease_seconds,
        heartbeat_seconds,
        5,
        300,
        30,
    )?;
    let service_id = plan.service_id.clone();
    let operating_context = load_project_operating_context(project_root)?;
    let tenant_context = serde_json::to_value(&operating_context)?;
    let lease_id = format!("evtlease_{}", Uuid::new_v4().to_string().replace('-', ""));
    let acquired_at = Utc::now();
    let mut lease_expires_at = acquired_at + ChronoDuration::seconds(lease_seconds as i64);
    let mut last_heartbeat_at = acquired_at;
    let mut heartbeat_count = 1usize;
    let mut lease_renewal_count = 0usize;
    let mut lease = json!({
        "lease_id": lease_id,
        "owner": lease_owner,
        "acquired_at": acquired_at.to_rfc3339(),
        "expires_at": lease_expires_at.to_rfc3339(),
        "ttl_seconds": lease_seconds,
        "renewal_count": lease_renewal_count,
    });
    let mut heartbeat = json!({
        "last_heartbeat_at": last_heartbeat_at.to_rfc3339(),
        "heartbeat_expires_at": (last_heartbeat_at
            + ChronoDuration::seconds(heartbeat_seconds as i64))
        .to_rfc3339(),
        "heartbeat_ttl_seconds": heartbeat_seconds,
        "heartbeat_count": heartbeat_count,
    });
    let running_data = json!({
        "plan": plan.clone(),
        "lease": lease.clone(),
        "heartbeat": heartbeat.clone(),
        "health": {
            "status": "running",
            "checked_at": last_heartbeat_at.to_rfc3339(),
            "heartbeat_count": heartbeat_count,
            "lease_renewal_count": lease_renewal_count,
        },
    });
    let acquired = store.try_save_event_service(EventServiceWrite {
        id: &service_id,
        service_kind: "worker",
        status: "running",
        tenant_context: &tenant_context,
        lease_owner: &lease_owner,
        lease_id: &lease_id,
        lease_acquired_at: &acquired_at.to_rfc3339(),
        lease_expires_at: &lease_expires_at.to_rfc3339(),
        last_heartbeat_at: &last_heartbeat_at.to_rfc3339(),
        heartbeat_ttl_seconds: heartbeat_seconds,
        data: &running_data,
    })?;
    if !acquired {
        let current = store
            .load_event_service(&service_id)?
            .map(EventServiceView::from);
        bail!(
            "event service lease conflict for {service_id}: current={}",
            serde_json::to_string(&current)?
        );
    }

    let requested_status = normalize_text(status).unwrap_or_else(|| "pending".to_string());
    let limit = limit.max(1);
    let max_cycles = max_cycles.max(1);
    let stop_file_display = stop_file.map(|path| path.display().to_string());
    let mut cycles = Vec::new();
    let mut scanned_count = 0usize;
    let mut routed_count = 0usize;
    let mut failed_count = 0usize;
    let mut idle_cycle_count = 0usize;
    let mut stopped_reason = "max_cycles_reached".to_string();
    let mut stop_requested = false;

    for cycle in 1..=max_cycles {
        if event_stop_file_requested(stop_file) {
            stop_requested = true;
            stopped_reason = "stop_file_requested".to_string();
            break;
        }
        let report = scan_inbound_event_inbox(
            store,
            project_root,
            Some(&requested_status),
            limit,
            dispatch_activations,
        )?;
        scanned_count += report.scanned_count;
        routed_count += report.routed_count;
        failed_count += report.failed_count;
        let idle = report.scanned_count == 0;
        if idle {
            idle_cycle_count += 1;
        }
        let should_stop_for_idle = idle && idle_exit;
        let should_sleep = cycle < max_cycles && !should_stop_for_idle && interval_seconds > 0;
        let slept_after_seconds = if should_sleep { interval_seconds } else { 0 };
        cycles.push(InboundEventWorkerLoopCycle {
            cycle,
            slept_after_seconds,
            report,
        });

        last_heartbeat_at = Utc::now();
        lease_expires_at = last_heartbeat_at + ChronoDuration::seconds(lease_seconds as i64);
        heartbeat_count += 1;
        lease_renewal_count += 1;
        lease = json!({
            "lease_id": lease_id,
            "owner": lease_owner,
            "acquired_at": acquired_at.to_rfc3339(),
            "expires_at": lease_expires_at.to_rfc3339(),
            "ttl_seconds": lease_seconds,
            "renewal_count": lease_renewal_count,
        });
        heartbeat = json!({
            "last_heartbeat_at": last_heartbeat_at.to_rfc3339(),
            "heartbeat_expires_at": (last_heartbeat_at
                + ChronoDuration::seconds(heartbeat_seconds as i64))
            .to_rfc3339(),
            "heartbeat_ttl_seconds": heartbeat_seconds,
            "heartbeat_count": heartbeat_count,
        });
        let running_health = json!({
            "schema_version": "forge.event_service_health.v1",
            "status": "running",
            "checked_at": last_heartbeat_at.to_rfc3339(),
            "request_count": scanned_count,
            "routed_count": routed_count,
            "failed_count": failed_count,
            "cycle_count": cycles.len(),
            "idle_cycle_count": idle_cycle_count,
            "heartbeat_count": heartbeat_count,
            "lease_renewal_count": lease_renewal_count,
            "last_cycle_at": last_heartbeat_at.to_rfc3339(),
            "last_cycle_idle": idle,
        });
        let running_data = json!({
            "plan": plan.clone(),
            "lease": lease.clone(),
            "heartbeat": heartbeat.clone(),
            "health": running_health,
            "worker_report_partial": {
                "schema_version": EVENT_WORKER_LOOP_SCHEMA_VERSION,
                "requested_status": requested_status,
                "limit": limit,
                "max_cycles": max_cycles,
                "interval_seconds": interval_seconds,
                "idle_exit": idle_exit,
                "dispatch_activations": dispatch_activations,
                "cycle_count": cycles.len(),
                "scanned_count": scanned_count,
                "routed_count": routed_count,
                "failed_count": failed_count,
                "idle_cycle_count": idle_cycle_count,
                "stop_requested": stop_requested,
                "stop_file": stop_file_display.clone(),
            },
        });
        store.save_event_service(EventServiceWrite {
            id: &service_id,
            service_kind: "worker",
            status: "running",
            tenant_context: &tenant_context,
            lease_owner: &lease_owner,
            lease_id: &lease_id,
            lease_acquired_at: &acquired_at.to_rfc3339(),
            lease_expires_at: &lease_expires_at.to_rfc3339(),
            last_heartbeat_at: &last_heartbeat_at.to_rfc3339(),
            heartbeat_ttl_seconds: heartbeat_seconds,
            data: &running_data,
        })?;

        if should_stop_for_idle {
            stopped_reason = "idle_exit".to_string();
            break;
        }
        if event_stop_file_requested(stop_file) {
            stop_requested = true;
            stopped_reason = "stop_file_requested".to_string();
            break;
        }
        if should_sleep {
            sleep(Duration::from_secs(interval_seconds));
        }
    }

    let worker_status = if stop_requested {
        "event_worker_loop_stopped"
    } else if failed_count > 0 {
        "event_worker_loop_completed_with_failures"
    } else if scanned_count == 0 {
        "event_worker_loop_idle"
    } else {
        "event_worker_loop_completed"
    };
    let worker_report = InboundEventWorkerLoopReport {
        schema_version: EVENT_WORKER_LOOP_SCHEMA_VERSION.to_string(),
        status: worker_status.to_string(),
        project_root: project_root.display().to_string(),
        requested_status,
        limit,
        max_cycles,
        interval_seconds,
        idle_exit,
        cycle_count: cycles.len(),
        scanned_count,
        routed_count,
        failed_count,
        idle_cycle_count,
        stopped_reason,
        stop_requested,
        stop_file: stop_file_display.clone(),
        cycles,
    };
    let last_cycle_at = last_heartbeat_at;
    let completed_at = Utc::now();
    last_heartbeat_at = completed_at;
    heartbeat_count += 1;
    heartbeat = json!({
        "last_heartbeat_at": last_heartbeat_at.to_rfc3339(),
        "heartbeat_expires_at": (last_heartbeat_at
            + ChronoDuration::seconds(heartbeat_seconds as i64))
        .to_rfc3339(),
        "heartbeat_ttl_seconds": heartbeat_seconds,
        "heartbeat_count": heartbeat_count,
    });
    let final_status = if worker_report.stop_requested {
        "stopped"
    } else if worker_report.failed_count > 0 {
        "completed_with_failures"
    } else {
        "completed"
    };
    let report_status = if worker_report.stop_requested {
        "event_service_run_stopped"
    } else if worker_report.failed_count > 0 {
        "event_service_run_completed_with_failures"
    } else {
        "event_service_run_completed"
    };
    let health = json!({
        "schema_version": "forge.event_service_health.v1",
        "status": final_status,
        "checked_at": completed_at.to_rfc3339(),
        "request_count": worker_report.scanned_count,
        "routed_count": worker_report.routed_count,
        "failed_count": worker_report.failed_count,
        "cycle_count": worker_report.cycle_count,
        "idle_cycle_count": worker_report.idle_cycle_count,
        "stop_requested": worker_report.stop_requested,
        "stop_file": stop_file_display,
        "heartbeat_count": heartbeat_count,
        "lease_renewal_count": lease_renewal_count,
        "last_cycle_at": last_cycle_at.to_rfc3339(),
    });
    let final_data = json!({
        "plan": plan.clone(),
        "lease": lease.clone(),
        "heartbeat": heartbeat.clone(),
        "worker_report": worker_report.clone(),
        "health": health.clone(),
        "completed_at": completed_at.to_rfc3339(),
    });
    store.save_event_service(EventServiceWrite {
        id: &service_id,
        service_kind: "worker",
        status: final_status,
        tenant_context: &tenant_context,
        lease_owner: &lease_owner,
        lease_id: &lease_id,
        lease_acquired_at: &acquired_at.to_rfc3339(),
        lease_expires_at: &lease_expires_at.to_rfc3339(),
        last_heartbeat_at: &completed_at.to_rfc3339(),
        heartbeat_ttl_seconds: heartbeat_seconds,
        data: &final_data,
    })?;
    let service = store
        .load_event_service(&service_id)?
        .map(EventServiceView::from)
        .with_context(|| format!("event service not found after run: {service_id}"))?;
    let mut report = EventServiceRunReport {
        schema_version: EVENT_SERVICE_RUN_SCHEMA_VERSION.to_string(),
        status: report_status.to_string(),
        service,
        lease,
        heartbeat,
        plan,
        worker_report: Some(worker_report),
        webhook_report: None,
        health,
        global_event_id: None,
    };
    let event_data = serde_json::to_value(&report)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "event_service_run",
        source_id: &service_id,
        workflow_id: None,
        kind: report_status,
        origin: "forge",
        status: report_status,
        data: &event_data,
        tenant_context: &tenant_context,
    })?;
    report.global_event_id = Some(global_event_id);
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
pub fn run_event_webhook_ingress_service(
    store: &ForgeStore,
    project_root: &Path,
    host: &str,
    port: u16,
    path: &str,
    origin: Option<&str>,
    action: Option<&str>,
    schema: Option<&str>,
    route: bool,
    max_requests: usize,
    max_body_bytes: usize,
    hmac_secret_env: Option<&str>,
    signature_header: &str,
    stop_file: Option<&Path>,
    lease_owner: &str,
    lease_seconds: u64,
    heartbeat_seconds: u64,
) -> Result<EventServiceRunReport> {
    let lease_owner = required_text("lease_owner", lease_owner)?;
    let origin = required_text("origin", origin.unwrap_or("external_webhook"))?;
    let action = required_text("action", action.unwrap_or("start_workflow"))?;
    let lease_seconds = lease_seconds.clamp(30, 86_400);
    let heartbeat_seconds = heartbeat_seconds.clamp(5, lease_seconds);
    let plan = build_event_service_plan(
        store,
        project_root,
        "webhook_ingress",
        None,
        1,
        1,
        0,
        false,
        false,
        host,
        port,
        path,
        Some(&origin),
        Some(&action),
        schema,
        route,
        max_requests,
        max_body_bytes,
        hmac_secret_env,
        signature_header,
        lease_seconds,
        heartbeat_seconds,
        5,
        300,
        30,
    )?;
    let service_id = plan.service_id.clone();
    let operating_context = load_project_operating_context(project_root)?;
    let tenant_context = serde_json::to_value(&operating_context)?;
    let lease_id = format!("evtlease_{}", Uuid::new_v4().to_string().replace('-', ""));
    let acquired_at = Utc::now();
    let mut lease_expires_at = acquired_at + ChronoDuration::seconds(lease_seconds as i64);
    let mut last_heartbeat_at = acquired_at;
    let mut heartbeat_count = 1usize;
    let mut lease_renewal_count = 0usize;
    let mut lease = json!({
        "lease_id": lease_id,
        "owner": lease_owner,
        "acquired_at": acquired_at.to_rfc3339(),
        "expires_at": lease_expires_at.to_rfc3339(),
        "ttl_seconds": lease_seconds,
        "renewal_count": lease_renewal_count,
    });
    let mut heartbeat = json!({
        "last_heartbeat_at": last_heartbeat_at.to_rfc3339(),
        "heartbeat_expires_at": (last_heartbeat_at
            + ChronoDuration::seconds(heartbeat_seconds as i64))
        .to_rfc3339(),
        "heartbeat_ttl_seconds": heartbeat_seconds,
        "heartbeat_count": heartbeat_count,
    });
    let stop_file_display = stop_file.map(|path| path.display().to_string());
    let running_data = json!({
        "plan": plan.clone(),
        "lease": lease.clone(),
        "heartbeat": heartbeat.clone(),
        "health": {
            "schema_version": "forge.event_service_health.v1",
            "status": "running",
            "checked_at": last_heartbeat_at.to_rfc3339(),
            "request_count": 0,
            "ingested_count": 0,
            "routed_count": 0,
            "failed_count": 0,
            "heartbeat_count": heartbeat_count,
            "lease_renewal_count": lease_renewal_count,
            "stop_requested": false,
            "stop_file": stop_file_display.clone(),
        },
    });
    let acquired = store.try_save_event_service(EventServiceWrite {
        id: &service_id,
        service_kind: "webhook_ingress",
        status: "running",
        tenant_context: &tenant_context,
        lease_owner: &lease_owner,
        lease_id: &lease_id,
        lease_acquired_at: &acquired_at.to_rfc3339(),
        lease_expires_at: &lease_expires_at.to_rfc3339(),
        last_heartbeat_at: &last_heartbeat_at.to_rfc3339(),
        heartbeat_ttl_seconds: heartbeat_seconds,
        data: &running_data,
    })?;
    if !acquired {
        let current = store
            .load_event_service(&service_id)?
            .map(EventServiceView::from);
        bail!(
            "event service lease conflict for {service_id}: current={}",
            serde_json::to_string(&current)?
        );
    }

    let mut progress_heartbeat_count = 0usize;
    let webhook_report_result = {
        let mut record_progress = |events: &[EventWebhookIngressEntry],
                                   phase: &str|
         -> Result<()> {
            let ingested_count = events
                .iter()
                .filter(|event| event.event_id.is_some())
                .count();
            let routed_count = events.iter().filter(|event| event.route.is_some()).count();
            let failed_count = events.iter().filter(|event| event.error.is_some()).count();
            last_heartbeat_at = Utc::now();
            heartbeat_count += 1;
            progress_heartbeat_count += 1;
            lease_renewal_count += 1;
            lease_expires_at = last_heartbeat_at + ChronoDuration::seconds(lease_seconds as i64);
            lease = json!({
                "lease_id": lease_id,
                "owner": lease_owner,
                "acquired_at": acquired_at.to_rfc3339(),
                "expires_at": lease_expires_at.to_rfc3339(),
                "ttl_seconds": lease_seconds,
                "renewal_count": lease_renewal_count,
            });
            heartbeat = json!({
                "last_heartbeat_at": last_heartbeat_at.to_rfc3339(),
                "heartbeat_expires_at": (last_heartbeat_at
                    + ChronoDuration::seconds(heartbeat_seconds as i64))
                .to_rfc3339(),
                "heartbeat_ttl_seconds": heartbeat_seconds,
                "heartbeat_count": heartbeat_count,
            });
            let stop_requested = phase == "stop_file_requested";
            let health = json!({
                "schema_version": "forge.event_service_health.v1",
                "status": "running",
                "phase": phase,
                "checked_at": last_heartbeat_at.to_rfc3339(),
                "request_count": events.len(),
                "ingested_count": ingested_count,
                "routed_count": routed_count,
                "failed_count": failed_count,
                "heartbeat_count": heartbeat_count,
                "progress_heartbeat_count": progress_heartbeat_count,
                "lease_renewal_count": lease_renewal_count,
                "stop_requested": stop_requested,
                "stop_file": stop_file_display.clone(),
                "last_event_status": events.last().map(|event| event.status.as_str()),
            });
            let progress_data = json!({
                "plan": plan.clone(),
                "lease": lease.clone(),
                "heartbeat": heartbeat.clone(),
                "health": health,
                "webhook_report_partial": {
                    "schema_version": EVENT_WEBHOOK_INGRESS_SCHEMA_VERSION,
                    "phase": phase,
                    "request_count": events.len(),
                    "ingested_count": ingested_count,
                    "routed_count": routed_count,
                    "failed_count": failed_count,
                    "stop_requested": stop_requested,
                    "stop_file": stop_file_display.clone(),
                },
            });
            store.save_event_service(EventServiceWrite {
                id: &service_id,
                service_kind: "webhook_ingress",
                status: "running",
                tenant_context: &tenant_context,
                lease_owner: &lease_owner,
                lease_id: &lease_id,
                lease_acquired_at: &acquired_at.to_rfc3339(),
                lease_expires_at: &lease_expires_at.to_rfc3339(),
                last_heartbeat_at: &last_heartbeat_at.to_rfc3339(),
                heartbeat_ttl_seconds: heartbeat_seconds,
                data: &progress_data,
            })?;
            Ok(())
        };
        run_event_webhook_ingress_server_with_progress(
            store,
            host,
            port,
            path,
            &origin,
            &action,
            "webhook",
            schema,
            project_root,
            route,
            max_requests,
            max_body_bytes,
            hmac_secret_env,
            signature_header,
            heartbeat_seconds,
            stop_file,
            Some(&mut record_progress),
        )
    };
    let webhook_report = match webhook_report_result {
        Ok(report) => report,
        Err(error) => {
            let failed_at = Utc::now();
            let health = json!({
                "schema_version": "forge.event_service_health.v1",
                "status": "failed",
                "checked_at": failed_at.to_rfc3339(),
                "request_count": 0,
                "ingested_count": 0,
                "routed_count": 0,
                "failed_count": 1,
                "heartbeat_count": heartbeat_count,
                "progress_heartbeat_count": progress_heartbeat_count,
                "lease_renewal_count": lease_renewal_count,
                "stop_requested": false,
                "stop_file": stop_file_display.clone(),
                "error": error.to_string(),
            });
            let failed_data = json!({
                "plan": plan.clone(),
                "lease": lease.clone(),
                "heartbeat": heartbeat.clone(),
                "health": health,
                "failed_at": failed_at.to_rfc3339(),
            });
            store.save_event_service(EventServiceWrite {
                id: &service_id,
                service_kind: "webhook_ingress",
                status: "failed",
                tenant_context: &tenant_context,
                lease_owner: &lease_owner,
                lease_id: &lease_id,
                lease_acquired_at: &acquired_at.to_rfc3339(),
                lease_expires_at: &lease_expires_at.to_rfc3339(),
                last_heartbeat_at: &last_heartbeat_at.to_rfc3339(),
                heartbeat_ttl_seconds: heartbeat_seconds,
                data: &failed_data,
            })?;
            return Err(error);
        }
    };

    let completed_at = Utc::now();
    last_heartbeat_at = completed_at;
    heartbeat_count += 1;
    lease_renewal_count += 1;
    lease_expires_at = completed_at + ChronoDuration::seconds(lease_seconds as i64);
    lease = json!({
        "lease_id": lease_id,
        "owner": lease_owner,
        "acquired_at": acquired_at.to_rfc3339(),
        "expires_at": lease_expires_at.to_rfc3339(),
        "ttl_seconds": lease_seconds,
        "renewal_count": lease_renewal_count,
    });
    heartbeat = json!({
        "last_heartbeat_at": last_heartbeat_at.to_rfc3339(),
        "heartbeat_expires_at": (last_heartbeat_at
            + ChronoDuration::seconds(heartbeat_seconds as i64))
        .to_rfc3339(),
        "heartbeat_ttl_seconds": heartbeat_seconds,
        "heartbeat_count": heartbeat_count,
    });
    let final_status = if webhook_report.stop_requested {
        "stopped"
    } else if webhook_report.failed_count > 0 {
        "completed_with_failures"
    } else {
        "completed"
    };
    let report_status = if webhook_report.stop_requested {
        "event_service_run_stopped"
    } else if webhook_report.failed_count > 0 {
        "event_service_run_completed_with_failures"
    } else {
        "event_service_run_completed"
    };
    let health = json!({
        "schema_version": "forge.event_service_health.v1",
        "status": final_status,
        "checked_at": completed_at.to_rfc3339(),
        "request_count": webhook_report.request_count,
        "ingested_count": webhook_report.ingested_count,
        "routed_count": webhook_report.routed_count,
        "failed_count": webhook_report.failed_count,
        "heartbeat_count": heartbeat_count,
        "progress_heartbeat_count": progress_heartbeat_count,
        "lease_renewal_count": lease_renewal_count,
        "stop_requested": webhook_report.stop_requested,
        "stop_file": webhook_report.stop_file,
        "bind_address": webhook_report.bind_address,
        "path": webhook_report.path,
        "auth_required": webhook_report.auth.required,
    });
    let final_data = json!({
        "plan": plan.clone(),
        "lease": lease.clone(),
        "heartbeat": heartbeat.clone(),
        "webhook_report": webhook_report.clone(),
        "health": health.clone(),
        "completed_at": completed_at.to_rfc3339(),
    });
    store.save_event_service(EventServiceWrite {
        id: &service_id,
        service_kind: "webhook_ingress",
        status: final_status,
        tenant_context: &tenant_context,
        lease_owner: &lease_owner,
        lease_id: &lease_id,
        lease_acquired_at: &acquired_at.to_rfc3339(),
        lease_expires_at: &lease_expires_at.to_rfc3339(),
        last_heartbeat_at: &completed_at.to_rfc3339(),
        heartbeat_ttl_seconds: heartbeat_seconds,
        data: &final_data,
    })?;
    let service = store
        .load_event_service(&service_id)?
        .map(EventServiceView::from)
        .with_context(|| format!("event service not found after run: {service_id}"))?;
    let mut report = EventServiceRunReport {
        schema_version: EVENT_SERVICE_RUN_SCHEMA_VERSION.to_string(),
        status: report_status.to_string(),
        service,
        lease,
        heartbeat,
        plan,
        worker_report: None,
        webhook_report: Some(webhook_report),
        health,
        global_event_id: None,
    };
    let event_data = serde_json::to_value(&report)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "event_service_run",
        source_id: &service_id,
        workflow_id: None,
        kind: report_status,
        origin: "forge",
        status: report_status,
        data: &event_data,
        tenant_context: &tenant_context,
    })?;
    report.global_event_id = Some(global_event_id);
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
pub fn run_event_service_supervisor(
    store: &ForgeStore,
    project_root: &Path,
    service_kind: &str,
    status: Option<&str>,
    limit: usize,
    max_cycles: usize,
    interval_seconds: u64,
    idle_exit: bool,
    dispatch_activations: bool,
    host: &str,
    port: u16,
    path: &str,
    origin: Option<&str>,
    action: Option<&str>,
    schema: Option<&str>,
    route: bool,
    max_requests: usize,
    max_body_bytes: usize,
    hmac_secret_env: Option<&str>,
    signature_header: &str,
    stop_file: Option<&Path>,
    lease_owner: &str,
    lease_seconds: u64,
    heartbeat_seconds: u64,
    max_runs: usize,
    backoff_initial_seconds: u64,
    backoff_max_seconds: u64,
) -> Result<EventServiceSupervisorReport> {
    let service_kind = normalize_event_service_kind(service_kind)?;
    let lease_owner = required_text("lease_owner", lease_owner)?;
    let max_runs = max_runs.max(1);
    let backoff_initial_seconds = backoff_initial_seconds.min(3600);
    let backoff_max_seconds = backoff_max_seconds.max(backoff_initial_seconds).min(86_400);
    let stop_file_display = stop_file.map(|path| path.display().to_string());
    let supervisor_id = format!(
        "evtsupervisor_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let started_at = Utc::now();
    let mut runs = Vec::new();
    let mut success_count = 0usize;
    let mut failure_count = 0usize;
    let mut stopped_count = 0usize;
    let mut stop_requested = false;
    let mut stopped_reason = "max_runs_reached".to_string();
    let mut current_backoff_seconds = backoff_initial_seconds;

    for run in 1..=max_runs {
        if event_stop_file_requested(stop_file) {
            stop_requested = true;
            stopped_reason = "stop_file_requested".to_string();
            break;
        }

        let service_result = if service_kind == "worker" {
            run_event_worker_service(
                store,
                project_root,
                status,
                limit,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                stop_file,
                &lease_owner,
                lease_seconds,
                heartbeat_seconds,
            )
        } else {
            run_event_webhook_ingress_service(
                store,
                project_root,
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
                stop_file,
                &lease_owner,
                lease_seconds,
                heartbeat_seconds,
            )
        };

        match service_result {
            Ok(report) => {
                let service_id = report.service.id.clone();
                let global_event_id = report.global_event_id;
                let run_status = report.status.clone();
                let run_stopped =
                    run_status == "event_service_run_stopped" || report.service.status == "stopped";
                let run_failed = run_status == "event_service_run_completed_with_failures"
                    || report.service.status == "failed"
                    || report.service.status == "completed_with_failures";

                let backoff_after_seconds = if run_failed && run < max_runs {
                    current_backoff_seconds
                } else {
                    0
                };

                if run_stopped {
                    stopped_count += 1;
                    stop_requested = true;
                    stopped_reason = "service_stop_requested".to_string();
                } else if run_failed {
                    failure_count += 1;
                } else {
                    success_count += 1;
                    current_backoff_seconds = backoff_initial_seconds;
                }

                runs.push(EventServiceSupervisorRun {
                    run,
                    status: run_status,
                    service_id: Some(service_id),
                    global_event_id,
                    backoff_after_seconds,
                    report: Some(report),
                    error: None,
                });

                if run_stopped {
                    break;
                }
                if backoff_after_seconds > 0 {
                    sleep(Duration::from_secs(backoff_after_seconds));
                    current_backoff_seconds = next_event_service_supervisor_backoff(
                        current_backoff_seconds,
                        backoff_max_seconds,
                    );
                }
            }
            Err(error) => {
                failure_count += 1;
                let backoff_after_seconds = if run < max_runs {
                    current_backoff_seconds
                } else {
                    0
                };
                runs.push(EventServiceSupervisorRun {
                    run,
                    status: "event_service_run_failed".to_string(),
                    service_id: None,
                    global_event_id: None,
                    backoff_after_seconds,
                    report: None,
                    error: Some(error.to_string()),
                });
                if backoff_after_seconds > 0 {
                    sleep(Duration::from_secs(backoff_after_seconds));
                    current_backoff_seconds = next_event_service_supervisor_backoff(
                        current_backoff_seconds,
                        backoff_max_seconds,
                    );
                }
            }
        }
    }

    let completed_at = Utc::now();
    let status = if stop_requested {
        "event_service_supervisor_stopped"
    } else if failure_count > 0 && success_count == 0 {
        "event_service_supervisor_failed"
    } else if failure_count > 0 {
        "event_service_supervisor_completed_with_failures"
    } else {
        "event_service_supervisor_completed"
    };
    let health = json!({
        "schema_version": "forge.event_service_supervisor_health.v1",
        "status": status,
        "started_at": started_at.to_rfc3339(),
        "completed_at": completed_at.to_rfc3339(),
        "service_kind": service_kind,
        "run_count": runs.len(),
        "success_count": success_count,
        "failure_count": failure_count,
        "stopped_count": stopped_count,
        "max_runs": max_runs,
        "stop_requested": stop_requested,
        "stop_file": stop_file_display,
        "backoff_initial_seconds": backoff_initial_seconds,
        "backoff_max_seconds": backoff_max_seconds,
    });
    let mut report = EventServiceSupervisorReport {
        schema_version: EVENT_SERVICE_SUPERVISOR_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        supervisor_id,
        service_kind,
        project_root: project_root.display().to_string(),
        max_runs,
        run_count: runs.len(),
        success_count,
        failure_count,
        stopped_count,
        backoff_initial_seconds,
        backoff_max_seconds,
        stopped_reason,
        stop_requested,
        stop_file: stop_file_display,
        health,
        runs,
        global_event_id: None,
    };
    let operating_context = load_project_operating_context(project_root)?;
    let tenant_context = serde_json::to_value(&operating_context)?;
    let event_data = serde_json::to_value(&report)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "event_service_supervisor",
        source_id: &report.supervisor_id,
        workflow_id: None,
        kind: status,
        origin: "forge",
        status,
        data: &event_data,
        tenant_context: &tenant_context,
    })?;
    report.global_event_id = Some(global_event_id);
    Ok(report)
}

fn next_event_service_supervisor_backoff(current: u64, max: u64) -> u64 {
    if current == 0 {
        0
    } else {
        current.saturating_mul(2).min(max)
    }
}

pub fn list_event_services(
    store: &ForgeStore,
    project_root: &Path,
    service_kind: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<EventServiceListReport> {
    let operating_context = load_project_operating_context(project_root)?;
    let (organization_id, brand_id, product_id) = event_service_tenant_filters_for_context(
        store,
        &operating_context,
        "events services list",
    )?;
    let service_kind = service_kind
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let status = status.map(str::trim).filter(|value| !value.is_empty());
    let services = store
        .list_event_services(
            service_kind,
            status,
            limit,
            organization_id.as_deref(),
            brand_id.as_deref(),
            product_id.as_deref(),
        )?
        .into_iter()
        .map(EventServiceView::from)
        .collect::<Vec<_>>();
    Ok(EventServiceListReport {
        schema_version: EVENT_SERVICES_SCHEMA_VERSION.to_string(),
        status: "event_services_loaded".to_string(),
        filters: json!({
            "service_kind": service_kind,
            "status": status,
            "limit": limit.max(1),
            "organization_id": organization_id,
            "brand_id": brand_id,
            "product_id": product_id,
        }),
        service_count: services.len(),
        services,
    })
}

pub fn recover_stale_event_services(
    store: &ForgeStore,
    project_root: &Path,
    service_kind: Option<&str>,
    limit: usize,
    origin: &str,
) -> Result<EventServiceRecoveryReport> {
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let service_kind = service_kind
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let origin = normalize_text(Some(origin)).unwrap_or_else(|| "forge".to_string());
    let limit = limit.max(1);
    let operating_context = load_project_operating_context(&project_root)?;
    let (organization_id, brand_id, product_id) = event_service_tenant_filters_for_context(
        store,
        &operating_context,
        "events services recover",
    )?;
    let recovery_id = format!(
        "evtsvcrecover_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let now = Utc::now();
    let observed_at = now.to_rfc3339();
    let service_records = store.list_event_services(
        service_kind,
        Some("running"),
        limit,
        organization_id.as_deref(),
        brand_id.as_deref(),
        product_id.as_deref(),
    )?;
    let mut recovered = Vec::new();
    let mut stale_running_count = 0usize;

    for service in service_records.iter() {
        if event_service_lease_active(service, now) {
            continue;
        }
        stale_running_count += 1;
        let mut data = service.data.clone();
        if let Some(object) = data.as_object_mut() {
            object.insert(
                "recovery".to_string(),
                json!({
                    "schema_version": "forge.event_service_recovery_marker.v1",
                    "status": "stale",
                    "origin": origin,
                    "recovered_at": observed_at,
                    "previous_status": service.status,
                    "lease_owner": service.lease_owner,
                    "lease_id": service.lease_id,
                    "lease_expires_at": service.lease_expires_at,
                    "last_heartbeat_at": service.last_heartbeat_at,
                    "reason": "running event service lease expired before recovery scan"
                }),
            );
        } else {
            data = json!({
                "previous_data": data,
                "recovery": {
                    "schema_version": "forge.event_service_recovery_marker.v1",
                    "status": "stale",
                    "origin": origin,
                    "recovered_at": observed_at,
                    "previous_status": service.status,
                    "lease_owner": service.lease_owner,
                    "lease_id": service.lease_id,
                    "lease_expires_at": service.lease_expires_at,
                    "last_heartbeat_at": service.last_heartbeat_at,
                    "reason": "running event service lease expired before recovery scan"
                }
            });
        }
        store.save_event_service(EventServiceWrite {
            id: &service.id,
            service_kind: &service.service_kind,
            status: "stale",
            tenant_context: &service.tenant_context,
            lease_owner: &service.lease_owner,
            lease_id: &service.lease_id,
            lease_acquired_at: &service.lease_acquired_at,
            lease_expires_at: &service.lease_expires_at,
            last_heartbeat_at: &service.last_heartbeat_at,
            heartbeat_ttl_seconds: service.heartbeat_ttl_seconds,
            data: &data,
        })?;
        let recovered_service = store
            .load_event_service(&service.id)?
            .map(EventServiceView::from)
            .with_context(|| {
                format!("event service missing after stale recovery: {}", service.id)
            })?;
        recovered.push(recovered_service);
    }

    let status = if recovered.is_empty() {
        "event_services_no_stale_running_leases"
    } else {
        "event_services_recovered_stale_running_leases"
    };
    let mut report = EventServiceRecoveryReport {
        schema_version: EVENT_SERVICES_RECOVERY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        recovery_id,
        project_root: project_root.display().to_string(),
        origin,
        filters: json!({
            "service_kind": service_kind,
            "status": "running",
            "limit": limit,
            "organization_id": organization_id,
            "brand_id": brand_id,
            "product_id": product_id,
        }),
        scanned_count: service_records.len(),
        stale_running_count,
        recovered_count: recovered.len(),
        services: recovered,
        global_event_id: None,
    };
    if report.recovered_count > 0 {
        let tenant_context = serde_json::to_value(&operating_context)?;
        let event_data = serde_json::to_value(&report)?;
        let global_event_id = store.record_global_event(GlobalEventWrite {
            source: "event_services_recovery",
            source_id: &report.recovery_id,
            workflow_id: None,
            kind: status,
            origin: &report.origin,
            status,
            data: &event_data,
            tenant_context: &tenant_context,
        })?;
        report.global_event_id = Some(global_event_id);
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
pub fn run_event_runtime_reconcile(
    store: &ForgeStore,
    project_root: &Path,
    status: Option<&str>,
    limit: usize,
    service_limit: usize,
    execute: bool,
    max_cycles: usize,
    interval_seconds: u64,
    idle_exit: bool,
    dispatch_activations: bool,
    recover_stale_services: bool,
    stop_file: Option<&Path>,
    lease_owner: &str,
    lease_seconds: u64,
    heartbeat_seconds: u64,
    max_runs: usize,
    backoff_initial_seconds: u64,
    backoff_max_seconds: u64,
    scan_schedules: bool,
    schedule_executor: &str,
    schedule_max_workers: usize,
    schedule_ttl_seconds: u64,
) -> Result<EventRuntimeReconcileReport> {
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let requested_status = normalize_text(status).unwrap_or_else(|| "pending".to_string());
    let limit = limit.max(1);
    let service_limit = service_limit.max(1);
    let lease_owner = required_text("lease_owner", lease_owner)?;
    let schedule_executor = required_text("schedule_executor", schedule_executor)?;
    let schedule_max_workers = schedule_max_workers.max(1);
    let schedule_ttl_seconds = schedule_ttl_seconds.max(1);
    let reconcile_id = format!(
        "evtreconcile_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );

    let operating_context = load_project_operating_context(&project_root)?;
    let (organization_id, brand_id, product_id) = event_inbox_tenant_filters_for_context(
        store,
        &operating_context,
        "events runtime reconcile registry snapshot",
    )?;
    let registry_report = list_workflows(store)?;
    let allowed_workflow_ids = event_runtime_allowed_workflow_ids(
        store,
        organization_id.as_deref(),
        brand_id.as_deref(),
        product_id.as_deref(),
    )?;
    let registry_workflows = registry_report
        .workflows
        .into_iter()
        .filter(|workflow| match &allowed_workflow_ids {
            Some(ids) => ids.contains(&workflow.workflow_id),
            None => true,
        })
        .collect::<Vec<_>>();
    let mut action_counts = BTreeMap::<String, usize>::new();
    let mut actionable_workflows = Vec::new();
    for workflow in &registry_workflows {
        let action = workflow.runtime.operator_action.clone();
        *action_counts.entry(action.clone()).or_default() += 1;
        if matches!(
            action.as_str(),
            "keep_event_listener_ready" | "wake_on_event"
        ) {
            actionable_workflows.push(EventRuntimeWorkflowTarget {
                workflow_id: workflow.workflow_id.clone(),
                current_goal: workflow.current_goal.clone(),
                lifecycle_kind: workflow.runtime.lifecycle_kind.clone(),
                operational_state: workflow.runtime.operational_state.clone(),
                operator_action: action,
                reason: workflow.runtime.reason.clone(),
            });
        }
    }
    let operator_actions = action_counts
        .into_iter()
        .map(
            |(action, workflow_count)| EventRuntimeOperatorActionSummary {
                action,
                workflow_count,
            },
        )
        .collect::<Vec<_>>();
    let registry = EventRuntimeRegistrySnapshot {
        schema_version: "forge.event_runtime_registry_snapshot.v1".to_string(),
        workflow_count: registry_workflows.len(),
        persistent_workflows: registry_workflows
            .iter()
            .filter(|workflow| workflow.runtime.persistent)
            .count(),
        idle_waiting_for_events: registry_workflows
            .iter()
            .filter(|workflow| workflow.runtime.scale_to_zero_policy == "idle_waiting_for_events")
            .count(),
        scaled_to_zero: registry_workflows
            .iter()
            .filter(|workflow| workflow.runtime.operational_state == "scaled_to_zero")
            .count(),
        operator_actions,
        actionable_workflows,
    };

    let pending_events = store.list_inbound_events(
        Some(&requested_status),
        limit,
        organization_id.as_deref(),
        brand_id.as_deref(),
        product_id.as_deref(),
    )?;
    let inbox = EventRuntimeInboxSnapshot {
        schema_version: "forge.event_runtime_inbox_snapshot.v1".to_string(),
        status_filter: requested_status.clone(),
        pending_event_count: pending_events.len(),
        sampled_limit: limit,
    };

    let service_recovery = if recover_stale_services {
        Some(recover_stale_event_services(
            store,
            &project_root,
            Some("worker"),
            service_limit,
            &lease_owner,
        )?)
    } else {
        None
    };

    let (service_organization_id, service_brand_id, service_product_id) =
        event_service_tenant_filters_for_context(
            store,
            &operating_context,
            "events runtime reconcile services snapshot",
        )?;
    let service_records = store.list_event_services(
        Some("worker"),
        None,
        service_limit,
        service_organization_id.as_deref(),
        service_brand_id.as_deref(),
        service_product_id.as_deref(),
    )?;
    let now = Utc::now();
    let running_count = service_records
        .iter()
        .filter(|service| service.status == "running")
        .count();
    let active_lease_count = service_records
        .iter()
        .filter(|service| service.status == "running" && event_service_lease_active(service, now))
        .count();
    let stale_running_count = service_records
        .iter()
        .filter(|service| service.status == "running" && !event_service_lease_active(service, now))
        .count();
    let terminal_count = service_records
        .iter()
        .filter(|service| {
            matches!(
                service.status.as_str(),
                "completed" | "completed_with_failures" | "failed" | "stopped"
            )
        })
        .count();
    let services = service_records
        .into_iter()
        .map(EventServiceView::from)
        .collect::<Vec<_>>();
    let service_snapshot = EventRuntimeServiceSnapshot {
        schema_version: "forge.event_runtime_service_snapshot.v1".to_string(),
        service_kind: "worker".to_string(),
        service_count: services.len(),
        running_count,
        active_lease_count,
        stale_running_count,
        terminal_count,
        services,
    };

    let needs_event_worker =
        inbox.pending_event_count > 0 || !registry.actionable_workflows.is_empty();
    let mut recommendations = Vec::new();
    if needs_event_worker {
        let required = active_lease_count == 0;
        let action = if required {
            "start_event_worker_supervisor"
        } else {
            "observe_active_event_worker"
        };
        let reason = if inbox.pending_event_count > 0 && !registry.actionable_workflows.is_empty() {
            "pending inbound events and persistent workflows require the event worker supervisor"
        } else if inbox.pending_event_count > 0 {
            "pending inbound events require the event worker supervisor"
        } else {
            "persistent workflows are idle or scaled to zero and must stay wakeable by events"
        };
        let workflow_ids = registry
            .actionable_workflows
            .iter()
            .map(|workflow| workflow.workflow_id.clone())
            .collect::<Vec<_>>();
        recommendations.push(EventRuntimeServiceRecommendation {
            service_kind: "worker".to_string(),
            action: action.to_string(),
            required,
            reason: reason.to_string(),
            workflow_count: workflow_ids.len(),
            pending_event_count: inbox.pending_event_count,
            active_service_count: active_lease_count,
            workflow_ids,
            command: event_worker_supervisor_command(
                &project_root,
                &requested_status,
                limit,
                max_cycles,
                interval_seconds,
                idle_exit,
                dispatch_activations,
                stop_file,
                &lease_owner,
                lease_seconds,
                heartbeat_seconds,
                max_runs,
                backoff_initial_seconds,
                backoff_max_seconds,
            ),
        });
    }

    let mut executions = Vec::new();
    if execute {
        for recommendation in &recommendations {
            if recommendation.required && recommendation.service_kind == "worker" {
                executions.push(run_event_service_supervisor(
                    store,
                    &project_root,
                    "worker",
                    Some(&requested_status),
                    limit,
                    max_cycles,
                    interval_seconds,
                    idle_exit,
                    dispatch_activations,
                    "127.0.0.1",
                    8787,
                    "/webhook",
                    None,
                    None,
                    None,
                    false,
                    1,
                    65_536,
                    None,
                    "X-Forge-Signature",
                    stop_file,
                    &lease_owner,
                    lease_seconds,
                    heartbeat_seconds,
                    max_runs,
                    backoff_initial_seconds,
                    backoff_max_seconds,
                )?);
            }
        }
    }

    let mut schedule_execution_count = 0usize;
    let mut schedule_scale_to_zero_count = 0usize;
    let schedule = if scan_schedules {
        let worker_status = build_schedule_worker_status(
            store,
            &schedule_executor,
            schedule_max_workers,
            schedule_ttl_seconds,
        )?;
        let scan_due = if execute {
            let report = scan_due_workflows_parallel(
                store,
                &schedule_executor,
                schedule_max_workers,
                schedule_ttl_seconds,
            )?;
            schedule_execution_count = report.summary.executed_workflows;
            schedule_scale_to_zero_count = report.summary.scale_to_zero_workflows;
            Some(report)
        } else {
            None
        };
        Some(EventRuntimeScheduleSnapshot {
            schema_version: "forge.event_runtime_schedule_snapshot.v1".to_string(),
            scan_schedules,
            executor: schedule_executor.clone(),
            max_workers: schedule_max_workers,
            ttl_seconds: schedule_ttl_seconds,
            worker_status,
            scan_due,
        })
    } else {
        None
    };

    let execution_failed = executions.iter().any(|execution| {
        execution.status == "event_service_supervisor_failed"
            || execution.status == "event_service_supervisor_completed_with_failures"
    });
    let status = if execution_failed {
        "event_runtime_reconcile_executed_with_failures"
    } else if !executions.is_empty()
        || schedule_execution_count > 0
        || schedule_scale_to_zero_count > 0
    {
        "event_runtime_reconcile_executed"
    } else if recommendations
        .iter()
        .any(|recommendation| recommendation.required)
    {
        "event_runtime_reconcile_action_required"
    } else if !recommendations.is_empty() {
        "event_runtime_reconcile_observing_active_service"
    } else {
        "event_runtime_reconcile_no_action"
    };

    let mut report = EventRuntimeReconcileReport {
        schema_version: EVENT_RUNTIME_RECONCILE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        reconcile_id,
        project_root: project_root.display().to_string(),
        execute,
        dispatch_activations,
        registry,
        inbox,
        services: service_snapshot,
        recover_stale_services,
        service_recovery,
        recommendation_count: recommendations.len(),
        recommendations,
        execution_count: executions.len(),
        executions,
        schedule_execution_count,
        schedule_scale_to_zero_count,
        schedule,
        global_event_id: None,
    };
    let operating_context = load_project_operating_context(&project_root)?;
    let tenant_context = serde_json::to_value(&operating_context)?;
    let event_data = serde_json::to_value(&report)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "event_runtime_reconcile",
        source_id: &report.reconcile_id,
        workflow_id: None,
        kind: status,
        origin: "forge",
        status,
        data: &event_data,
        tenant_context: &tenant_context,
    })?;
    report.global_event_id = Some(global_event_id);
    Ok(report)
}

fn event_service_lease_active(service: &StoredEventServiceRecord, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(&service.lease_expires_at)
        .map(|expires_at| expires_at.with_timezone(&Utc) > now)
        .unwrap_or(true)
}

#[allow(clippy::too_many_arguments)]
fn event_worker_supervisor_command(
    project_root: &Path,
    status: &str,
    limit: usize,
    max_cycles: usize,
    interval_seconds: u64,
    idle_exit: bool,
    dispatch_activations: bool,
    stop_file: Option<&Path>,
    lease_owner: &str,
    lease_seconds: u64,
    heartbeat_seconds: u64,
    max_runs: usize,
    backoff_initial_seconds: u64,
    backoff_max_seconds: u64,
) -> Vec<String> {
    let mut command = vec![
        "forge".to_string(),
        "events".to_string(),
        "service-supervise".to_string(),
        "--kind".to_string(),
        "worker".to_string(),
        "--project-root".to_string(),
        project_root.display().to_string(),
        "--status".to_string(),
        status.to_string(),
        "--limit".to_string(),
        limit.to_string(),
        "--max-cycles".to_string(),
        max_cycles.to_string(),
        "--interval-seconds".to_string(),
        interval_seconds.to_string(),
        "--lease-owner".to_string(),
        lease_owner.to_string(),
        "--lease-seconds".to_string(),
        lease_seconds.to_string(),
        "--heartbeat-seconds".to_string(),
        heartbeat_seconds.to_string(),
        "--max-runs".to_string(),
        max_runs.to_string(),
        "--backoff-initial-seconds".to_string(),
        backoff_initial_seconds.to_string(),
        "--backoff-max-seconds".to_string(),
        backoff_max_seconds.to_string(),
    ];
    if idle_exit {
        command.push("--idle-exit".to_string());
    }
    if dispatch_activations {
        command.push("--dispatch-activations".to_string());
    }
    if let Some(stop_file) = stop_file {
        command.extend(["--stop-file".to_string(), stop_file.display().to_string()]);
    }
    command.extend(["--output".to_string(), "json".to_string()]);
    command
}

#[allow(clippy::too_many_arguments)]
pub fn run_event_runtime_daemon(
    store: &ForgeStore,
    project_root: &Path,
    status: Option<&str>,
    limit: usize,
    service_limit: usize,
    execute: bool,
    max_cycles: usize,
    interval_seconds: u64,
    idle_exit: bool,
    dispatch_activations: bool,
    continuous: bool,
    cycle_retention: usize,
    recover_stale_services: bool,
    stop_file: Option<&Path>,
    lease_owner: &str,
    lease_seconds: u64,
    heartbeat_seconds: u64,
    max_runs: usize,
    backoff_initial_seconds: u64,
    backoff_max_seconds: u64,
    scan_schedules: bool,
    schedule_executor: &str,
    schedule_max_workers: usize,
    schedule_ttl_seconds: u64,
) -> Result<EventRuntimeDaemonReport> {
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let requested_status = normalize_text(status).unwrap_or_else(|| "pending".to_string());
    let lease_owner = required_text("lease_owner", lease_owner)?;
    let lease_seconds = lease_seconds.clamp(30, 86_400);
    let heartbeat_seconds = heartbeat_seconds.clamp(5, lease_seconds);
    let schedule_executor = required_text("schedule_executor", schedule_executor)?;
    let schedule_max_workers = schedule_max_workers.max(1);
    let schedule_ttl_seconds = schedule_ttl_seconds.max(1);
    let max_cycles = max_cycles.max(1);
    let cycle_retention = cycle_retention.max(1);
    let limit = limit.max(1);
    let service_limit = service_limit.max(1);
    let service_id = format!(
        "evtrtdaemon_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let operating_context = load_project_operating_context(&project_root)?;
    let tenant_context = serde_json::to_value(&operating_context)?;
    let lease_id = format!("lease_{}", Uuid::new_v4().to_string().replace('-', ""));
    let stop_file_display = stop_file.map(|path| path.display().to_string());
    let started_at = Utc::now();
    let lease_expires_at =
        (started_at + ChronoDuration::seconds(lease_seconds as i64)).to_rfc3339();
    let running_data = json!({
        "schema_version": "forge.event_runtime_daemon_state.v1",
        "service_id": service_id,
        "status": "running",
        "project_root": project_root.display().to_string(),
        "execute": execute,
        "requested_status": requested_status,
        "limit": limit,
        "service_limit": service_limit,
        "max_cycles": max_cycles,
        "interval_seconds": interval_seconds,
        "idle_exit": idle_exit,
        "dispatch_activations": dispatch_activations,
        "continuous": continuous,
        "cycle_retention": cycle_retention,
        "recover_stale_services": recover_stale_services,
        "scan_schedules": scan_schedules,
        "schedule_executor": schedule_executor,
        "schedule_max_workers": schedule_max_workers,
        "schedule_ttl_seconds": schedule_ttl_seconds,
        "stop_file": stop_file_display,
        "started_at": started_at.to_rfc3339(),
        "health": {
            "schema_version": "forge.event_runtime_daemon_health.v1",
            "status": "running",
            "cycle_count": 0,
            "retained_cycle_count": 0,
            "dropped_cycle_count": 0,
            "recommendation_count": 0,
            "execution_count": 0,
            "schedule_execution_count": 0,
            "schedule_scale_to_zero_count": 0,
            "failed_count": 0
        }
    });
    let acquired = store.try_save_event_service(EventServiceWrite {
        id: &service_id,
        service_kind: "runtime_reconcile",
        status: "running",
        tenant_context: &tenant_context,
        lease_owner: &lease_owner,
        lease_id: &lease_id,
        lease_acquired_at: &started_at.to_rfc3339(),
        lease_expires_at: &lease_expires_at,
        last_heartbeat_at: &started_at.to_rfc3339(),
        heartbeat_ttl_seconds: heartbeat_seconds,
        data: &running_data,
    })?;
    if !acquired {
        bail!("event runtime daemon lease is already held: {service_id}");
    }

    let mut cycles = Vec::new();
    let mut recommendation_count = 0usize;
    let mut execution_count = 0usize;
    let mut schedule_execution_count = 0usize;
    let mut schedule_scale_to_zero_count = 0usize;
    let mut failed_count = 0usize;
    let mut dropped_cycle_count = 0usize;
    let mut stop_requested = false;
    let mut stopped_reason = if continuous {
        "continuous_interrupted".to_string()
    } else {
        "max_cycles_reached".to_string()
    };

    let mut cycle = 0usize;
    loop {
        if !continuous && cycle >= max_cycles {
            break;
        }
        if event_stop_file_requested(stop_file) {
            stop_requested = true;
            stopped_reason = "stop_file_requested".to_string();
            break;
        }
        cycle += 1;

        let report = run_event_runtime_reconcile(
            store,
            &project_root,
            Some(&requested_status),
            limit,
            service_limit,
            execute,
            1,
            0,
            true,
            dispatch_activations,
            recover_stale_services,
            stop_file,
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
        recommendation_count += report.recommendation_count;
        execution_count += report.execution_count;
        schedule_execution_count += report.schedule_execution_count;
        schedule_scale_to_zero_count += report.schedule_scale_to_zero_count;
        if report.status == "event_runtime_reconcile_executed_with_failures" {
            failed_count += 1;
        }

        let should_continue_after_cycle = continuous || cycle < max_cycles;
        let slept_after_seconds = if should_continue_after_cycle && interval_seconds > 0 {
            interval_seconds
        } else {
            0
        };
        cycles.push(EventRuntimeDaemonCycle {
            cycle,
            slept_after_seconds,
            report,
        });
        while cycles.len() > cycle_retention {
            cycles.remove(0);
            dropped_cycle_count += 1;
        }

        let heartbeat_at = Utc::now();
        let heartbeat_expires_at =
            (heartbeat_at + ChronoDuration::seconds(lease_seconds as i64)).to_rfc3339();
        let running_health = json!({
            "schema_version": "forge.event_runtime_daemon_health.v1",
            "status": "running",
            "cycle_count": cycle,
            "retained_cycle_count": cycles.len(),
            "dropped_cycle_count": dropped_cycle_count,
            "recommendation_count": recommendation_count,
            "execution_count": execution_count,
            "schedule_execution_count": schedule_execution_count,
            "schedule_scale_to_zero_count": schedule_scale_to_zero_count,
            "failed_count": failed_count,
            "last_cycle": cycle,
        });
        let running_data = json!({
            "schema_version": "forge.event_runtime_daemon_state.v1",
            "service_id": service_id,
            "status": "running",
            "project_root": project_root.display().to_string(),
            "execute": execute,
            "requested_status": requested_status,
            "limit": limit,
            "service_limit": service_limit,
            "max_cycles": max_cycles,
            "interval_seconds": interval_seconds,
            "idle_exit": idle_exit,
            "continuous": continuous,
            "cycle_retention": cycle_retention,
            "recover_stale_services": recover_stale_services,
            "scan_schedules": scan_schedules,
            "schedule_executor": schedule_executor,
            "schedule_max_workers": schedule_max_workers,
            "schedule_ttl_seconds": schedule_ttl_seconds,
            "stop_file": stop_file_display,
            "started_at": started_at.to_rfc3339(),
            "health": running_health,
        });
        store.save_event_service(EventServiceWrite {
            id: &service_id,
            service_kind: "runtime_reconcile",
            status: "running",
            tenant_context: &tenant_context,
            lease_owner: &lease_owner,
            lease_id: &lease_id,
            lease_acquired_at: &started_at.to_rfc3339(),
            lease_expires_at: &heartbeat_expires_at,
            last_heartbeat_at: &heartbeat_at.to_rfc3339(),
            heartbeat_ttl_seconds: heartbeat_seconds,
            data: &running_data,
        })?;

        if event_stop_file_requested(stop_file) {
            stop_requested = true;
            stopped_reason = "stop_file_requested".to_string();
            break;
        }
        if idle_exit && cycles.last().is_some_and(runtime_daemon_cycle_is_idle) {
            stopped_reason = "idle_exit".to_string();
            break;
        }
        if slept_after_seconds > 0 {
            sleep(Duration::from_secs(slept_after_seconds));
        }
    }

    let completed_at = Utc::now();
    let service_status = if stop_requested {
        "stopped"
    } else if failed_count > 0 {
        "completed_with_failures"
    } else {
        "completed"
    };
    let report_status = if stop_requested {
        "event_runtime_daemon_stopped"
    } else if failed_count > 0 {
        "event_runtime_daemon_completed_with_failures"
    } else {
        "event_runtime_daemon_completed"
    };
    let health = json!({
        "schema_version": "forge.event_runtime_daemon_health.v1",
        "status": report_status,
        "started_at": started_at.to_rfc3339(),
        "completed_at": completed_at.to_rfc3339(),
        "cycle_count": cycle,
        "retained_cycle_count": cycles.len(),
        "dropped_cycle_count": dropped_cycle_count,
        "recommendation_count": recommendation_count,
        "execution_count": execution_count,
        "schedule_execution_count": schedule_execution_count,
        "schedule_scale_to_zero_count": schedule_scale_to_zero_count,
        "failed_count": failed_count,
        "stopped_reason": stopped_reason,
        "stop_requested": stop_requested,
        "stop_file": stop_file_display,
    });
    let final_data = json!({
        "schema_version": "forge.event_runtime_daemon_state.v1",
        "service_id": service_id,
        "status": report_status,
        "project_root": project_root.display().to_string(),
        "execute": execute,
        "requested_status": requested_status,
        "limit": limit,
        "service_limit": service_limit,
        "max_cycles": max_cycles,
        "interval_seconds": interval_seconds,
        "idle_exit": idle_exit,
        "dispatch_activations": dispatch_activations,
        "continuous": continuous,
        "cycle_retention": cycle_retention,
        "recover_stale_services": recover_stale_services,
        "scan_schedules": scan_schedules,
        "schedule_executor": schedule_executor,
        "schedule_max_workers": schedule_max_workers,
        "schedule_ttl_seconds": schedule_ttl_seconds,
        "stop_file": stop_file_display,
        "started_at": started_at.to_rfc3339(),
        "completed_at": completed_at.to_rfc3339(),
        "health": health,
    });
    store.save_event_service(EventServiceWrite {
        id: &service_id,
        service_kind: "runtime_reconcile",
        status: service_status,
        tenant_context: &tenant_context,
        lease_owner: &lease_owner,
        lease_id: &lease_id,
        lease_acquired_at: &started_at.to_rfc3339(),
        lease_expires_at: &completed_at.to_rfc3339(),
        last_heartbeat_at: &completed_at.to_rfc3339(),
        heartbeat_ttl_seconds: heartbeat_seconds,
        data: &final_data,
    })?;
    let service = store
        .load_event_service(&service_id)?
        .map(EventServiceView::from)
        .with_context(|| {
            format!("event runtime daemon service missing after save: {service_id}")
        })?;
    let mut report = EventRuntimeDaemonReport {
        schema_version: EVENT_RUNTIME_DAEMON_SCHEMA_VERSION.to_string(),
        status: report_status.to_string(),
        service,
        project_root: project_root.display().to_string(),
        execute,
        requested_status,
        limit,
        service_limit,
        max_cycles,
        interval_seconds,
        idle_exit,
        dispatch_activations,
        continuous,
        cycle_retention,
        recover_stale_services,
        scan_schedules,
        schedule_executor,
        schedule_max_workers,
        schedule_ttl_seconds,
        cycle_count: cycle,
        retained_cycle_count: cycles.len(),
        dropped_cycle_count,
        recommendation_count,
        execution_count,
        schedule_execution_count,
        schedule_scale_to_zero_count,
        failed_count,
        stopped_reason,
        stop_requested,
        stop_file: stop_file_display,
        health,
        cycles,
        global_event_id: None,
    };
    let event_data = serde_json::to_value(&report)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "event_runtime_daemon",
        source_id: &service_id,
        workflow_id: None,
        kind: report_status,
        origin: "forge",
        status: report_status,
        data: &event_data,
        tenant_context: &tenant_context,
    })?;
    report.global_event_id = Some(global_event_id);
    Ok(report)
}

fn runtime_daemon_cycle_is_idle(cycle: &EventRuntimeDaemonCycle) -> bool {
    cycle.report.recommendation_count == 0
        && cycle.report.execution_count == 0
        && cycle.report.schedule_execution_count == 0
        && cycle.report.schedule_scale_to_zero_count == 0
}

#[allow(clippy::too_many_arguments)]
pub fn run_event_webhook_ingress_server(
    store: &ForgeStore,
    host: &str,
    port: u16,
    path: &str,
    default_origin: &str,
    default_action: &str,
    transport: &str,
    schema: Option<&str>,
    project_root: &Path,
    route_after_ingest: bool,
    max_requests: usize,
    max_body_bytes: usize,
    hmac_secret_env: Option<&str>,
    signature_header: &str,
    stop_file: Option<&Path>,
) -> Result<EventWebhookIngressReport> {
    run_event_webhook_ingress_server_with_progress(
        store,
        host,
        port,
        path,
        default_origin,
        default_action,
        transport,
        schema,
        project_root,
        route_after_ingest,
        max_requests,
        max_body_bytes,
        hmac_secret_env,
        signature_header,
        60,
        stop_file,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_event_webhook_ingress_server_with_progress(
    store: &ForgeStore,
    host: &str,
    port: u16,
    path: &str,
    default_origin: &str,
    default_action: &str,
    transport: &str,
    schema: Option<&str>,
    project_root: &Path,
    route_after_ingest: bool,
    max_requests: usize,
    max_body_bytes: usize,
    hmac_secret_env: Option<&str>,
    signature_header: &str,
    progress_interval_seconds: u64,
    stop_file: Option<&Path>,
    mut progress_callback: WebhookIngressProgressCallback<'_>,
) -> Result<EventWebhookIngressReport> {
    let host = required_text("host", host)?;
    let path = normalize_webhook_path(path)?;
    let default_origin = required_text("origin", default_origin)?;
    let default_action = required_text("action", default_action)?;
    let transport = normalize_text(Some(transport)).unwrap_or_else(|| "webhook".to_string());
    let schema = normalize_text(schema);
    let hmac_verifier = build_webhook_hmac_verifier(hmac_secret_env, signature_header)?;
    let bind_addresses = resolve_webhook_bind_addresses(&host, port)?;
    let local_only = bind_addresses
        .iter()
        .all(|address| address.ip().is_loopback());
    let allow_unsigned_mutations = local_only
        && event_env_flag(WEBHOOK_ALLOW_INSECURE_LOCAL_ENV)
        && !event_env_flag("FORGE_PRODUCTION_MODE");
    validate_webhook_ingress_security(
        local_only,
        hmac_verifier.is_some(),
        &default_action,
        allow_unsigned_mutations,
    )?;
    let rate_limit_per_minute = configured_webhook_rate_limit();
    let auth = EventWebhookIngressAuthReport {
        required: hmac_verifier.is_some(),
        scheme: hmac_verifier
            .as_ref()
            .map(|_| "hmac_sha256".to_string())
            .unwrap_or_else(|| "none".to_string()),
        signature_header: hmac_verifier
            .as_ref()
            .map(|verifier| verifier.signature_header.clone()),
        secret_env: hmac_verifier
            .as_ref()
            .map(|verifier| verifier.secret_env.clone()),
        timestamp_header: hmac_verifier
            .as_ref()
            .map(|_| WEBHOOK_TIMESTAMP_HEADER.to_string()),
        nonce_header: hmac_verifier
            .as_ref()
            .map(|_| WEBHOOK_NONCE_HEADER.to_string()),
        max_clock_skew_seconds: hmac_verifier
            .as_ref()
            .map(|_| WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS),
        replay_protection: hmac_verifier.is_some(),
        rate_limit_per_minute,
    };
    let max_requests = max_requests.max(1);
    let max_body_bytes = max_body_bytes.clamp(256, 1_048_576);
    let listener = TcpListener::bind(bind_addresses.as_slice())
        .with_context(|| format!("failed to bind webhook ingress on {host}:{port}"))?;
    let bind_address = listener.local_addr()?.to_string();
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let mut events = Vec::new();
    let mut security_state =
        WebhookIngressSecurityState::new(allow_unsigned_mutations, rate_limit_per_minute);
    let stop_file_display = stop_file.map(|path| path.display().to_string());
    let mut stop_requested = false;
    let mut stopped_reason = "max_requests_reached".to_string();

    if progress_callback.is_some() || stop_file.is_some() {
        listener
            .set_nonblocking(true)
            .context("failed to switch webhook ingress listener to nonblocking mode")?;
        emit_webhook_ingress_progress(&mut progress_callback, &events, "listening")?;
        let progress_interval = Duration::from_secs(progress_interval_seconds.max(1));
        let mut last_progress_at = Instant::now();
        while events.len() < max_requests {
            if event_stop_file_requested(stop_file) {
                stop_requested = true;
                stopped_reason = "stop_file_requested".to_string();
                emit_webhook_ingress_progress(
                    &mut progress_callback,
                    &events,
                    "stop_file_requested",
                )?;
                break;
            }
            match listener.accept() {
                Ok((mut stream, peer_address)) => {
                    let entry = build_webhook_ingress_entry_from_stream(
                        store,
                        &mut stream,
                        peer_address,
                        &path,
                        &default_origin,
                        &default_action,
                        &transport,
                        schema.as_deref(),
                        &project_root,
                        route_after_ingest,
                        max_body_bytes,
                        hmac_verifier.as_ref(),
                        &mut security_state,
                    );
                    write_webhook_ingress_response(&mut stream, &entry)?;
                    events.push(entry);
                    emit_webhook_ingress_progress(
                        &mut progress_callback,
                        &events,
                        "request_completed",
                    )?;
                    last_progress_at = Instant::now();
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if last_progress_at.elapsed() >= progress_interval {
                        emit_webhook_ingress_progress(
                            &mut progress_callback,
                            &events,
                            "waiting_for_request",
                        )?;
                        last_progress_at = Instant::now();
                    }
                    sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(error.into()),
            }
        }
    } else {
        for _ in 0..max_requests {
            let (mut stream, peer_address) = listener.accept()?;
            let entry = build_webhook_ingress_entry_from_stream(
                store,
                &mut stream,
                peer_address,
                &path,
                &default_origin,
                &default_action,
                &transport,
                schema.as_deref(),
                &project_root,
                route_after_ingest,
                max_body_bytes,
                hmac_verifier.as_ref(),
                &mut security_state,
            );
            write_webhook_ingress_response(&mut stream, &entry)?;
            events.push(entry);
        }
    }

    let ingested_count = events
        .iter()
        .filter(|event| event.event_id.is_some())
        .count();
    let routed_count = events.iter().filter(|event| event.route.is_some()).count();
    let failed_count = events.iter().filter(|event| event.error.is_some()).count();
    let status = if stop_requested {
        "event_webhook_ingress_stopped"
    } else if failed_count > 0 {
        "event_webhook_ingress_completed_with_failures"
    } else {
        "event_webhook_ingress_completed"
    };
    Ok(EventWebhookIngressReport {
        schema_version: EVENT_WEBHOOK_INGRESS_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        bind_address,
        path,
        default_origin,
        default_action,
        transport,
        schema,
        route_after_ingest,
        auth,
        max_requests,
        max_body_bytes,
        request_count: events.len(),
        ingested_count,
        routed_count,
        failed_count,
        stopped_reason,
        stop_requested,
        stop_file: stop_file_display,
        events,
    })
}

fn emit_webhook_ingress_progress(
    progress_callback: &mut WebhookIngressProgressCallback<'_>,
    events: &[EventWebhookIngressEntry],
    phase: &str,
) -> Result<()> {
    if let Some(callback) = progress_callback.as_mut() {
        callback(events, phase)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_webhook_ingress_entry_from_stream(
    store: &ForgeStore,
    stream: &mut TcpStream,
    peer_address: SocketAddr,
    path: &str,
    default_origin: &str,
    default_action: &str,
    transport: &str,
    schema: Option<&str>,
    project_root: &Path,
    route_after_ingest: bool,
    max_body_bytes: usize,
    hmac_verifier: Option<&WebhookHmacVerifier>,
    security_state: &mut WebhookIngressSecurityState,
) -> EventWebhookIngressEntry {
    match handle_webhook_ingress_stream(
        store,
        stream,
        peer_address,
        path,
        default_origin,
        default_action,
        transport,
        schema,
        project_root,
        route_after_ingest,
        max_body_bytes,
        hmac_verifier,
        security_state,
    ) {
        Ok(entry) => entry,
        Err(error) => {
            let error_message = error.to_string();
            let http_status = webhook_ingress_error_status(&error_message);
            EventWebhookIngressEntry {
                request_id: format!("webhook_{}", Uuid::new_v4().to_string().replace('-', "")),
                method: "unknown".to_string(),
                path: path.to_string(),
                http_status,
                status: "webhook_ingress_failed".to_string(),
                origin: default_origin.to_string(),
                action: default_action.to_string(),
                auth_verified: hmac_verifier.map(|_| false),
                event_id: None,
                event: None,
                route: None,
                error: Some(error_message),
            }
        }
    }
}

fn webhook_ingress_error_status(message: &str) -> u16 {
    if message.contains("rate limit exceeded") {
        429
    } else if message.contains("replay detected") {
        409
    } else if message.contains("HMAC")
        || message.contains("signature")
        || message.contains("timestamp")
        || message.contains("nonce")
        || message.contains("requires authentication")
    {
        401
    } else {
        400
    }
}

pub fn route_inbound_event(
    store: &ForgeStore,
    event_id: &str,
    project_root: &Path,
) -> Result<InboundEventRouteReport> {
    let event = store.load_inbound_event(event_id)?;
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let addon_dirs = vec![project_root.join(".forge/addons")];
    let addon_catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
    let adapter_policy = evaluate_inbound_event_adapter_policy(&addon_catalog, &event);
    let addon_event_adapter_plan =
        build_inbound_addon_event_adapter_plan(&addon_catalog, &event, &adapter_policy);
    if !adapter_policy.allowed {
        bail!(
            "inbound event blocked by adapter policy: {}",
            adapter_policy.status
        );
    }
    if event.status == "routed" {
        return Ok(InboundEventRouteReport {
            schema_version: EVENT_ROUTE_SCHEMA_VERSION.to_string(),
            status: "already_routed".to_string(),
            event_id: event.id.clone(),
            action: event.action.clone(),
            origin: event.origin.clone(),
            adapter_policy,
            addon_event_adapter_plan,
            route_decision: "event was already routed".to_string(),
            workflow_id: event
                .data
                .get("workflow_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            workflow_goal: event
                .data
                .get("workflow_goal")
                .and_then(Value::as_str)
                .map(str::to_string),
            created_workflow: None,
            route_result: None,
            event: inbound_event_view(store, event),
        });
    }

    match normalized_action(&event.action).as_str() {
        "start_workflow" => route_start_workflow(
            store,
            event,
            &project_root,
            &addon_catalog,
            adapter_policy,
            addon_event_adapter_plan,
        ),
        "continue_workflow" => {
            route_continue_workflow(store, event, adapter_policy, addon_event_adapter_plan)
        }
        "modify_workflow" => {
            route_modify_workflow(store, event, adapter_policy, addon_event_adapter_plan)
        }
        "pause_workflow" => route_status_workflow(
            store,
            event,
            "pause_workflow",
            adapter_policy,
            addon_event_adapter_plan,
        ),
        "resume_workflow" => route_status_workflow(
            store,
            event,
            "resume_workflow",
            adapter_policy,
            addon_event_adapter_plan,
        ),
        "complete_workflow" => route_status_workflow(
            store,
            event,
            "complete_workflow",
            adapter_policy,
            addon_event_adapter_plan,
        ),
        other => bail!("unsupported inbound event action for routing: {other}"),
    }
}

pub fn dispatch_inbound_event_activations(
    store: &ForgeStore,
    event_id: &str,
    project_root: &Path,
    dry_run: bool,
) -> Result<InboundEventActivationDispatchReport> {
    let route = route_inbound_event(store, event_id, project_root)?;
    dispatch_inbound_event_activations_for_route(store, &route, project_root, dry_run)
}

fn dispatch_inbound_event_activations_for_route(
    store: &ForgeStore,
    route: &InboundEventRouteReport,
    project_root: &Path,
    dry_run: bool,
) -> Result<InboundEventActivationDispatchReport> {
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let addon_dirs = vec![project_root.join(".forge/addons")];
    let addon_catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
    let activation_plan = &route
        .addon_event_adapter_plan
        .event_workflow_activation_plan;
    let mut dispatch_reports = Vec::new();
    let mut skipped_count = 0;

    for activation in &activation_plan.activations {
        if !activation.dispatch_allowed {
            skipped_count += 1;
            continue;
        }
        let mut dispatched_for_activation = false;
        for contract in activation
            .runtime_contracts
            .iter()
            .filter(|contract| contract.dispatch_allowed)
        {
            let input = event_activation_dispatch_input(route, activation, contract);
            let source = format!(
                "event_inbox:{}:{}:{}",
                route.event_id, activation.source_kind, activation.source_id
            );
            dispatch_reports.push(enqueue_addon_runtime_contract_dispatch(
                store,
                &addon_catalog,
                Some(&activation.addon_id),
                &contract.contract_id,
                input,
                &source,
                dry_run,
            )?);
            dispatched_for_activation = true;
        }
        if !dispatched_for_activation {
            skipped_count += 1;
        }
    }

    let dispatch_attempt_count = dispatch_reports.len();
    let queued_count = dispatch_reports
        .iter()
        .map(|report| report.queued_count)
        .sum::<usize>();
    let dry_run_count = dispatch_reports
        .iter()
        .flat_map(|report| report.dispatches.iter())
        .filter(|dispatch| dispatch.status == "dry_run")
        .count();
    let blocked_count = dispatch_reports
        .iter()
        .map(|report| report.blocked_count)
        .sum::<usize>();
    let status = event_activation_dispatch_status(
        activation_plan.activation_count,
        dispatch_attempt_count,
        queued_count,
        dry_run_count,
        blocked_count,
        skipped_count,
    );
    let notes = event_activation_dispatch_notes(status, dry_run, skipped_count);

    Ok(InboundEventActivationDispatchReport {
        schema_version: EVENT_ACTIVATION_DISPATCH_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        event_id: route.event_id.clone(),
        dry_run,
        activation_count: activation_plan.activation_count,
        dispatch_attempt_count,
        queued_count,
        dry_run_count,
        blocked_count,
        skipped_count,
        route: route.clone(),
        dispatch_reports,
        notes,
    })
}

fn event_activation_dispatch_input(
    route: &InboundEventRouteReport,
    activation: &InboundEventWorkflowActivation,
    contract: &AddonRuntimeContractPolicyEntry,
) -> Value {
    json!({
        "schema_version": "forge.event_workflow_activation.v1",
        "event_id": route.event_id,
        "origin": route.origin,
        "action": route.action,
        "normalized_action": route.adapter_policy.normalized_action,
        "route_decision": route.route_decision,
        "workflow_id": route.workflow_id,
        "workflow_goal": route.workflow_goal,
        "source_kind": activation.source_kind,
        "source_id": activation.source_id,
        "addon_id": activation.addon_id,
        "capability_id": activation.capability_id,
        "workflow_extension_id": activation.workflow_extension_id,
        "event_type": activation.event_type,
        "channel": activation.channel,
        "adapter_id": activation.adapter_id,
        "contract_id": contract.contract_id,
        "contract_type": contract.contract_type,
        "runtime": contract.runtime,
        "entrypoint": contract.entrypoint,
    })
}

fn event_activation_dispatch_status(
    activation_count: usize,
    dispatch_attempt_count: usize,
    queued_count: usize,
    dry_run_count: usize,
    blocked_count: usize,
    skipped_count: usize,
) -> &'static str {
    if activation_count == 0 {
        "event_activation_dispatch_unmatched"
    } else if dispatch_attempt_count == 0 {
        "event_activation_dispatch_blocked"
    } else if blocked_count > 0 || skipped_count > 0 {
        "event_activation_dispatch_partially_queued"
    } else if dry_run_count > 0 && queued_count == 0 {
        "event_activation_dispatch_dry_run"
    } else {
        "event_activation_dispatch_queued"
    }
}

fn event_activation_dispatch_notes(
    status: &str,
    dry_run: bool,
    skipped_count: usize,
) -> Vec<String> {
    let mut notes = Vec::new();
    match status {
        "event_activation_dispatch_unmatched" => {
            notes.push("No event workflow activations were available for dispatch.".to_string());
        }
        "event_activation_dispatch_blocked" => {
            notes.push(
                "Event workflow activations were present but none were dispatch-ready.".to_string(),
            );
        }
        "event_activation_dispatch_partially_queued" => {
            notes.push(format!(
                "{skipped_count} event workflow activation(s) were skipped because they were not dispatch-ready."
            ));
        }
        "event_activation_dispatch_dry_run" => {
            notes.push(
                "Event workflow activation dispatches were planned as dry-run ledger entries only."
                    .to_string(),
            );
        }
        _ => {
            notes.push(
                "Event workflow activations were queued in the Addon runtime dispatch ledger."
                    .to_string(),
            );
        }
    }
    if dry_run {
        notes.push("Dry-run mode did not persist dispatches to the runtime ledger.".to_string());
    }
    notes
}

pub fn emit_event_egress(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    input: EventEgressEmitInput,
    operating_context: &OperatingContextSpec,
) -> Result<EventEgressEmitReport> {
    let input = normalize_event_egress_input(input)?;
    let adapter_policy = evaluate_event_egress_adapter_policy(catalog, &input);
    if !adapter_policy.allowed {
        bail!(
            "event egress blocked by adapter policy: {}",
            adapter_policy.status
        );
    }
    let matched_adapter = adapter_policy
        .matched_adapter
        .clone()
        .with_context(|| "event egress policy allowed without a matched adapter")?;
    let request = build_event_egress_request(&matched_adapter, &input);
    if input.dry_run {
        let status = "event_egress_dry_run".to_string();
        let global_event_id = record_event_egress_global_event(
            store,
            &request,
            &adapter_policy,
            None,
            &status,
            true,
            operating_context,
        )?;
        return Ok(EventEgressEmitReport {
            schema_version: EVENT_EGRESS_EMIT_SCHEMA_VERSION.to_string(),
            status,
            dry_run: true,
            global_event_id: Some(global_event_id),
            adapter_policy,
            request,
            delivery: None,
            workflow_artifact: None,
        });
    }
    ensure_event_egress_tenant_policy(store, &request, operating_context)?;
    let delivery = deliver_event_egress(&matched_adapter, &request)?;
    let status = if delivery.success {
        "event_egress_delivered"
    } else {
        "event_egress_delivery_failed"
    };
    let workflow_artifact = if delivery.success {
        attach_event_egress_delivery_artifact(
            store,
            &request,
            &adapter_policy,
            &delivery,
            status,
            operating_context,
        )?
    } else {
        None
    };
    Ok(EventEgressEmitReport {
        schema_version: EVENT_EGRESS_EMIT_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        dry_run: false,
        global_event_id: Some(record_event_egress_global_event(
            store,
            &request,
            &adapter_policy,
            Some(&delivery),
            status,
            false,
            operating_context,
        )?),
        adapter_policy,
        request,
        delivery: Some(delivery),
        workflow_artifact,
    })
}

fn ensure_event_egress_tenant_policy(
    store: &ForgeStore,
    request: &EventEgressRequestEnvelope,
    operating_context: &OperatingContextSpec,
) -> Result<()> {
    if let Some(workflow_id) = extract_string(&request.payload, &["workflow_id"]) {
        return ensure_workflow_policy(store, &workflow_id, "event egress delivery");
    }
    ensure_operating_context_policy(store, operating_context, "event egress delivery")
}

fn normalize_event_egress_input(mut input: EventEgressEmitInput) -> Result<EventEgressEmitInput> {
    input.adapter_id = required_text("adapter_id", &input.adapter_id)?;
    input.event_type = required_text("event_type", &input.event_type)?;
    input.action = required_text("action", &input.action)?;
    input.origin = normalize_text(Some(&input.origin)).unwrap_or_else(|| "forge".to_string());
    input.addon_id = input
        .addon_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok(input)
}

fn evaluate_event_egress_adapter_policy(
    catalog: &AddonCatalog,
    input: &EventEgressEmitInput,
) -> EventEgressAdapterPolicyReport {
    let normalized_action = normalized_action(&input.action);
    let adapter_report = list_addon_event_adapters(catalog, input.addon_id.as_deref(), None, None);
    let matching_adapters = adapter_report
        .adapters
        .into_iter()
        .filter(|adapter| text_matches(&adapter.adapter.id, &input.adapter_id))
        .collect::<Vec<_>>();

    if matching_adapters.is_empty() {
        return blocked_egress_adapter_policy(
            input,
            normalized_action,
            None,
            "no_declared_adapter",
            "no Addon event adapter with this id is declared",
        );
    }
    if matching_adapters.len() > 1 && input.addon_id.is_none() {
        return blocked_egress_adapter_policy(
            input,
            normalized_action,
            matching_adapters.into_iter().next(),
            "ambiguous_adapter",
            "more than one Addon declares this adapter id; pass addon_id",
        );
    }

    let matched_adapter = matching_adapters.into_iter().next();
    let Some(adapter) = matched_adapter.clone() else {
        return blocked_egress_adapter_policy(
            input,
            normalized_action,
            None,
            "no_declared_adapter",
            "no Addon event adapter with this id is declared",
        );
    };

    let mut allowed = true;
    let mut status = "matched".to_string();
    let mut issues = Vec::new();
    let direction = adapter.adapter.direction.trim();
    if !text_matches(direction, "egress") && !text_matches(direction, "bidirectional") {
        status = "adapter_direction_not_egress".to_string();
        allowed = false;
        issues.push("adapter direction must be egress or bidirectional".to_string());
    }

    let transport = adapter.adapter.transport.trim();
    let is_telegram_transport = transport_is_telegram(transport);
    if !text_matches(transport, "webhook")
        && !text_matches(transport, "http")
        && !is_telegram_transport
    {
        status = "adapter_transport_not_supported".to_string();
        allowed = false;
        issues.push(format!("unsupported egress transport `{transport}`"));
    }

    if !adapter.permission_gate.allowed {
        status = adapter.permission_gate.status.clone();
        allowed = false;
        issues.push(format!(
            "permission gate denied adapter: {}",
            adapter.permission_gate.status
        ));
    }

    if adapter.adapter.actions.is_empty() {
        status = "adapter_action_not_declared".to_string();
        allowed = false;
        issues.push("egress adapter must explicitly declare allowed actions".to_string());
    } else if !adapter_action_matches(&adapter, &input.action, &normalized_action) {
        status = "adapter_action_not_allowed".to_string();
        allowed = false;
        issues.push("egress action is not declared by the adapter".to_string());
    }

    if adapter.adapter.event_types.is_empty() {
        status = "adapter_event_type_not_declared".to_string();
        allowed = false;
        issues.push("egress adapter must explicitly declare event_types".to_string());
    } else if !adapter
        .adapter
        .event_types
        .iter()
        .any(|event_type| text_matches(event_type, &input.event_type))
    {
        status = "adapter_event_type_not_allowed".to_string();
        allowed = false;
        issues.push("egress event_type is not declared by the adapter".to_string());
    }

    if !string_list_matches(&adapter.adapter.origins, &input.origin) {
        status = "adapter_origin_not_allowed".to_string();
        allowed = false;
        issues.push("egress origin is not allowed by the adapter".to_string());
    }

    let auth_supported_for_egress = auth_is_hmac(&adapter.adapter.auth)
        || auth_is_bearer(&adapter.adapter.auth)
        || (is_telegram_transport && auth_is_bot_token(&adapter.adapter.auth));
    if auth_requires_verification(&adapter.adapter.auth) && !auth_supported_for_egress {
        status = "adapter_auth_not_supported_for_egress".to_string();
        allowed = false;
        issues.push(format!(
            "egress adapter auth `{}` requires a transport-specific signer/secret provider",
            adapter.adapter.auth
        ));
    } else if auth_supported_for_egress
        && !input.dry_run
        && !event_adapter_has_secret_provider(&adapter.adapter)
    {
        status = "adapter_auth_secret_required".to_string();
        allowed = false;
        issues.push(
            "authenticated egress adapter requires secret_env, hmac_secret_env or credential_vault"
                .to_string(),
        );
    }

    match adapter.adapter.endpoint.as_deref().map(str::trim) {
        _ if is_telegram_transport => {}
        Some(endpoint) if !endpoint.is_empty() => match parse_http_event_endpoint(endpoint) {
            Ok(parsed) => {
                if !event_egress_allowed_host(&parsed.host, &adapter.adapter.allowed_hosts) {
                    status = "adapter_endpoint_host_not_allowed".to_string();
                    allowed = false;
                    issues.push(format!(
                        "endpoint host `{}` is not local and is not in allowed_hosts",
                        parsed.host
                    ));
                }
            }
            Err(error) => {
                status = "adapter_endpoint_invalid".to_string();
                allowed = false;
                issues.push(error.to_string());
            }
        },
        _ if !input.dry_run => {
            status = "adapter_endpoint_required".to_string();
            allowed = false;
            issues.push("non-dry-run egress requires adapter.endpoint".to_string());
        }
        _ => {}
    }

    EventEgressAdapterPolicyReport {
        schema_version: EVENT_ADAPTER_POLICY_SCHEMA_VERSION.to_string(),
        status,
        allowed,
        enforced: true,
        adapter_id: input.adapter_id.clone(),
        addon_id: input.addon_id.clone(),
        origin: input.origin.clone(),
        action: input.action.clone(),
        normalized_action,
        event_type: input.event_type.clone(),
        transport: normalize_text(Some(&adapter.adapter.transport)),
        issues,
        matched_adapter: Some(adapter),
    }
}

fn blocked_egress_adapter_policy(
    input: &EventEgressEmitInput,
    normalized_action: String,
    matched_adapter: Option<AddonEventAdapterView>,
    status: &str,
    issue: &str,
) -> EventEgressAdapterPolicyReport {
    EventEgressAdapterPolicyReport {
        schema_version: EVENT_ADAPTER_POLICY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        allowed: false,
        enforced: true,
        adapter_id: input.adapter_id.clone(),
        addon_id: input.addon_id.clone(),
        origin: input.origin.clone(),
        action: input.action.clone(),
        normalized_action,
        event_type: input.event_type.clone(),
        transport: matched_adapter
            .as_ref()
            .and_then(|adapter| normalize_text(Some(&adapter.adapter.transport))),
        issues: vec![issue.to_string()],
        matched_adapter,
    }
}

fn build_event_egress_request(
    adapter: &AddonEventAdapterView,
    input: &EventEgressEmitInput,
) -> EventEgressRequestEnvelope {
    EventEgressRequestEnvelope {
        schema_version: EVENT_EGRESS_REQUEST_SCHEMA_VERSION.to_string(),
        request_id: format!("egress_{}", Uuid::new_v4()),
        addon_id: adapter.addon_id.clone(),
        adapter_id: adapter.adapter.id.clone(),
        transport: adapter.adapter.transport.clone(),
        direction: adapter.adapter.direction.clone(),
        auth: adapter.adapter.auth.clone(),
        secret_env: adapter.adapter.secret_env.clone(),
        credential_vault: adapter.adapter.credential_vault.clone(),
        signature_header: adapter.adapter.signature_header.clone(),
        event_type: input.event_type.clone(),
        action: input.action.clone(),
        origin: input.origin.clone(),
        schema: adapter.adapter.schema.clone(),
        issued_at: Utc::now().to_rfc3339(),
        payload: input.payload.clone(),
    }
}

fn deliver_event_egress(
    adapter: &AddonEventAdapterView,
    request: &EventEgressRequestEnvelope,
) -> Result<EventEgressDeliveryReport> {
    if transport_is_telegram(&adapter.adapter.transport) {
        return deliver_telegram_event_egress(adapter, request);
    }
    let endpoint = adapter
        .adapter
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .with_context(|| "event egress adapter endpoint is required")?;
    let parsed = parse_http_event_endpoint(endpoint)?;
    let timeout = Duration::from_secs(adapter.adapter.timeout_seconds.unwrap_or(10).clamp(1, 120));
    let max_response_bytes = adapter
        .adapter
        .max_response_bytes
        .unwrap_or(65_536)
        .clamp(256, 1_048_576);
    let body = serde_json::to_vec(request)?;
    let signature = build_event_egress_auth_headers(&adapter.adapter, &body)?;
    let response = if parsed.scheme.eq_ignore_ascii_case("https") {
        let resolved_address = if event_env_value_is("FORGE_EVENT_EGRESS_HTTPS_MODE", "simulate") {
            None
        } else {
            Some(resolve_event_egress_address(&parsed)?)
        };
        post_event_egress_https_curl(
            endpoint,
            &parsed,
            resolved_address,
            &body,
            timeout,
            max_response_bytes,
            &signature.headers,
        )?
    } else {
        let resolved_address = resolve_event_egress_address(&parsed)?;
        post_event_egress_json(
            &parsed,
            resolved_address,
            &body,
            timeout,
            max_response_bytes,
            &signature.headers,
        )?
    };
    let success = (200..300).contains(&response.status_code);
    Ok(EventEgressDeliveryReport {
        transport: adapter.adapter.transport.clone(),
        endpoint: endpoint.to_string(),
        auth_scheme: signature.auth_scheme,
        signed: signature.signed,
        signature_header: signature.signature_header,
        secret_env: signature.secret_env,
        secret_source: signature.secret_source,
        credential_vault: signature.credential_vault,
        success,
        status_code: response.status_code,
        response_bytes: response.body.len(),
        response_sha256: hex_sha256(&response.body),
        response_truncated: response.truncated,
    })
}

fn deliver_telegram_event_egress(
    adapter: &AddonEventAdapterView,
    request: &EventEgressRequestEnvelope,
) -> Result<EventEgressDeliveryReport> {
    if !auth_is_bot_token(&adapter.adapter.auth) {
        bail!("telegram egress requires bot_token auth");
    }
    let resolved_secret = resolve_event_egress_secret(&adapter.adapter, "telegram egress")?;
    let token = resolved_secret.value;
    if token.is_empty() {
        bail!(
            "telegram egress secret provider `{}` returned an empty value",
            resolved_secret.source
        );
    }
    validate_http_header_value("telegram bot token", &token)?;
    let chat_id = telegram_chat_id(&request.payload).with_context(
        || {
            "telegram egress requires payload.chat_id, payload.authorized_chat, TELEGRAM_CHAT_ID or TELEGRAM_REPORT_CHAT_ID"
        },
    )?;
    validate_http_header_value("telegram chat id", &chat_id)?;
    let timeout_seconds = adapter.adapter.timeout_seconds.unwrap_or(30).clamp(1, 120);
    let max_response_bytes = adapter
        .adapter
        .max_response_bytes
        .unwrap_or(65_536)
        .clamp(256, 1_048_576);
    let method = telegram_delivery_method(request);
    let endpoint = format!("telegram://bot_api/{method}");
    let simulated = env::var("FORGE_TELEGRAM_EGRESS_MODE")
        .map(|value| value.eq_ignore_ascii_case("simulate"))
        .unwrap_or(false);
    let response = if simulated {
        telegram_simulated_response(request, &method, &chat_id)
    } else {
        run_telegram_curl(&token, &chat_id, request, &method, timeout_seconds)?
    };
    let success = (200..300).contains(&response.status_code);
    let body = response
        .body
        .iter()
        .copied()
        .take(max_response_bytes)
        .collect::<Vec<_>>();
    Ok(EventEgressDeliveryReport {
        transport: adapter.adapter.transport.clone(),
        endpoint,
        auth_scheme: "bot_token".to_string(),
        signed: false,
        signature_header: None,
        secret_env: resolved_secret.env_name,
        secret_source: resolved_secret.source,
        credential_vault: resolved_secret.credential_vault,
        success,
        status_code: response.status_code,
        response_bytes: body.len(),
        response_sha256: hex_sha256(&body),
        response_truncated: response.truncated || response.body.len() > max_response_bytes,
    })
}

fn telegram_chat_id(payload: &Value) -> Option<String> {
    extract_string(payload, &["chat_id"])
        .or_else(|| extract_string(payload, &["authorized_chat"]))
        .or_else(|| extract_string(payload, &["authorized_chat_id"]))
        .or_else(|| env::var("TELEGRAM_REPORT_CHAT_ID").ok())
        .or_else(|| env::var("TELEGRAM_CHAT_ID").ok())
        .and_then(|value| normalize_text(Some(&value)))
}

fn telegram_delivery_method(request: &EventEgressRequestEnvelope) -> String {
    if text_matches(&request.adapter_id, "telegram.bot_send_document")
        || text_matches(&request.event_type, "telegram.document")
        || text_matches(&request.event_type, "telegram.report")
        || telegram_document_path(&request.payload).is_some()
    {
        "sendDocument".to_string()
    } else {
        "sendMessage".to_string()
    }
}

fn telegram_message_text(payload: &Value) -> Option<String> {
    extract_string(payload, &["message"])
        .or_else(|| extract_string(payload, &["text"]))
        .or_else(|| extract_string(payload, &["caption"]))
        .and_then(|value| normalize_text(Some(&value)))
}

fn telegram_caption(payload: &Value) -> String {
    telegram_message_text(payload).unwrap_or_else(|| "Forge report".to_string())
}

fn telegram_document_path(payload: &Value) -> Option<String> {
    extract_string(payload, &["document_path"])
        .or_else(|| extract_string(payload, &["report_path"]))
        .or_else(|| extract_string(payload, &["path"]))
        .or_else(|| extract_string(payload, &["document_ref"]))
        .and_then(|value| normalize_text(Some(&value)))
}

fn telegram_simulated_response(
    request: &EventEgressRequestEnvelope,
    method: &str,
    chat_id: &str,
) -> EventEgressHttpResponse {
    let body = json!({
        "ok": true,
        "simulated": true,
        "method": method,
        "request_id": request.request_id,
        "chat_id_sha256": hex_sha256(chat_id.as_bytes()),
    })
    .to_string()
    .into_bytes();
    EventEgressHttpResponse {
        status_code: 200,
        body,
        truncated: false,
    }
}

fn run_telegram_curl(
    token: &str,
    chat_id: &str,
    request: &EventEgressRequestEnvelope,
    method: &str,
    timeout_seconds: u64,
) -> Result<EventEgressHttpResponse> {
    let url = format!("https://api.telegram.org/bot{token}/{method}");
    let mut command = Command::new("curl");
    command.args([
        "-sS",
        "--max-time",
        &timeout_seconds.to_string(),
        "-X",
        "POST",
        &url,
    ]);
    match method {
        "sendDocument" => {
            let document_path = telegram_document_path(&request.payload).with_context(|| {
                "telegram sendDocument requires document_path, report_path, path or document_ref"
            })?;
            if !Path::new(&document_path).is_file() {
                bail!("telegram sendDocument document path does not exist: {document_path}");
            }
            command.args(["-F", &format!("chat_id={chat_id}")]);
            command.args(["-F", &format!("document=@{document_path}")]);
            command.args([
                "-F",
                &format!("caption={}", telegram_caption(&request.payload)),
            ]);
        }
        _ => {
            let text = telegram_message_text(&request.payload)
                .with_context(|| "telegram sendMessage requires payload.message or payload.text")?;
            command.args(["-d", &format!("chat_id={chat_id}")]);
            command.args(["--data-urlencode", &format!("text={text}")]);
        }
    }
    let output = command
        .output()
        .with_context(|| "failed to execute curl for telegram egress")?;
    let status_code = if output.status.success() { 200 } else { 0 };
    let mut body = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    if body.is_empty() && !output.status.success() {
        body = b"telegram curl failed without output".to_vec();
    }
    Ok(EventEgressHttpResponse {
        status_code,
        body,
        truncated: false,
    })
}

fn attach_event_egress_delivery_artifact(
    store: &ForgeStore,
    request: &EventEgressRequestEnvelope,
    adapter_policy: &EventEgressAdapterPolicyReport,
    delivery: &EventEgressDeliveryReport,
    status: &str,
    operating_context: &OperatingContextSpec,
) -> Result<Option<ArtifactAttachReport>> {
    let Some(workflow_id) = extract_string(&request.payload, &["workflow_id"]) else {
        return Ok(None);
    };
    if store.load_workflow(&workflow_id).is_err() {
        return Ok(None);
    }
    let evidence_dir = store
        .base_dir()
        .join(".forge")
        .join("event-egress-delivery");
    fs::create_dir_all(&evidence_dir)?;
    let source_path = evidence_dir.join(format!("{}.json", request.request_id));
    let evidence = json!({
        "schema_version": EVENT_EGRESS_DELIVERY_EVIDENCE_SCHEMA_VERSION,
        "status": status,
        "workflow_id": workflow_id,
        "recorded_at": Utc::now().to_rfc3339(),
        "request": request,
        "adapter_policy": adapter_policy,
        "delivery": delivery,
        "tenant_context": operating_context,
    });
    fs::write(&source_path, serde_json::to_vec_pretty(&evidence)?)?;
    let artifact_kind = if transport_is_telegram(&request.transport) {
        "telegram_delivery_record"
    } else {
        "event_egress_delivery"
    };
    Ok(Some(attach_workflow_artifact(
        store,
        &workflow_id,
        &source_path,
        artifact_kind,
        &request.origin,
    )?))
}

fn record_event_egress_global_event(
    store: &ForgeStore,
    request: &EventEgressRequestEnvelope,
    adapter_policy: &EventEgressAdapterPolicyReport,
    delivery: Option<&EventEgressDeliveryReport>,
    status: &str,
    dry_run: bool,
    operating_context: &OperatingContextSpec,
) -> Result<i64> {
    let tenant_context = serde_json::to_value(operating_context)?;
    let workflow_id = extract_string(&request.payload, &["workflow_id"]);
    let event_data = json!({
        "request": request,
        "adapter_policy": adapter_policy,
        "delivery": delivery,
        "dry_run": dry_run,
    });
    store.record_global_event(GlobalEventWrite {
        source: "event_egress",
        source_id: &request.request_id,
        workflow_id: workflow_id.as_deref(),
        kind: status,
        origin: &request.origin,
        status,
        data: &event_data,
        tenant_context: &tenant_context,
    })
}

fn build_event_egress_auth_headers(
    adapter: &EventAdapterDeclaration,
    body: &[u8],
) -> Result<EventEgressSignatureHeaders> {
    if !auth_is_hmac(&adapter.auth) && !auth_is_bearer(&adapter.auth) {
        return Ok(EventEgressSignatureHeaders {
            auth_scheme: "none".to_string(),
            signed: false,
            signature_header: None,
            secret_env: None,
            secret_source: "none".to_string(),
            credential_vault: None,
            headers: Vec::new(),
        });
    }
    let resolved_secret = resolve_event_egress_secret(adapter, "event egress")?;
    let secret = resolved_secret.value;
    if secret.is_empty() {
        bail!(
            "event egress secret provider `{}` returned an empty value",
            resolved_secret.source
        );
    }
    if auth_is_bearer(&adapter.auth) {
        validate_http_header_value("bearer secret", &secret)?;
        let auth_header = adapter
            .signature_header
            .as_deref()
            .and_then(|value| normalize_text(Some(value)))
            .unwrap_or_else(|| "Authorization".to_string());
        let auth_header = normalize_http_header_name("auth_header", &auth_header)?;
        return Ok(EventEgressSignatureHeaders {
            auth_scheme: "bearer".to_string(),
            signed: false,
            signature_header: Some(auth_header.clone()),
            secret_env: resolved_secret.env_name,
            secret_source: resolved_secret.source,
            credential_vault: resolved_secret.credential_vault,
            headers: vec![(auth_header, format!("Bearer {secret}"))],
        });
    }
    let signature_header = adapter
        .signature_header
        .as_deref()
        .and_then(|value| normalize_text(Some(value)))
        .unwrap_or_else(|| "X-Forge-Signature".to_string());
    let signature_header = normalize_http_header_name("signature_header", &signature_header)?;
    let timestamp = Utc::now().timestamp().to_string();
    let nonce = Uuid::new_v4().simple().to_string();
    let signed_payload = webhook_signature_payload(&timestamp, &nonce, body);
    let signature = format!(
        "sha256={}",
        hex_encode(&hmac_sha256(secret.as_bytes(), &signed_payload))
    );
    Ok(EventEgressSignatureHeaders {
        auth_scheme: "hmac".to_string(),
        signed: true,
        signature_header: Some(signature_header.clone()),
        secret_env: resolved_secret.env_name,
        secret_source: resolved_secret.source,
        credential_vault: resolved_secret.credential_vault,
        headers: vec![
            (signature_header, signature),
            ("X-Forge-Timestamp".to_string(), timestamp),
            ("X-Forge-Nonce".to_string(), nonce),
        ],
    })
}

struct EventEgressResolvedSecret {
    value: String,
    env_name: Option<String>,
    source: String,
    credential_vault: Option<EventAdapterCredentialVaultRef>,
}

fn event_adapter_has_secret_provider(adapter: &EventAdapterDeclaration) -> bool {
    adapter
        .secret_env
        .as_deref()
        .and_then(|value| normalize_text(Some(value)))
        .is_some()
        || adapter.credential_vault.as_ref().is_some_and(|vault| {
            !vault.contract.trim().is_empty()
                && !vault.data.trim().is_empty()
                && !vault.record.trim().is_empty()
                && !vault.field.trim().is_empty()
        })
}

fn resolve_event_egress_secret(
    adapter: &EventAdapterDeclaration,
    purpose: &str,
) -> Result<EventEgressResolvedSecret> {
    let env_name = adapter
        .secret_env
        .as_deref()
        .and_then(|value| normalize_text(Some(value)));
    if let Some(credential_vault) = adapter.credential_vault.as_ref() {
        let value = resolve_event_egress_secret_from_vault(credential_vault, purpose)?;
        return Ok(EventEgressResolvedSecret {
            value,
            env_name,
            source: "credential_vault".to_string(),
            credential_vault: Some(credential_vault.clone()),
        });
    }
    let secret_env = env_name.with_context(|| {
        format!("{purpose} requires secret_env, hmac_secret_env or credential_vault")
    })?;
    let value = env::var(&secret_env)
        .with_context(|| format!("{purpose} secret env `{secret_env}` is not set"))?;
    Ok(EventEgressResolvedSecret {
        value,
        env_name: Some(secret_env),
        source: "env".to_string(),
        credential_vault: None,
    })
}

fn resolve_event_egress_secret_from_vault(
    credential_vault: &EventAdapterCredentialVaultRef,
    purpose: &str,
) -> Result<String> {
    let contract = required_vault_path("contract", &credential_vault.contract)?;
    let data = required_vault_path("data", &credential_vault.data)?;
    let record = required_text("credential_vault.record", &credential_vault.record)?;
    let field = required_text("credential_vault.field", &credential_vault.field)?;
    let vault_bin = credential_vault
        .vault_bin
        .as_deref()
        .and_then(|value| normalize_text(Some(value)))
        .map(PathBuf::from);
    let bin = resolve_credential_vault_bin(vault_bin.as_deref());
    let output = Command::new(&bin)
        .arg("resolve")
        .arg("--contract")
        .arg(&contract)
        .arg("--data")
        .arg(&data)
        .arg("--record")
        .arg(&record)
        .arg("--field")
        .arg(&field)
        .arg("--allow-secret-stdout")
        .arg("--no-newline")
        .output()
        .with_context(|| format!("failed to run credential-vault for {purpose} secret"))?;
    if !output.status.success() {
        let exit_code = output.status.code().unwrap_or(1);
        bail!(
            "credential-vault resolve failed for {purpose} secret `{record}:{field}` with exit code {exit_code}"
        );
    }
    let value = String::from_utf8(output.stdout)
        .with_context(|| format!("credential-vault returned non-UTF-8 {purpose} secret"))?;
    if value.is_empty() {
        bail!("credential-vault returned an empty {purpose} secret for `{record}:{field}`");
    }
    Ok(value)
}

fn required_vault_path(name: &str, value: &str) -> Result<PathBuf> {
    let value = required_text(&format!("credential_vault.{name}"), value)?;
    Ok(PathBuf::from(value))
}

fn parse_http_event_endpoint(endpoint: &str) -> Result<ParsedHttpEndpoint> {
    let endpoint = endpoint.trim();
    let (scheme, rest, default_port) = if let Some(rest) = endpoint.strip_prefix("http://") {
        ("http".to_string(), rest, 80)
    } else if let Some(rest) = endpoint.strip_prefix("https://") {
        ("https".to_string(), rest, 443)
    } else {
        bail!("event egress endpoint must use explicit http:// or https://");
    };
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        bail!("event egress endpoint host must not be empty");
    }
    if authority.contains('@') {
        bail!("event egress endpoint must not include userinfo");
    }
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, remainder) = bracketed
            .split_once(']')
            .with_context(|| "invalid bracketed IPv6 endpoint host")?;
        let port = remainder
            .strip_prefix(':')
            .map(|value| value.parse::<u16>())
            .transpose()
            .with_context(|| "invalid event egress endpoint port")?
            .unwrap_or(default_port);
        (host.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => (
                host.to_string(),
                port.parse::<u16>()
                    .with_context(|| "invalid event egress endpoint port")?,
            ),
            _ => (authority.to_string(), default_port),
        }
    };
    if host.trim().is_empty() {
        bail!("event egress endpoint host must not be empty");
    }
    if !host
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        bail!("event egress endpoint host contains unsupported characters");
    }
    if path.contains('\r') || path.contains('\n') {
        bail!("event egress endpoint path must not contain CR/LF");
    }
    Ok(ParsedHttpEndpoint {
        scheme,
        host,
        port,
        path,
    })
}

fn normalize_http_header_name(field: &str, value: &str) -> Result<String> {
    let value = required_text(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!("{field} must contain only ASCII letters, digits or hyphen");
    }
    Ok(value)
}

fn validate_http_header_value(field: &str, value: &str) -> Result<()> {
    if value.contains('\r') || value.contains('\n') {
        bail!("{field} must not contain CR/LF characters");
    }
    Ok(())
}

fn event_egress_allowed_host(host: &str, allowed_hosts: &[String]) -> bool {
    is_local_http_host(host)
        || allowed_hosts.iter().any(|allowed| {
            allowed.trim() == "*" || allowed.trim().eq_ignore_ascii_case(host.trim())
        })
}

fn is_local_http_host(host: &str) -> bool {
    let host = host.trim().trim_matches('[').trim_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn resolve_event_egress_address(endpoint: &ParsedHttpEndpoint) -> Result<SocketAddr> {
    let addresses = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .with_context(|| "failed to resolve event egress endpoint")?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        bail!("event egress endpoint did not resolve to any address");
    }
    let explicit_loopback = is_local_http_host(&endpoint.host);
    for address in &addresses {
        if address.ip().is_loopback() && explicit_loopback {
            continue;
        }
        if !ip_is_public_for_outbound(address.ip()) {
            bail!(
                "event egress endpoint resolved to blocked private, link-local or reserved address"
            );
        }
    }
    Ok(addresses[0])
}

fn ip_is_public_for_outbound(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.is_documentation()
                || octets[0] == 0
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 198 && matches!(octets[1], 18 | 19))
                || octets[0] >= 240)
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return ip_is_public_for_outbound(IpAddr::V4(mapped));
            }
            let segments = ip.segments();
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || segments[..6].iter().all(|segment| *segment == 0)
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] & 0xffc0) == 0xfec0
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || (segments[0] == 0x0100
                    && segments[1] == 0
                    && segments[2] == 0
                    && segments[3] == 0))
        }
    }
}

fn post_event_egress_json(
    endpoint: &ParsedHttpEndpoint,
    address: SocketAddr,
    body: &[u8],
    timeout: Duration,
    max_response_bytes: usize,
    extra_headers: &[(String, String)],
) -> Result<EventEgressHttpResponse> {
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .with_context(|| "failed to connect to event egress endpoint")?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let host_header = if endpoint.port == 80 {
        endpoint.host.clone()
    } else {
        format!("{}:{}", endpoint.host, endpoint.port)
    };
    let extra_headers = extra_headers
        .iter()
        .map(|(name, value)| {
            let name = normalize_http_header_name("event egress header", name)?;
            validate_http_header_value("event egress header value", value)?;
            Ok(format!("{name}: {value}\r\n"))
        })
        .collect::<Result<Vec<_>>>()?
        .join("");
    let request_headers = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: forge-core/event-egress\r\nContent-Type: application/json\r\nAccept: application/json\r\n{extra_headers}Connection: close\r\nContent-Length: {}\r\n\r\n",
        endpoint.path,
        host_header,
        body.len()
    );
    stream.write_all(request_headers.as_bytes())?;
    stream.write_all(body)?;
    let read_limit = max_response_bytes.saturating_add(8192).saturating_add(1);
    let mut response = Vec::new();
    let mut limited = stream.take(read_limit as u64);
    limited.read_to_end(&mut response)?;
    let truncated = response.len() >= read_limit;
    let status_code = parse_http_event_status_code(&response)?;
    if (300..400).contains(&status_code) {
        bail!("event egress redirects are denied");
    }
    let response_body = http_event_response_body(&response);
    Ok(EventEgressHttpResponse {
        status_code,
        body: response_body
            .iter()
            .copied()
            .take(max_response_bytes)
            .collect(),
        truncated: truncated || response_body.len() > max_response_bytes,
    })
}

fn post_event_egress_https_curl(
    endpoint: &str,
    parsed: &ParsedHttpEndpoint,
    resolved_address: Option<SocketAddr>,
    body: &[u8],
    timeout: Duration,
    max_response_bytes: usize,
    extra_headers: &[(String, String)],
) -> Result<EventEgressHttpResponse> {
    let simulated = event_env_value_is("FORGE_EVENT_EGRESS_HTTPS_MODE", "simulate");
    if simulated {
        let body = json!({
            "ok": true,
            "simulated": true,
            "transport": "https",
            "endpoint_sha256": hex_sha256(endpoint.as_bytes()),
            "request_bytes": body.len(),
        })
        .to_string()
        .into_bytes();
        return Ok(EventEgressHttpResponse {
            status_code: 202,
            body,
            truncated: false,
        });
    }

    let timeout_seconds = timeout.as_secs().max(1).to_string();
    let resolved_address =
        resolved_address.with_context(|| "event egress HTTPS endpoint address was not resolved")?;
    let resolve_value = curl_resolve_value(&parsed.host, parsed.port, resolved_address.ip());
    let mut command = Command::new("curl");
    command.args([
        "-sS",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        "0",
        "--noproxy",
        "*",
        "--resolve",
        &resolve_value,
        "--max-time",
        &timeout_seconds,
        "-X",
        "POST",
        endpoint,
        "-H",
        "User-Agent: forge-core/event-egress",
        "-H",
        "Content-Type: application/json",
        "-H",
        "Accept: application/json",
    ]);
    for (name, value) in extra_headers {
        let name = normalize_http_header_name("event egress header", name)?;
        validate_http_header_value("event egress header value", value)?;
        command.args(["-H", &format!("{name}: {value}")]);
    }
    command.args(["--data-binary", "@-", "-w", "\n%{http_code}"]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| "failed to execute curl for HTTPS event egress")?;
    child
        .stdin
        .as_mut()
        .with_context(|| "failed to open curl stdin for HTTPS event egress")?
        .write_all(body)?;
    let output = child
        .wait_with_output()
        .with_context(|| "failed to wait for HTTPS event egress curl")?;
    let (status_code, response_body) = parse_curl_http_status(&output.stdout)
        .unwrap_or_else(|| (if output.status.success() { 200 } else { 0 }, output.stdout));
    if (300..400).contains(&status_code) {
        bail!("event egress redirects are denied");
    }
    let response_body = if response_body.is_empty() && !output.status.success() {
        output.stderr
    } else {
        response_body
    };
    let truncated = response_body.len() > max_response_bytes;
    Ok(EventEgressHttpResponse {
        status_code,
        body: response_body.into_iter().take(max_response_bytes).collect(),
        truncated,
    })
}

fn curl_resolve_value(host: &str, port: u16, ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => format!("{host}:{port}:{ip}"),
        IpAddr::V6(ip) => format!("{host}:{port}:[{ip}]"),
    }
}

fn parse_curl_http_status(output: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = output.iter().rposition(|byte| *byte == b'\n')?;
    let status_text = std::str::from_utf8(&output[split + 1..]).ok()?.trim();
    if status_text.len() != 3 || !status_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let status_code = status_text.parse::<u16>().ok()?;
    Some((status_code, output[..split].to_vec()))
}

fn parse_http_event_status_code(response: &[u8]) -> Result<u16> {
    let text = String::from_utf8_lossy(response);
    let status_line = text
        .lines()
        .next()
        .with_context(|| "empty event egress HTTP response")?;
    status_line
        .split_whitespace()
        .nth(1)
        .with_context(|| "event egress HTTP response did not include a status code")?
        .parse::<u16>()
        .with_context(|| "invalid event egress HTTP status code")
}

fn http_event_response_body(response: &[u8]) -> &[u8] {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| &response[index + 4..])
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn handle_webhook_ingress_stream(
    store: &ForgeStore,
    stream: &mut TcpStream,
    peer_address: SocketAddr,
    expected_path: &str,
    default_origin: &str,
    default_action: &str,
    transport: &str,
    schema: Option<&str>,
    project_root: &Path,
    route_after_ingest: bool,
    max_body_bytes: usize,
    hmac_verifier: Option<&WebhookHmacVerifier>,
    security_state: &mut WebhookIngressSecurityState,
) -> Result<EventWebhookIngressEntry> {
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    let request_id = format!("webhook_{}", Uuid::new_v4().to_string().replace('-', ""));
    security_state.check_rate_limit(peer_address.ip())?;
    let request = read_webhook_http_request(stream, max_body_bytes)?;
    if request.method != "POST" {
        bail!("webhook ingress accepts POST only");
    }
    if request.path != expected_path {
        bail!(
            "webhook ingress path mismatch: expected `{expected_path}`, got `{}`",
            request.path
        );
    }
    let auth_verified = if let Some(verifier) = hmac_verifier {
        verify_webhook_hmac(verifier, &request, security_state)?;
        Some(true)
    } else {
        None
    };
    let payload = serde_json::from_slice::<Value>(&request.body)
        .with_context(|| "webhook ingress body must be valid JSON")?;
    let inbound_input = normalize_webhook_inbound_input(
        payload,
        default_origin,
        default_action,
        transport,
        schema,
        auth_verified,
    )?;
    let origin = inbound_input.origin.clone();
    let action = inbound_input.action.clone();
    if event_route_action_mutates_workflow(&normalized_action(&action))
        && auth_verified != Some(true)
        && !security_state.allow_unsigned_mutations
    {
        bail!("mutable webhook action `{action}` requires authentication");
    }
    let operating_context = load_project_operating_context(project_root)?;
    let ingest = ingest_inbound_event_with_context(store, inbound_input, &operating_context)?;
    let event_id = ingest.event.id.clone();
    let route = if route_after_ingest {
        match route_inbound_event(store, &event_id, project_root) {
            Ok(route) => Some(route),
            Err(error) => {
                let failed_data = json!({
                    "event_id": event_id,
                    "origin": origin,
                    "action": action,
                    "webhook_ingress_error": error.to_string(),
                    "project_root": project_root.display().to_string(),
                });
                store.update_inbound_event_status(&event_id, "failed", &failed_data)?;
                bail!("webhook event was ingested but route failed: {error}");
            }
        }
    } else {
        None
    };
    let event = store.load_inbound_event(&event_id)?;
    let status = if route.is_some() {
        "webhook_event_ingested_and_routed"
    } else {
        "webhook_event_ingested"
    };
    Ok(EventWebhookIngressEntry {
        request_id,
        method: request.method,
        path: request.path,
        http_status: 202,
        status: status.to_string(),
        origin,
        action,
        auth_verified,
        event_id: Some(event_id),
        event: Some(inbound_event_view(store, event)),
        route,
        error: None,
    })
}

fn read_webhook_http_request(
    stream: &mut TcpStream,
    max_body_bytes: usize,
) -> Result<WebhookHttpRequest> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            bail!("webhook request closed before headers");
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let header_end = index + 4;
            if header_end > 16_384 {
                bail!("webhook request headers exceeded 16KiB");
            }
            break header_end;
        }
        if request.len() > 16_384 {
            bail!("webhook request headers exceeded 16KiB");
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
    let mut lines = headers.lines();
    let request_line = lines
        .next()
        .with_context(|| "webhook request missing request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .with_context(|| "webhook request missing method")?
        .to_string();
    let path = request_parts
        .next()
        .with_context(|| "webhook request missing path")?
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let parsed_headers = parse_webhook_headers(&headers)?;
    if parsed_headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("transfer-encoding"))
    {
        bail!("webhook ingress does not accept Transfer-Encoding requests");
    }
    let content_lengths = parsed_headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| {
            value
                .trim()
                .parse::<usize>()
                .with_context(|| "webhook request has invalid Content-Length")
        })
        .collect::<Result<Vec<_>>>()?;
    if content_lengths.len() != 1 {
        bail!("webhook request requires exactly one Content-Length header");
    }
    let content_length = content_lengths[0];
    if content_length > max_body_bytes {
        bail!("webhook body exceeds max_body_bytes ({max_body_bytes})");
    }
    while request.len() < header_end + content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            bail!("webhook request closed before body was fully read");
        }
        request.extend_from_slice(&buffer[..read]);
    }
    Ok(WebhookHttpRequest {
        method,
        path,
        headers: parsed_headers,
        body: request[header_end..header_end + content_length].to_vec(),
    })
}

fn normalize_webhook_inbound_input(
    payload: Value,
    default_origin: &str,
    default_action: &str,
    transport: &str,
    schema: Option<&str>,
    auth_verified: Option<bool>,
) -> Result<InboundEventIngestInput> {
    let origin =
        extract_string(&payload, &["origin"]).unwrap_or_else(|| default_origin.to_string());
    let action =
        extract_string(&payload, &["action"]).unwrap_or_else(|| default_action.to_string());
    let data = payload
        .get("data")
        .cloned()
        .unwrap_or_else(|| payload.clone());
    Ok(InboundEventIngestInput {
        origin: required_text("origin", &origin)?,
        action: required_text("action", &action)?,
        data: enrich_webhook_event_data(data, transport, schema, auth_verified),
    })
}

fn enrich_webhook_event_data(
    mut data: Value,
    transport: &str,
    schema: Option<&str>,
    auth_verified: Option<bool>,
) -> Value {
    match &mut data {
        Value::Object(map) => {
            map.entry("transport".to_string())
                .or_insert_with(|| Value::String(transport.to_string()));
            if let Some(schema) = schema {
                map.entry("schema".to_string())
                    .or_insert_with(|| Value::String(schema.to_string()));
            }
            if let Some(auth_verified) = auth_verified {
                map.insert("auth_verified".to_string(), Value::Bool(auth_verified));
            }
            data
        }
        _ => json!({
            "payload": data,
            "transport": transport,
            "schema": schema,
            "auth_verified": auth_verified,
        }),
    }
}

fn parse_webhook_headers(headers: &str) -> Result<Vec<(String, String)>> {
    headers
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (name, value) = line
                .split_once(':')
                .with_context(|| "malformed webhook HTTP header")?;
            let name = normalize_http_header_name("webhook header", name)?;
            Ok((name.to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect()
}

fn build_webhook_hmac_verifier(
    hmac_secret_env: Option<&str>,
    signature_header: &str,
) -> Result<Option<WebhookHmacVerifier>> {
    let Some(secret_env) = normalize_text(hmac_secret_env) else {
        return Ok(None);
    };
    let signature_header = normalize_http_header_name("signature_header", signature_header)?;
    let secret = env::var(&secret_env)
        .with_context(|| format!("webhook HMAC secret env `{secret_env}` is not set"))?;
    if secret.len() < MIN_WEBHOOK_HMAC_SECRET_BYTES {
        bail!(
            "webhook HMAC secret env `{secret_env}` must contain at least {MIN_WEBHOOK_HMAC_SECRET_BYTES} bytes"
        );
    }
    Ok(Some(WebhookHmacVerifier {
        secret_env,
        signature_header,
        secret: secret.into_bytes(),
    }))
}

fn verify_webhook_hmac(
    verifier: &WebhookHmacVerifier,
    request: &WebhookHttpRequest,
    security_state: &mut WebhookIngressSecurityState,
) -> Result<()> {
    let header_name = verifier.signature_header.to_ascii_lowercase();
    let signature = unique_webhook_header(request, &header_name)?.with_context(|| {
        format!(
            "webhook request missing signature header `{}`",
            verifier.signature_header
        )
    })?;
    let signature_hex = signature.strip_prefix("sha256=").unwrap_or(signature);
    let provided = decode_hex_bytes(signature_hex)
        .with_context(|| "webhook HMAC signature must be hex or sha256=<hex>")?;
    let timestamp =
        unique_webhook_header(request, WEBHOOK_TIMESTAMP_HEADER)?.with_context(|| {
            format!("webhook request missing timestamp header `{WEBHOOK_TIMESTAMP_HEADER}`")
        })?;
    let nonce = unique_webhook_header(request, WEBHOOK_NONCE_HEADER)?.with_context(|| {
        format!("webhook request missing nonce header `{WEBHOOK_NONCE_HEADER}`")
    })?;
    validate_webhook_nonce(nonce)?;
    let timestamp_seconds = timestamp
        .parse::<i64>()
        .with_context(|| "webhook timestamp must be Unix epoch seconds")?;
    let signed_payload = webhook_signature_payload(timestamp, nonce, &request.body);
    let expected = hmac_sha256(&verifier.secret, &signed_payload);
    if !constant_time_eq(&provided, &expected) {
        bail!("webhook HMAC signature mismatch");
    }
    security_state.accept_fresh_nonce(timestamp_seconds, nonce)?;
    Ok(())
}

impl WebhookIngressSecurityState {
    fn new(allow_unsigned_mutations: bool, rate_limit_per_minute: usize) -> Self {
        Self {
            seen_nonces: BTreeMap::new(),
            requests_by_peer: BTreeMap::new(),
            rate_limit_per_minute,
            allow_unsigned_mutations,
        }
    }

    fn check_rate_limit(&mut self, peer_ip: IpAddr) -> Result<()> {
        let now = Instant::now();
        let requests = self.requests_by_peer.entry(peer_ip).or_default();
        while requests
            .front()
            .is_some_and(|started| now.duration_since(*started) >= Duration::from_secs(60))
        {
            requests.pop_front();
        }
        if requests.len() >= self.rate_limit_per_minute {
            bail!(
                "webhook rate limit exceeded for peer; limit is {} requests per minute",
                self.rate_limit_per_minute
            );
        }
        requests.push_back(now);
        Ok(())
    }

    fn accept_fresh_nonce(&mut self, timestamp_seconds: i64, nonce: &str) -> Result<()> {
        let now_seconds = Utc::now().timestamp();
        if now_seconds.abs_diff(timestamp_seconds) > WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS as u64 {
            bail!(
                "webhook timestamp is outside the {} second acceptance window",
                WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS
            );
        }
        let now = Instant::now();
        self.seen_nonces.retain(|_, seen_at| {
            now.duration_since(*seen_at)
                < Duration::from_secs((WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS * 2) as u64)
        });
        if self.seen_nonces.contains_key(nonce) {
            bail!("webhook replay detected for nonce");
        }
        self.seen_nonces.insert(nonce.to_string(), now);
        Ok(())
    }
}

fn configured_webhook_rate_limit() -> usize {
    env::var(WEBHOOK_RATE_LIMIT_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(WEBHOOK_RATE_LIMIT_PER_MINUTE_DEFAULT)
        .clamp(1, 10_000)
}

fn unique_webhook_header<'a>(
    request: &'a WebhookHttpRequest,
    name: &str,
) -> Result<Option<&'a str>> {
    let mut matches = request
        .headers
        .iter()
        .filter(|(header_name, _)| header_name.eq_ignore_ascii_case(name));
    let first = matches.next().map(|(_, value)| value.trim());
    if matches.next().is_some() {
        bail!("webhook request contains duplicate security header `{name}`");
    }
    Ok(first)
}

fn validate_webhook_nonce(nonce: &str) -> Result<()> {
    if !(16..=128).contains(&nonce.len())
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("webhook nonce must contain 16-128 ASCII letters, digits, hyphen or underscore");
    }
    Ok(())
}

fn webhook_signature_payload(timestamp: &str, nonce: &str, body: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(timestamp.len() + nonce.len() + body.len() + 2);
    payload.extend_from_slice(timestamp.as_bytes());
    payload.push(b'.');
    payload.extend_from_slice(nonce.as_bytes());
    payload.push(b'.');
    payload.extend_from_slice(body);
    payload
}

fn resolve_webhook_bind_addresses(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let addresses = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve webhook ingress bind host `{host}`"))?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        bail!("webhook ingress bind host `{host}` did not resolve");
    }
    Ok(addresses)
}

fn validate_webhook_ingress_security(
    local_only: bool,
    hmac_configured: bool,
    default_action: &str,
    allow_unsigned_mutations: bool,
) -> Result<()> {
    if !local_only && !hmac_configured {
        bail!("webhook ingress exposed beyond loopback requires HMAC authentication");
    }
    if event_route_action_mutates_workflow(&normalized_action(default_action))
        && !hmac_configured
        && !allow_unsigned_mutations
    {
        bail!(
            "mutable webhook action `{default_action}` requires HMAC authentication; set --hmac-secret-env"
        );
    }
    Ok(())
}

fn event_env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn event_env_value_is(name: &str, expected: &str) -> bool {
    env::var(name)
        .map(|value| value.trim().eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn hmac_sha256(secret: &[u8], body: &[u8]) -> Vec<u8> {
    let mut key = secret.to_vec();
    if key.len() > 64 {
        key = Sha256::digest(&key).to_vec();
    }
    key.resize(64, 0);
    let mut outer_key_pad = vec![0x5c; 64];
    let mut inner_key_pad = vec![0x36; 64];
    for (index, byte) in key.iter().enumerate() {
        outer_key_pad[index] ^= byte;
        inner_key_pad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(&inner_key_pad);
    inner.update(body);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&outer_key_pad);
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex_bytes(value: &str) -> Result<Vec<u8>> {
    let value = value.trim();
    if !value.len().is_multiple_of(2) {
        bail!("hex string must have an even length");
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks(2) {
        let text = std::str::from_utf8(chunk)?;
        bytes.push(u8::from_str_radix(text, 16)?);
    }
    Ok(bytes)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in left.iter().zip(right.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn write_webhook_ingress_response(
    stream: &mut TcpStream,
    entry: &EventWebhookIngressEntry,
) -> Result<()> {
    let body = serde_json::to_string(&json!({
        "schema_version": EVENT_WEBHOOK_INGRESS_RESPONSE_SCHEMA_VERSION,
        "status": entry.status,
        "request_id": entry.request_id,
        "event_id": entry.event_id,
        "route_decision": entry.route.as_ref().map(|route| route.route_decision.clone()),
        "error": entry.error,
    }))?;
    let status_text = match entry.http_status {
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let retry_after = if entry.http_status == 429 {
        "Retry-After: 60\r\n"
    } else {
        ""
    };
    let response = format!(
        concat!(
            "HTTP/1.1 {} {}\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: {}\r\n",
            "Cache-Control: no-store\r\n",
            "X-Content-Type-Options: nosniff\r\n",
            "{}",
            "Connection: close\r\n\r\n{}"
        ),
        entry.http_status,
        status_text,
        body.len(),
        retry_after,
        body
    );
    stream.write_all(response.as_bytes())?;
    Ok(())
}

fn normalize_webhook_path(path: &str) -> Result<String> {
    let path = required_text("path", path)?;
    if path.starts_with('/') {
        Ok(path)
    } else {
        Ok(format!("/{path}"))
    }
}

fn normalize_event_service_kind(service_kind: &str) -> Result<String> {
    let service_kind = required_text("service_kind", service_kind)?;
    match service_kind.trim().to_ascii_lowercase().as_str() {
        "worker" | "event_worker" | "event-worker" | "inbox_worker" | "inbox-worker" => {
            Ok("worker".to_string())
        }
        "webhook" | "webhook_ingress" | "webhook-ingress" | "http_webhook" | "http-webhook" => {
            Ok("webhook_ingress".to_string())
        }
        other => {
            bail!("unsupported event service kind `{other}`; expected worker or webhook_ingress")
        }
    }
}

fn select_tail(events: Vec<StoreEvent>, limit: Option<usize>) -> Vec<StoreEvent> {
    let Some(limit) = limit.filter(|limit| *limit > 0) else {
        return events;
    };
    let start = events.len().saturating_sub(limit);
    events.into_iter().skip(start).collect()
}

fn select_tail_envelopes(
    events: Vec<WorkflowEventEnvelope>,
    limit: Option<usize>,
) -> Vec<WorkflowEventEnvelope> {
    let Some(limit) = limit.filter(|limit| *limit > 0) else {
        return events;
    };
    let start = events.len().saturating_sub(limit);
    events.into_iter().skip(start).collect()
}

fn select_timeline_page(
    events: Vec<WorkflowEventEnvelope>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
) -> (Vec<WorkflowEventEnvelope>, EventTimelinePage) {
    let normalized_limit = limit.filter(|limit| *limit > 0);
    let mut has_more = false;
    let selected = if let Some(after_sequence) = after_sequence {
        let filtered = events
            .into_iter()
            .filter(|event| event.store_sequence > after_sequence)
            .collect::<Vec<_>>();
        match normalized_limit {
            Some(limit) => {
                has_more = filtered.len() > limit;
                filtered.into_iter().take(limit).collect::<Vec<_>>()
            }
            None => filtered,
        }
    } else {
        if let Some(limit) = normalized_limit {
            has_more = events.len() > limit;
        }
        select_tail_envelopes(events, normalized_limit)
    };
    let next_cursor = selected.last().map(|event| event.store_sequence);
    (
        selected,
        EventTimelinePage {
            schema_version: "forge.event_timeline.page.v1".to_string(),
            after_sequence,
            limit: normalized_limit,
            next_cursor,
            has_more,
        },
    )
}

fn select_observability_page(
    events: Vec<EventObservabilityRecord>,
    limit: Option<usize>,
    after_sequence: Option<i64>,
) -> (Vec<EventObservabilityRecord>, EventTimelinePage) {
    let normalized_limit = limit.filter(|limit| *limit > 0);
    let mut has_more = false;
    let selected = if let Some(after_sequence) = after_sequence {
        let filtered = events
            .into_iter()
            .filter(|event| event.store_sequence > after_sequence)
            .collect::<Vec<_>>();
        match normalized_limit {
            Some(limit) => {
                has_more = filtered.len() > limit;
                filtered.into_iter().take(limit).collect::<Vec<_>>()
            }
            None => filtered,
        }
    } else {
        if let Some(limit) = normalized_limit {
            has_more = events.len() > limit;
            let start = events.len().saturating_sub(limit);
            events.into_iter().skip(start).collect::<Vec<_>>()
        } else {
            events
        }
    };
    let next_cursor = selected.last().map(|event| event.store_sequence);
    (
        selected,
        EventTimelinePage {
            schema_version: "forge.event_timeline.page.v1".to_string(),
            after_sequence,
            limit: normalized_limit,
            next_cursor,
            has_more,
        },
    )
}

fn count_entries(counts: BTreeMap<String, usize>) -> Vec<EventObservabilityCount> {
    counts
        .into_iter()
        .map(|(id, count)| EventObservabilityCount { id, count })
        .collect()
}

fn workflow_matches_tenant(
    workflow: &Workflow,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
) -> bool {
    let context = &workflow.intent.operating_context;
    filter_matches(organization_id, &context.organization.id)
        && filter_matches(brand_id, &context.brand.id)
        && filter_matches(product_id, &context.product.id)
}

fn filter_matches(filter: Option<&str>, value: &str) -> bool {
    normalize_text(filter)
        .as_deref()
        .map(|filter| filter == value)
        .unwrap_or(true)
}

fn filter_matches_optional(filter: Option<&str>, value: Option<&str>) -> bool {
    let Some(filter) = normalize_text(filter) else {
        return true;
    };
    value.map(|value| value == filter).unwrap_or(false)
}

fn global_event_matches_filters(
    event: &StoredGlobalEventRecord,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
) -> bool {
    filter_matches_optional(workflow_id, event.workflow_id.as_deref())
        && filter_matches(organization_id, &event.organization_id)
        && filter_matches(brand_id, &event.brand_id)
        && filter_matches(product_id, &event.product_id)
}

fn event_inbox_tenant_filters_for_context(
    store: &ForgeStore,
    operating_context: &OperatingContextSpec,
    action: &str,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    if operating_context.tenant_policy_mode != "enforce" {
        return Ok((None, None, None));
    }
    ensure_operating_context_policy(store, operating_context, action)?;
    Ok((
        Some(operating_context.organization.id.clone()),
        Some(operating_context.brand.id.clone()),
        Some(operating_context.product.id.clone()),
    ))
}

fn event_service_tenant_filters_for_context(
    store: &ForgeStore,
    operating_context: &OperatingContextSpec,
    action: &str,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    if operating_context.tenant_policy_mode != "enforce" {
        return Ok((None, None, None));
    }
    ensure_operating_context_policy(store, operating_context, action)?;
    Ok((
        Some(operating_context.organization.id.clone()),
        Some(operating_context.brand.id.clone()),
        Some(operating_context.product.id.clone()),
    ))
}

fn event_runtime_allowed_workflow_ids(
    store: &ForgeStore,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
) -> Result<Option<BTreeSet<String>>> {
    if organization_id.is_none() && brand_id.is_none() && product_id.is_none() {
        return Ok(None);
    }

    let ids = store
        .load_workflows()?
        .into_iter()
        .filter(|workflow| workflow_matches_tenant(workflow, organization_id, brand_id, product_id))
        .map(|workflow| workflow.id)
        .collect::<BTreeSet<_>>();
    Ok(Some(ids))
}

fn enforce_timeline_tenant_filter(
    action: &str,
    label: &str,
    requested: Option<&str>,
    allowed: &str,
) -> Result<String> {
    if let Some(requested) = normalize_text(requested) {
        if requested != allowed {
            bail!(
                "multi-tenant enforcement blocked {action}: requested {label} {requested} is outside operating context {allowed}"
            );
        }
    }
    Ok(allowed.to_string())
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn evaluate_inbound_event_adapter_policy(
    catalog: &AddonCatalog,
    event: &InboundEventRecord,
) -> InboundEventAdapterPolicyReport {
    let normalized_action = normalized_action(&event.action);
    let transport = extract_string(&event.data, &["transport", "adapter_transport"]);
    let schema = extract_string(&event.data, &["schema", "event_schema", "event_type"]);
    let auth_verified = extract_bool(&event.data, &["auth_verified", "authenticated"]);
    let adapter_report = list_addon_event_adapters(catalog, None, None, Some("ingress"));
    let source_candidates = adapter_report
        .adapters
        .into_iter()
        .filter(|adapter| adapter_source_matches(adapter, &event.origin, transport.as_deref()))
        .collect::<Vec<_>>();

    if source_candidates.is_empty() {
        return InboundEventAdapterPolicyReport {
            schema_version: EVENT_ADAPTER_POLICY_SCHEMA_VERSION.to_string(),
            status: "no_declared_adapter".to_string(),
            allowed: true,
            enforced: false,
            origin: event.origin.clone(),
            action: event.action.clone(),
            normalized_action,
            transport,
            schema,
            auth_verified,
            issues: Vec::new(),
            matched_adapter: None,
        };
    }

    let action_candidates = source_candidates
        .into_iter()
        .filter(|adapter| adapter_action_matches(adapter, &event.action, &normalized_action))
        .collect::<Vec<_>>();
    if action_candidates.is_empty() {
        return blocked_adapter_policy(
            event,
            BlockedAdapterPolicyInput {
                normalized_action,
                transport,
                schema,
                auth_verified,
                status: "adapter_action_not_allowed",
                issue: "event origin matched at least one adapter, but action is not declared",
                matched_adapter: None,
            },
        );
    }

    let schema_candidates = action_candidates
        .into_iter()
        .filter(|adapter| adapter_schema_matches(adapter, schema.as_deref()))
        .collect::<Vec<_>>();
    if schema_candidates.is_empty() {
        return blocked_adapter_policy(
            event,
            BlockedAdapterPolicyInput {
                normalized_action,
                transport,
                schema,
                auth_verified,
                status: "adapter_schema_mismatch",
                issue: "event schema does not match the declared adapter schema or event types",
                matched_adapter: None,
            },
        );
    }

    let matched_adapter = schema_candidates.into_iter().next();
    let mut issues = Vec::new();
    let mut status = "matched".to_string();
    let mut allowed = true;

    if let Some(adapter) = &matched_adapter {
        let gate = &adapter.permission_gate;
        if !gate.allowed {
            status = gate.status.clone();
            allowed = false;
            issues.push(format!("permission gate denied adapter: {}", gate.status));
        }
        if auth_requires_verification(&adapter.adapter.auth) {
            match auth_verified {
                Some(true) => {}
                Some(false) => {
                    status = "adapter_auth_not_verified".to_string();
                    allowed = false;
                    issues.push("adapter auth was explicitly reported as not verified".to_string());
                }
                None => {
                    if allowed {
                        status = "matched_auth_unverified".to_string();
                    }
                    issues.push(format!(
                        "adapter declares auth `{}` but event did not include auth_verified evidence",
                        adapter.adapter.auth
                    ));
                }
            }
        }
    }

    InboundEventAdapterPolicyReport {
        schema_version: EVENT_ADAPTER_POLICY_SCHEMA_VERSION.to_string(),
        status,
        allowed,
        enforced: true,
        origin: event.origin.clone(),
        action: event.action.clone(),
        normalized_action,
        transport,
        schema,
        auth_verified,
        issues,
        matched_adapter,
    }
}

fn build_inbound_addon_event_adapter_plan(
    catalog: &AddonCatalog,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
) -> InboundEventAddonAdapterPlan {
    let adapter_report = list_addon_event_adapters(catalog, None, None, Some("ingress"));
    let event_extension_registry = adapter_report.event_extension_registry;
    let mut adapters = adapter_report
        .adapters
        .into_iter()
        .filter(|adapter| {
            adapter_source_matches(adapter, &event.origin, adapter_policy.transport.as_deref())
        })
        .map(|adapter| inbound_event_addon_adapter_plan_entry(event, adapter_policy, adapter))
        .collect::<Vec<_>>();
    adapters.sort_by(|left, right| {
        addon_event_adapter_plan_status_rank(&left.status)
            .cmp(&addon_event_adapter_plan_status_rank(&right.status))
            .then_with(|| left.adapter_id.cmp(&right.adapter_id))
            .then_with(|| left.addon_id.cmp(&right.addon_id))
    });
    let event_extension_matches = build_inbound_event_extension_matches(
        &event_extension_registry,
        event,
        adapter_policy,
        &adapters,
    );
    let event_workflow_activation_plan = build_inbound_event_workflow_activation_plan(
        catalog,
        event,
        adapter_policy,
        &event_extension_matches,
    );

    let matched_count = adapters
        .iter()
        .filter(|entry| entry.status == "matched")
        .count();
    let allowed_count = adapters.iter().filter(|entry| entry.allowed).count();
    let blocked_count = adapters.len().saturating_sub(allowed_count);
    let status = if !adapter_policy.enforced {
        "addon_event_adapter_plan_unenforced"
    } else if matched_count > 0 {
        "addon_event_adapter_plan_ready"
    } else {
        "addon_event_adapter_plan_blocked"
    };

    InboundEventAddonAdapterPlan {
        schema_version: EVENT_ADDON_ADAPTER_PLAN_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        enforced: adapter_policy.enforced,
        origin: event.origin.clone(),
        action: event.action.clone(),
        normalized_action: adapter_policy.normalized_action.clone(),
        transport: adapter_policy.transport.clone(),
        schema: adapter_policy.schema.clone(),
        auth_verified: adapter_policy.auth_verified,
        source_candidate_count: adapters.len(),
        matched_count,
        allowed_count,
        blocked_count,
        event_extension_matches,
        event_workflow_activation_plan,
        next_commands: inbound_event_addon_adapter_next_commands(event, &adapters),
        notes: inbound_event_addon_adapter_plan_notes(adapter_policy, matched_count, &adapters),
        adapters,
    }
}

fn build_inbound_event_extension_matches(
    registry: &AddonEventExtensionRegistry,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    adapters: &[InboundEventAddonAdapterPlanEntry],
) -> InboundEventExtensionMatches {
    let event_type_candidates = inbound_event_type_candidates(event, adapter_policy, adapters);
    let matched_adapter_ids = adapters
        .iter()
        .filter(|adapter| adapter.status == "matched")
        .map(|adapter| adapter.adapter_id.clone())
        .collect::<BTreeSet<_>>();
    let matched_channels = registry
        .channels
        .iter()
        .filter(|channel| {
            event_channel_matches(channel, event, adapter_policy, &event_type_candidates)
        })
        .cloned()
        .collect::<Vec<_>>();
    let matched_channel_ids = matched_channels
        .iter()
        .map(|channel| channel.channel.id.clone())
        .collect::<BTreeSet<_>>();
    let matched_triggers = registry
        .triggers
        .iter()
        .filter(|trigger| {
            event_trigger_matches(
                trigger,
                event,
                adapter_policy,
                &event_type_candidates,
                &matched_channel_ids,
                &matched_adapter_ids,
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let matched_listeners = registry
        .listeners
        .iter()
        .filter(|listener| {
            event_listener_matches(
                listener,
                event,
                adapter_policy,
                &event_type_candidates,
                &matched_channel_ids,
                &matched_adapter_ids,
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let status = if matched_triggers.is_empty()
        && matched_listeners.is_empty()
        && matched_channels.is_empty()
    {
        "event_extensions_unmatched"
    } else {
        "event_extensions_matched"
    };
    let notes = if status == "event_extensions_matched" {
        vec![
            "Addon Event Extensions matched the inbound event without executing handlers."
                .to_string(),
        ]
    } else {
        vec![
            "No Addon Event Extension trigger, listener or channel matched the inbound event."
                .to_string(),
        ]
    };

    InboundEventExtensionMatches {
        schema_version: EVENT_EXTENSION_MATCHES_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        matched_trigger_count: matched_triggers.len(),
        matched_listener_count: matched_listeners.len(),
        matched_channel_count: matched_channels.len(),
        triggers: matched_triggers,
        listeners: matched_listeners,
        channels: matched_channels,
        notes,
    }
}

fn build_inbound_event_workflow_activation_plan(
    catalog: &AddonCatalog,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    matches: &InboundEventExtensionMatches,
) -> InboundEventWorkflowActivationPlan {
    let mut activations = Vec::new();
    let mut seen = BTreeSet::new();

    for trigger in &matches.triggers {
        let activation =
            event_workflow_activation_from_trigger(catalog, event, adapter_policy, trigger);
        if seen.insert(activation.id.clone()) {
            activations.push(activation);
        }
    }
    for listener in &matches.listeners {
        let activation =
            event_workflow_activation_from_listener(catalog, event, adapter_policy, listener);
        if seen.insert(activation.id.clone()) {
            activations.push(activation);
        }
    }

    activations.sort_by(|left, right| {
        event_workflow_activation_status_rank(left.dispatch_allowed)
            .cmp(&event_workflow_activation_status_rank(
                right.dispatch_allowed,
            ))
            .then_with(|| left.addon_id.cmp(&right.addon_id))
            .then_with(|| left.source_kind.cmp(&right.source_kind))
            .then_with(|| left.source_id.cmp(&right.source_id))
    });

    let dispatch_ready_count = activations
        .iter()
        .filter(|activation| activation.dispatch_allowed)
        .count();
    let blocked_count = activations.len().saturating_sub(dispatch_ready_count);
    let status = if activations.is_empty() {
        "workflow_activation_unmatched"
    } else if dispatch_ready_count == activations.len() {
        "workflow_activation_ready"
    } else if dispatch_ready_count > 0 {
        "workflow_activation_partially_ready"
    } else {
        "workflow_activation_blocked"
    };
    let next_commands = activations
        .iter()
        .flat_map(|activation| activation.dispatch_commands.clone())
        .collect::<Vec<_>>();
    let notes = event_workflow_activation_plan_notes(status, &activations);

    InboundEventWorkflowActivationPlan {
        schema_version: EVENT_WORKFLOW_ACTIVATION_PLAN_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        activation_count: activations.len(),
        dispatch_ready_count,
        blocked_count,
        activations,
        next_commands,
        notes,
    }
}

fn event_workflow_activation_from_trigger(
    catalog: &AddonCatalog,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    trigger: &AddonEventTriggerView,
) -> InboundEventWorkflowActivation {
    let activation = EventWorkflowActivationInput {
        source_kind: "trigger",
        source_id: &trigger.trigger.id,
        addon_id: &trigger.addon_id,
        addon_name: &trigger.addon_name,
        addon_version: &trigger.addon_version,
        addon_lifecycle: &trigger.addon_lifecycle,
        capability_id: &trigger.trigger.capability_id,
        workflow_extension_id: &trigger.trigger.workflow_extension_id,
        event_type: &trigger.trigger.event_type,
        channel: &trigger.trigger.channel,
        adapter_id: &trigger.trigger.adapter_id,
        runtime_contract_id: None,
        permission_gate: &trigger.permission_gate,
    };
    event_workflow_activation(catalog, event, adapter_policy, activation)
}

fn event_workflow_activation_from_listener(
    catalog: &AddonCatalog,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    listener: &AddonEventListenerView,
) -> InboundEventWorkflowActivation {
    let runtime_contract_id = normalize_text(Some(&listener.listener.runtime_contract_id));
    let activation = EventWorkflowActivationInput {
        source_kind: "listener",
        source_id: &listener.listener.id,
        addon_id: &listener.addon_id,
        addon_name: &listener.addon_name,
        addon_version: &listener.addon_version,
        addon_lifecycle: &listener.addon_lifecycle,
        capability_id: &listener.listener.capability_id,
        workflow_extension_id: &listener.listener.workflow_extension_id,
        event_type: &listener.listener.event_type,
        channel: &listener.listener.channel,
        adapter_id: &listener.listener.adapter_id,
        runtime_contract_id: runtime_contract_id.as_deref(),
        permission_gate: &listener.permission_gate,
    };
    event_workflow_activation(catalog, event, adapter_policy, activation)
}

struct EventWorkflowActivationInput<'a> {
    source_kind: &'a str,
    source_id: &'a str,
    addon_id: &'a str,
    addon_name: &'a str,
    addon_version: &'a str,
    addon_lifecycle: &'a str,
    capability_id: &'a str,
    workflow_extension_id: &'a str,
    event_type: &'a str,
    channel: &'a str,
    adapter_id: &'a str,
    runtime_contract_id: Option<&'a str>,
    permission_gate: &'a AddonPermissionGate,
}

fn event_workflow_activation(
    catalog: &AddonCatalog,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    input: EventWorkflowActivationInput<'_>,
) -> InboundEventWorkflowActivation {
    let runtime_contracts = event_workflow_activation_runtime_contracts(catalog, &input);
    let mut issues = Vec::new();
    if !input.permission_gate.allowed {
        issues.push(format!(
            "event {} permission gate denied activation: {}",
            input.source_kind, input.permission_gate.status
        ));
    }
    if runtime_contracts.is_empty() {
        issues.push("no runtime contract matched this event workflow activation".to_string());
    }
    for contract in runtime_contracts
        .iter()
        .filter(|contract| !contract.dispatch_allowed)
    {
        issues.push(format!(
            "runtime contract {} is blocked: {}",
            contract.contract_id, contract.status
        ));
        issues.extend(contract.issues.clone());
    }
    let dispatch_allowed = input.permission_gate.allowed
        && runtime_contracts
            .iter()
            .any(|contract| contract.dispatch_allowed);
    let dispatch_commands = if input.permission_gate.allowed {
        runtime_contracts
            .iter()
            .filter(|contract| contract.dispatch_allowed)
            .map(|contract| {
                event_workflow_activation_dispatch_command(event, adapter_policy, &input, contract)
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let reason = if dispatch_allowed {
        "matched Addon Event Extension can dispatch a runtime contract".to_string()
    } else {
        "matched Addon Event Extension is not dispatch-ready".to_string()
    };

    InboundEventWorkflowActivation {
        id: format!(
            "{}:{}:{}",
            input.addon_id, input.source_kind, input.source_id
        ),
        source_kind: input.source_kind.to_string(),
        source_id: input.source_id.to_string(),
        addon_id: input.addon_id.to_string(),
        addon_name: input.addon_name.to_string(),
        addon_version: input.addon_version.to_string(),
        addon_lifecycle: input.addon_lifecycle.to_string(),
        capability_id: input.capability_id.to_string(),
        workflow_extension_id: input.workflow_extension_id.to_string(),
        event_type: input.event_type.to_string(),
        channel: input.channel.to_string(),
        adapter_id: input.adapter_id.to_string(),
        normalized_action: adapter_policy.normalized_action.clone(),
        operation: adapter_policy.normalized_action.clone(),
        permission_gate: input.permission_gate.clone(),
        runtime_contract_count: runtime_contracts.len(),
        dispatch_allowed,
        runtime_contracts,
        dispatch_commands,
        issues,
        reason,
    }
}

fn event_workflow_activation_runtime_contracts(
    catalog: &AddonCatalog,
    input: &EventWorkflowActivationInput<'_>,
) -> Vec<AddonRuntimeContractPolicyEntry> {
    let capability_filter = normalize_text(Some(input.capability_id));
    let contract_filter = input.runtime_contract_id.and_then(|contract_id| {
        normalize_text(Some(contract_id)).filter(|contract_id| !contract_id.is_empty())
    });
    let policy = evaluate_addon_runtime_contract_policy(
        catalog,
        Some(input.addon_id),
        contract_filter.as_deref(),
        None,
        if contract_filter.is_some() {
            None
        } else {
            capability_filter.as_deref()
        },
        None,
    );
    let workflow_extension_filter = normalize_text(Some(input.workflow_extension_id));

    let mut contracts = policy
        .contracts
        .into_iter()
        .filter(|contract| {
            if let Some(filter) = contract_filter.as_deref() {
                return contract.contract_id == filter;
            }
            workflow_extension_filter
                .as_deref()
                .map(|workflow_extension| {
                    contract.contract.workflow_extension_id == workflow_extension
                })
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    contracts.sort_by(|left, right| {
        event_workflow_contract_status_rank(left.dispatch_allowed)
            .cmp(&event_workflow_contract_status_rank(right.dispatch_allowed))
            .then_with(|| left.contract_type.cmp(&right.contract_type))
            .then_with(|| left.contract_id.cmp(&right.contract_id))
    });
    contracts
}

fn event_workflow_activation_dispatch_command(
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    activation: &EventWorkflowActivationInput<'_>,
    contract: &AddonRuntimeContractPolicyEntry,
) -> Vec<String> {
    let input = json!({
        "schema_version": "forge.event_workflow_activation.v1",
        "event_id": event.id,
        "origin": event.origin,
        "action": event.action,
        "normalized_action": adapter_policy.normalized_action,
        "source_kind": activation.source_kind,
        "source_id": activation.source_id,
        "addon_id": activation.addon_id,
        "capability_id": activation.capability_id,
        "workflow_extension_id": activation.workflow_extension_id,
        "event_type": activation.event_type,
        "channel": activation.channel,
        "adapter_id": activation.adapter_id,
        "contract_id": contract.contract_id,
        "contract_type": contract.contract_type,
    });
    vec![
        "forge".to_string(),
        "addons".to_string(),
        "dispatch-contract".to_string(),
        "--addon".to_string(),
        activation.addon_id.to_string(),
        "--contract".to_string(),
        contract.contract_id.clone(),
        "--source".to_string(),
        format!("event_inbox:{}", event.id),
        "--input".to_string(),
        serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string()),
        "--output".to_string(),
        "json".to_string(),
    ]
}

fn event_workflow_activation_plan_notes(
    status: &str,
    activations: &[InboundEventWorkflowActivation],
) -> Vec<String> {
    match status {
        "workflow_activation_unmatched" => vec![
            "No matched Addon Event Extension declared a workflow activation.".to_string(),
        ],
        "workflow_activation_ready" => vec![
            "Matched Addon Event Extensions are ready to dispatch runtime contracts; Forge did not execute handlers inline.".to_string(),
        ],
        "workflow_activation_partially_ready" => vec![format!(
            "{} event workflow activation(s) need permission or contract repair before dispatch.",
            activations
                .iter()
                .filter(|activation| !activation.dispatch_allowed)
                .count()
        )],
        _ => vec![
            "Matched Addon Event Extensions are blocked by permission gates or missing runtime contracts.".to_string(),
        ],
    }
}

fn event_workflow_activation_status_rank(dispatch_allowed: bool) -> usize {
    if dispatch_allowed {
        0
    } else {
        1
    }
}

fn event_workflow_contract_status_rank(dispatch_allowed: bool) -> usize {
    if dispatch_allowed {
        0
    } else {
        1
    }
}

fn inbound_event_type_candidates(
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    adapters: &[InboundEventAddonAdapterPlanEntry],
) -> BTreeSet<String> {
    let mut candidates = BTreeSet::new();
    if let Some(schema) = adapter_policy.schema.as_deref() {
        insert_normalized_candidate(&mut candidates, schema);
    }
    for key in ["schema", "event_schema", "event_type"] {
        if let Some(value) = extract_string(&event.data, &[key]) {
            insert_normalized_candidate(&mut candidates, &value);
        }
    }
    for adapter in adapters
        .iter()
        .filter(|adapter| adapter.status == "matched")
    {
        if let Some(schema) = adapter.schema.as_deref() {
            insert_normalized_candidate(&mut candidates, schema);
        }
        for event_type in &adapter.event_types {
            insert_normalized_candidate(&mut candidates, event_type);
        }
    }
    candidates
}

fn insert_normalized_candidate(candidates: &mut BTreeSet<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        candidates.insert(value.to_ascii_lowercase());
    }
}

fn event_channel_matches(
    channel: &AddonEventChannelView,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    event_type_candidates: &BTreeSet<String>,
) -> bool {
    let channel = &channel.channel;
    let direction_matches = channel.direction.trim().is_empty()
        || text_matches(&channel.direction, "ingress")
        || text_matches(&channel.direction, "bidirectional");
    let origin_matches = string_list_matches(&channel.origins, &event.origin);
    let transport_matches = adapter_policy
        .transport
        .as_deref()
        .map(|transport| {
            channel.transport.trim().is_empty() || text_matches(&channel.transport, transport)
        })
        .unwrap_or(true);
    direction_matches
        && origin_matches
        && transport_matches
        && event_extension_actions_match(
            &channel.actions,
            &event.action,
            &adapter_policy.normalized_action,
        )
        && event_extension_type_matches(&channel.event_types, event_type_candidates)
        && event_extension_schema_matches(&channel.schema, event_type_candidates)
}

fn event_trigger_matches(
    trigger: &AddonEventTriggerView,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    event_type_candidates: &BTreeSet<String>,
    matched_channel_ids: &BTreeSet<String>,
    matched_adapter_ids: &BTreeSet<String>,
) -> bool {
    let trigger = &trigger.trigger;
    event_extension_single_type_matches(&trigger.event_type, event_type_candidates)
        && event_extension_reference_matches(&trigger.channel, matched_channel_ids)
        && event_extension_reference_matches(&trigger.adapter_id, matched_adapter_ids)
        && event_extension_actions_match(
            &trigger.actions,
            &event.action,
            &adapter_policy.normalized_action,
        )
}

fn event_listener_matches(
    listener: &AddonEventListenerView,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    event_type_candidates: &BTreeSet<String>,
    matched_channel_ids: &BTreeSet<String>,
    matched_adapter_ids: &BTreeSet<String>,
) -> bool {
    let listener = &listener.listener;
    event_extension_single_type_matches(&listener.event_type, event_type_candidates)
        && event_extension_reference_matches(&listener.channel, matched_channel_ids)
        && event_extension_reference_matches(&listener.adapter_id, matched_adapter_ids)
        && event_extension_actions_match(
            &listener.actions,
            &event.action,
            &adapter_policy.normalized_action,
        )
}

fn event_extension_actions_match(
    actions: &[String],
    raw_action: &str,
    normalized_action: &str,
) -> bool {
    actions.is_empty()
        || actions.iter().any(|action| {
            text_matches(action, raw_action) || text_matches(action, normalized_action)
        })
}

fn event_extension_type_matches(event_types: &[String], candidates: &BTreeSet<String>) -> bool {
    event_types.is_empty()
        || candidates.is_empty()
        || event_types
            .iter()
            .any(|event_type| candidate_matches(event_type, candidates))
}

fn event_extension_single_type_matches(event_type: &str, candidates: &BTreeSet<String>) -> bool {
    event_type.trim().is_empty()
        || candidates.is_empty()
        || candidate_matches(event_type, candidates)
}

fn event_extension_schema_matches(schema: &str, candidates: &BTreeSet<String>) -> bool {
    schema.trim().is_empty() || candidates.is_empty() || candidate_matches(schema, candidates)
}

fn event_extension_reference_matches(reference: &str, matches: &BTreeSet<String>) -> bool {
    let reference = reference.trim();
    reference.is_empty() || matches.contains(reference)
}

fn candidate_matches(expected: &str, candidates: &BTreeSet<String>) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    expected == "*" || candidates.contains(&expected)
}

fn inbound_event_addon_adapter_plan_entry(
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    adapter: AddonEventAdapterView,
) -> InboundEventAddonAdapterPlanEntry {
    let action_matched =
        adapter_action_matches(&adapter, &event.action, &adapter_policy.normalized_action);
    let schema_matched = adapter_schema_matches(&adapter, adapter_policy.schema.as_deref());
    let permission_gate = adapter.permission_gate.clone();
    let auth_required = auth_requires_verification(&adapter.adapter.auth);
    let auth_verified = adapter_policy.auth_verified;
    let mut issues = Vec::new();
    let mut status = "matched".to_string();
    let mut allowed = true;

    if !action_matched {
        status = "action_not_allowed".to_string();
        allowed = false;
        issues.push("adapter origin matched, but action is not declared".to_string());
    } else if !schema_matched {
        status = "schema_mismatch".to_string();
        allowed = false;
        issues.push(
            "event schema does not match the declared adapter schema or event types".to_string(),
        );
    } else if !permission_gate.allowed {
        status = permission_gate.status.clone();
        allowed = false;
        issues.push(format!(
            "permission gate denied adapter: {}",
            permission_gate.status
        ));
    } else if auth_required {
        match auth_verified {
            Some(true) => {}
            Some(false) => {
                status = "auth_not_verified".to_string();
                allowed = false;
                issues.push("adapter auth was explicitly reported as not verified".to_string());
            }
            None => {
                status = "auth_unverified".to_string();
                issues.push(format!(
                    "adapter declares auth `{}` but event did not include auth_verified evidence",
                    adapter.adapter.auth
                ));
            }
        }
    }

    InboundEventAddonAdapterPlanEntry {
        addon_id: adapter.addon_id,
        addon_name: adapter.addon_name,
        addon_version: adapter.addon_version,
        addon_lifecycle: adapter.addon_lifecycle,
        adapter_id: adapter.adapter.id.clone(),
        adapter_title: adapter.adapter.title.clone(),
        transport: adapter.adapter.transport.clone(),
        direction: adapter.adapter.direction.clone(),
        status,
        allowed,
        source_matched: true,
        action_matched,
        schema_matched,
        auth_required,
        auth_verified,
        mutates_workflow: event_route_action_mutates_workflow(&adapter_policy.normalized_action),
        route_decision: allowed.then(|| adapter_policy.normalized_action.clone()),
        origins: adapter.adapter.origins.clone(),
        actions: adapter.adapter.actions.clone(),
        event_types: adapter.adapter.event_types.clone(),
        schema: normalize_text(Some(&adapter.adapter.schema)),
        permission_gate,
        issues,
    }
}

fn inbound_event_addon_adapter_next_commands(
    event: &InboundEventRecord,
    adapters: &[InboundEventAddonAdapterPlanEntry],
) -> Vec<Vec<String>> {
    let mut commands = vec![
        vec![
            "forge".to_string(),
            "events".to_string(),
            "route".to_string(),
            "--event".to_string(),
            event.id.clone(),
            "--project-root".to_string(),
            "<project-root>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        vec![
            "forge".to_string(),
            "events".to_string(),
            "adapters".to_string(),
            "--direction".to_string(),
            "ingress".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    ];

    for adapter in adapters {
        for permission in &adapter.permission_gate.human_approval_required {
            commands.push(vec![
                "forge".to_string(),
                "addons".to_string(),
                "authorize-permission".to_string(),
                "--addon".to_string(),
                adapter.addon_id.clone(),
                "--permission".to_string(),
                permission.clone(),
                "--risk".to_string(),
                "review".to_string(),
                "--approved-by".to_string(),
                "<operator>".to_string(),
                "--source".to_string(),
                "event-adapter-plan".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]);
        }
    }
    commands
}

fn inbound_event_addon_adapter_plan_notes(
    adapter_policy: &InboundEventAdapterPolicyReport,
    matched_count: usize,
    adapters: &[InboundEventAddonAdapterPlanEntry],
) -> Vec<String> {
    let mut notes = Vec::new();
    if !adapter_policy.enforced {
        notes.push("No Addon ingress adapter claimed this origin; Forge kept legacy inbox routing available.".to_string());
    } else if matched_count > 0 {
        notes.push(
            "Addon ingress adapter candidates are ready for Forge event routing.".to_string(),
        );
    } else if adapters.is_empty() {
        notes.push("No Addon ingress adapter matched the event origin and transport.".to_string());
    } else {
        notes.push("Addon ingress adapter candidates matched the origin but are blocked by action, schema, auth or permission gates.".to_string());
    }
    if !adapter_policy.issues.is_empty() {
        notes.extend(adapter_policy.issues.clone());
    }
    notes
}

fn addon_event_adapter_plan_status_rank(status: &str) -> usize {
    match status {
        "matched" => 0,
        "auth_unverified" => 1,
        "auth_not_verified" => 2,
        "action_not_allowed" => 3,
        "schema_mismatch" => 4,
        _ => 5,
    }
}

fn event_route_action_mutates_workflow(action: &str) -> bool {
    matches!(
        action,
        "start_workflow"
            | "continue_workflow"
            | "modify_workflow"
            | "pause_workflow"
            | "resume_workflow"
            | "end_workflow"
            | "complete_workflow"
    )
}

fn blocked_adapter_policy(
    event: &InboundEventRecord,
    input: BlockedAdapterPolicyInput<'_>,
) -> InboundEventAdapterPolicyReport {
    InboundEventAdapterPolicyReport {
        schema_version: EVENT_ADAPTER_POLICY_SCHEMA_VERSION.to_string(),
        status: input.status.to_string(),
        allowed: false,
        enforced: true,
        origin: event.origin.clone(),
        action: event.action.clone(),
        normalized_action: input.normalized_action,
        transport: input.transport,
        schema: input.schema,
        auth_verified: input.auth_verified,
        issues: vec![input.issue.to_string()],
        matched_adapter: input.matched_adapter,
    }
}

fn adapter_source_matches(
    adapter: &AddonEventAdapterView,
    origin: &str,
    transport: Option<&str>,
) -> bool {
    let origin_matches = string_list_matches(&adapter.adapter.origins, origin);
    let transport_matches = transport
        .map(|transport| text_matches(&adapter.adapter.transport, transport))
        .unwrap_or(true);
    origin_matches && transport_matches
}

fn adapter_action_matches(
    adapter: &AddonEventAdapterView,
    raw_action: &str,
    normalized_action: &str,
) -> bool {
    adapter.adapter.actions.is_empty()
        || adapter.adapter.actions.iter().any(|action| {
            text_matches(action, raw_action) || text_matches(action, normalized_action)
        })
}

fn adapter_schema_matches(adapter: &AddonEventAdapterView, schema: Option<&str>) -> bool {
    let Some(schema) = schema.map(str::trim).filter(|schema| !schema.is_empty()) else {
        return true;
    };
    adapter.adapter.schema.trim().is_empty()
        || text_matches(&adapter.adapter.schema, schema)
        || adapter
            .adapter
            .event_types
            .iter()
            .any(|event_type| text_matches(event_type, schema))
}

fn auth_requires_verification(auth: &str) -> bool {
    let auth = auth.trim();
    !auth.is_empty()
        && !["none", "forge_policy"]
            .iter()
            .any(|allowed| auth.eq_ignore_ascii_case(allowed))
}

fn auth_is_hmac(auth: &str) -> bool {
    auth.trim().eq_ignore_ascii_case("hmac") || auth.trim().eq_ignore_ascii_case("hmac_sha256")
}

fn auth_is_bearer(auth: &str) -> bool {
    auth.trim().eq_ignore_ascii_case("bearer") || auth.trim().eq_ignore_ascii_case("bearer_token")
}

fn auth_is_bot_token(auth: &str) -> bool {
    auth.trim().eq_ignore_ascii_case("bot_token")
        || auth.trim().eq_ignore_ascii_case("telegram_bot_token")
}

fn transport_is_telegram(transport: &str) -> bool {
    transport.trim().eq_ignore_ascii_case("telegram")
        || transport.trim().eq_ignore_ascii_case("telegram_bot_api")
}

fn string_list_matches(values: &[String], expected: &str) -> bool {
    values.is_empty()
        || values
            .iter()
            .any(|value| value == "*" || text_matches(value, expected))
}

fn text_matches(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn route_start_workflow(
    store: &ForgeStore,
    event: InboundEventRecord,
    project_root: &Path,
    addon_catalog: &AddonCatalog,
    adapter_policy: InboundEventAdapterPolicyReport,
    addon_event_adapter_plan: InboundEventAddonAdapterPlan,
) -> Result<InboundEventRouteReport> {
    let goal = extract_goal(&event.data)
        .with_context(|| format!("inbound event {} does not include a goal", event.id))?;
    let operating_context = load_project_operating_context(project_root)?;
    ensure_operating_context_policy(store, &operating_context, "route start_workflow")?;
    let mut workflow = create_workflow(parse_intent_with_catalog_and_context(
        &goal,
        addon_catalog,
        operating_context,
    ));
    workflow.status = "planned".to_string();
    store.save_workflow(&workflow)?;
    record_inbound_event_routed(store, &workflow, &event, &adapter_policy, "start_workflow")?;

    let routed_data = enrich_event_data(
        event.data.clone(),
        &workflow.id,
        &workflow.goal,
        project_root.display().to_string(),
    );
    store.update_inbound_event_status(&event.id, "routed", &routed_data)?;
    let routed_event = store.load_inbound_event(&event.id)?;

    Ok(InboundEventRouteReport {
        schema_version: EVENT_ROUTE_SCHEMA_VERSION.to_string(),
        status: "event_routed".to_string(),
        event_id: routed_event.id.clone(),
        action: routed_event.action.clone(),
        origin: routed_event.origin.clone(),
        adapter_policy,
        addon_event_adapter_plan,
        route_decision: "start_workflow".to_string(),
        workflow_id: Some(workflow.id.clone()),
        workflow_goal: Some(workflow.goal.clone()),
        created_workflow: Some(workflow),
        route_result: None,
        event: inbound_event_view(store, routed_event),
    })
}

fn route_modify_workflow(
    store: &ForgeStore,
    event: InboundEventRecord,
    adapter_policy: InboundEventAdapterPolicyReport,
    addon_event_adapter_plan: InboundEventAddonAdapterPlan,
) -> Result<InboundEventRouteReport> {
    let workflow_id = extract_workflow_id(&event.data)
        .with_context(|| format!("inbound event {} does not include a workflow_id", event.id))?;
    let origin = format!("event_inbox:{}", event.origin);
    let expected_revision = extract_u64(&event.data, &["expected_revision"])?;
    let mutation = extract_string(&event.data, &["mutation"])
        .map(|value| normalize_workflow_mutation(&value))
        .unwrap_or_else(|| "update_goal".to_string());
    let revision = execute_workflow_mutation(
        store,
        &event,
        &workflow_id,
        &origin,
        &mutation,
        expected_revision,
    )?;
    let workflow = store.load_workflow(&workflow_id)?;
    let route_decision = format!("modify_workflow revision {revision} mutation {mutation}");
    record_inbound_event_routed(store, &workflow, &event, &adapter_policy, &route_decision)?;
    let routed_data = enrich_event_data(
        event.data.clone(),
        &workflow.id,
        &workflow.goal,
        "existing_workflow".to_string(),
    );
    store.update_inbound_event_status(&event.id, "routed", &routed_data)?;
    let routed_event = store.load_inbound_event(&event.id)?;

    Ok(InboundEventRouteReport {
        schema_version: EVENT_ROUTE_SCHEMA_VERSION.to_string(),
        status: "event_routed".to_string(),
        event_id: routed_event.id.clone(),
        action: routed_event.action.clone(),
        origin: routed_event.origin.clone(),
        adapter_policy,
        addon_event_adapter_plan,
        route_decision,
        workflow_id: Some(workflow.id.clone()),
        workflow_goal: Some(workflow.goal.clone()),
        created_workflow: Some(workflow),
        route_result: Some(json!({
            "mutation": mutation,
            "revision": revision,
        })),
        event: inbound_event_view(store, routed_event),
    })
}

fn execute_workflow_mutation(
    store: &ForgeStore,
    event: &InboundEventRecord,
    workflow_id: &str,
    origin: &str,
    mutation: &str,
    expected_revision: Option<u64>,
) -> Result<u64> {
    match mutation {
        "update_goal" => {
            let new_goal = extract_goal(&event.data).with_context(|| {
                format!("inbound event {} does not include a new goal", event.id)
            })?;
            Ok(update_workflow_goal_with_expected_revision(
                store,
                workflow_id,
                &new_goal,
                origin,
                expected_revision,
            )?
            .revision)
        }
        "add_task" => {
            let description = extract_string(&event.data, &["description", "task_description"])
                .with_context(|| {
                    format!(
                        "inbound event {} add_task does not include description",
                        event.id
                    )
                })?;
            Ok(add_workflow_task(
                store,
                workflow_id,
                WorkflowTaskAddInput {
                    task_id: extract_string(&event.data, &["task_id"]),
                    description,
                    priority: extract_string(&event.data, &["priority"])
                        .unwrap_or_else(|| "medium".to_string()),
                    origin: origin.to_string(),
                    expected_revision,
                },
            )?
            .revision)
        }
        "update_task" => {
            let task_id = required_modify_workflow_task_id(event)?;
            let title = extract_string(&event.data, &["title"]);
            let goal = extract_string(&event.data, &["task_goal", "goal"]);
            let expected_output = extract_string(&event.data, &["expected_output"]);
            Ok(update_workflow_task_with_expected_revision(
                store,
                workflow_id,
                WorkflowTaskUpdateInput {
                    task_id: &task_id,
                    title: title.as_deref(),
                    goal: goal.as_deref(),
                    expected_output: expected_output.as_deref(),
                    origin,
                },
                expected_revision,
            )?
            .revision)
        }
        "set_priority" => {
            let task_id = required_modify_workflow_task_id(event)?;
            let priority = extract_string(&event.data, &["priority"]).with_context(|| {
                format!(
                    "inbound event {} set_priority does not include priority",
                    event.id
                )
            })?;
            Ok(set_workflow_task_priority(
                store,
                workflow_id,
                WorkflowTaskPriorityInput {
                    task_id,
                    priority,
                    origin: origin.to_string(),
                    expected_revision,
                },
            )?
            .revision)
        }
        "add_dependency" | "remove_dependency" => {
            let task_id = required_modify_workflow_task_id(event)?;
            let dependency_task_id =
                extract_string(&event.data, &["depends_on", "dependency_task_id"]).with_context(
                    || {
                        format!(
                            "inbound event {} {mutation} does not include depends_on",
                            event.id
                        )
                    },
                )?;
            let input = WorkflowTaskDependencyInput {
                task_id,
                dependency_task_id,
                origin: origin.to_string(),
                expected_revision,
            };
            if mutation == "add_dependency" {
                Ok(add_workflow_task_dependency(store, workflow_id, input)?.revision)
            } else {
                Ok(remove_workflow_task_dependency(store, workflow_id, input)?.revision)
            }
        }
        "set_impediment" => {
            let task_id = required_modify_workflow_task_id(event)?;
            let reason = extract_string(&event.data, &["reason"]).with_context(|| {
                format!(
                    "inbound event {} set_impediment does not include reason",
                    event.id
                )
            })?;
            Ok(set_workflow_task_impediment(
                store,
                workflow_id,
                WorkflowTaskImpedimentInput {
                    task_id,
                    reason,
                    kind: extract_string(&event.data, &["kind"])
                        .unwrap_or_else(|| "manual".to_string()),
                    origin: origin.to_string(),
                    expected_revision,
                },
            )?
            .revision)
        }
        "clear_impediment" => {
            let task_id = required_modify_workflow_task_id(event)?;
            Ok(clear_workflow_task_impediment(
                store,
                workflow_id,
                WorkflowTaskImpedimentClearInput {
                    task_id,
                    impediment_id: extract_string(&event.data, &["impediment_id", "impediment"]),
                    origin: origin.to_string(),
                    expected_revision,
                },
            )?
            .revision)
        }
        _ => bail!(
            "inbound event {} uses unsupported workflow mutation `{mutation}`",
            event.id
        ),
    }
}

fn required_modify_workflow_task_id(event: &InboundEventRecord) -> Result<String> {
    extract_string(&event.data, &["task_id"]).with_context(|| {
        format!(
            "inbound event {} workflow mutation does not include task_id",
            event.id
        )
    })
}

fn normalize_workflow_mutation(value: &str) -> String {
    match value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
        .as_str()
    {
        "goal" | "change_goal" | "workflow_goal" => "update_goal".to_string(),
        other => other.to_string(),
    }
}

fn extract_u64(data: &Value, keys: &[&str]) -> Result<Option<u64>> {
    for candidate in value_candidates(data) {
        for key in keys {
            let Some(value) = candidate.get(*key) else {
                continue;
            };
            if value.is_null() {
                continue;
            }
            if let Some(number) = value.as_u64() {
                return Ok(Some(number));
            }
            if let Some(number) = value
                .as_str()
                .map(str::trim)
                .and_then(|value| value.parse::<u64>().ok())
            {
                return Ok(Some(number));
            }
            bail!("field `{key}` must be a non-negative integer");
        }
    }
    Ok(None)
}

fn extract_i32(data: &Value, keys: &[&str]) -> Result<Option<i32>> {
    for candidate in value_candidates(data) {
        for key in keys {
            let Some(value) = candidate.get(*key) else {
                continue;
            };
            if value.is_null() {
                continue;
            }
            let number = value
                .as_i64()
                .or_else(|| {
                    value
                        .as_str()
                        .map(str::trim)
                        .and_then(|value| value.parse::<i64>().ok())
                })
                .with_context(|| format!("field `{key}` must be an integer"))?;
            return i32::try_from(number)
                .map(Some)
                .with_context(|| format!("field `{key}` is outside the i32 range"));
        }
    }
    Ok(None)
}

fn route_continue_workflow(
    store: &ForgeStore,
    event: InboundEventRecord,
    adapter_policy: InboundEventAdapterPolicyReport,
    addon_event_adapter_plan: InboundEventAddonAdapterPlan,
) -> Result<InboundEventRouteReport> {
    let continue_action = continue_action_for_data(&event.data)?;
    let origin = format!("event_inbox:{}", event.origin);
    let route_result = match continue_action.as_str() {
        "attach_artifact" => continue_attach_artifact(store, &event.data, &origin)?,
        "checkpoint" => continue_checkpoint(store, &event.data, &origin)?,
        "answer_interaction" => continue_answer_interaction(store, &event.data, &origin)?,
        "complete_task" => continue_complete_task(store, &event.data, &origin)?,
        "drive_run" => continue_drive_run(store, &event.data, &origin)?,
        other => bail!("unsupported continue_workflow action: {other}"),
    };
    let workflow_id = route_result
        .get("workflow_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| workflow_id_for_event_data(store, &event.data).ok())
        .with_context(|| {
            format!(
                "continue_workflow event {} did not resolve a workflow id",
                event.id
            )
        })?;
    let workflow = store.load_workflow(&workflow_id)?;
    let route_decision = format!("continue_workflow:{continue_action}");
    record_inbound_event_routed(store, &workflow, &event, &adapter_policy, &route_decision)?;
    let routed_data = enrich_event_data(
        event.data.clone(),
        &workflow.id,
        &workflow.goal,
        "existing_workflow".to_string(),
    );
    store.update_inbound_event_status(&event.id, "routed", &routed_data)?;
    let routed_event = store.load_inbound_event(&event.id)?;

    Ok(InboundEventRouteReport {
        schema_version: EVENT_ROUTE_SCHEMA_VERSION.to_string(),
        status: "event_routed".to_string(),
        event_id: routed_event.id.clone(),
        action: routed_event.action.clone(),
        origin: routed_event.origin.clone(),
        adapter_policy,
        addon_event_adapter_plan,
        route_decision,
        workflow_id: Some(workflow.id.clone()),
        workflow_goal: Some(workflow.goal.clone()),
        created_workflow: Some(workflow),
        route_result: Some(route_result),
        event: inbound_event_view(store, routed_event),
    })
}

fn route_status_workflow(
    store: &ForgeStore,
    event: InboundEventRecord,
    route_decision: &str,
    adapter_policy: InboundEventAdapterPolicyReport,
    addon_event_adapter_plan: InboundEventAddonAdapterPlan,
) -> Result<InboundEventRouteReport> {
    let workflow_id = extract_workflow_id(&event.data)
        .with_context(|| format!("inbound event {} does not include a workflow_id", event.id))?;
    let origin = format!("event_inbox:{}", event.origin);
    let status_report = match route_decision {
        "pause_workflow" => pause_workflow(store, &workflow_id, &origin)?,
        "resume_workflow" => resume_workflow(store, &workflow_id, &origin)?,
        "complete_workflow" => complete_workflow(store, &workflow_id, &origin)?,
        other => bail!("unsupported status workflow route: {other}"),
    };
    let workflow = store.load_workflow(&workflow_id)?;
    let route_decision = format!("{route_decision} revision {}", status_report.revision);
    record_inbound_event_routed(store, &workflow, &event, &adapter_policy, &route_decision)?;
    let routed_data = enrich_event_data(
        event.data.clone(),
        &workflow.id,
        &workflow.goal,
        "existing_workflow".to_string(),
    );
    store.update_inbound_event_status(&event.id, "routed", &routed_data)?;
    let routed_event = store.load_inbound_event(&event.id)?;

    Ok(InboundEventRouteReport {
        schema_version: EVENT_ROUTE_SCHEMA_VERSION.to_string(),
        status: "event_routed".to_string(),
        event_id: routed_event.id.clone(),
        action: routed_event.action.clone(),
        origin: routed_event.origin.clone(),
        adapter_policy,
        addon_event_adapter_plan,
        route_decision,
        workflow_id: Some(workflow.id.clone()),
        workflow_goal: Some(workflow.goal.clone()),
        created_workflow: Some(workflow),
        route_result: Some(serde_json::to_value(&status_report)?),
        event: inbound_event_view(store, routed_event),
    })
}

fn continue_attach_artifact(store: &ForgeStore, data: &Value, origin: &str) -> Result<Value> {
    let workflow_id = workflow_id_for_event_data(store, data)?;
    let path = extract_string(data, &["artifact_path", "path"])
        .with_context(|| "continue_workflow attach_artifact requires artifact_path or path")?;
    let kind = extract_string(data, &["kind", "artifact_kind"])
        .unwrap_or_else(|| "event_artifact".to_string());
    serde_json::to_value(attach_workflow_artifact(
        store,
        &workflow_id,
        &PathBuf::from(path),
        &kind,
        origin,
    )?)
    .map_err(Into::into)
}

fn continue_checkpoint(store: &ForgeStore, data: &Value, origin: &str) -> Result<Value> {
    let workflow_id = workflow_id_for_event_data(store, data)?;
    let task_id =
        extract_string(data, &["task_id"]).with_context(|| "checkpoint requires task_id")?;
    let workflow = store.load_workflow(&workflow_id)?;
    let workflow_revision = data
        .get("workflow_revision")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            workflow
                .revisions
                .last()
                .map(|revision| revision.revision)
                .unwrap_or(0)
        });
    let state = extract_string(data, &["checkpoint_state", "state"])
        .unwrap_or_else(|| "continued".to_string());
    let summary = extract_string(data, &["summary"])
        .with_context(|| "checkpoint continue action requires summary")?;
    let context_sha256 = extract_string(data, &["context_sha256"])
        .with_context(|| "checkpoint continue action requires context_sha256")?;
    let executor = extract_string(data, &["executor"]).unwrap_or_else(|| origin.to_string());
    let context_routing_cache_key = extract_string(data, &["context_routing_cache_key"]);
    serde_json::to_value(record_task_checkpoint(
        store,
        TaskCheckpointRequest {
            workflow_id: &workflow_id,
            task_id: &task_id,
            executor: &executor,
            state: &state,
            summary: &summary,
            context_sha256: &context_sha256,
            context_routing_cache_key: context_routing_cache_key.as_deref(),
            workflow_revision,
        },
    )?)
    .map_err(Into::into)
}

fn continue_answer_interaction(store: &ForgeStore, data: &Value, origin: &str) -> Result<Value> {
    let workflow_id = workflow_id_for_event_data(store, data)?;
    let task_id = extract_string(data, &["task_id"])
        .with_context(|| "answer_interaction requires task_id")?;
    let selected_options = extract_string_array(data, "selected_options")?;
    let field_values = extract_string_array(data, "field_values")?;
    let rationale = extract_string(data, &["rationale"]);
    serde_json::to_value(answer_human_interaction(
        store,
        &workflow_id,
        &task_id,
        &selected_options,
        &field_values,
        rationale.as_deref(),
        origin,
    )?)
    .map_err(Into::into)
}

fn continue_complete_task(store: &ForgeStore, data: &Value, origin: &str) -> Result<Value> {
    let run_id =
        extract_string(data, &["run_id"]).with_context(|| "complete_task requires run_id")?;
    let task_id =
        extract_string(data, &["task_id"]).with_context(|| "complete_task requires task_id")?;
    let summary =
        extract_string(data, &["summary"]).with_context(|| "complete_task requires summary")?;
    let executor = extract_string(data, &["executor"]).unwrap_or_else(|| origin.to_string());
    let artifacts = extract_string_array(data, "artifacts")?
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let evidence_command = extract_string(data, &["evidence_command"]);
    let evidence_exit_code = extract_i32(data, &["evidence_exit_code"])?;
    let evidence_summary = extract_string(data, &["evidence_summary"]);
    serde_json::to_value(complete_ready_task(
        store,
        &run_id,
        RequestTaskCompletionInput {
            task_id: &task_id,
            executor: &executor,
            summary: &summary,
            artifact_paths: &artifacts,
            evidence_command: evidence_command.as_deref(),
            evidence_exit_code,
            evidence_summary: evidence_summary.as_deref(),
            estimated_usd: data
                .get("estimated_usd")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            tokens_in: data.get("tokens_in").and_then(Value::as_i64).unwrap_or(0),
            tokens_out: data.get("tokens_out").and_then(Value::as_i64).unwrap_or(0),
            ttl_seconds: data
                .get("ttl_seconds")
                .and_then(Value::as_u64)
                .unwrap_or(300),
            context_budget: data
                .get("context_budget")
                .and_then(Value::as_u64)
                .map(|value| value as usize),
            origin,
        },
    )?)
    .map_err(Into::into)
}

fn continue_drive_run(store: &ForgeStore, data: &Value, origin: &str) -> Result<Value> {
    let run_id = extract_string(data, &["run_id"]).with_context(|| "drive_run requires run_id")?;
    let executor = extract_string(data, &["executor"]).unwrap_or_else(|| origin.to_string());
    let ttl_seconds = data
        .get("ttl_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(300);
    serde_json::to_value(drive_request(
        store,
        &run_id,
        &executor,
        ttl_seconds,
        origin,
    )?)
    .map_err(Into::into)
}

fn enrich_event_data(
    mut data: Value,
    workflow_id: &str,
    workflow_goal: &str,
    project_root: String,
) -> Value {
    let route = json!({
        "workflow_id": workflow_id,
        "workflow_goal": workflow_goal,
        "project_root": project_root,
    });
    match &mut data {
        Value::Object(map) => {
            map.insert("route".to_string(), route);
            map.insert(
                "workflow_id".to_string(),
                Value::String(workflow_id.to_string()),
            );
            map.insert(
                "workflow_goal".to_string(),
                Value::String(workflow_goal.to_string()),
            );
            data
        }
        _ => json!({
            "payload": data,
            "route": route,
            "workflow_id": workflow_id,
            "workflow_goal": workflow_goal,
        }),
    }
}

fn record_inbound_event_routed(
    store: &ForgeStore,
    workflow: &Workflow,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    route_decision: &str,
) -> Result<()> {
    let data =
        inbound_event_routed_runtime_data(store, workflow, event, adapter_policy, route_decision);
    store.record_event(&workflow.id, "inbound_event_routed", &data)?;
    Ok(())
}

fn inbound_event_routed_runtime_data(
    store: &ForgeStore,
    workflow: &Workflow,
    event: &InboundEventRecord,
    adapter_policy: &InboundEventAdapterPolicyReport,
    route_decision: &str,
) -> Value {
    let matched_adapter = adapter_policy.matched_adapter.as_ref();
    let addon_id = matched_adapter.map(|adapter| adapter.addon_id.clone());
    let adapter_id = matched_adapter.map(|adapter| adapter.adapter.id.clone());
    let direction = matched_adapter
        .map(|adapter| adapter.adapter.direction.clone())
        .filter(|direction| !direction.trim().is_empty())
        .unwrap_or_else(|| "ingress".to_string());
    let transport = adapter_policy.transport.clone().or_else(|| {
        matched_adapter
            .map(|adapter| adapter.adapter.transport.clone())
            .filter(|transport| !transport.trim().is_empty())
    });
    let event_type = adapter_policy.schema.clone().or_else(|| {
        matched_adapter.and_then(|adapter| adapter.adapter.event_types.first().cloned())
    });
    json!({
        "event_id": event.id,
        "origin": event.origin,
        "action": event.action,
        "workflow_id": workflow.id,
        "goal": workflow.goal,
        "source": "event_inbox",
        "route_decision": route_decision,
        "addon_id": addon_id,
        "adapter_id": adapter_id,
        "direction": direction,
        "transport": transport,
        "event_type": event_type,
        "schema": adapter_policy.schema,
        "identity_context": inbound_event_identity_context(store, event),
        "adapter_policy": adapter_policy,
    })
}

fn extract_goal(data: &Value) -> Option<String> {
    extract_string(data, &["new_goal", "goal"])
        .or_else(|| {
            data.get("payload")
                .and_then(|payload| extract_string(payload, &["new_goal", "goal"]))
        })
        .or_else(|| {
            data.get("message")
                .and_then(|payload| extract_string(payload, &["new_goal", "goal"]))
        })
}

fn extract_workflow_id(data: &Value) -> Option<String> {
    extract_string(data, &["workflow_id"])
        .or_else(|| {
            data.get("payload")
                .and_then(|payload| extract_string(payload, &["workflow_id"]))
        })
        .or_else(|| {
            data.get("message")
                .and_then(|payload| extract_string(payload, &["workflow_id"]))
        })
}

fn workflow_id_for_event_data(store: &ForgeStore, data: &Value) -> Result<String> {
    if let Some(workflow_id) = extract_workflow_id(data) {
        return Ok(workflow_id);
    }
    let run_id = extract_string(data, &["run_id"])
        .with_context(|| "continue_workflow requires workflow_id or run_id")?;
    Ok(load_run_record(store, &run_id)?.workflow_id)
}

fn continue_action_for_data(data: &Value) -> Result<String> {
    if let Some(action) = extract_string(data, &["continue_action", "route_action", "kind"]) {
        let action = normalized_continue_action(&action);
        if action != "continue_workflow" {
            return Ok(action);
        }
    }
    if has_any_key(data, &["artifact_path", "path"]) {
        return Ok("attach_artifact".to_string());
    }
    if has_any_key(data, &["context_sha256", "checkpoint_state"]) {
        return Ok("checkpoint".to_string());
    }
    if has_any_key(data, &["selected_options", "field_values"]) {
        return Ok("answer_interaction".to_string());
    }
    if has_any_key(data, &["run_id"]) {
        if has_any_key(data, &["task_id"]) && has_any_key(data, &["summary"]) {
            return Ok("complete_task".to_string());
        }
        return Ok("drive_run".to_string());
    }
    bail!("continue_workflow event requires continue_action or recognizable continuation fields")
}

fn normalized_continue_action(action: &str) -> String {
    match action
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "continue" | "continue_workflow" => "continue_workflow".to_string(),
        "artifact" | "attach" | "attach_artifact" => "attach_artifact".to_string(),
        "checkpoint" | "record_checkpoint" | "task_checkpoint" => "checkpoint".to_string(),
        "answer" | "answer_human" | "human_answer" | "answer_interaction" => {
            "answer_interaction".to_string()
        }
        "complete" | "complete_task" | "task_complete" => "complete_task".to_string(),
        "drive" | "drive_run" | "run_drive" => "drive_run".to_string(),
        other => other.to_string(),
    }
}

fn has_any_key(data: &Value, keys: &[&str]) -> bool {
    value_candidates(data)
        .into_iter()
        .any(|candidate| keys.iter().any(|key| candidate.get(*key).is_some()))
}

fn normalized_action(action: &str) -> String {
    match action
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "start" | "create_workflow" | "new_workflow" => "start_workflow".to_string(),
        "continue" => "continue_workflow".to_string(),
        "modify" | "update_goal" | "change_goal" => "modify_workflow".to_string(),
        "pause" => "pause_workflow".to_string(),
        "resume" => "resume_workflow".to_string(),
        "complete" | "end" | "end_workflow" | "finish_workflow" => "complete_workflow".to_string(),
        other => other.to_string(),
    }
}

fn required_text(name: &str, value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{name} must not be empty");
    }
    Ok(value.to_string())
}

fn inbound_event_view(store: &ForgeStore, event: InboundEventRecord) -> InboundEventView {
    let operating_context =
        serde_json::from_value::<OperatingContextSpec>(event.tenant_context.clone())
            .unwrap_or_default();
    let identity_context = inbound_event_identity_context(store, &event);
    InboundEventView {
        id: event.id,
        origin: event.origin,
        action: event.action,
        status: event.status,
        tenant_context: EventTenantContext::from(&operating_context),
        identity_context,
        data: event.data,
        created_at: event.created_at,
        processed_at: event.processed_at,
    }
}

fn inbound_event_identity_context(
    store: &ForgeStore,
    event: &InboundEventRecord,
) -> Option<EventIdentityContext> {
    let source_identity = extract_inbound_event_source_identity(&event.data)?;
    let resolved = resolve_identity(store, &source_identity.scope, &source_identity.id).ok()?;
    Some(EventIdentityContext {
        schema_version: EVENT_IDENTITY_CONTEXT_SCHEMA_VERSION.to_string(),
        resolution_status: resolved.status,
        source_identity,
        canonical_identity: resolved.canonical_identity,
        identity_count: resolved.identity_count,
        link_count: resolved.link_count,
    })
}

fn extract_inbound_event_source_identity(data: &Value) -> Option<ContextIdentityRef> {
    [
        data.get("identity"),
        data.get("source_identity"),
        data.get("actor").and_then(|actor| actor.get("identity")),
        data.get("sender").and_then(|sender| sender.get("identity")),
        data.get("payload")
            .and_then(|payload| payload.get("identity")),
        data.get("message")
            .and_then(|message| message.get("identity")),
    ]
    .into_iter()
    .flatten()
    .find_map(context_identity_ref_from_value)
}

fn context_identity_ref_from_value(value: &Value) -> Option<ContextIdentityRef> {
    let scope = extract_string(value, &["scope", "kind", "type"])?;
    let id = extract_string(value, &["id", "identity_id", "user_id"])?;
    let label = extract_string(value, &["label", "name"])
        .unwrap_or_else(|| format!("{}:{}", scope.trim(), id.trim()));
    Some(ContextIdentityRef {
        scope: scope.trim().to_string(),
        id: id.trim().to_string(),
        label,
    })
}

impl From<InboundEventRecord> for InboundEventView {
    fn from(event: InboundEventRecord) -> Self {
        let operating_context =
            serde_json::from_value::<OperatingContextSpec>(event.tenant_context)
                .unwrap_or_default();
        Self {
            id: event.id,
            origin: event.origin,
            action: event.action,
            status: event.status,
            tenant_context: EventTenantContext::from(&operating_context),
            identity_context: None,
            data: event.data,
            created_at: event.created_at,
            processed_at: event.processed_at,
        }
    }
}

fn envelope_event(event: StoreEvent, tenant_context: &EventTenantContext) -> WorkflowEventEnvelope {
    let origin = extract_string(&event.data, &["origin", "actor", "executor"])
        .unwrap_or_else(|| "forge".to_string());
    let observability = build_event_observability(&event.kind, &event.data);
    WorkflowEventEnvelope {
        schema_version: EVENT_ENVELOPE_SCHEMA_VERSION.to_string(),
        event_id: format!("evt_{}_{}", event.workflow_id, event.id),
        store_sequence: event.id,
        workflow_id: event.workflow_id.clone(),
        kind: event.kind.clone(),
        category: categorize_event(&event.kind),
        severity: infer_severity(&event.kind, &event.data),
        origin,
        source: extract_string(&event.data, &["source"])
            .unwrap_or_else(|| "forge_store".to_string()),
        occurred_at: event.created_at,
        correlation: EventCorrelation {
            run_id: extract_string(&event.data, &["run_id"]),
            request_id: extract_string(&event.data, &["request_id"]),
            task_id: extract_string(&event.data, &["task_id", "task"]),
            artifact_id: extract_string(&event.data, &["artifact_id", "artifact"]),
            interaction_id: extract_string(&event.data, &["interaction_id"]),
        },
        tenant_context: tenant_context.clone(),
        observability,
        data: event.data,
    }
}

fn global_event_envelope(event: StoredGlobalEventRecord) -> WorkflowEventEnvelope {
    let operating_context =
        serde_json::from_value::<OperatingContextSpec>(event.tenant_context.clone())
            .unwrap_or_default();
    let mut data = event.data;
    if let Value::Object(map) = &mut data {
        map.insert(
            "global_event".to_string(),
            json!({
                "source": event.source,
                "source_id": event.source_id,
                "status": event.status,
            }),
        );
    }
    WorkflowEventEnvelope {
        schema_version: EVENT_ENVELOPE_SCHEMA_VERSION.to_string(),
        event_id: format!("evtg_{}", event.id),
        store_sequence: event.id,
        workflow_id: event.workflow_id.unwrap_or_else(|| "_global".to_string()),
        kind: event.kind.clone(),
        category: categorize_event(&event.kind),
        severity: infer_severity(&event.kind, &data),
        origin: event.origin,
        source: data
            .get("global_event")
            .and_then(|value| value.get("source"))
            .and_then(Value::as_str)
            .unwrap_or("global_events")
            .to_string(),
        occurred_at: event.created_at,
        correlation: EventCorrelation {
            run_id: extract_string(&data, &["run_id"]),
            request_id: extract_string(&data, &["request_id"]),
            task_id: extract_string(&data, &["task_id", "task"]),
            artifact_id: extract_string(&data, &["artifact_id", "artifact"]),
            interaction_id: extract_string(&data, &["interaction_id"]),
        },
        tenant_context: EventTenantContext::from(&operating_context),
        observability: build_event_observability(&event.kind, &data),
        data,
    }
}

pub(crate) fn build_event_observability(kind: &str, data: &Value) -> EventObservability {
    let context_budget_bytes = extract_observability_i64(
        data,
        &[
            "context_budget_bytes",
            "effective_budget_bytes",
            "effective_context_budget_bytes",
            "effective_budget",
            "budget_bytes",
            "requested_budget",
        ],
    );
    let context_remaining_bytes = extract_observability_i64(
        data,
        &[
            "context_remaining_bytes",
            "remaining_context_bytes",
            "remaining_budget_bytes",
            "remaining_budget",
        ],
    );
    let selected_context_bytes = extract_observability_i64(
        data,
        &[
            "selected_context_bytes",
            "context_bytes",
            "selected_bytes",
            "content_bytes",
        ],
    )
    .or_else(|| derive_selected_context_bytes(context_budget_bytes, context_remaining_bytes));
    let context_pressure_bps = extract_observability_i64(
        data,
        &[
            "context_pressure_bps",
            "context_utilization_bps",
            "budget_utilization_bps",
        ],
    )
    .or_else(|| derive_context_pressure_bps(context_budget_bytes, selected_context_bytes));
    let context_pressure_state = extract_observability_string(
        data,
        &[
            "context_pressure_state",
            "budget_status",
            "route_status",
            "routing_quality_status",
        ],
    )
    .or_else(|| derive_context_pressure_state(context_pressure_bps));
    EventObservability {
        schema_version: "forge.event_observability.v1".to_string(),
        node_ref: extract_observability_string(data, &["node_id", "node", "task_id", "task"]),
        addon_id: extract_observability_string(data, &["addon_id", "addon"]),
        duration_ms: extract_observability_i64(
            data,
            &[
                "duration_ms",
                "elapsed_ms",
                "latency_ms",
                "execution_ms",
                "runtime_ms",
            ],
        ),
        retry_count: extract_observability_i64(
            data,
            &["retry_count", "retries", "attempt_count", "attempt"],
        ),
        wait_state: extract_wait_state(kind, data),
        wait_seconds: extract_observability_i64(
            data,
            &[
                "wait_seconds",
                "sleep_seconds",
                "delay_seconds",
                "backoff_seconds",
            ],
        ),
        context_budget_bytes,
        selected_context_bytes,
        context_remaining_bytes,
        context_pressure_bps,
        context_pressure_state,
        memory_level: extract_observability_string(data, &["memory_level"]),
        memory_scope: extract_observability_string(data, &["memory_scope"]),
    }
}

fn derive_selected_context_bytes(
    context_budget_bytes: Option<i64>,
    context_remaining_bytes: Option<i64>,
) -> Option<i64> {
    let budget = context_budget_bytes?;
    let remaining = context_remaining_bytes?;
    Some(budget.saturating_sub(remaining).max(0))
}

fn derive_context_pressure_bps(
    context_budget_bytes: Option<i64>,
    selected_context_bytes: Option<i64>,
) -> Option<i64> {
    let budget = context_budget_bytes?;
    if budget <= 0 {
        return None;
    }
    let selected = selected_context_bytes?.max(0) as i128;
    let budget = budget as i128;
    Some(((selected * 10_000) / budget).min(10_000) as i64)
}

fn derive_context_pressure_state(context_pressure_bps: Option<i64>) -> Option<String> {
    let pressure = context_pressure_bps?;
    Some(
        if pressure >= 9_000 {
            "critical"
        } else if pressure >= 7_500 {
            "high"
        } else {
            "normal"
        }
        .to_string(),
    )
}

fn extract_wait_state(kind: &str, data: &Value) -> Option<String> {
    let explicit = extract_observability_string(data, &["wait_state", "state", "phase", "status"]);
    if explicit
        .as_deref()
        .is_some_and(|value| value.to_ascii_lowercase().contains("wait"))
    {
        return explicit;
    }
    let lower = kind.to_ascii_lowercase();
    lower.contains("wait").then(|| kind.to_string())
}

fn extract_observability_string(data: &Value, keys: &[&str]) -> Option<String> {
    for candidate in observability_value_candidates(data) {
        for key in keys {
            if let Some(value) = candidate.get(*key) {
                if let Some(text) = value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    return Some(text.to_string());
                }
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    let id = id.trim();
                    if !id.is_empty() {
                        return Some(id.to_string());
                    }
                }
            }
        }
    }
    None
}

fn extract_observability_i64(data: &Value, keys: &[&str]) -> Option<i64> {
    for candidate in observability_value_candidates(data) {
        for key in keys {
            if let Some(value) = candidate.get(*key) {
                if let Some(number) = value.as_i64() {
                    return Some(number);
                }
                if let Some(number) = value.as_u64().and_then(|number| i64::try_from(number).ok()) {
                    return Some(number);
                }
                if let Some(number) = value
                    .as_str()
                    .map(str::trim)
                    .and_then(|value| value.parse::<i64>().ok())
                {
                    return Some(number);
                }
            }
        }
    }
    None
}

fn observability_value_candidates(data: &Value) -> Vec<&Value> {
    let mut candidates = value_candidates(data);
    for _ in 0..2 {
        let roots = candidates.clone();
        for candidate in roots {
            for key in [
                "context",
                "context_route",
                "routing_summary",
                "routing_quality",
                "routing_repair",
                "routing_economy",
                "budget_plan",
                "selection_receipt",
                "replay_manifest",
                "memory_policy",
                "operating_context",
                "request",
            ] {
                if let Some(value) = candidate.get(key) {
                    candidates.push(value);
                }
            }
        }
    }
    for key in [
        "summary",
        "health",
        "timing",
        "cost",
        "worker_report",
        "worker_report_partial",
        "webhook_report",
        "webhook_report_partial",
        "delivery",
    ] {
        if let Some(value) = data.get(key) {
            candidates.push(value);
        }
    }
    candidates
}

fn extract_string(data: &Value, keys: &[&str]) -> Option<String> {
    for candidate in value_candidates(data) {
        for key in keys {
            if let Some(value) = candidate.get(*key).and_then(Value::as_str) {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn extract_bool(data: &Value, keys: &[&str]) -> Option<bool> {
    for candidate in value_candidates(data) {
        for key in keys {
            if let Some(value) = candidate.get(*key).and_then(Value::as_bool) {
                return Some(value);
            }
        }
    }
    None
}

fn extract_string_array(data: &Value, key: &str) -> Result<Vec<String>> {
    let Some(value) = value_candidates(data)
        .into_iter()
        .find_map(|candidate| candidate.get(key))
    else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .with_context(|| format!("{key} entries must be non-empty strings"))
            })
            .collect(),
        Value::String(item) => {
            let item = item.trim();
            if item.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![item.to_string()])
            }
        }
        Value::Object(map) if key == "field_values" => Ok(map
            .iter()
            .map(|(field, value)| {
                let value = value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string());
                format!("{field}={value}")
            })
            .collect()),
        _ => bail!("{key} must be a string, string array or field_values object"),
    }
}

fn value_candidates(data: &Value) -> Vec<&Value> {
    let mut candidates = vec![data];
    for key in ["payload", "message", "data", "route"] {
        if let Some(value) = data.get(key) {
            candidates.push(value);
        }
    }
    candidates
}

pub(crate) fn categorize_event(kind: &str) -> String {
    let lower = kind.to_ascii_lowercase();
    if lower.starts_with("workflow_") || lower.contains("goal") {
        "workflow".to_string()
    } else if lower.contains("async_request") || lower.contains("run_") {
        "runtime".to_string()
    } else if lower.contains("checkpoint") || lower.contains("handoff") || lower.contains("lease") {
        "coordination".to_string()
    } else if lower.contains("interaction") || lower.contains("human") {
        "human_collaboration".to_string()
    } else if lower.contains("creative")
        || lower.contains("collaboration")
        || lower.contains("token")
    {
        "creative_workspace".to_string()
    } else if lower.contains("schedule") || lower.contains("loop") {
        "schedule".to_string()
    } else if lower.contains("quota") || lower.contains("cost") {
        "cost_governance".to_string()
    } else {
        "operational".to_string()
    }
}

pub(crate) fn infer_severity(kind: &str, data: &Value) -> String {
    let lower = kind.to_ascii_lowercase();
    let status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower.contains("failed")
        || lower.contains("error")
        || lower.contains("cancelled")
        || status.contains("failed")
        || status.contains("error")
        || status.contains("cancelled")
    {
        "error".to_string()
    } else if lower.contains("blocked")
        || lower.contains("missing")
        || lower.contains("expired")
        || status.contains("blocked")
        || status.contains("missing")
        || status.contains("expired")
    {
        "warning".to_string()
    } else {
        "info".to_string()
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn mutable_and_exposed_webhooks_fail_closed_without_hmac() {
        assert!(validate_webhook_ingress_security(true, false, "start_workflow", false).is_err());
        assert!(validate_webhook_ingress_security(false, false, "observe", false).is_err());
        assert!(validate_webhook_ingress_security(true, false, "start_workflow", true).is_ok());
        assert!(validate_webhook_ingress_security(false, true, "start_workflow", false).is_ok());
    }

    #[test]
    fn webhook_hmac_rejects_replayed_nonce() {
        let body = br#"{"action":"start_workflow","data":{"goal":"secure"}}"#.to_vec();
        let timestamp = Utc::now().timestamp().to_string();
        let nonce = "unit-test-nonce-00000001";
        let secret = b"0123456789abcdef0123456789abcdef";
        let signature = format!(
            "sha256={}",
            hex_encode(&hmac_sha256(
                secret,
                &webhook_signature_payload(&timestamp, nonce, &body)
            ))
        );
        let request = WebhookHttpRequest {
            method: "POST".to_string(),
            path: "/webhook".to_string(),
            headers: vec![
                ("x-forge-signature".to_string(), signature),
                (WEBHOOK_TIMESTAMP_HEADER.to_string(), timestamp),
                (WEBHOOK_NONCE_HEADER.to_string(), nonce.to_string()),
            ],
            body,
        };
        let verifier = WebhookHmacVerifier {
            secret_env: "UNIT_TEST_SECRET".to_string(),
            signature_header: "X-Forge-Signature".to_string(),
            secret: secret.to_vec(),
        };
        let mut state = WebhookIngressSecurityState {
            seen_nonces: BTreeMap::new(),
            requests_by_peer: BTreeMap::new(),
            rate_limit_per_minute: 60,
            allow_unsigned_mutations: false,
        };
        verify_webhook_hmac(&verifier, &request, &mut state).unwrap();
        let replay = verify_webhook_hmac(&verifier, &request, &mut state).unwrap_err();
        assert!(replay.to_string().contains("replay detected"));
    }

    #[test]
    fn webhook_rate_limit_fails_closed_per_peer() {
        let mut state = WebhookIngressSecurityState {
            seen_nonces: BTreeMap::new(),
            requests_by_peer: BTreeMap::new(),
            rate_limit_per_minute: 1,
            allow_unsigned_mutations: false,
        };
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        state.check_rate_limit(peer).unwrap();
        let limited = state.check_rate_limit(peer).unwrap_err();
        assert!(limited.to_string().contains("rate limit exceeded"));
        let stale = state
            .accept_fresh_nonce(
                Utc::now().timestamp() - WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS - 1,
                "stale-request-nonce-0001",
            )
            .unwrap_err();
        assert!(stale.to_string().contains("outside"));
    }

    #[test]
    fn outbound_ssrf_filter_blocks_private_and_link_local_addresses() {
        assert!(!ip_is_public_for_outbound(IpAddr::V4(Ipv4Addr::new(
            10, 0, 0, 1
        ))));
        assert!(!ip_is_public_for_outbound(IpAddr::V4(Ipv4Addr::new(
            169, 254, 169, 254
        ))));
        assert!(!ip_is_public_for_outbound(IpAddr::V6(
            "fe80::1".parse::<Ipv6Addr>().unwrap()
        )));
        assert!(ip_is_public_for_outbound(IpAddr::V4(Ipv4Addr::new(
            8, 8, 8, 8
        ))));
    }
}
