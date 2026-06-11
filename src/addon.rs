use crate::artifact::hex_sha256;
use crate::credential_vault::resolve_credential_vault_bin;
use crate::graph::{
    create_workflow, task as workflow_task, AtomicTask, ExecutorKind, ValidationRule, Workflow,
    WorkflowRevision,
};
use crate::intent::{parse_intent, parse_intent_with_catalog};
use crate::storage::{
    AddonMarketplacePackageWrite, AddonPermissionAuthorizationWrite, AddonTrustKeyWrite,
    ForgeStore, RuntimeContractDispatchWrite, RuntimeWorkerWrite, StoredAddonCapabilityRecord,
    StoredAddonCapabilityWrite, StoredAddonMarketplacePackageRecord,
    StoredAddonPermissionAuthorizationRecord, StoredAddonRecord, StoredAddonTrustKeyRecord,
    StoredGlobalEventRecord, StoredRuntimeContractDispatchRecord, StoredRuntimeWorkerRecord,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const ADDON_CATALOG_SCHEMA_VERSION: &str = "forge.addon_catalog.v1";
pub const CAPABILITY_RESOLUTION_SCHEMA_VERSION: &str = "forge.capability_resolution.v1";
pub const ADDON_VALIDATION_SCHEMA_VERSION: &str = "forge.addon_validation.v1";
pub const INSTALLED_ADDONS_SCHEMA_VERSION: &str = "forge.installed_addons.v1";
pub const ADDON_LIFECYCLE_SCHEMA_VERSION: &str = "forge.addon_lifecycle.v1";
pub const ADDON_CAPABILITY_INDEX_SCHEMA_VERSION: &str = "forge.addon_capability_index.v1";
pub const ADDON_EVENT_ADAPTERS_SCHEMA_VERSION: &str = "forge.addon_event_adapters.v1";
pub const ADDON_OBSERVABILITY_SCHEMA_VERSION: &str = "forge.addon_observability.v1";
pub const ADDON_RUNTIME_CONTRACTS_SCHEMA_VERSION: &str = "forge.addon_runtime_contracts.v1";
pub const ADDON_PLANNER_REGISTRY_SCHEMA_VERSION: &str = "forge.addon_planner_registry.v1";

type CapabilitySuggestionAction = (
    String,
    String,
    String,
    Vec<Vec<String>>,
    Vec<String>,
    Vec<String>,
);

pub struct AddonPlannerDispatchInput<'a> {
    pub addon_id: Option<&'a str>,
    pub contract_id: &'a str,
    pub goal: &'a str,
    pub constraints: &'a [String],
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub context: serde_json::Value,
    pub source: &'a str,
    pub dry_run: bool,
}

pub struct AddonPlanningStrategyInput<'a> {
    pub dispatch: AddonPlannerDispatchInput<'a>,
    pub worker_id: &'a str,
    pub lease_seconds: u64,
}

struct AddonPlanningStrategyReportInput<'a> {
    status: &'a str,
    goal: &'a str,
    contract_id: &'a str,
    worker_id: &'a str,
    source: &'a str,
    dry_run: bool,
    dispatch_report: AddonRuntimeContractDispatchReport,
    strategy_result: Option<serde_json::Value>,
    validation: AddonPlanningStrategyResultValidation,
    equivalence: AddonPlanningStrategyEquivalence,
}

pub struct AddonRuntimeContractCompletionInput<'a> {
    pub dispatch_id: &'a str,
    pub worker_id: &'a str,
    pub completion_status: &'a str,
    pub result: serde_json::Value,
    pub signature: Option<&'a str>,
    pub attestation: serde_json::Value,
    pub dry_run: bool,
}

pub struct AddonPackageInput<'a> {
    pub manifest_path: &'a Path,
    pub addon_dirs: &'a [PathBuf],
    pub repository: Option<&'a str>,
    pub channel: &'a str,
    pub signature: Option<&'a str>,
    pub public_key: Option<&'a str>,
    pub package_path: Option<&'a Path>,
}

pub struct AddonTrustKeyInput<'a> {
    pub repository: &'a str,
    pub channel: &'a str,
    pub public_key: &'a str,
    pub trust_level: &'a str,
    pub approved_by: &'a str,
    pub source: &'a str,
    pub data: serde_json::Value,
}

pub struct CapabilityRegistrySyncInput<'a> {
    pub registry_sources: &'a [String],
    pub cache_dir: Option<&'a Path>,
    pub allow_remote: bool,
    pub max_bytes: u64,
    pub max_packages: usize,
    pub lock_path: Option<&'a Path>,
}

struct ExternalApiAttestationInput<'a> {
    started_at: chrono::DateTime<Utc>,
    status_code: Option<u16>,
    response_bytes: Option<usize>,
    outcome: &'a str,
    response_sha256: Option<&'a str>,
}

struct RuntimeDispatchUpdateInput<'a> {
    policy: AddonRuntimeContractPolicyEntry,
    status: &'a str,
    report_status: &'a str,
    worker: &'a str,
    dry_run: bool,
    outcome: serde_json::Value,
}

struct AddonMigrationTaskInput<'a> {
    id: &'a str,
    title: &'a str,
    dependencies: &'a [&'a str],
    context_requirements: &'a [&'a str],
    validation_rules: Vec<ValidationRule>,
    expected_output: &'a str,
    executor: ExecutorKind,
    human_required: bool,
}
pub const ADDON_RUNTIME_CONTRACT_POLICY_SCHEMA_VERSION: &str =
    "forge.addon_runtime_contract_policy.v1";
pub const ADDON_RUNTIME_CONTRACT_DISPATCH_SCHEMA_VERSION: &str =
    "forge.addon_runtime_contract_dispatch.v1";
pub const ADDON_PLANNER_DISPATCH_INPUT_SCHEMA_VERSION: &str =
    "forge.addon_planner_dispatch_input.v1";
pub const ADDON_PLANNING_STRATEGY_EXECUTION_SCHEMA_VERSION: &str =
    "forge.addon_planning_strategy_execution.v1";
pub const ADDON_PLANNING_STRATEGY_RESULT_SCHEMA_VERSION: &str =
    "forge.addon_planning_strategy_result.v1";
pub const ADDON_RUNTIME_WORKERS_SCHEMA_VERSION: &str = "forge.addon_runtime_workers.v1";
pub const ADDON_VIEWS_SCHEMA_VERSION: &str = "forge.addon_views.v1";
pub const ADDON_PACKAGE_SCHEMA_VERSION: &str = "forge.addon_package.v1";
pub const ADDON_MARKETPLACE_SCHEMA_VERSION: &str = "forge.addon_marketplace.v1";
pub const ADDON_PACKAGE_FETCH_SCHEMA_VERSION: &str = "forge.addon_package_fetch.v1";
pub const ADDON_REGISTRY_SYNC_SCHEMA_VERSION: &str = "forge.addon_registry_sync.v1";
pub const ADDON_PACKAGE_LOCK_SCHEMA_VERSION: &str = "forge.addon_package_lock.v1";
pub const ADDON_PACKAGE_LOCK_ENFORCEMENT_SCHEMA_VERSION: &str =
    "forge.addon_package_lock_enforcement.v1";
pub const ADDON_TRUST_STORE_SCHEMA_VERSION: &str = "forge.addon_trust_store.v1";
pub const ADDON_PACKAGE_POLICY_SCHEMA_VERSION: &str = "forge.addon_package_policy.v1";
pub const ADDON_PACKAGE_INSTALL_SCHEMA_VERSION: &str = "forge.addon_package_install.v1";
pub const ADDON_MIGRATION_WORKFLOW_SCHEMA_VERSION: &str = "forge.addon_migration_workflow.v1";
pub const ADDON_PERMISSION_AUTHORIZATIONS_SCHEMA_VERSION: &str =
    "forge.addon_permission_authorizations.v1";

pub const CAP_WORKFLOW_RUNTIME: &str = "workflow_runtime";
pub const CAP_DYNAMIC_WORKFLOW: &str = "dynamic_workflow";
pub const CAP_EVENT_ENGINE: &str = "event_engine";
pub const CAP_CONTEXT_ROUTING: &str = "context_routing";
pub const CAP_MEMORY_GOVERNANCE: &str = "memory_governance";
pub const CAP_IDENTITY_ROUTING: &str = "identity_routing";
pub const CAP_PERSONALITY_ROUTING: &str = "personality_routing";
pub const CAP_HUMAN_COLLABORATION: &str = "human_collaboration";
pub const CAP_OBSERVABILITY: &str = "observability";
pub const CAP_ADDON_REGISTRY: &str = "addon_registry";
pub const CAP_WORKFLOW_AUTOMATION_RESEARCH: &str = "workflow_automation_research";
pub const CAP_HACKATHON_FACTORY: &str = "hackathon_factory";
pub const CAP_DAILY_GOAL_RESEARCH: &str = "daily_goal_research";
pub const CAP_VISUAL_WORKSPACE: &str = "visual_workspace";
pub const CAP_ASYNC_RUNTIME: &str = "async_runtime";
pub const CAP_TELEGRAM_NOTIFICATION: &str = "telegram_notification";
pub const CAP_SOURCE_CODE_PATCH_LIFECYCLE: &str = "source_code_patch_lifecycle";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonCatalog {
    #[serde(default = "addon_catalog_schema_version")]
    pub schema_version: String,
    pub status: String,
    #[serde(default)]
    pub addon_dirs: Vec<String>,
    pub addon_count: usize,
    pub capability_count: usize,
    pub addons: Vec<AddonManifest>,
}

impl Default for AddonCatalog {
    fn default() -> Self {
        Self {
            schema_version: addon_catalog_schema_version(),
            status: "loaded".to_string(),
            addon_dirs: Vec::new(),
            addon_count: 0,
            capability_count: 0,
            addons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonManifest {
    #[serde(default = "addon_manifest_schema_version")]
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_addon_lifecycle")]
    pub lifecycle: String,
    #[serde(default = "default_addon_source")]
    pub source: String,
    #[serde(default)]
    pub dependencies: Vec<AddonDependency>,
    #[serde(default)]
    pub permissions: Vec<AddonPermission>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityDeclaration>,
    #[serde(default)]
    pub workflows: Vec<WorkflowExtensionDeclaration>,
    #[serde(default)]
    pub runtime_contracts: Vec<AddonRuntimeContractDeclaration>,
    #[serde(default)]
    pub views: Vec<AddonView>,
    #[serde(default)]
    pub artifact_types: Vec<ArtifactTypeDeclaration>,
    #[serde(default)]
    pub event_types: Vec<EventTypeDeclaration>,
    #[serde(default)]
    pub event_adapters: Vec<EventAdapterDeclaration>,
    #[serde(default)]
    pub context_providers: Vec<ContextProviderDeclaration>,
    #[serde(default)]
    pub memory_providers: Vec<MemoryProviderDeclaration>,
    #[serde(default)]
    pub integrations: Vec<IntegrationDeclaration>,
    #[serde(default)]
    pub compatibility: AddonCompatibility,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonDependency {
    pub id: String,
    #[serde(default)]
    pub version_req: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPermission {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_permission_risk")]
    pub risk: String,
    #[serde(default)]
    pub requires_human_approval: bool,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub integrations: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub tenant_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDeclaration {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub requires_capabilities: Vec<String>,
    #[serde(default)]
    pub workflow_extensions: Vec<String>,
    #[serde(default)]
    pub deliverables: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub unknowns: Vec<String>,
    #[serde(default)]
    pub event_triggers: Vec<String>,
    #[serde(default)]
    pub artifact_types: Vec<String>,
    #[serde(default)]
    pub view_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExtensionDeclaration {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonRuntimeContractDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub contract_type: String,
    #[serde(default)]
    pub capability_id: String,
    #[serde(default)]
    pub workflow_extension_id: String,
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub entrypoint: String,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeContractReport {
    pub schema_version: String,
    pub status: String,
    pub contract_count: usize,
    pub filters: AddonRuntimeContractFilters,
    pub contracts: Vec<AddonRuntimeContractView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeContractFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeContractView {
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    #[serde(default)]
    pub permission_gate: AddonPermissionGate,
    pub contract: AddonRuntimeContractDeclaration,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonPlannerRegistryReport {
    pub schema_version: String,
    pub status: String,
    pub planner_count: usize,
    pub filters: AddonPlannerRegistryFilters,
    pub planners: Vec<AddonPlannerRegistration>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonPlannerRegistryFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_extension_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonPlannerRegistration {
    pub id: String,
    pub status: String,
    pub source: String,
    pub planner_kind: String,
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub contract_id: String,
    pub capability_id: String,
    pub workflow_extension_id: String,
    pub runtime: String,
    pub entrypoint: String,
    pub dispatch_allowed: bool,
    pub permission_gate: AddonPermissionGate,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default)]
    pub commands: Vec<Vec<String>>,
    #[serde(default)]
    pub mcp_tools: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeContractPolicyReport {
    pub schema_version: String,
    pub status: String,
    pub contract_count: usize,
    pub dispatch_allowed_count: usize,
    pub blocked_count: usize,
    pub filters: AddonRuntimeContractPolicyFilters,
    pub contracts: Vec<AddonRuntimeContractPolicyEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeContractPolicyFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonRuntimeContractPolicyEntry {
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub contract_id: String,
    pub contract_type: String,
    pub capability_id: String,
    pub runtime: String,
    pub entrypoint: String,
    pub dispatch_allowed: bool,
    pub status: String,
    pub issues: Vec<String>,
    pub permission_gate: AddonPermissionGate,
    pub contract: AddonRuntimeContractDeclaration,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeContractDispatchReport {
    pub schema_version: String,
    pub status: String,
    pub dispatch_count: usize,
    pub queued_count: usize,
    pub claimed_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub needs_external_worker_count: usize,
    pub blocked_count: usize,
    pub dry_run: bool,
    pub dispatches: Vec<AddonRuntimeContractDispatchEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeContractDispatchEntry {
    pub id: String,
    pub addon_id: String,
    pub contract_id: String,
    pub contract_type: String,
    pub capability_id: String,
    pub runtime: String,
    pub entrypoint: String,
    pub status: String,
    pub source: String,
    pub input: serde_json::Value,
    pub policy: AddonRuntimeContractPolicyEntry,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonPlanningStrategyExecutionReport {
    pub schema_version: String,
    pub status: String,
    pub goal: String,
    pub contract_id: String,
    pub worker_id: String,
    pub source: String,
    pub dry_run: bool,
    pub dispatch_report: AddonRuntimeContractDispatchReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_result: Option<serde_json::Value>,
    pub validation: AddonPlanningStrategyResultValidation,
    pub equivalence: AddonPlanningStrategyEquivalence,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonPlanningStrategyResultValidation {
    pub schema_version: String,
    pub status: String,
    pub task_count: usize,
    pub issue_count: usize,
    pub issues: Vec<String>,
    pub tasks: Vec<AddonPlanningStrategyTaskShape>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonPlanningStrategyEquivalence {
    pub schema_version: String,
    pub status: String,
    pub replacement_ready: bool,
    pub core_task_count: usize,
    pub external_task_count: usize,
    pub matching_task_id_count: usize,
    pub matching_task_title_count: usize,
    pub missing_core_task_ids: Vec<String>,
    pub extra_external_task_ids: Vec<String>,
    pub dependency_issue_count: usize,
    pub dependency_issues: Vec<String>,
    pub validation_rule_coverage_percent: f64,
    pub core_plan_sha256: String,
    pub external_plan_sha256: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPlanningStrategyTaskShape {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub executor: String,
    pub context_requirement_count: usize,
    pub validation_rule_count: usize,
    pub expected_output_present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeWorkerReport {
    pub schema_version: String,
    pub status: String,
    pub worker_count: usize,
    pub available_count: usize,
    pub filters: AddonRuntimeWorkerFilters,
    pub workers: Vec<AddonRuntimeWorkerEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonRuntimeWorkerFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonRuntimeWorkerEntry {
    pub id: String,
    pub runtime: String,
    pub status: String,
    pub trust_level: String,
    pub source: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonView {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub surface: String,
    #[serde(
        default = "default_addon_view_type",
        rename = "type",
        alias = "view_type"
    )]
    pub view_type: String,
    #[serde(default)]
    pub component: String,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub layout: AddonViewLayout,
    #[serde(default)]
    pub data_bindings: Vec<AddonViewDataBinding>,
    #[serde(default)]
    pub actions: Vec<AddonViewAction>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub props: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonViewLayout {
    #[serde(default)]
    pub zone: String,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub width: String,
    #[serde(default)]
    pub height: String,
    #[serde(default)]
    pub density: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonViewDataBinding {
    pub id: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub refresh_seconds: u64,
    #[serde(default)]
    pub required_capability: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonViewAction {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(
        default = "default_addon_view_action_type",
        rename = "type",
        alias = "action_type"
    )]
    pub action_type: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub permission: String,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub payload_schema: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonViewReport {
    pub schema_version: String,
    pub status: String,
    pub view_count: usize,
    pub filters: AddonViewFilters,
    pub views: Vec<AddonViewEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonViewFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonViewEntry {
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub permission_gate: AddonPermissionGate,
    pub view: AddonView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactTypeDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub generic_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTypeDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAdapterDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub origins: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub event_types: Vec<String>,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub auth: String,
    #[serde(
        default,
        alias = "hmac_secret_env",
        skip_serializing_if = "Option::is_none"
    )]
    pub secret_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_vault: Option<EventAdapterCredentialVaultRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_header: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_response_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAdapterCredentialVaultRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_bin: Option<String>,
    pub contract: String,
    pub data: String,
    pub record: String,
    pub field: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonEventAdapterReport {
    pub schema_version: String,
    pub status: String,
    pub adapter_count: usize,
    pub filters: AddonEventAdapterFilters,
    pub adapters: Vec<AddonEventAdapterView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonEventAdapterFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonEventAdapterView {
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub permission_gate: AddonPermissionGate,
    pub adapter: EventAdapterDeclaration,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonObservabilityReport {
    pub schema_version: String,
    pub status: String,
    pub addon_count: usize,
    pub enabled_count: usize,
    pub disabled_count: usize,
    pub unauthorized_count: usize,
    pub filters: AddonObservabilityFilters,
    pub totals: AddonObservabilityTotals,
    pub addons: Vec<AddonObservabilityEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonObservabilityFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
    pub dispatch_limit: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AddonObservabilityTotals {
    pub capability_count: usize,
    pub dependency_count: usize,
    pub permission_count: usize,
    pub runtime_contract_count: usize,
    pub view_count: usize,
    pub artifact_type_count: usize,
    pub event_type_count: usize,
    pub event_adapter_count: usize,
    pub integration_count: usize,
    pub dispatch_count: usize,
    pub queued_dispatch_count: usize,
    pub completed_dispatch_count: usize,
    pub failed_dispatch_count: usize,
    pub blocked_dispatch_count: usize,
    pub needs_external_worker_count: usize,
    pub runtime_event_count: usize,
    pub runtime_consumed_event_count: usize,
    pub runtime_emitted_event_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddonObservabilityEntry {
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub source: String,
    pub capability_count: usize,
    pub dependency_count: usize,
    pub permission_count: usize,
    pub runtime_contract_count: usize,
    pub view_count: usize,
    pub artifact_type_count: usize,
    pub event_type_count: usize,
    pub event_adapter_count: usize,
    pub integration_count: usize,
    pub context_provider_count: usize,
    pub memory_provider_count: usize,
    pub permission_gate: AddonPermissionGate,
    pub dependencies: Vec<AddonDependency>,
    pub capabilities: Vec<String>,
    pub runtime_contracts: Vec<String>,
    pub views: Vec<String>,
    pub artifact_types: Vec<String>,
    pub event_types: Vec<String>,
    pub integrations: Vec<String>,
    pub event_flow: AddonEventFlowSummary,
    pub dispatches: AddonDispatchObservability,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AddonEventFlowSummary {
    pub ingress_adapter_count: usize,
    pub egress_adapter_count: usize,
    pub bidirectional_adapter_count: usize,
    pub consumed_event_types: Vec<String>,
    pub emitted_event_types: Vec<String>,
    pub transports: Vec<String>,
    pub runtime_event_count: usize,
    pub runtime_consumed_event_count: usize,
    pub runtime_emitted_event_count: usize,
    pub runtime_event_types: Vec<String>,
    pub runtime_transports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_runtime_event_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AddonDispatchObservability {
    pub dispatch_count: usize,
    pub queued_count: usize,
    pub claimed_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub blocked_count: usize,
    pub dry_run_count: usize,
    pub needs_external_worker_count: usize,
    pub latest_dispatch_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonPermissionGate {
    pub schema_version: String,
    pub allowed: bool,
    pub status: String,
    pub required_permissions: Vec<String>,
    pub declared_permissions: Vec<String>,
    pub undeclared_permissions: Vec<String>,
    pub human_approval_required: Vec<String>,
    pub high_risk_permissions: Vec<String>,
    pub tools: Vec<String>,
    pub resources: Vec<String>,
    pub integrations: Vec<String>,
    pub actions: Vec<String>,
    pub tenant_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextProviderDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub provides_sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProviderDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub provider_type: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub memory_levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub integration_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonCompatibility {
    #[serde(default)]
    pub forge_version_req: String,
    #[serde(default)]
    pub api_versions: Vec<String>,
    #[serde(default)]
    pub runtimes: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub migrations: Vec<AddonMigrationPlan>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonMigrationPlan {
    #[serde(default)]
    pub from_version: String,
    #[serde(default)]
    pub to_version: String,
    #[serde(default)]
    pub strategy: String,
    #[serde(default)]
    pub data_migration: String,
    #[serde(default)]
    pub rollback: String,
    #[serde(default)]
    pub requires_backup: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityIntentOverlay {
    #[serde(default)]
    pub deliverables: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub unknowns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityNeed {
    pub id: String,
    pub title: String,
    pub source_addon: String,
    pub source_addon_version: String,
    pub reason: String,
    pub required: bool,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub matched_keywords: Vec<String>,
    #[serde(default)]
    pub workflow_extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExtensionActivation {
    pub id: String,
    pub kind: String,
    pub description: String,
    pub source_addon: String,
    pub source_addon_version: String,
    pub source_capability: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeContractActivation {
    pub id: String,
    pub title: String,
    pub contract_type: String,
    pub runtime: String,
    pub entrypoint: String,
    pub source_addon: String,
    pub source_addon_version: String,
    pub source_capability: String,
    pub workflow_extension_id: String,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    pub permission_gate: AddonPermissionGate,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingCapability {
    pub id: String,
    pub required_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySuggestion {
    pub capability_id: String,
    pub required_by: String,
    pub action: String,
    pub status: String,
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub reason: String,
    #[serde(default)]
    pub commands: Vec<Vec<String>>,
    #[serde(default)]
    pub mcp_tools: Vec<String>,
    #[serde(default)]
    pub permission_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResolutionReport {
    #[serde(default = "capability_resolution_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub goal: String,
    pub planning_strategy: String,
    #[serde(default)]
    pub required_capabilities: Vec<CapabilityNeed>,
    #[serde(default)]
    pub missing_capabilities: Vec<MissingCapability>,
    #[serde(default)]
    pub capability_suggestions: Vec<CapabilitySuggestion>,
    #[serde(default)]
    pub active_addons: Vec<String>,
    #[serde(default)]
    pub available_capabilities: Vec<String>,
    #[serde(default)]
    pub workflow_extensions: Vec<WorkflowExtensionActivation>,
    #[serde(default)]
    pub runtime_contracts: Vec<RuntimeContractActivation>,
    #[serde(default)]
    pub registry_syncs: Vec<AddonRegistrySyncReport>,
    #[serde(default)]
    pub intent_overlay: CapabilityIntentOverlay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonCatalogValidationReport {
    #[serde(default = "addon_validation_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub addon_count: usize,
    pub capability_count: usize,
    pub issue_count: usize,
    #[serde(default)]
    pub issues: Vec<AddonValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonValidationIssue {
    pub severity: String,
    pub code: String,
    pub subject: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAddonListReport {
    #[serde(default = "installed_addons_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub addon_count: usize,
    pub addons: Vec<InstalledAddonView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonLifecycleReport {
    #[serde(default = "addon_lifecycle_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub addon: InstalledAddonView,
    pub validation: AddonCatalogValidationReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_workflow: Option<AddonMigrationWorkflowReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageReport {
    #[serde(default = "addon_package_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub package_id: String,
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub manifest_path: String,
    pub manifest_sha256: String,
    #[serde(default)]
    pub manifest_canonical_sha256: String,
    pub manifest_bytes: u64,
    pub distribution: AddonPackageDistribution,
    pub signature: AddonPackageSignature,
    pub summary: AddonPackageSummary,
    pub validation: AddonCatalogValidationReport,
    pub manifest: AddonManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written_package_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written_package_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageDistribution {
    pub repository: String,
    pub channel: String,
    pub source: String,
    pub update_strategy: String,
    pub install_command: String,
    pub upgrade_command: String,
    pub downgrade_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageSignature {
    pub status: String,
    pub scheme: String,
    pub signature: String,
    pub public_key: String,
    pub payload_sha256: String,
    pub verification_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageSummary {
    pub capability_count: usize,
    pub dependency_count: usize,
    pub permission_count: usize,
    pub workflow_extension_count: usize,
    pub runtime_contract_count: usize,
    pub view_count: usize,
    pub event_adapter_count: usize,
    pub integration_count: usize,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub permissions: Vec<String>,
    pub runtime_contracts: Vec<String>,
    pub views: Vec<String>,
    pub compatibility: AddonCompatibilitySummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonCompatibilitySummary {
    pub forge_version_req: String,
    pub api_versions: Vec<String>,
    pub runtimes: Vec<String>,
    pub features: Vec<String>,
    pub platforms: Vec<String>,
    pub migration_count: usize,
    pub migrations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackagePolicyReport {
    #[serde(default = "addon_package_policy_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub package_id: String,
    pub addon_id: String,
    pub addon_version: String,
    pub repository: String,
    pub channel: String,
    pub package_sha256: String,
    pub manifest_sha256: String,
    pub manifest_canonical_sha256: String,
    pub install_allowed: bool,
    pub trusted_key_count: usize,
    pub issues: Vec<String>,
    pub signature: AddonPackageSignaturePolicy,
    pub trusted_keys: Vec<AddonTrustKeyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageSignaturePolicy {
    pub status: String,
    pub scheme: String,
    pub verification_status: String,
    pub public_key: String,
    pub payload_sha256: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonMarketplaceReport {
    #[serde(default = "addon_marketplace_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub package_count: usize,
    pub installable_count: usize,
    pub blocked_count: usize,
    pub filters: AddonMarketplaceFilters,
    pub packages: Vec<AddonMarketplacePackageEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonMarketplaceFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonMarketplacePackageEntry {
    pub package_id: String,
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub repository: String,
    pub channel: String,
    pub manifest_sha256: String,
    pub manifest_canonical_sha256: String,
    pub package_sha256: String,
    pub status: String,
    pub signature_status: String,
    pub verification_status: String,
    pub source: String,
    pub summary: AddonPackageSummary,
    pub policy: AddonPackagePolicyReport,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonMarketplacePublishReport {
    #[serde(default = "addon_marketplace_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub package: AddonMarketplacePackageEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageFetchReport {
    #[serde(default = "addon_package_fetch_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub source: String,
    pub source_kind: String,
    pub remote_allowed: bool,
    pub cached_package_path: String,
    pub bytes: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    pub max_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<AddonPackageLockEnforcementReport>,
    pub marketplace: AddonMarketplacePublishReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonRegistrySyncReport {
    #[serde(default = "addon_registry_sync_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub source: String,
    pub source_kind: String,
    pub remote_allowed: bool,
    pub package_count: usize,
    pub fetched_count: usize,
    pub blocked_count: usize,
    pub max_packages: usize,
    pub max_bytes: u64,
    #[serde(default)]
    pub fetches: Vec<AddonPackageFetchReport>,
    #[serde(default)]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageLockReport {
    #[serde(default = "addon_package_lock_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub generated_at: String,
    pub package_count: usize,
    pub filters: AddonMarketplaceFilters,
    #[serde(default)]
    pub packages: Vec<AddonPackageLockEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_lock_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_lock_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageLockEntry {
    pub package_id: String,
    pub addon_id: String,
    pub addon_name: String,
    pub addon_version: String,
    pub repository: String,
    pub channel: String,
    pub status: String,
    pub install_allowed: bool,
    pub manifest_sha256: String,
    pub manifest_canonical_sha256: String,
    pub package_sha256: String,
    pub signature_status: String,
    pub verification_status: String,
    pub source: String,
    pub capability_count: usize,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageLockEnforcementReport {
    #[serde(default = "addon_package_lock_enforcement_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub lock_path: String,
    pub lock_sha256: String,
    pub package_id: String,
    pub addon_id: String,
    pub addon_version: String,
    pub repository: String,
    pub channel: String,
    pub package_sha256: String,
    pub manifest_sha256: String,
    pub manifest_canonical_sha256: String,
    pub lock_entry: AddonPackageLockEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AddonRegistryIndex {
    #[serde(default)]
    schema_version: String,
    #[serde(default)]
    packages: Vec<AddonRegistryPackageSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AddonRegistryPackageSource {
    source: String,
    #[serde(default)]
    expected_sha256: Option<String>,
    #[serde(default)]
    allow_remote: Option<bool>,
    #[serde(default)]
    max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonTrustStoreReport {
    #[serde(default = "addon_trust_store_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub key_count: usize,
    pub filters: AddonTrustStoreFilters,
    pub keys: Vec<AddonTrustKeyEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddonTrustStoreFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonTrustKeyEntry {
    pub key_id: String,
    pub repository: String,
    pub channel: String,
    pub public_key: String,
    pub status: String,
    pub trust_level: String,
    pub approved_by: String,
    pub source: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonTrustKeyChangeReport {
    #[serde(default = "addon_trust_store_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub key: AddonTrustKeyEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPackageInstallReport {
    #[serde(default = "addon_package_install_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<AddonPackageLockEnforcementReport>,
    pub package: AddonMarketplacePackageEntry,
    pub lifecycle: AddonLifecycleReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonMigrationWorkflowReport {
    #[serde(default = "addon_migration_workflow_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub workflow_id: String,
    pub from_addon_id: String,
    pub to_addon_id: String,
    pub from_version: String,
    pub to_version: String,
    pub migration_strategy: String,
    pub data_migration: String,
    pub rollback: String,
    pub requires_backup: bool,
    pub task_count: usize,
    pub tasks: Vec<AddonMigrationWorkflowTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonMigrationWorkflowTask {
    pub id: String,
    pub title: String,
    pub executor: String,
    pub human_required: bool,
    pub expected_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPermissionAuthorizationReport {
    #[serde(default = "addon_permission_authorizations_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub authorization_count: usize,
    #[serde(default)]
    pub authorizations: Vec<AddonPermissionAuthorizationView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPermissionAuthorizationChangeReport {
    #[serde(default = "addon_permission_authorizations_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub authorization: AddonPermissionAuthorizationView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonPermissionAuthorizationView {
    pub addon_id: String,
    pub permission_id: String,
    pub status: String,
    pub risk: String,
    pub approved_by: String,
    pub source: String,
    pub granted_at: String,
    pub updated_at: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAddonView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub lifecycle: String,
    pub source: String,
    pub capability_count: usize,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonCapabilityIndexReport {
    #[serde(default = "addon_capability_index_schema_version")]
    pub schema_version: String,
    pub status: String,
    pub capability_count: usize,
    pub enabled_count: usize,
    pub disabled_count: usize,
    #[serde(default)]
    pub capabilities: Vec<AddonCapabilityIndexView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonCapabilityIndexView {
    pub addon_id: String,
    pub capability_id: String,
    pub title: String,
    pub lifecycle: String,
    pub source: String,
    pub addon_version: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub workflow_extensions: Vec<String>,
    pub updated_at: String,
}

impl Default for CapabilityResolutionReport {
    fn default() -> Self {
        Self {
            schema_version: capability_resolution_schema_version(),
            status: "not_resolved".to_string(),
            goal: String::new(),
            planning_strategy: "capability_first_addon_registry".to_string(),
            required_capabilities: Vec::new(),
            missing_capabilities: Vec::new(),
            capability_suggestions: Vec::new(),
            active_addons: Vec::new(),
            available_capabilities: Vec::new(),
            workflow_extensions: Vec::new(),
            runtime_contracts: Vec::new(),
            registry_syncs: Vec::new(),
            intent_overlay: CapabilityIntentOverlay::default(),
        }
    }
}

impl CapabilityResolutionReport {
    pub fn has_capability(&self, capability_id: &str) -> bool {
        self.required_capabilities
            .iter()
            .any(|capability| capability.id == capability_id)
    }

    pub fn workflow_extensions(&self) -> BTreeSet<String> {
        self.workflow_extensions
            .iter()
            .map(|extension| extension.id.clone())
            .chain(
                self.required_capabilities
                    .iter()
                    .flat_map(|capability| capability.workflow_extensions.iter().cloned()),
            )
            .collect()
    }
}

impl CapabilityResolutionReport {
    pub fn workflow_extensions_by_addon(
        &self,
        addon_id: &str,
    ) -> Vec<&WorkflowExtensionActivation> {
        self.workflow_extensions
            .iter()
            .filter(|extension| extension.source_addon == addon_id)
            .collect()
    }

    pub fn runtime_contracts_by_addon(&self, addon_id: &str) -> Vec<&RuntimeContractActivation> {
        self.runtime_contracts
            .iter()
            .filter(|contract| contract.source_addon == addon_id)
            .collect()
    }
}

pub fn builtin_addon_catalog() -> AddonCatalog {
    finalize_catalog(
        vec![
            core_kernel_addon(),
            workflow_automation_addon(),
            visual_workspace_addon(),
            software_development_addon(),
            hackathon_factory_addon(),
            daily_goal_research_addon(),
            notification_addon(),
            async_runtime_addon(),
        ],
        Vec::new(),
    )
}

pub fn load_addon_catalog(addon_dirs: &[PathBuf]) -> Result<AddonCatalog> {
    let mut addons = builtin_addon_catalog().addons;
    let mut loaded_dirs = Vec::new();

    for addon_dir in addon_dirs {
        if !addon_dir.exists() {
            continue;
        }
        loaded_dirs.push(addon_dir.display().to_string());
        for entry in fs::read_dir(addon_dir)
            .with_context(|| format!("failed to read addon dir {}", addon_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || !is_manifest_file(&path) {
                continue;
            }
            let mut manifest = load_addon_manifest_from_path(&path)?;
            manifest.source = format!("file:{}", path.display());
            addons.push(manifest);
        }
    }

    Ok(finalize_catalog(addons, loaded_dirs))
}

pub fn load_addon_catalog_from_store(
    store: &ForgeStore,
    addon_dirs: &[PathBuf],
) -> Result<AddonCatalog> {
    load_addon_catalog_with_records(
        addon_dirs,
        store.list_installed_addons()?,
        store.list_addon_permission_authorizations(None, None, Some("approved"))?,
    )
}

pub fn load_addon_manifest_from_path(path: &Path) -> Result<AddonManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read addon manifest {}", path.display()))?;
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        serde_json::from_str(&content)
            .with_context(|| format!("invalid addon JSON manifest {}", path.display()))
    } else {
        serde_yaml::from_str(&content)
            .with_context(|| format!("invalid addon YAML manifest {}", path.display()))
    }
}

pub fn list_installed_addons(store: &ForgeStore) -> Result<InstalledAddonListReport> {
    let addons = store
        .list_installed_addons()?
        .into_iter()
        .map(installed_view_from_record)
        .collect::<Result<Vec<_>>>()?;
    Ok(InstalledAddonListReport {
        schema_version: installed_addons_schema_version(),
        status: "installed_addons_loaded".to_string(),
        addon_count: addons.len(),
        addons,
    })
}

pub fn list_addon_capability_index(
    store: &ForgeStore,
    addon_id: Option<&str>,
    capability_id: Option<&str>,
    lifecycle: Option<&str>,
) -> Result<AddonCapabilityIndexReport> {
    sync_installed_addon_capability_index(store)?;
    let capabilities = store
        .list_addon_capabilities(addon_id, capability_id, lifecycle)?
        .into_iter()
        .map(capability_index_view_from_record)
        .collect::<Result<Vec<_>>>()?;
    let enabled_count = capabilities
        .iter()
        .filter(|capability| capability.lifecycle == "enabled")
        .count();
    let disabled_count = capabilities
        .iter()
        .filter(|capability| capability.lifecycle == "disabled")
        .count();
    Ok(AddonCapabilityIndexReport {
        schema_version: addon_capability_index_schema_version(),
        status: "addon_capability_index_loaded".to_string(),
        capability_count: capabilities.len(),
        enabled_count,
        disabled_count,
        capabilities,
    })
}

pub fn list_addon_event_adapters(
    catalog: &AddonCatalog,
    addon_id: Option<&str>,
    transport: Option<&str>,
    direction: Option<&str>,
) -> AddonEventAdapterReport {
    let addon_filter = normalize_filter(addon_id);
    let transport_filter = normalize_filter(transport);
    let direction_filter = normalize_filter(direction);
    let adapters = catalog
        .addons
        .iter()
        .filter(|addon| {
            addon_filter
                .as_deref()
                .map(|filter| addon.id == filter)
                .unwrap_or(true)
        })
        .flat_map(|addon| {
            let transport_filter = transport_filter.clone();
            let direction_filter = direction_filter.clone();
            addon
                .event_adapters
                .iter()
                .filter(move |adapter| {
                    filter_matches(transport_filter.as_deref(), &adapter.transport)
                        && filter_matches(direction_filter.as_deref(), &adapter.direction)
                })
                .cloned()
                .map(|adapter| AddonEventAdapterView {
                    addon_id: addon.id.clone(),
                    addon_name: addon.name.clone(),
                    addon_version: addon.version.clone(),
                    addon_lifecycle: addon.lifecycle.clone(),
                    permission_gate: addon_permission_gate(addon, &adapter.permissions),
                    adapter,
                })
        })
        .collect::<Vec<_>>();
    AddonEventAdapterReport {
        schema_version: addon_event_adapters_schema_version(),
        status: "addon_event_adapters_loaded".to_string(),
        adapter_count: adapters.len(),
        filters: AddonEventAdapterFilters {
            addon_id: addon_filter,
            transport: transport_filter,
            direction: direction_filter,
        },
        adapters,
    }
}

pub fn addon_observability_report(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    addon_id: Option<&str>,
    lifecycle: Option<&str>,
    dispatch_limit: usize,
) -> Result<AddonObservabilityReport> {
    let addon_filter = normalize_filter(addon_id);
    let lifecycle_filter = normalize_filter(lifecycle);
    let dispatch_limit = dispatch_limit.max(1);
    let runtime_events = store.load_global_events()?;
    let mut addons = Vec::new();
    for addon in catalog.addons.iter().filter(|addon| {
        addon_filter
            .as_deref()
            .map(|filter| addon.id == filter)
            .unwrap_or(true)
            && filter_matches(lifecycle_filter.as_deref(), &addon.lifecycle)
    }) {
        addons.push(addon_observability_entry(
            store,
            addon,
            dispatch_limit,
            &runtime_events,
        )?);
    }
    let enabled_count = addons
        .iter()
        .filter(|addon| addon.addon_lifecycle == "enabled")
        .count();
    let disabled_count = addons
        .iter()
        .filter(|addon| addon.addon_lifecycle == "disabled")
        .count();
    let unauthorized_count = addons
        .iter()
        .filter(|addon| addon.addon_lifecycle == "unauthorized")
        .count();
    let totals = addon_observability_totals(&addons);
    Ok(AddonObservabilityReport {
        schema_version: addon_observability_schema_version(),
        status: "addon_observability_loaded".to_string(),
        addon_count: addons.len(),
        enabled_count,
        disabled_count,
        unauthorized_count,
        filters: AddonObservabilityFilters {
            addon_id: addon_filter,
            lifecycle: lifecycle_filter,
            dispatch_limit,
        },
        totals,
        addons,
    })
}

pub fn list_addon_runtime_contracts(
    catalog: &AddonCatalog,
    addon_id: Option<&str>,
    contract_type: Option<&str>,
    capability_id: Option<&str>,
    lifecycle: Option<&str>,
) -> AddonRuntimeContractReport {
    let addon_filter = normalize_filter(addon_id);
    let contract_type_filter = normalize_filter(contract_type);
    let capability_filter = normalize_filter(capability_id);
    let lifecycle_filter = normalize_filter(lifecycle);
    let contracts = catalog
        .addons
        .iter()
        .filter(|addon| {
            addon_filter
                .as_deref()
                .map(|filter| addon.id == filter)
                .unwrap_or(true)
                && filter_matches(lifecycle_filter.as_deref(), &addon.lifecycle)
        })
        .flat_map(|addon| {
            let contract_type_filter = contract_type_filter.clone();
            let capability_filter = capability_filter.clone();
            addon
                .runtime_contracts
                .iter()
                .filter(move |contract| {
                    filter_matches(contract_type_filter.as_deref(), &contract.contract_type)
                        && capability_filter
                            .as_deref()
                            .map(|filter| contract.capability_id == filter)
                            .unwrap_or(true)
                })
                .cloned()
                .map(|contract| AddonRuntimeContractView {
                    addon_id: addon.id.clone(),
                    addon_name: addon.name.clone(),
                    addon_version: addon.version.clone(),
                    addon_lifecycle: addon.lifecycle.clone(),
                    permission_gate: addon_permission_gate(addon, &contract.permissions),
                    contract,
                })
        })
        .collect::<Vec<_>>();
    AddonRuntimeContractReport {
        schema_version: addon_runtime_contracts_schema_version(),
        status: "addon_runtime_contracts_loaded".to_string(),
        contract_count: contracts.len(),
        filters: AddonRuntimeContractFilters {
            addon_id: addon_filter,
            contract_type: contract_type_filter,
            capability_id: capability_filter,
            lifecycle: lifecycle_filter,
        },
        contracts,
    }
}

pub fn list_addon_planner_registry(
    catalog: &AddonCatalog,
    addon_id: Option<&str>,
    capability_id: Option<&str>,
    workflow_extension_id: Option<&str>,
    lifecycle: Option<&str>,
) -> AddonPlannerRegistryReport {
    let addon_filter = normalize_filter(addon_id);
    let capability_filter = normalize_filter(capability_id);
    let workflow_extension_filter = normalize_filter(workflow_extension_id);
    let lifecycle_filter = normalize_filter(lifecycle);
    let mut planners = Vec::new();

    for contract_type in ["planning_strategy", "replanning_strategy"] {
        let report = evaluate_addon_runtime_contract_policy(
            catalog,
            addon_filter.as_deref(),
            None,
            Some(contract_type),
            capability_filter.as_deref(),
            lifecycle_filter.as_deref(),
        );
        planners.extend(report.contracts.into_iter().filter_map(|entry| {
            if !filter_matches(
                workflow_extension_filter.as_deref(),
                &entry.contract.workflow_extension_id,
            ) {
                return None;
            }
            Some(planner_registration_from_policy_entry(entry))
        }));
    }

    planners.sort_by(|left, right| {
        left.addon_id
            .cmp(&right.addon_id)
            .then_with(|| left.capability_id.cmp(&right.capability_id))
            .then_with(|| left.workflow_extension_id.cmp(&right.workflow_extension_id))
            .then_with(|| left.contract_id.cmp(&right.contract_id))
    });

    AddonPlannerRegistryReport {
        schema_version: addon_planner_registry_schema_version(),
        status: "addon_planner_registry_loaded".to_string(),
        planner_count: planners.len(),
        filters: AddonPlannerRegistryFilters {
            addon_id: addon_filter,
            capability_id: capability_filter,
            workflow_extension_id: workflow_extension_filter,
            lifecycle: lifecycle_filter,
        },
        planners,
    }
}

pub fn evaluate_addon_runtime_contract_policy(
    catalog: &AddonCatalog,
    addon_id: Option<&str>,
    contract_id: Option<&str>,
    contract_type: Option<&str>,
    capability_id: Option<&str>,
    lifecycle: Option<&str>,
) -> AddonRuntimeContractPolicyReport {
    let contract_filter = normalize_filter(contract_id);
    let report =
        list_addon_runtime_contracts(catalog, addon_id, contract_type, capability_id, lifecycle);
    let contracts = report
        .contracts
        .into_iter()
        .filter(|contract| {
            contract_filter
                .as_deref()
                .map(|filter| contract.contract.id == filter)
                .unwrap_or(true)
        })
        .map(runtime_contract_policy_entry)
        .collect::<Vec<_>>();
    let dispatch_allowed_count = contracts
        .iter()
        .filter(|contract| contract.dispatch_allowed)
        .count();
    let blocked_count = contracts.len().saturating_sub(dispatch_allowed_count);
    AddonRuntimeContractPolicyReport {
        schema_version: addon_runtime_contract_policy_schema_version(),
        status: if blocked_count == 0 {
            "runtime_contract_policy_ready".to_string()
        } else {
            "runtime_contract_policy_blocked".to_string()
        },
        contract_count: contracts.len(),
        dispatch_allowed_count,
        blocked_count,
        filters: AddonRuntimeContractPolicyFilters {
            addon_id: normalize_filter(addon_id),
            contract_id: contract_filter,
            contract_type: normalize_filter(contract_type),
            capability_id: normalize_filter(capability_id),
            lifecycle: normalize_filter(lifecycle),
        },
        contracts,
    }
}

pub fn enqueue_addon_runtime_contract_dispatch(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    addon_id: Option<&str>,
    contract_id: &str,
    input: serde_json::Value,
    source: &str,
    dry_run: bool,
) -> Result<AddonRuntimeContractDispatchReport> {
    let policy = evaluate_addon_runtime_contract_policy(
        catalog,
        addon_id,
        Some(contract_id),
        None,
        None,
        None,
    );
    if policy.contracts.is_empty() {
        bail!("runtime contract not found: {contract_id}");
    }
    if policy.contracts.len() > 1 {
        bail!("runtime contract id is ambiguous across Addons: {contract_id}");
    }
    let policy_entry = policy.contracts.into_iter().next().unwrap();
    let dispatch_id = format!("rtcd_{}", Uuid::new_v4().to_string().replace('-', ""));
    let status = if policy_entry.dispatch_allowed {
        if dry_run {
            "dry_run"
        } else {
            "queued"
        }
    } else {
        "blocked"
    };
    let data = serde_json::json!({
        "dispatch_contract": "forge.addon_runtime_contract_dispatch.v1",
        "source": source,
        "dry_run": dry_run,
        "policy_status": policy_entry.status,
        "dispatch_allowed": policy_entry.dispatch_allowed,
    });
    let dispatch = AddonRuntimeContractDispatchEntry {
        id: dispatch_id,
        addon_id: policy_entry.addon_id.clone(),
        contract_id: policy_entry.contract_id.clone(),
        contract_type: policy_entry.contract_type.clone(),
        capability_id: policy_entry.capability_id.clone(),
        runtime: policy_entry.runtime.clone(),
        entrypoint: policy_entry.entrypoint.clone(),
        status: status.to_string(),
        source: source.to_string(),
        input,
        policy: policy_entry,
        data,
        created_at: String::new(),
        updated_at: String::new(),
    };

    if !dry_run {
        let policy_value = serde_json::to_value(&dispatch.policy)?;
        store.save_runtime_contract_dispatch(RuntimeContractDispatchWrite {
            id: &dispatch.id,
            addon_id: &dispatch.addon_id,
            contract_id: &dispatch.contract_id,
            contract_type: &dispatch.contract_type,
            capability_id: &dispatch.capability_id,
            runtime: &dispatch.runtime,
            entrypoint: &dispatch.entrypoint,
            status: &dispatch.status,
            source: &dispatch.source,
            input: &dispatch.input,
            policy: &policy_value,
            data: &dispatch.data,
        })?;
    }

    Ok(dispatch_report(
        if dispatch.status == "queued" {
            "runtime_contract_dispatch_queued"
        } else if dispatch.status == "dry_run" {
            "runtime_contract_dispatch_dry_run"
        } else {
            "runtime_contract_dispatch_blocked"
        },
        dry_run,
        vec![dispatch],
    ))
}

pub fn enqueue_addon_planner_dispatch(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    input: AddonPlannerDispatchInput<'_>,
) -> Result<AddonRuntimeContractDispatchReport> {
    let policy = evaluate_addon_runtime_contract_policy(
        catalog,
        input.addon_id,
        Some(input.contract_id),
        None,
        None,
        None,
    );
    if policy.contracts.is_empty() {
        bail!("planner runtime contract not found: {}", input.contract_id);
    }
    if policy.contracts.len() > 1 {
        bail!(
            "planner runtime contract id is ambiguous across Addons: {}",
            input.contract_id
        );
    }
    let policy_entry = policy.contracts.into_iter().next().unwrap();
    if !matches!(
        policy_entry.contract_type.as_str(),
        "planning_strategy" | "replanning_strategy"
    ) {
        bail!(
            "runtime contract {} is not a planner strategy: {}",
            input.contract_id,
            policy_entry.contract_type
        );
    }

    let dispatch_payload = serde_json::json!({
        "schema_version": ADDON_PLANNER_DISPATCH_INPUT_SCHEMA_VERSION,
        "goal": input.goal,
        "constraints": input.constraints,
        "workflow_id": input.workflow_id,
        "task_id": input.task_id,
        "planner": {
            "addon_id": policy_entry.addon_id,
            "contract_id": policy_entry.contract_id,
            "contract_type": policy_entry.contract_type,
            "capability_id": policy_entry.capability_id,
            "workflow_extension_id": policy_entry.contract.workflow_extension_id,
            "runtime": policy_entry.runtime,
            "entrypoint": policy_entry.entrypoint,
        },
        "context": input.context,
        "requested_at": Utc::now().to_rfc3339(),
    });
    enqueue_addon_runtime_contract_dispatch(
        store,
        catalog,
        input.addon_id,
        input.contract_id,
        dispatch_payload,
        input.source,
        input.dry_run,
    )
}

pub fn execute_addon_planning_strategy(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    input: AddonPlanningStrategyInput<'_>,
) -> Result<AddonPlanningStrategyExecutionReport> {
    let core_workflow = create_workflow(parse_intent_with_catalog(input.dispatch.goal, catalog));
    let core_tasks = core_workflow
        .tasks
        .iter()
        .map(addon_planner_task_shape_from_atomic_task)
        .collect::<Vec<_>>();
    let core_plan_sha256 = hex_sha256(&serde_json::to_vec(&core_tasks)?);
    let dispatch_context = addon_planner_dispatch_context(
        input.dispatch.context.clone(),
        &core_workflow,
        &core_tasks,
        &core_plan_sha256,
    )?;
    let enqueue_report = enqueue_addon_planner_dispatch(
        store,
        catalog,
        AddonPlannerDispatchInput {
            addon_id: input.dispatch.addon_id,
            contract_id: input.dispatch.contract_id,
            goal: input.dispatch.goal,
            constraints: input.dispatch.constraints,
            workflow_id: input.dispatch.workflow_id,
            task_id: input.dispatch.task_id,
            context: dispatch_context,
            source: input.dispatch.source,
            dry_run: input.dispatch.dry_run,
        },
    )?;

    if input.dispatch.dry_run || enqueue_report.blocked_count > 0 {
        let validation = addon_planning_strategy_validation_not_executed(
            if input.dispatch.dry_run {
                "dry_run"
            } else {
                "dispatch_blocked"
            },
            if input.dispatch.dry_run {
                "dry-run does not execute the planner worker"
            } else {
                "planner dispatch is blocked by runtime contract policy"
            },
        );
        let equivalence =
            addon_planning_strategy_equivalence_not_ready(&core_tasks, &[], &core_plan_sha256)?;
        return Ok(addon_planning_strategy_execution_report(
            AddonPlanningStrategyReportInput {
                status: if input.dispatch.dry_run {
                    "planning_strategy_execution_dry_run"
                } else {
                    "planning_strategy_dispatch_blocked"
                },
                goal: input.dispatch.goal,
                contract_id: input.dispatch.contract_id,
                worker_id: input.worker_id,
                source: input.dispatch.source,
                dry_run: input.dispatch.dry_run,
                dispatch_report: enqueue_report,
                strategy_result: None,
                validation,
                equivalence,
            },
        ));
    }

    let dispatch_id = enqueue_report
        .dispatches
        .first()
        .map(|dispatch| dispatch.id.clone())
        .with_context(|| "planner dispatch report did not include dispatch entry")?;
    let execution_report = execute_addon_runtime_contract_dispatch(
        store,
        catalog,
        &dispatch_id,
        input.worker_id,
        input.lease_seconds,
        false,
    )?;
    let strategy_result = addon_planning_strategy_result_from_dispatch(&execution_report);
    let validation = validate_addon_planning_strategy_result(strategy_result.as_ref())?;
    let equivalence =
        compare_addon_planning_strategy_to_core(&core_tasks, &validation.tasks, &core_plan_sha256)?;
    let status = if execution_report.completed_count == 0 {
        "planning_strategy_execution_failed"
    } else if validation.status != "valid" {
        "planning_strategy_result_invalid"
    } else if equivalence.replacement_ready {
        "planning_strategy_equivalence_validated"
    } else {
        "planning_strategy_equivalence_review_required"
    };

    Ok(addon_planning_strategy_execution_report(
        AddonPlanningStrategyReportInput {
            status,
            goal: input.dispatch.goal,
            contract_id: input.dispatch.contract_id,
            worker_id: input.worker_id,
            source: input.dispatch.source,
            dry_run: false,
            dispatch_report: execution_report,
            strategy_result,
            validation,
            equivalence,
        },
    ))
}

fn addon_planner_task_shape_from_atomic_task(task: &AtomicTask) -> AddonPlanningStrategyTaskShape {
    AddonPlanningStrategyTaskShape {
        id: task.id.clone(),
        title: task.title.clone(),
        dependencies: task.dependencies.clone(),
        executor: executor_kind_name(&task.executor).to_string(),
        context_requirement_count: task.context_requirements.len(),
        validation_rule_count: task.validation_rules.len(),
        expected_output_present: !task.expected_output.trim().is_empty(),
    }
}

fn executor_kind_name(executor: &ExecutorKind) -> &'static str {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
}

fn addon_planner_dispatch_context(
    context: serde_json::Value,
    core_workflow: &Workflow,
    core_tasks: &[AddonPlanningStrategyTaskShape],
    core_plan_sha256: &str,
) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "provided_context": context,
        "core_reference": {
            "schema_version": "forge.addon_planning_strategy_core_reference.v1",
            "workflow_id": core_workflow.id,
            "intent": core_workflow.intent,
            "task_count": core_tasks.len(),
            "tasks": core_tasks,
            "atomic_tasks": core_workflow.tasks,
            "plan_sha256": core_plan_sha256,
            "replacement_policy": "external planning strategies are advisory until schema and equivalence validation pass",
        }
    }))
}

fn addon_planning_strategy_validation_not_executed(
    status: &str,
    issue: &str,
) -> AddonPlanningStrategyResultValidation {
    AddonPlanningStrategyResultValidation {
        schema_version: ADDON_PLANNING_STRATEGY_RESULT_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        task_count: 0,
        issue_count: 1,
        issues: vec![issue.to_string()],
        tasks: Vec::new(),
    }
}

fn addon_planning_strategy_equivalence_not_ready(
    core_tasks: &[AddonPlanningStrategyTaskShape],
    external_tasks: &[AddonPlanningStrategyTaskShape],
    core_plan_sha256: &str,
) -> Result<AddonPlanningStrategyEquivalence> {
    let mut equivalence =
        compare_addon_planning_strategy_to_core(core_tasks, external_tasks, core_plan_sha256)?;
    equivalence.status = "not_executed".to_string();
    equivalence.replacement_ready = false;
    equivalence
        .notes
        .push("planner worker was not executed, so equivalence cannot be proven".to_string());
    Ok(equivalence)
}

fn addon_planning_strategy_result_from_dispatch(
    report: &AddonRuntimeContractDispatchReport,
) -> Option<serde_json::Value> {
    report
        .dispatches
        .first()
        .and_then(|dispatch| dispatch.data.pointer("/runtime_processing/outcome/result"))
        .cloned()
}

fn validate_addon_planning_strategy_result(
    result: Option<&serde_json::Value>,
) -> Result<AddonPlanningStrategyResultValidation> {
    let mut issues = Vec::new();
    let Some(result) = result else {
        return Ok(addon_planning_strategy_validation_not_executed(
            "missing_result",
            "planner dispatch did not return a result payload",
        ));
    };
    let tasks_value = result
        .get("tasks")
        .or_else(|| result.pointer("/plan/tasks"))
        .or_else(|| result.pointer("/workflow/tasks"));
    let Some(tasks_value) = tasks_value else {
        return Ok(addon_planning_strategy_validation_not_executed(
            "invalid",
            "planner result must include tasks, plan.tasks or workflow.tasks",
        ));
    };
    let Some(tasks_array) = tasks_value.as_array() else {
        return Ok(addon_planning_strategy_validation_not_executed(
            "invalid",
            "planner result tasks must be an array",
        ));
    };

    let mut task_shapes = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for (index, task) in tasks_array.iter().enumerate() {
        let id = task
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let title = task
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if id.is_empty() {
            issues.push(format!("task[{index}] is missing id"));
        }
        if title.is_empty() {
            issues.push(format!("task[{index}] is missing title"));
        }
        if !id.is_empty() && !seen_ids.insert(id.clone()) {
            issues.push(format!("duplicate task id: {id}"));
        }
        let dependencies = task
            .get("dependencies")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let executor = task
            .get("executor")
            .and_then(|value| value.as_str())
            .or_else(|| {
                task.pointer("/executor/kind")
                    .and_then(|value| value.as_str())
            })
            .unwrap_or("mixed")
            .to_string();
        let context_requirement_count = task
            .get("context_requirements")
            .and_then(|value| value.as_array())
            .map(Vec::len)
            .unwrap_or(0);
        let validation_rule_count = task
            .get("validation_rules")
            .and_then(|value| value.as_array())
            .map(Vec::len)
            .unwrap_or(0);
        if validation_rule_count == 0 {
            issues.push(format!("task {id} has no validation rules"));
        }
        let expected_output_present = task
            .get("expected_output")
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if !expected_output_present {
            issues.push(format!("task {id} is missing expected_output"));
        }
        task_shapes.push(AddonPlanningStrategyTaskShape {
            id,
            title,
            dependencies,
            executor,
            context_requirement_count,
            validation_rule_count,
            expected_output_present,
        });
    }

    for task in &task_shapes {
        for dependency in &task.dependencies {
            if !seen_ids.contains(dependency) {
                issues.push(format!(
                    "task {} depends on unknown task id {}",
                    task.id, dependency
                ));
            }
        }
    }

    let status = if issues.is_empty() {
        "valid"
    } else {
        "invalid"
    };
    Ok(AddonPlanningStrategyResultValidation {
        schema_version: ADDON_PLANNING_STRATEGY_RESULT_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        task_count: task_shapes.len(),
        issue_count: issues.len(),
        issues,
        tasks: task_shapes,
    })
}

fn compare_addon_planning_strategy_to_core(
    core_tasks: &[AddonPlanningStrategyTaskShape],
    external_tasks: &[AddonPlanningStrategyTaskShape],
    core_plan_sha256: &str,
) -> Result<AddonPlanningStrategyEquivalence> {
    let core_by_id = core_tasks
        .iter()
        .map(|task| (task.id.clone(), task))
        .collect::<BTreeMap<_, _>>();
    let external_by_id = external_tasks
        .iter()
        .map(|task| (task.id.clone(), task))
        .collect::<BTreeMap<_, _>>();
    let missing_core_task_ids = core_by_id
        .keys()
        .filter(|id| !external_by_id.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    let extra_external_task_ids = external_by_id
        .keys()
        .filter(|id| !core_by_id.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    let matching_task_id_count = core_by_id
        .keys()
        .filter(|id| external_by_id.contains_key(*id))
        .count();
    let matching_task_title_count = core_by_id
        .iter()
        .filter(|(id, core)| {
            external_by_id
                .get(*id)
                .map(|external| external.title == core.title)
                .unwrap_or(false)
        })
        .count();
    let mut dependency_issues = Vec::new();
    for (id, external) in &external_by_id {
        let Some(core) = core_by_id.get(id) else {
            continue;
        };
        let core_dependencies = core.dependencies.iter().collect::<BTreeSet<_>>();
        let external_dependencies = external.dependencies.iter().collect::<BTreeSet<_>>();
        if core_dependencies != external_dependencies {
            dependency_issues.push(format!(
                "task {id} dependency set differs from Core reference"
            ));
        }
    }
    let core_validation_rules = core_tasks
        .iter()
        .map(|task| task.validation_rule_count)
        .sum::<usize>();
    let covered_validation_rules = external_tasks
        .iter()
        .filter_map(|task| core_by_id.get(&task.id).map(|core| (task, *core)))
        .map(|(external, core)| {
            external
                .validation_rule_count
                .min(core.validation_rule_count)
        })
        .sum::<usize>();
    let validation_rule_coverage_percent = if core_validation_rules == 0 {
        100.0
    } else {
        ((covered_validation_rules as f64 / core_validation_rules as f64) * 10000.0).round() / 100.0
    };
    let external_plan_sha256 = hex_sha256(&serde_json::to_vec(external_tasks)?);
    let replacement_ready = !core_tasks.is_empty()
        && core_tasks.len() == external_tasks.len()
        && missing_core_task_ids.is_empty()
        && extra_external_task_ids.is_empty()
        && matching_task_title_count == core_tasks.len()
        && dependency_issues.is_empty()
        && validation_rule_coverage_percent >= 100.0;
    let status = if replacement_ready {
        "equivalent"
    } else if external_tasks.is_empty() {
        "not_ready"
    } else {
        "review_required"
    };
    let mut notes = Vec::new();
    if replacement_ready {
        notes.push(
            "external planner matches the Core reference shape and can be considered for controlled promotion"
                .to_string(),
        );
    } else {
        notes.push(
            "external planner result remains advisory until schema and equivalence gaps are resolved"
                .to_string(),
        );
    }
    Ok(AddonPlanningStrategyEquivalence {
        schema_version: "forge.addon_planning_strategy_equivalence.v1".to_string(),
        status: status.to_string(),
        replacement_ready,
        core_task_count: core_tasks.len(),
        external_task_count: external_tasks.len(),
        matching_task_id_count,
        matching_task_title_count,
        missing_core_task_ids,
        extra_external_task_ids,
        dependency_issue_count: dependency_issues.len(),
        dependency_issues,
        validation_rule_coverage_percent,
        core_plan_sha256: core_plan_sha256.to_string(),
        external_plan_sha256,
        notes,
    })
}

fn addon_planning_strategy_execution_report(
    input: AddonPlanningStrategyReportInput<'_>,
) -> AddonPlanningStrategyExecutionReport {
    AddonPlanningStrategyExecutionReport {
        schema_version: ADDON_PLANNING_STRATEGY_EXECUTION_SCHEMA_VERSION.to_string(),
        status: input.status.to_string(),
        goal: input.goal.to_string(),
        contract_id: input.contract_id.to_string(),
        worker_id: input.worker_id.to_string(),
        source: input.source.to_string(),
        dry_run: input.dry_run,
        dispatch_report: input.dispatch_report,
        strategy_result: input.strategy_result,
        validation: input.validation,
        equivalence: input.equivalence,
    }
}

pub fn list_addon_runtime_contract_dispatches(
    store: &ForgeStore,
    addon_id: Option<&str>,
    contract_id: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<AddonRuntimeContractDispatchReport> {
    let dispatches = store
        .list_runtime_contract_dispatches(addon_id, contract_id, status, limit)?
        .into_iter()
        .map(stored_runtime_dispatch_entry)
        .collect::<Result<Vec<_>>>()?;
    Ok(dispatch_report(
        "runtime_contract_dispatches_loaded",
        false,
        dispatches,
    ))
}

pub fn register_addon_runtime_worker(
    store: &ForgeStore,
    worker_id: &str,
    runtime: &str,
    status: &str,
    trust_level: &str,
    source: &str,
    data: serde_json::Value,
) -> Result<AddonRuntimeWorkerReport> {
    if worker_id.trim().is_empty() {
        bail!("runtime worker id is required");
    }
    if runtime.trim().is_empty() {
        bail!("runtime worker runtime is required");
    }
    let status = if status.trim().is_empty() {
        "available"
    } else {
        status
    };
    let trust_level = if trust_level.trim().is_empty() {
        "local"
    } else {
        trust_level
    };
    store.save_runtime_worker(RuntimeWorkerWrite {
        id: worker_id,
        runtime,
        status,
        trust_level,
        source,
        data: &data,
    })?;
    let mut report =
        list_addon_runtime_workers(store, Some(runtime), Some(status), Some(trust_level), 20)?;
    report.status = "runtime_worker_registered".to_string();
    Ok(report)
}

pub fn list_addon_runtime_workers(
    store: &ForgeStore,
    runtime: Option<&str>,
    status: Option<&str>,
    trust_level: Option<&str>,
    limit: usize,
) -> Result<AddonRuntimeWorkerReport> {
    let workers = store
        .list_runtime_workers(runtime, status, trust_level, limit)?
        .into_iter()
        .map(runtime_worker_entry)
        .collect::<Vec<_>>();
    Ok(runtime_worker_report(
        "runtime_workers_loaded",
        runtime,
        status,
        trust_level,
        workers,
    ))
}

pub fn run_addon_runtime_contract_dispatch(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    dispatch_id: &str,
    worker: &str,
    dry_run: bool,
) -> Result<AddonRuntimeContractDispatchReport> {
    let record = store
        .load_runtime_contract_dispatch(dispatch_id)?
        .with_context(|| format!("runtime contract dispatch not found: {dispatch_id}"))?;
    let entry = stored_runtime_dispatch_entry(record)?;
    if entry.status != "queued" {
        return Ok(dispatch_report(
            "runtime_contract_dispatch_not_queued",
            dry_run,
            vec![entry],
        ));
    }

    let policy = evaluate_addon_runtime_contract_policy(
        catalog,
        Some(&entry.addon_id),
        Some(&entry.contract_id),
        None,
        None,
        None,
    );
    if policy.contracts.len() != 1 {
        let detail = if policy.contracts.is_empty() {
            "runtime contract missing during dispatch policy recheck"
        } else {
            "runtime contract ambiguous during dispatch policy recheck"
        };
        let prior_policy = entry.policy.clone();
        return update_runtime_dispatch_entry(
            store,
            entry,
            RuntimeDispatchUpdateInput {
                policy: prior_policy,
                status: "blocked",
                report_status: "runtime_contract_dispatch_blocked",
                worker,
                dry_run,
                outcome: serde_json::json!({
                    "outcome": "policy_recheck_failed",
                    "reason": detail,
                }),
            },
        );
    }

    let policy_entry = policy.contracts.into_iter().next().unwrap();
    if !policy_entry.dispatch_allowed {
        return update_runtime_dispatch_entry(
            store,
            entry,
            RuntimeDispatchUpdateInput {
                policy: policy_entry,
                status: "blocked",
                report_status: "runtime_contract_dispatch_blocked",
                worker,
                dry_run,
                outcome: serde_json::json!({
                    "outcome": "policy_recheck_failed",
                    "reason": "runtime contract is no longer dispatchable",
                }),
            },
        );
    }

    if policy_entry.contract_type != entry.contract_type
        || policy_entry.capability_id != entry.capability_id
        || policy_entry.runtime != entry.runtime
        || policy_entry.entrypoint != entry.entrypoint
    {
        return update_runtime_dispatch_entry(
            store,
            entry,
            RuntimeDispatchUpdateInput {
                policy: policy_entry,
                status: "blocked",
                report_status: "runtime_contract_dispatch_blocked",
                worker,
                dry_run,
                outcome: serde_json::json!({
                    "outcome": "contract_changed_after_enqueue",
                    "reason": "runtime contract shape changed after the dispatch was queued",
                }),
            },
        );
    }

    if entry.runtime == "forge_core_builtin" {
        if let Some(output) = execute_builtin_runtime_contract(&entry) {
            return update_runtime_dispatch_entry(
                store,
                entry,
                RuntimeDispatchUpdateInput {
                    policy: policy_entry,
                    status: "completed",
                    report_status: "runtime_contract_dispatch_completed",
                    worker,
                    dry_run,
                    outcome: serde_json::json!({
                        "outcome": "completed",
                        "runtime": "forge_core_builtin",
                        "output": output,
                    }),
                },
            );
        }
        let entrypoint = entry.entrypoint.clone();
        return update_runtime_dispatch_entry(
            store,
            entry,
            RuntimeDispatchUpdateInput {
                policy: policy_entry,
                status: "blocked",
                report_status: "runtime_contract_dispatch_blocked",
                worker,
                dry_run,
                outcome: serde_json::json!({
                    "outcome": "unsupported_builtin_entrypoint",
                    "runtime": "forge_core_builtin",
                    "entrypoint": entrypoint,
                }),
            },
        );
    }

    let runtime = entry.runtime.clone();
    let entrypoint = entry.entrypoint.clone();
    let eligible_workers = store
        .list_runtime_workers(Some(&runtime), Some("available"), None, 20)?
        .into_iter()
        .map(runtime_worker_entry)
        .collect::<Vec<_>>();
    let eligible_worker_count = eligible_workers.len();
    update_runtime_dispatch_entry(
        store,
        entry,
        RuntimeDispatchUpdateInput {
            policy: policy_entry,
            status: "needs_external_worker",
            report_status: "runtime_contract_dispatch_needs_external_worker",
            worker,
            dry_run,
            outcome: serde_json::json!({
                "outcome": "needs_external_worker",
                "runtime": runtime,
                "entrypoint": entrypoint,
                "eligible_worker_count": eligible_worker_count,
                "eligible_workers": eligible_workers,
                "reason": "Forge Core records the dispatch but does not execute external runtime code inline",
            }),
        },
    )
}

pub fn execute_addon_runtime_contract_dispatch(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    dispatch_id: &str,
    worker_id: &str,
    lease_seconds: u64,
    dry_run: bool,
) -> Result<AddonRuntimeContractDispatchReport> {
    let mut entry = load_runtime_dispatch_entry(store, dispatch_id)?;
    if dry_run {
        return Ok(runtime_dispatch_preview_report(
            entry,
            None,
            "runtime_contract_dispatch_local_worker_dry_run",
            worker_id,
            true,
            serde_json::json!({
                "outcome": "local_process_worker_dry_run",
                "worker_id": worker_id,
                "current_status": "preview_only",
                "reason": "dry-run does not claim, execute or complete the dispatch",
            }),
        ));
    }

    if entry.status == "queued" {
        let preflight =
            run_addon_runtime_contract_dispatch(store, catalog, dispatch_id, worker_id, false)?;
        let status = preflight
            .dispatches
            .first()
            .map(|dispatch| dispatch.status.as_str())
            .unwrap_or("");
        if status != "needs_external_worker" {
            return Ok(preflight);
        }
        entry = load_runtime_dispatch_entry(store, dispatch_id)?;
    }

    if entry.status == "needs_external_worker" {
        let claim = claim_addon_runtime_contract_dispatch(
            store,
            catalog,
            dispatch_id,
            worker_id,
            lease_seconds,
            false,
        )?;
        if claim.status != "runtime_contract_dispatch_claimed" {
            return Ok(claim);
        }
        entry = load_runtime_dispatch_entry(store, dispatch_id)?;
    }

    if entry.status != "claimed_external_worker" {
        return Ok(dispatch_report(
            "runtime_contract_dispatch_not_claimed",
            false,
            vec![entry],
        ));
    }

    let claimed_worker_id = claimed_external_worker_id(&entry);
    if claimed_worker_id.as_deref() != Some(worker_id) {
        return Ok(runtime_dispatch_preview_report(
            entry,
            None,
            "runtime_contract_dispatch_worker_rejected",
            worker_id,
            false,
            serde_json::json!({
                "outcome": "worker_does_not_own_dispatch",
                "worker_id": worker_id,
                "claimed_worker_id": claimed_worker_id,
            }),
        ));
    }

    let worker = match store.load_runtime_worker(worker_id)? {
        Some(worker) => runtime_worker_entry(worker),
        None => {
            return Ok(runtime_dispatch_preview_report(
                entry,
                None,
                "runtime_contract_dispatch_worker_rejected",
                worker_id,
                false,
                serde_json::json!({
                    "outcome": "worker_not_registered",
                    "worker_id": worker_id,
                }),
            ));
        }
    };

    let recheck = recheck_runtime_dispatch_policy(catalog, &entry);
    if !recheck.allowed {
        return update_runtime_dispatch_entry(
            store,
            entry,
            RuntimeDispatchUpdateInput {
                policy: recheck.policy,
                status: "blocked",
                report_status: "runtime_contract_dispatch_blocked",
                worker: worker_id,
                dry_run: false,
                outcome: recheck.outcome.unwrap_or_else(|| {
                    serde_json::json!({
                        "outcome": "policy_recheck_failed",
                        "reason": "runtime contract is no longer dispatchable",
                    })
                }),
            },
        );
    }

    let execution = match execute_registered_runtime_worker(&worker, &entry) {
        Ok(execution) => execution,
        Err(error) => {
            return update_runtime_dispatch_entry(
                store,
                entry,
                RuntimeDispatchUpdateInput {
                    policy: recheck.policy,
                    status: "blocked",
                    report_status: "runtime_contract_dispatch_worker_rejected",
                    worker: worker_id,
                    dry_run: false,
                    outcome: serde_json::json!({
                        "outcome": "runtime_worker_rejected",
                        "worker_id": worker_id,
                        "reason": error.to_string(),
                    }),
                },
            );
        }
    };
    complete_addon_runtime_contract_dispatch(
        store,
        catalog,
        AddonRuntimeContractCompletionInput {
            dispatch_id,
            worker_id,
            completion_status: &execution.status,
            result: execution.result,
            signature: execution.signature.as_deref(),
            attestation: execution.attestation,
            dry_run: false,
        },
    )
}

pub fn claim_addon_runtime_contract_dispatch(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    dispatch_id: &str,
    worker_id: &str,
    lease_seconds: u64,
    dry_run: bool,
) -> Result<AddonRuntimeContractDispatchReport> {
    let record = store
        .load_runtime_contract_dispatch(dispatch_id)?
        .with_context(|| format!("runtime contract dispatch not found: {dispatch_id}"))?;
    let entry = stored_runtime_dispatch_entry(record)?;
    if entry.status != "needs_external_worker" {
        return Ok(dispatch_report(
            "runtime_contract_dispatch_not_claimable",
            dry_run,
            vec![entry],
        ));
    }
    let worker = match store.load_runtime_worker(worker_id)? {
        Some(worker) => runtime_worker_entry(worker),
        None => {
            return Ok(runtime_dispatch_preview_report(
                entry,
                None,
                "runtime_contract_dispatch_worker_rejected",
                worker_id,
                dry_run,
                serde_json::json!({
                    "outcome": "worker_not_registered",
                    "worker_id": worker_id,
                }),
            ));
        }
    };
    let recheck = recheck_runtime_dispatch_policy(catalog, &entry);
    if !recheck.allowed {
        return update_runtime_dispatch_entry(
            store,
            entry,
            RuntimeDispatchUpdateInput {
                policy: recheck.policy,
                status: "blocked",
                report_status: "runtime_contract_dispatch_blocked",
                worker: worker_id,
                dry_run,
                outcome: recheck.outcome.unwrap_or_else(|| {
                    serde_json::json!({
                        "outcome": "policy_recheck_failed",
                        "reason": "runtime contract is no longer dispatchable",
                    })
                }),
            },
        );
    }
    if worker.runtime != entry.runtime {
        let dispatch_runtime = entry.runtime.clone();
        let worker_runtime = worker.runtime.clone();
        return Ok(runtime_dispatch_preview_report(
            entry,
            Some(recheck.policy),
            "runtime_contract_dispatch_worker_rejected",
            worker_id,
            dry_run,
            serde_json::json!({
                "outcome": "worker_runtime_mismatch",
                "worker_id": worker.id,
                "worker_runtime": worker_runtime,
                "dispatch_runtime": dispatch_runtime,
            }),
        ));
    }
    if worker.status != "available" {
        return Ok(runtime_dispatch_preview_report(
            entry,
            Some(recheck.policy),
            "runtime_contract_dispatch_worker_rejected",
            worker_id,
            dry_run,
            serde_json::json!({
                "outcome": "worker_not_available",
                "worker_id": worker.id,
                "worker_status": worker.status,
            }),
        ));
    }

    update_runtime_dispatch_entry(
        store,
        entry,
        RuntimeDispatchUpdateInput {
            policy: recheck.policy,
            status: "claimed_external_worker",
            report_status: "runtime_contract_dispatch_claimed",
            worker: worker_id,
            dry_run,
            outcome: serde_json::json!({
                "outcome": "claimed_external_worker",
                "claim": {
                    "worker_id": worker.id.clone(),
                    "runtime": worker.runtime.clone(),
                    "trust_level": worker.trust_level.clone(),
                    "lease_seconds": lease_seconds,
                    "worker": worker,
                },
            }),
        },
    )
}

pub fn complete_addon_runtime_contract_dispatch(
    store: &ForgeStore,
    catalog: &AddonCatalog,
    input: AddonRuntimeContractCompletionInput<'_>,
) -> Result<AddonRuntimeContractDispatchReport> {
    let final_status = match input.completion_status {
        "completed" | "success" => "completed",
        "failed" | "failure" | "error" => "failed",
        _ => bail!("completion status must be completed or failed"),
    };
    let record = store
        .load_runtime_contract_dispatch(input.dispatch_id)?
        .with_context(|| format!("runtime contract dispatch not found: {}", input.dispatch_id))?;
    let entry = stored_runtime_dispatch_entry(record)?;
    if entry.status != "claimed_external_worker" {
        return Ok(dispatch_report(
            "runtime_contract_dispatch_not_claimed",
            input.dry_run,
            vec![entry],
        ));
    }
    let worker = match store.load_runtime_worker(input.worker_id)? {
        Some(worker) => runtime_worker_entry(worker),
        None => {
            return Ok(runtime_dispatch_preview_report(
                entry,
                None,
                "runtime_contract_dispatch_completion_rejected",
                input.worker_id,
                input.dry_run,
                serde_json::json!({
                    "outcome": "worker_not_registered",
                    "worker_id": input.worker_id,
                }),
            ));
        }
    };
    let claimed_worker_id = claimed_external_worker_id(&entry);
    if claimed_worker_id.as_deref() != Some(input.worker_id) {
        return Ok(runtime_dispatch_preview_report(
            entry,
            None,
            "runtime_contract_dispatch_completion_rejected",
            input.worker_id,
            input.dry_run,
            serde_json::json!({
                "outcome": "worker_does_not_own_dispatch",
                "worker_id": input.worker_id,
                "claimed_worker_id": claimed_worker_id,
            }),
        ));
    }
    let claimed_worker = match claimed_external_worker_snapshot(&entry) {
        Some(worker) => worker,
        None => {
            return Ok(runtime_dispatch_preview_report(
                entry,
                None,
                "runtime_contract_dispatch_completion_rejected",
                input.worker_id,
                input.dry_run,
                serde_json::json!({
                    "outcome": "missing_claim_worker_snapshot",
                    "worker_id": input.worker_id,
                }),
            ));
        }
    };
    let recheck = recheck_runtime_dispatch_policy(catalog, &entry);
    if !recheck.allowed {
        return update_runtime_dispatch_entry(
            store,
            entry,
            RuntimeDispatchUpdateInput {
                policy: recheck.policy,
                status: "blocked",
                report_status: "runtime_contract_dispatch_blocked",
                worker: input.worker_id,
                dry_run: input.dry_run,
                outcome: recheck.outcome.unwrap_or_else(|| {
                    serde_json::json!({
                        "outcome": "policy_recheck_failed",
                        "reason": "runtime contract is no longer dispatchable",
                    })
                }),
            },
        );
    }
    if worker.runtime != entry.runtime {
        let dispatch_runtime = entry.runtime.clone();
        let worker_runtime = worker.runtime.clone();
        return Ok(runtime_dispatch_preview_report(
            entry,
            Some(recheck.policy),
            "runtime_contract_dispatch_completion_rejected",
            input.worker_id,
            input.dry_run,
            serde_json::json!({
                "outcome": "worker_runtime_mismatch",
                "worker_id": worker.id,
                "worker_runtime": worker_runtime,
                "dispatch_runtime": dispatch_runtime,
            }),
        ));
    }
    if claimed_worker.runtime != entry.runtime {
        let dispatch_runtime = entry.runtime.clone();
        let claimed_runtime = claimed_worker.runtime.clone();
        return Ok(runtime_dispatch_preview_report(
            entry,
            Some(recheck.policy),
            "runtime_contract_dispatch_completion_rejected",
            input.worker_id,
            input.dry_run,
            serde_json::json!({
                "outcome": "claim_runtime_mismatch",
                "worker_id": input.worker_id,
                "claimed_worker_runtime": claimed_runtime,
                "dispatch_runtime": dispatch_runtime,
            }),
        ));
    }
    if worker.status == "disabled" {
        return Ok(runtime_dispatch_preview_report(
            entry,
            Some(recheck.policy),
            "runtime_contract_dispatch_completion_rejected",
            input.worker_id,
            input.dry_run,
            serde_json::json!({
                "outcome": "worker_disabled",
                "worker_id": worker.id,
            }),
        ));
    }
    let signature_value = input.signature.unwrap_or("").trim().to_string();
    let signature_required = matches!(claimed_worker.trust_level.as_str(), "signed" | "trusted");
    if signature_required && signature_value.is_empty() {
        return Ok(runtime_dispatch_preview_report(
            entry,
            Some(recheck.policy),
            "runtime_contract_dispatch_completion_rejected",
            input.worker_id,
            input.dry_run,
            serde_json::json!({
                "outcome": "missing_worker_signature",
                "worker_id": claimed_worker.id,
                "trust_level": claimed_worker.trust_level,
                "signature_verification_source": "claim_snapshot",
            }),
        ));
    }
    let result_sha256 = hex_sha256(&serde_json::to_vec(&input.result)?);
    let attestation_sha256 = hex_sha256(&serde_json::to_vec(&input.attestation)?);
    let signature_verification = verify_external_completion_signature(
        &entry.id,
        input.worker_id,
        final_status,
        &result_sha256,
        &attestation_sha256,
        &claimed_worker,
        &signature_value,
    );
    let signature_status = signature_verification["status"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    if signature_status == "invalid" || (signature_required && signature_status != "verified") {
        return Ok(runtime_dispatch_preview_report(
            entry,
            Some(recheck.policy),
            "runtime_contract_dispatch_completion_rejected",
            input.worker_id,
            input.dry_run,
            serde_json::json!({
                "outcome": "worker_signature_verification_failed",
                "worker_id": claimed_worker.id,
                "trust_level": claimed_worker.trust_level,
                "signature_verification_source": "claim_snapshot",
                "signature_verification": signature_verification,
            }),
        ));
    }
    let report_status = if final_status == "completed" {
        "runtime_contract_dispatch_external_completed"
    } else {
        "runtime_contract_dispatch_external_failed"
    };

    update_runtime_dispatch_entry(
        store,
        entry,
        RuntimeDispatchUpdateInput {
            policy: recheck.policy,
            status: final_status,
            report_status,
            worker: input.worker_id,
            dry_run: input.dry_run,
            outcome: serde_json::json!({
                "outcome": format!("external_worker_{final_status}"),
                "worker_id": claimed_worker.id.clone(),
                "worker_runtime": claimed_worker.runtime.clone(),
                "worker_trust_level": claimed_worker.trust_level.clone(),
                "current_worker_status": worker.status.clone(),
                "signature_required": signature_required,
                "signature_status": signature_status,
                "signature_verification_source": "claim_snapshot",
                "signature_verification": signature_verification,
                "signature": signature_value,
                "result_sha256": result_sha256,
                "result": input.result,
                "attestation_sha256": attestation_sha256,
                "attestation": input.attestation,
            }),
        },
    )
}

pub fn list_addon_views(
    catalog: &AddonCatalog,
    addon_id: Option<&str>,
    surface: Option<&str>,
    lifecycle: Option<&str>,
) -> AddonViewReport {
    let addon_filter = normalize_filter(addon_id);
    let surface_filter = normalize_filter(surface);
    let lifecycle_filter = normalize_filter(lifecycle);
    let views = catalog
        .addons
        .iter()
        .filter(|addon| {
            addon_filter
                .as_deref()
                .map(|filter| addon.id == filter)
                .unwrap_or(true)
                && filter_matches(lifecycle_filter.as_deref(), &addon.lifecycle)
        })
        .flat_map(|addon| {
            let surface_filter = surface_filter.clone();
            addon
                .views
                .iter()
                .filter(move |view| filter_matches(surface_filter.as_deref(), &view.surface))
                .cloned()
                .map(|view| AddonViewEntry {
                    addon_id: addon.id.clone(),
                    addon_name: addon.name.clone(),
                    addon_version: addon.version.clone(),
                    addon_lifecycle: addon.lifecycle.clone(),
                    permission_gate: addon_permission_gate(addon, &view.permissions),
                    view,
                })
        })
        .collect::<Vec<_>>();
    AddonViewReport {
        schema_version: addon_views_schema_version(),
        status: "addon_views_loaded".to_string(),
        view_count: views.len(),
        filters: AddonViewFilters {
            addon_id: addon_filter,
            surface: surface_filter,
            lifecycle: lifecycle_filter,
        },
        views,
    }
}

pub fn list_addon_permission_authorizations(
    store: &ForgeStore,
    addon_id: Option<&str>,
    permission_id: Option<&str>,
    status: Option<&str>,
) -> Result<AddonPermissionAuthorizationReport> {
    let authorizations = store
        .list_addon_permission_authorizations(addon_id, permission_id, status)?
        .into_iter()
        .map(addon_permission_authorization_view_from_record)
        .collect::<Vec<_>>();
    Ok(AddonPermissionAuthorizationReport {
        schema_version: addon_permission_authorizations_schema_version(),
        status: "addon_permission_authorizations_loaded".to_string(),
        authorization_count: authorizations.len(),
        authorizations,
    })
}

pub fn authorize_addon_permission(
    store: &ForgeStore,
    addon_id: &str,
    permission_id: &str,
    risk: &str,
    approved_by: &str,
    source: &str,
) -> Result<AddonPermissionAuthorizationChangeReport> {
    store.save_addon_permission_authorization(AddonPermissionAuthorizationWrite {
        addon_id,
        permission_id,
        status: "approved",
        risk,
        approved_by,
        source,
        data: &serde_json::json!({
            "source": source,
            "approval_contract": "human-approved addon permission",
        }),
    })?;
    sync_installed_addon_capability_index(store)?;
    addon_permission_authorization_change_report(
        store,
        addon_id,
        permission_id,
        "approved",
        "authorize_permission",
    )
}

pub fn revoke_addon_permission(
    store: &ForgeStore,
    addon_id: &str,
    permission_id: &str,
    approved_by: &str,
    source: &str,
) -> Result<AddonPermissionAuthorizationChangeReport> {
    store.save_addon_permission_authorization(AddonPermissionAuthorizationWrite {
        addon_id,
        permission_id,
        status: "revoked",
        risk: "unknown",
        approved_by,
        source,
        data: &serde_json::json!({
            "source": source,
            "revocation_contract": "human-revoked addon permission",
        }),
    })?;
    sync_installed_addon_capability_index(store)?;
    addon_permission_authorization_change_report(
        store,
        addon_id,
        permission_id,
        "revoked",
        "revoke_permission",
    )
}

pub fn install_addon(
    store: &ForgeStore,
    manifest_path: &Path,
    addon_dirs: &[PathBuf],
) -> Result<AddonLifecycleReport> {
    let mut manifest = load_addon_manifest_from_path(manifest_path)?;
    manifest.lifecycle = "enabled".to_string();
    manifest.source = format!("file:{}", manifest_path.display());
    let source = manifest.source.clone();
    validate_candidate_catalog(store, addon_dirs, Some(manifest.clone()))?;
    ensure_candidate_migration_against_installed(store, &manifest)?;
    ensure_addon_permissions_authorized(store, &manifest)?;
    let migration_workflow =
        create_candidate_migration_workflow_if_needed(store, &manifest, "install", "forge_cli")?;
    store.save_installed_addon(
        &manifest.id,
        "enabled",
        &source,
        &serde_json::to_value(&manifest)?,
    )?;
    materialize_installed_addon_capabilities(store, &manifest, "enabled")?;
    let mut report = lifecycle_report(store, addon_dirs, &manifest.id, "installed", "install")?;
    report.migration_workflow = migration_workflow;
    Ok(report)
}

pub fn package_addon(
    store: &ForgeStore,
    input: AddonPackageInput<'_>,
) -> Result<AddonPackageReport> {
    let mut manifest = load_addon_manifest_from_path(input.manifest_path)?;
    manifest.source = format!("file:{}", input.manifest_path.display());
    let manifest_bytes = fs::read(input.manifest_path).with_context(|| {
        format!(
            "failed to read addon manifest {}",
            input.manifest_path.display()
        )
    })?;
    let manifest_sha256 = hex_sha256(&manifest_bytes);
    let manifest_canonical_sha256 = hex_sha256(&serde_json::to_vec(&manifest)?);
    let validation = validate_candidate_catalog(store, input.addon_dirs, Some(manifest.clone()))?;
    let repository = input
        .repository
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| manifest.metadata.get("repository").map(String::as_str))
        .unwrap_or("local");
    let channel = if input.channel.trim().is_empty() {
        "stable"
    } else {
        input.channel.trim()
    };
    let package_id = format!("{}@{}", manifest.id, manifest.version);
    let payload_bytes = addon_package_signature_payload_bytes(
        &package_id,
        &manifest.id,
        &manifest.version,
        &manifest_sha256,
        &manifest_canonical_sha256,
        repository,
        channel,
    )?;
    let payload_sha256 = hex_sha256(&payload_bytes);
    let signature_value = input.signature.unwrap_or("").trim();
    let public_key_value = input.public_key.unwrap_or("").trim();
    let signature_status = if signature_value.is_empty() {
        "unsigned"
    } else if public_key_value.is_empty() {
        "signature_without_public_key"
    } else {
        "declared"
    };
    let mut report = AddonPackageReport {
        schema_version: addon_package_schema_version(),
        status: "addon_package_ready".to_string(),
        package_id: package_id.clone(),
        addon_id: manifest.id.clone(),
        addon_name: manifest.name.clone(),
        addon_version: manifest.version.clone(),
        manifest_path: input.manifest_path.display().to_string(),
        manifest_sha256,
        manifest_canonical_sha256,
        manifest_bytes: manifest_bytes.len() as u64,
        distribution: AddonPackageDistribution {
            repository: repository.to_string(),
            channel: channel.to_string(),
            source: manifest.source.clone(),
            update_strategy: "manual_install_upgrade_downgrade".to_string(),
            install_command: format!(
                "forge addons install --manifest {} --output json",
                input.manifest_path.display()
            ),
            upgrade_command: format!(
                "forge addons upgrade --manifest {} --output json",
                input.manifest_path.display()
            ),
            downgrade_command: format!(
                "forge addons downgrade --manifest {} --output json",
                input.manifest_path.display()
            ),
        },
        signature: AddonPackageSignature {
            status: signature_status.to_string(),
            scheme: "ed25519_detached".to_string(),
            signature: signature_value.to_string(),
            public_key: public_key_value.to_string(),
            payload_sha256,
            verification_note:
                "package records detached signature metadata; trust verification is a marketplace policy gate"
                    .to_string(),
        },
        summary: addon_package_summary(&manifest),
        validation,
        manifest,
        written_package_path: None,
        written_package_sha256: None,
    };
    if let Some(path) = input.package_path {
        let bytes = serde_json::to_vec_pretty(&report)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create package directory {}", parent.display())
            })?;
        }
        fs::write(path, &bytes)
            .with_context(|| format!("failed to write addon package {}", path.display()))?;
        report.written_package_path = Some(path.display().to_string());
        report.written_package_sha256 = Some(hex_sha256(&bytes));
    }
    Ok(report)
}

pub fn trust_addon_package_key(
    store: &ForgeStore,
    input: AddonTrustKeyInput<'_>,
) -> Result<AddonTrustKeyChangeReport> {
    let repository = input.repository.trim();
    if repository.is_empty() {
        bail!("repository is required for addon trust keys");
    }
    let channel = normalize_marketplace_channel(input.channel);
    let public_key = normalize_hex(input.public_key);
    decode_hex_exact(&public_key, 32).with_context(|| "invalid Ed25519 public key hex")?;
    let trust_level = if input.trust_level.trim().is_empty() {
        "trusted"
    } else {
        input.trust_level.trim()
    };
    let approved_by = if input.approved_by.trim().is_empty() {
        "human"
    } else {
        input.approved_by.trim()
    };
    let source = if input.source.trim().is_empty() {
        "cli"
    } else {
        input.source.trim()
    };
    let key_id = format!(
        "addon-trust:{}",
        hex_sha256(format!("{repository}\n{channel}\n{public_key}").as_bytes())
    );
    store.save_addon_trust_key(AddonTrustKeyWrite {
        key_id: &key_id,
        repository,
        channel: &channel,
        public_key: &public_key,
        status: "trusted",
        trust_level,
        approved_by,
        source,
        data: &input.data,
    })?;
    let key = store
        .list_addon_trust_keys(
            Some(repository),
            Some(&channel),
            Some(&public_key),
            Some("trusted"),
            1,
        )?
        .into_iter()
        .next()
        .map(addon_trust_key_entry_from_record)
        .transpose()?
        .with_context(|| "trusted addon key was not persisted")?;
    Ok(AddonTrustKeyChangeReport {
        schema_version: addon_trust_store_schema_version(),
        status: "trusted".to_string(),
        action: "trust_key".to_string(),
        key,
    })
}

pub fn list_addon_trust_store(
    store: &ForgeStore,
    repository: Option<&str>,
    channel: Option<&str>,
    public_key: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<AddonTrustStoreReport> {
    let public_key = public_key.map(normalize_hex);
    let keys = store
        .list_addon_trust_keys(repository, channel, public_key.as_deref(), status, limit)?
        .into_iter()
        .map(addon_trust_key_entry_from_record)
        .collect::<Result<Vec<_>>>()?;
    Ok(AddonTrustStoreReport {
        schema_version: addon_trust_store_schema_version(),
        status: "loaded".to_string(),
        key_count: keys.len(),
        filters: AddonTrustStoreFilters {
            repository: repository.map(str::to_string),
            channel: channel.map(str::to_string),
            public_key,
            status: status.map(str::to_string),
        },
        keys,
    })
}

pub fn evaluate_addon_package_policy(
    store: &ForgeStore,
    package: &AddonPackageReport,
    package_sha256: Option<&str>,
) -> Result<AddonPackagePolicyReport> {
    let repository = package.distribution.repository.trim().to_string();
    let channel = normalize_marketplace_channel(&package.distribution.channel);
    let package_sha256 = package_sha256.unwrap_or("").trim().to_string();
    let mut issues = Vec::new();

    if package.schema_version != ADDON_PACKAGE_SCHEMA_VERSION {
        issues.push(format!(
            "unsupported_package_schema:{}",
            package.schema_version
        ));
    }
    if package.package_id != format!("{}@{}", package.addon_id, package.addon_version) {
        issues.push("package_id_does_not_match_addon_version".to_string());
    }
    if package.addon_id != package.manifest.id || package.addon_version != package.manifest.version
    {
        issues.push("embedded_manifest_identity_mismatch".to_string());
    }
    if repository.is_empty() {
        issues.push("missing_repository".to_string());
    }
    if channel.is_empty() {
        issues.push("missing_channel".to_string());
    }

    let computed_manifest_canonical_sha256 = hex_sha256(&serde_json::to_vec(&package.manifest)?);
    if package.manifest_canonical_sha256.trim().is_empty() {
        issues.push("missing_manifest_canonical_sha256".to_string());
    } else if package.manifest_canonical_sha256 != computed_manifest_canonical_sha256 {
        issues.push("manifest_canonical_hash_mismatch".to_string());
    }

    let payload_bytes = addon_package_signature_payload_bytes(
        &package.package_id,
        &package.addon_id,
        &package.addon_version,
        &package.manifest_sha256,
        &computed_manifest_canonical_sha256,
        &repository,
        &channel,
    )?;
    let payload_sha256 = hex_sha256(&payload_bytes);
    if package.signature.payload_sha256 != payload_sha256 {
        issues.push("signature_payload_hash_mismatch".to_string());
    }

    let signature_value = package.signature.signature.trim();
    let public_key = normalize_hex(&package.signature.public_key);
    let mut trusted_keys = Vec::new();
    let (verification_status, reason) = if signature_value.is_empty() {
        issues.push("unsigned_package".to_string());
        (
            "unsigned".to_string(),
            "package has no detached signature".to_string(),
        )
    } else if public_key.is_empty() {
        issues.push("missing_signature_public_key".to_string());
        (
            "missing_public_key".to_string(),
            "package signature has no public key".to_string(),
        )
    } else if package.signature.scheme != "ed25519_detached" {
        issues.push(format!(
            "unsupported_signature_scheme:{}",
            package.signature.scheme
        ));
        (
            "unsupported_scheme".to_string(),
            "only ed25519_detached package signatures are supported".to_string(),
        )
    } else {
        trusted_keys = store
            .list_addon_trust_keys(
                Some(&repository),
                Some(&channel),
                Some(&public_key),
                Some("trusted"),
                20,
            )?
            .into_iter()
            .map(addon_trust_key_entry_from_record)
            .collect::<Result<Vec<_>>>()?;
        if trusted_keys.is_empty() {
            issues.push("public_key_not_trusted_for_repository_channel".to_string());
        }
        match verify_addon_package_signature_bytes(&payload_bytes, signature_value, &public_key) {
            Ok(()) => (
                "verified".to_string(),
                "signature verified against package payload".to_string(),
            ),
            Err(error) => {
                issues.push("signature_verification_failed".to_string());
                ("invalid".to_string(), error.to_string())
            }
        }
    };

    let install_allowed = issues.is_empty() && verification_status == "verified";
    Ok(AddonPackagePolicyReport {
        schema_version: addon_package_policy_schema_version(),
        status: if install_allowed {
            "install_allowed".to_string()
        } else {
            "blocked".to_string()
        },
        package_id: package.package_id.clone(),
        addon_id: package.addon_id.clone(),
        addon_version: package.addon_version.clone(),
        repository,
        channel,
        package_sha256,
        manifest_sha256: package.manifest_sha256.clone(),
        manifest_canonical_sha256: computed_manifest_canonical_sha256,
        install_allowed,
        trusted_key_count: trusted_keys.len(),
        issues,
        signature: AddonPackageSignaturePolicy {
            status: package.signature.status.clone(),
            scheme: package.signature.scheme.clone(),
            verification_status,
            public_key,
            payload_sha256,
            reason,
        },
        trusted_keys,
    })
}

pub fn publish_addon_package(
    store: &ForgeStore,
    package_path: &Path,
    source: &str,
) -> Result<AddonMarketplacePublishReport> {
    let (package, package_bytes, package_sha256) = load_addon_package_from_path(package_path)?;
    let policy = evaluate_addon_package_policy(store, &package, Some(&package_sha256))?;
    let source = marketplace_package_source(package_path, source);
    let package_value = serde_json::from_slice::<serde_json::Value>(&package_bytes)?;
    let status = if policy.install_allowed {
        "installable"
    } else {
        "blocked"
    };
    store.save_addon_marketplace_package(AddonMarketplacePackageWrite {
        package_id: &package.package_id,
        addon_id: &package.addon_id,
        addon_version: &package.addon_version,
        repository: &policy.repository,
        channel: &policy.channel,
        manifest_sha256: &package.manifest_sha256,
        package_sha256: &package_sha256,
        status,
        signature_status: &package.signature.status,
        verification_status: &policy.signature.verification_status,
        source: &source,
        package: &package_value,
    })?;
    let record = store
        .list_addon_marketplace_packages(
            Some(&policy.repository),
            Some(&policy.channel),
            Some(&package.addon_id),
            None,
            50,
        )?
        .into_iter()
        .find(|record| record.package_id == package.package_id)
        .with_context(|| "published addon package was not persisted")?;
    Ok(AddonMarketplacePublishReport {
        schema_version: addon_marketplace_schema_version(),
        status: "published".to_string(),
        action: "publish_package".to_string(),
        package: addon_marketplace_entry_from_record(store, record)?,
    })
}

pub fn list_addon_marketplace(
    store: &ForgeStore,
    repository: Option<&str>,
    channel: Option<&str>,
    addon_id: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<AddonMarketplaceReport> {
    let packages = store
        .list_addon_marketplace_packages(repository, channel, addon_id, status, limit)?
        .into_iter()
        .map(|record| addon_marketplace_entry_from_record(store, record))
        .collect::<Result<Vec<_>>>()?;
    let installable_count = packages
        .iter()
        .filter(|package| package.policy.install_allowed)
        .count();
    let blocked_count = packages.len().saturating_sub(installable_count);
    Ok(AddonMarketplaceReport {
        schema_version: addon_marketplace_schema_version(),
        status: "loaded".to_string(),
        package_count: packages.len(),
        installable_count,
        blocked_count,
        filters: AddonMarketplaceFilters {
            repository: repository.map(str::to_string),
            channel: channel.map(str::to_string),
            addon_id: addon_id.map(str::to_string),
            status: status.map(str::to_string),
        },
        packages,
    })
}

pub fn create_addon_package_lock(
    store: &ForgeStore,
    repository: Option<&str>,
    channel: Option<&str>,
    addon_id: Option<&str>,
    status: Option<&str>,
    write_path: Option<&Path>,
    limit: usize,
) -> Result<AddonPackageLockReport> {
    let packages = store
        .list_addon_marketplace_packages(repository, channel, addon_id, status, limit)?
        .into_iter()
        .map(|record| addon_marketplace_entry_from_record(store, record))
        .collect::<Result<Vec<_>>>()?;
    let entries = packages
        .iter()
        .map(addon_package_lock_entry)
        .collect::<Vec<_>>();
    let mut report = AddonPackageLockReport {
        schema_version: addon_package_lock_schema_version(),
        status: "locked".to_string(),
        generated_at: Utc::now().to_rfc3339(),
        package_count: entries.len(),
        filters: AddonMarketplaceFilters {
            repository: repository.map(str::to_string),
            channel: channel.map(str::to_string),
            addon_id: addon_id.map(str::to_string),
            status: status.map(str::to_string),
        },
        packages: entries,
        written_lock_path: None,
        written_lock_sha256: None,
    };
    if let Some(path) = write_path {
        let mut writable_report = report.clone();
        writable_report.written_lock_path = None;
        writable_report.written_lock_sha256 = None;
        let bytes = serde_json::to_vec_pretty(&writable_report)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create package lock directory {}",
                    parent.display()
                )
            })?;
        }
        fs::write(path, &bytes)
            .with_context(|| format!("failed to write package lock {}", path.display()))?;
        report.written_lock_path = Some(path.display().to_string());
        report.written_lock_sha256 = Some(hex_sha256(&bytes));
    }
    Ok(report)
}

pub fn fetch_addon_package(
    store: &ForgeStore,
    source: &str,
    cache_dir: Option<&Path>,
    expected_sha256: Option<&str>,
    allow_remote: bool,
    max_bytes: u64,
    lock_path: Option<&Path>,
) -> Result<AddonPackageFetchReport> {
    let source = source.trim();
    if source.is_empty() {
        bail!("package source is required");
    }
    let max_bytes = max_bytes.max(1);
    let (source_kind, bytes) = read_addon_package_source(source, allow_remote, max_bytes)?;
    let bytes_len = bytes.len() as u64;
    if bytes_len > max_bytes {
        bail!(
            "addon package source exceeded max bytes: {} > {}",
            bytes_len,
            max_bytes
        );
    }
    let package_sha256 = hex_sha256(&bytes);
    let expected_sha256 = expected_sha256
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase);
    if let Some(expected) = &expected_sha256 {
        if expected != &package_sha256 {
            bail!(
                "addon package sha256 mismatch: expected {}, got {}",
                expected,
                package_sha256
            );
        }
    }
    let cache_dir = cache_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| store.base_dir().join("addon-package-cache"));
    fs::create_dir_all(&cache_dir).with_context(|| {
        format!(
            "failed to create addon package cache directory {}",
            cache_dir.display()
        )
    })?;
    let cached_package_path = cache_dir.join(format!("{package_sha256}.package.json"));
    fs::write(&cached_package_path, &bytes).with_context(|| {
        format!(
            "failed to write cached addon package {}",
            cached_package_path.display()
        )
    })?;
    let (package, _, cached_package_sha256) = load_addon_package_from_path(&cached_package_path)?;
    if cached_package_sha256 != package_sha256 {
        bail!(
            "cached addon package sha256 changed while writing cache: expected {}, got {}",
            package_sha256,
            cached_package_sha256
        );
    }
    let policy = evaluate_addon_package_policy(store, &package, Some(&package_sha256))?;
    let lock = match lock_path {
        Some(path) => Some(enforce_addon_package_lock(
            path,
            &package,
            &policy,
            &package_sha256,
        )?),
        None => None,
    };
    let marketplace = publish_addon_package(
        store,
        &cached_package_path,
        &cached_package_path.display().to_string(),
    )?;
    Ok(AddonPackageFetchReport {
        schema_version: addon_package_fetch_schema_version(),
        status: "fetched".to_string(),
        action: "fetch_package".to_string(),
        source: source.to_string(),
        source_kind,
        remote_allowed: allow_remote,
        cached_package_path: cached_package_path.display().to_string(),
        bytes: bytes_len,
        sha256: package_sha256,
        expected_sha256,
        max_bytes,
        lock,
        marketplace,
    })
}

pub fn sync_addon_package_registry(
    store: &ForgeStore,
    source: &str,
    cache_dir: Option<&Path>,
    allow_remote: bool,
    max_bytes: u64,
    max_packages: usize,
    lock_path: Option<&Path>,
) -> Result<AddonRegistrySyncReport> {
    let source = source.trim();
    if source.is_empty() {
        bail!("registry source is required");
    }
    let max_bytes = max_bytes.max(1);
    let max_packages = max_packages.max(1);
    let (source_kind, bytes) = read_addon_package_source(source, allow_remote, max_bytes)?;
    let index = parse_addon_registry_index(&bytes)?;
    if index.packages.len() > max_packages {
        bail!(
            "addon registry index package count exceeded max packages: {} > {}",
            index.packages.len(),
            max_packages
        );
    }

    let mut fetches = Vec::new();
    let mut issues = Vec::new();
    for package in &index.packages {
        let package_allow_remote = allow_remote && package.allow_remote.unwrap_or(allow_remote);
        match fetch_addon_package(
            store,
            &package.source,
            cache_dir,
            package.expected_sha256.as_deref(),
            package_allow_remote,
            package.max_bytes.unwrap_or(max_bytes),
            lock_path,
        ) {
            Ok(report) => fetches.push(report),
            Err(error) => issues.push(format!("{}: {error}", package.source)),
        }
    }
    let blocked_count = issues.len();
    let fetched_count = fetches.len();
    let status = if blocked_count == 0 {
        "synced"
    } else if fetched_count == 0 {
        "blocked"
    } else {
        "partial"
    };
    Ok(AddonRegistrySyncReport {
        schema_version: addon_registry_sync_schema_version(),
        status: status.to_string(),
        action: "sync_registry".to_string(),
        source: source.to_string(),
        source_kind,
        remote_allowed: allow_remote,
        package_count: index.packages.len(),
        fetched_count,
        blocked_count,
        max_packages,
        max_bytes,
        fetches,
        issues,
    })
}

pub fn create_addon_migration_workflow(
    store: &ForgeStore,
    from_manifest_path: &Path,
    to_manifest_path: &Path,
    action: &str,
    origin: &str,
) -> Result<AddonMigrationWorkflowReport> {
    let mut previous = load_addon_manifest_from_path(from_manifest_path)?;
    previous.source = format!("file:{}", from_manifest_path.display());
    let mut candidate = load_addon_manifest_from_path(to_manifest_path)?;
    candidate.source = format!("file:{}", to_manifest_path.display());
    validate_addon_version_change_direction(&previous, &candidate, action)?;
    ensure_version_change_migration_plan(&previous, &candidate)?;
    create_and_save_addon_migration_workflow(store, &previous, &candidate, action, origin)
}

pub fn install_addon_package(
    store: &ForgeStore,
    package_path: &Path,
    addon_dirs: &[PathBuf],
    lock_path: Option<&Path>,
) -> Result<AddonPackageInstallReport> {
    let (package, package_bytes, package_sha256) = load_addon_package_from_path(package_path)?;
    let policy = evaluate_addon_package_policy(store, &package, Some(&package_sha256))?;
    let lock = match lock_path {
        Some(path) => Some(enforce_addon_package_lock(
            path,
            &package,
            &policy,
            &package_sha256,
        )?),
        None => None,
    };
    if !policy.install_allowed {
        bail!(
            "addon package policy blocked install: {}",
            policy.issues.join(", ")
        );
    }
    let package_value = serde_json::from_slice::<serde_json::Value>(&package_bytes)?;
    let source = format!(
        "marketplace:{}:{}:{}#{}",
        policy.repository, policy.channel, package.package_id, package_sha256
    );
    store.save_addon_marketplace_package(AddonMarketplacePackageWrite {
        package_id: &package.package_id,
        addon_id: &package.addon_id,
        addon_version: &package.addon_version,
        repository: &policy.repository,
        channel: &policy.channel,
        manifest_sha256: &package.manifest_sha256,
        package_sha256: &package_sha256,
        status: "installable",
        signature_status: &package.signature.status,
        verification_status: &policy.signature.verification_status,
        source: &source,
        package: &package_value,
    })?;

    let mut manifest = package.manifest.clone();
    manifest.lifecycle = "enabled".to_string();
    manifest.source = source.clone();
    validate_candidate_catalog(store, addon_dirs, Some(manifest.clone()))?;
    ensure_candidate_migration_against_installed(store, &manifest)?;
    ensure_addon_permissions_authorized(store, &manifest)?;
    let migration_workflow = create_candidate_migration_workflow_if_needed(
        store,
        &manifest,
        "install_package",
        "forge_cli",
    )?;
    store.save_installed_addon(
        &manifest.id,
        "enabled",
        &source,
        &serde_json::to_value(&manifest)?,
    )?;
    materialize_installed_addon_capabilities(store, &manifest, "enabled")?;
    let lifecycle = lifecycle_report(
        store,
        addon_dirs,
        &manifest.id,
        "installed",
        "install_package",
    )?;
    let mut lifecycle = lifecycle;
    lifecycle.migration_workflow = migration_workflow;
    let package_entry = store
        .list_addon_marketplace_packages(
            Some(&policy.repository),
            Some(&policy.channel),
            Some(&package.addon_id),
            Some("installable"),
            50,
        )?
        .into_iter()
        .find(|record| record.package_id == package.package_id)
        .map(|record| addon_marketplace_entry_from_record(store, record))
        .transpose()?
        .with_context(|| "installed addon package was not persisted")?;
    Ok(AddonPackageInstallReport {
        schema_version: addon_package_install_schema_version(),
        status: "installed".to_string(),
        action: "install_package".to_string(),
        lock,
        package: package_entry,
        lifecycle,
    })
}

pub fn upgrade_addon(
    store: &ForgeStore,
    manifest_path: &Path,
    addon_dirs: &[PathBuf],
) -> Result<AddonLifecycleReport> {
    change_installed_addon_version(
        store,
        manifest_path,
        addon_dirs,
        AddonVersionChange::Upgrade,
        "upgraded",
        "upgrade",
    )
}

pub fn downgrade_addon(
    store: &ForgeStore,
    manifest_path: &Path,
    addon_dirs: &[PathBuf],
) -> Result<AddonLifecycleReport> {
    change_installed_addon_version(
        store,
        manifest_path,
        addon_dirs,
        AddonVersionChange::Downgrade,
        "downgraded",
        "downgrade",
    )
}

pub fn enable_addon(
    store: &ForgeStore,
    addon_id: &str,
    addon_dirs: &[PathBuf],
) -> Result<AddonLifecycleReport> {
    update_addon_lifecycle(store, addon_id, "enabled", addon_dirs, "enabled", "enable")
}

pub fn disable_addon(
    store: &ForgeStore,
    addon_id: &str,
    addon_dirs: &[PathBuf],
) -> Result<AddonLifecycleReport> {
    update_addon_lifecycle(
        store, addon_id, "disabled", addon_dirs, "disabled", "disable",
    )
}

pub fn uninstall_addon(
    store: &ForgeStore,
    addon_id: &str,
    addon_dirs: &[PathBuf],
) -> Result<AddonLifecycleReport> {
    let record = store.load_installed_addon(addon_id)?;
    let view = installed_view_from_record(record)?;
    store.delete_installed_addon(addon_id)?;
    store.delete_addon_capabilities(addon_id)?;
    let catalog = load_addon_catalog_from_store(store, addon_dirs)?;
    Ok(AddonLifecycleReport {
        schema_version: addon_lifecycle_schema_version(),
        status: "uninstalled".to_string(),
        action: "uninstall".to_string(),
        addon: view,
        validation: validate_addon_catalog(&catalog),
        migration_workflow: None,
    })
}

pub fn resolve_goal_capabilities(goal: &str, catalog: &AddonCatalog) -> CapabilityResolutionReport {
    let normalized_goal = goal.trim();
    let lower_goal = normalized_goal.to_lowercase();
    let available_capabilities = catalog
        .addons
        .iter()
        .filter(|addon| addon_enabled(addon))
        .flat_map(|addon| {
            addon
                .capabilities
                .iter()
                .map(|capability| capability.id.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut required = Vec::new();
    let mut seen = BTreeSet::new();

    for capability_id in [
        CAP_WORKFLOW_RUNTIME,
        CAP_DYNAMIC_WORKFLOW,
        CAP_EVENT_ENGINE,
        CAP_CONTEXT_ROUTING,
        CAP_MEMORY_GOVERNANCE,
        CAP_IDENTITY_ROUTING,
        CAP_PERSONALITY_ROUTING,
        CAP_HUMAN_COLLABORATION,
        CAP_OBSERVABILITY,
        CAP_ADDON_REGISTRY,
    ] {
        if let Some((addon, capability)) = find_capability(catalog, capability_id) {
            push_need(
                &mut required,
                &mut seen,
                addon,
                capability,
                "capacidade universal requerida por qualquer workflow Forge".to_string(),
                Vec::new(),
            );
        }
    }

    for addon in &catalog.addons {
        if !addon_enabled(addon) {
            continue;
        }
        for capability in &addon.capabilities {
            if seen.contains(&capability.id) {
                continue;
            }
            let matched = capability
                .keywords
                .iter()
                .filter(|keyword| {
                    let keyword = keyword.trim().to_lowercase();
                    !keyword.is_empty() && lower_goal.contains(&keyword)
                })
                .cloned()
                .collect::<Vec<_>>();
            if !matched.is_empty() {
                push_need(
                    &mut required,
                    &mut seen,
                    addon,
                    capability,
                    "objetivo corresponde aos gatilhos declarados pela capacidade".to_string(),
                    matched,
                );
            }
        }
    }

    let mut missing = Vec::new();
    for need in &required {
        if let Some((_addon, declaration)) = find_capability(catalog, &need.id) {
            for dependency in &declaration.requires_capabilities {
                if !available_capabilities.contains(dependency) {
                    missing.push(MissingCapability {
                        id: dependency.clone(),
                        required_by: need.id.clone(),
                        reason: "dependência declarada por capacidade instalada".to_string(),
                    });
                }
            }
        }
    }
    let capability_suggestions = build_capability_suggestions(catalog, &missing);

    let active_addons = required
        .iter()
        .map(|capability| capability.source_addon.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let workflow_extensions = build_workflow_extension_activations(catalog, &required);
    let runtime_contracts = build_runtime_contract_activations(catalog, &required);
    let intent_overlay = build_intent_overlay(catalog, &required);

    CapabilityResolutionReport {
        schema_version: capability_resolution_schema_version(),
        status: if missing.is_empty() {
            "resolved".to_string()
        } else {
            "missing_capabilities".to_string()
        },
        goal: normalized_goal.to_string(),
        planning_strategy: "capability_first_addon_registry".to_string(),
        required_capabilities: required,
        missing_capabilities: missing,
        capability_suggestions,
        active_addons,
        available_capabilities: available_capabilities.into_iter().collect(),
        workflow_extensions,
        runtime_contracts,
        registry_syncs: Vec::new(),
        intent_overlay,
    }
}

pub fn resolve_goal_capabilities_with_store(
    store: &ForgeStore,
    goal: &str,
    catalog: &AddonCatalog,
) -> Result<CapabilityResolutionReport> {
    let mut report = resolve_goal_capabilities(goal, catalog);
    append_marketplace_capability_suggestions(store, &mut report)?;
    Ok(report)
}

pub fn resolve_goal_capabilities_with_registry_sync(
    store: &ForgeStore,
    goal: &str,
    catalog: &AddonCatalog,
    input: CapabilityRegistrySyncInput<'_>,
) -> Result<CapabilityResolutionReport> {
    let mut registry_syncs = Vec::new();
    for source in input
        .registry_sources
        .iter()
        .map(|source| source.trim())
        .filter(|source| !source.is_empty())
    {
        registry_syncs.push(sync_addon_package_registry(
            store,
            source,
            input.cache_dir,
            input.allow_remote,
            input.max_bytes,
            input.max_packages,
            input.lock_path,
        )?);
    }
    let mut report = resolve_goal_capabilities_with_store(store, goal, catalog)?;
    report.registry_syncs = registry_syncs;
    Ok(report)
}

pub fn validate_addon_catalog(catalog: &AddonCatalog) -> AddonCatalogValidationReport {
    let mut issues = Vec::new();
    let mut addon_ids = BTreeSet::new();
    let mut capability_ids = BTreeSet::new();
    let installed_addons = catalog
        .addons
        .iter()
        .filter(|addon| addon_enabled(addon))
        .map(|addon| addon.id.clone())
        .collect::<BTreeSet<_>>();
    let installed_addon_versions = catalog
        .addons
        .iter()
        .filter(|addon| addon_enabled(addon))
        .map(|addon| (addon.id.clone(), addon.version.clone()))
        .collect::<BTreeMap<_, _>>();
    let installed_capabilities = catalog
        .addons
        .iter()
        .filter(|addon| addon_enabled(addon))
        .flat_map(|addon| {
            addon
                .capabilities
                .iter()
                .map(|capability| capability.id.clone())
        })
        .collect::<BTreeSet<_>>();

    for addon in &catalog.addons {
        if !addon_ids.insert(addon.id.clone()) {
            issues.push(validation_issue(
                "error",
                "duplicate_addon_id",
                &addon.id,
                "addon id is declared more than once",
            ));
        }
        if !addon_enabled(addon) {
            continue;
        }
        validate_addon_compatibility(addon, &mut issues);
        for dependency in &addon.dependencies {
            if dependency.required && !installed_addons.contains(&dependency.id) {
                issues.push(validation_issue(
                    "error",
                    "missing_required_addon_dependency",
                    &addon.id,
                    &format!(
                        "required addon dependency {} is not installed",
                        dependency.id
                    ),
                ));
            }
            if let Some(installed_version) = installed_addon_versions.get(&dependency.id) {
                if !dependency.version_req.trim().is_empty()
                    && !addon_version_req_satisfied(&dependency.version_req, installed_version)
                {
                    issues.push(validation_issue(
                        if dependency.required { "error" } else { "warning" },
                        "unsatisfied_addon_version_requirement",
                        &addon.id,
                        &format!(
                            "addon dependency {} requires version `{}` but installed version is `{installed_version}`",
                            dependency.id, dependency.version_req
                        ),
                    ));
                }
            }
        }
        for permission in &addon.permissions {
            if permission.risk == "high" && !permission.requires_human_approval {
                issues.push(validation_issue(
                    "warning",
                    "high_risk_permission_without_human_gate",
                    &format!("{}:{}", addon.id, permission.id),
                    "high risk addon permission should require human approval",
                ));
            }
        }
        validate_addon_permission_references(addon, &mut issues);
        for capability in &addon.capabilities {
            if !capability_ids.insert(capability.id.clone()) {
                issues.push(validation_issue(
                    "error",
                    "duplicate_capability_id",
                    &capability.id,
                    "capability id is declared by more than one addon",
                ));
            }
            for required_capability in &capability.requires_capabilities {
                if !installed_capabilities.contains(required_capability) {
                    issues.push(validation_issue(
                        "error",
                        "missing_required_capability",
                        &capability.id,
                        &format!(
                            "capability requires unavailable capability {required_capability}"
                        ),
                    ));
                }
            }
        }
    }

    let has_errors = issues.iter().any(|issue| issue.severity == "error");
    AddonCatalogValidationReport {
        schema_version: addon_validation_schema_version(),
        status: if has_errors {
            "invalid".to_string()
        } else {
            "valid".to_string()
        },
        addon_count: catalog.addon_count,
        capability_count: catalog.capability_count,
        issue_count: issues.len(),
        issues,
    }
}

pub fn default_addon_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from(".forge/addons")]
}

fn load_addon_catalog_with_records(
    addon_dirs: &[PathBuf],
    records: Vec<StoredAddonRecord>,
    approved_permissions: Vec<StoredAddonPermissionAuthorizationRecord>,
) -> Result<AddonCatalog> {
    let mut catalog = load_addon_catalog(addon_dirs)?;
    if !records.is_empty() {
        catalog
            .addon_dirs
            .push("sqlite:installed_addons".to_string());
    }
    let approved_permissions = approved_permissions
        .into_iter()
        .map(|authorization| (authorization.addon_id, authorization.permission_id))
        .collect::<BTreeSet<_>>();
    for record in records {
        let mut manifest = installed_manifest_from_record(&record)?;
        if manifest.lifecycle == "enabled"
            && !manifest_permissions_authorized(&manifest, &approved_permissions)
        {
            manifest.lifecycle = "unauthorized".to_string();
        }
        upsert_addon(&mut catalog.addons, manifest);
    }
    for manifest in &mut catalog.addons {
        if manifest.lifecycle == "enabled"
            && !manifest_permissions_authorized(manifest, &approved_permissions)
        {
            manifest.lifecycle = "unauthorized".to_string();
        }
    }
    Ok(finalize_catalog(catalog.addons, catalog.addon_dirs))
}

fn validate_candidate_catalog(
    store: &ForgeStore,
    addon_dirs: &[PathBuf],
    candidate: Option<AddonManifest>,
) -> Result<AddonCatalogValidationReport> {
    let mut catalog = load_addon_catalog_from_store(store, addon_dirs)?;
    if let Some(candidate) = candidate {
        upsert_addon(&mut catalog.addons, candidate);
        catalog = finalize_catalog(catalog.addons, catalog.addon_dirs);
    }
    let validation = validate_addon_catalog(&catalog);
    if validation.status != "valid" {
        let issue_summary = validation
            .issues
            .iter()
            .map(|issue| format!("{}:{}", issue.code, issue.subject))
            .collect::<Vec<_>>()
            .join(", ");
        bail!("addon catalog validation failed: {issue_summary}");
    }
    Ok(validation)
}

fn addon_package_summary(manifest: &AddonManifest) -> AddonPackageSummary {
    AddonPackageSummary {
        capability_count: manifest.capabilities.len(),
        dependency_count: manifest.dependencies.len(),
        permission_count: manifest.permissions.len(),
        workflow_extension_count: manifest.workflows.len(),
        runtime_contract_count: manifest.runtime_contracts.len(),
        view_count: manifest.views.len(),
        event_adapter_count: manifest.event_adapters.len(),
        integration_count: manifest.integrations.len(),
        capabilities: manifest
            .capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect(),
        dependencies: manifest
            .dependencies
            .iter()
            .map(|dependency| {
                if dependency.version_req.trim().is_empty() {
                    dependency.id.clone()
                } else {
                    format!("{} {}", dependency.id, dependency.version_req)
                }
            })
            .collect(),
        permissions: manifest
            .permissions
            .iter()
            .map(|permission| permission.id.clone())
            .collect(),
        runtime_contracts: manifest
            .runtime_contracts
            .iter()
            .map(|contract| contract.id.clone())
            .collect(),
        views: manifest.views.iter().map(|view| view.id.clone()).collect(),
        compatibility: addon_compatibility_summary(&manifest.compatibility),
    }
}

fn addon_compatibility_summary(compatibility: &AddonCompatibility) -> AddonCompatibilitySummary {
    AddonCompatibilitySummary {
        forge_version_req: compatibility.forge_version_req.clone(),
        api_versions: compatibility.api_versions.clone(),
        runtimes: compatibility.runtimes.clone(),
        features: compatibility.features.clone(),
        platforms: compatibility.platforms.clone(),
        migration_count: compatibility.migrations.len(),
        migrations: compatibility
            .migrations
            .iter()
            .map(|migration| {
                format!(
                    "{} -> {} ({})",
                    migration.from_version, migration.to_version, migration.strategy
                )
            })
            .collect(),
    }
}

fn addon_package_signature_payload_bytes(
    package_id: &str,
    addon_id: &str,
    addon_version: &str,
    manifest_sha256: &str,
    manifest_canonical_sha256: &str,
    repository: &str,
    channel: &str,
) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&serde_json::json!({
        "schema_version": ADDON_PACKAGE_SCHEMA_VERSION,
        "package_id": package_id,
        "addon_id": addon_id,
        "addon_version": addon_version,
        "manifest_sha256": manifest_sha256,
        "manifest_canonical_sha256": manifest_canonical_sha256,
        "repository": repository,
        "channel": channel,
    }))?)
}

fn load_addon_package_from_path(path: &Path) -> Result<(AddonPackageReport, Vec<u8>, String)> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read addon package {}", path.display()))?;
    let package: AddonPackageReport = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse addon package {}", path.display()))?;
    let package_sha256 = hex_sha256(&bytes);
    Ok((package, bytes, package_sha256))
}

fn normalize_marketplace_channel(channel: &str) -> String {
    let channel = channel.trim();
    if channel.is_empty() {
        "stable".to_string()
    } else {
        channel.to_string()
    }
}

fn normalize_hex(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn verify_addon_package_signature_bytes(
    payload_bytes: &[u8],
    signature: &str,
    public_key: &str,
) -> Result<()> {
    let public_key_bytes = decode_hex_exact(public_key, 32)
        .with_context(|| "invalid Ed25519 package public key hex")?;
    let public_key_array: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must decode to 32 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_array)
        .with_context(|| "invalid Ed25519 package public key")?;
    let signature_bytes =
        decode_hex_exact(signature, 64).with_context(|| "invalid Ed25519 package signature hex")?;
    let signature_array: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must decode to 64 bytes"))?;
    let signature = Signature::from_bytes(&signature_array);
    verifying_key
        .verify(payload_bytes, &signature)
        .with_context(|| "package signature verification failed")
}

fn addon_trust_key_entry_from_record(
    record: StoredAddonTrustKeyRecord,
) -> Result<AddonTrustKeyEntry> {
    Ok(AddonTrustKeyEntry {
        key_id: record.key_id,
        repository: record.repository,
        channel: record.channel,
        public_key: record.public_key,
        status: record.status,
        trust_level: record.trust_level,
        approved_by: record.approved_by,
        source: record.source,
        data: record.data,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn addon_marketplace_entry_from_record(
    store: &ForgeStore,
    record: StoredAddonMarketplacePackageRecord,
) -> Result<AddonMarketplacePackageEntry> {
    let package: AddonPackageReport = serde_json::from_value(record.package)?;
    let mut policy = evaluate_addon_package_policy(store, &package, Some(&record.package_sha256))?;
    if policy.package_sha256.is_empty() {
        policy.package_sha256 = record.package_sha256.clone();
    }
    let status = if policy.install_allowed {
        "installable".to_string()
    } else {
        "blocked".to_string()
    };
    Ok(AddonMarketplacePackageEntry {
        package_id: record.package_id,
        addon_id: record.addon_id,
        addon_name: package.addon_name,
        addon_version: record.addon_version,
        repository: record.repository,
        channel: record.channel,
        manifest_sha256: record.manifest_sha256,
        manifest_canonical_sha256: package.manifest_canonical_sha256,
        package_sha256: record.package_sha256,
        status,
        signature_status: record.signature_status,
        verification_status: policy.signature.verification_status.clone(),
        source: record.source,
        summary: package.summary,
        policy,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn load_addon_package_lock_from_path(path: &Path) -> Result<(AddonPackageLockReport, String)> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read addon package lock {}", path.display()))?;
    let lock = serde_json::from_slice::<AddonPackageLockReport>(&bytes)
        .or_else(|_| serde_yaml::from_slice::<AddonPackageLockReport>(&bytes))
        .with_context(|| format!("failed to parse addon package lock {}", path.display()))?;
    if lock.schema_version != ADDON_PACKAGE_LOCK_SCHEMA_VERSION {
        bail!(
            "unsupported addon package lock schema: {}",
            lock.schema_version
        );
    }
    Ok((lock, hex_sha256(&bytes)))
}

fn enforce_addon_package_lock(
    lock_path: &Path,
    package: &AddonPackageReport,
    policy: &AddonPackagePolicyReport,
    package_sha256: &str,
) -> Result<AddonPackageLockEnforcementReport> {
    let (lock, lock_sha256) = load_addon_package_lock_from_path(lock_path)?;
    let package_sha256 = package_sha256.trim();
    let matching_id_entries = lock
        .packages
        .iter()
        .filter(|entry| entry.package_id == package.package_id)
        .collect::<Vec<_>>();
    let entry = matching_id_entries
        .iter()
        .copied()
        .find(|entry| entry.package_sha256.eq_ignore_ascii_case(package_sha256))
        .with_context(|| {
            if matching_id_entries.is_empty() {
                format!(
                    "addon package lock blocked install: package {} is not present in {}",
                    package.package_id,
                    lock_path.display()
                )
            } else {
                let locked_hashes = matching_id_entries
                    .iter()
                    .map(|entry| entry.package_sha256.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "addon package lock blocked install: package {} hash {} does not match locked hash(es) {}",
                    package.package_id,
                    package_sha256,
                    locked_hashes
                )
            }
        })?;

    let mut issues = Vec::new();
    if !entry.install_allowed {
        issues.push("locked_entry_not_install_allowed".to_string());
    }
    if entry.status != "installable" {
        issues.push(format!("locked_entry_status:{}", entry.status));
    }
    if !policy.install_allowed {
        issues.push(format!(
            "current_policy_not_install_allowed:{}",
            policy.issues.join("|")
        ));
    }
    if entry.addon_id != package.addon_id {
        issues.push(format!(
            "addon_id_mismatch:{}!={}",
            entry.addon_id, package.addon_id
        ));
    }
    if entry.addon_version != package.addon_version {
        issues.push(format!(
            "addon_version_mismatch:{}!={}",
            entry.addon_version, package.addon_version
        ));
    }
    if entry.repository != policy.repository {
        issues.push(format!(
            "repository_mismatch:{}!={}",
            entry.repository, policy.repository
        ));
    }
    if entry.channel != policy.channel {
        issues.push(format!(
            "channel_mismatch:{}!={}",
            entry.channel, policy.channel
        ));
    }
    if entry.manifest_sha256 != package.manifest_sha256 {
        issues.push("manifest_sha256_mismatch".to_string());
    }
    if entry.manifest_canonical_sha256 != policy.manifest_canonical_sha256 {
        issues.push("manifest_canonical_sha256_mismatch".to_string());
    }
    if !entry.package_sha256.eq_ignore_ascii_case(package_sha256) {
        issues.push("package_sha256_mismatch".to_string());
    }
    if entry.signature_status != package.signature.status {
        issues.push(format!(
            "signature_status_mismatch:{}!={}",
            entry.signature_status, package.signature.status
        ));
    }
    if entry.verification_status != policy.signature.verification_status {
        issues.push(format!(
            "verification_status_mismatch:{}!={}",
            entry.verification_status, policy.signature.verification_status
        ));
    }
    if !issues.is_empty() {
        bail!("addon package lock blocked install: {}", issues.join(", "));
    }

    Ok(AddonPackageLockEnforcementReport {
        schema_version: addon_package_lock_enforcement_schema_version(),
        status: "matched".to_string(),
        action: "enforce_package_lock".to_string(),
        lock_path: lock_path.display().to_string(),
        lock_sha256,
        package_id: package.package_id.clone(),
        addon_id: package.addon_id.clone(),
        addon_version: package.addon_version.clone(),
        repository: policy.repository.clone(),
        channel: policy.channel.clone(),
        package_sha256: package_sha256.to_string(),
        manifest_sha256: package.manifest_sha256.clone(),
        manifest_canonical_sha256: policy.manifest_canonical_sha256.clone(),
        lock_entry: entry.clone(),
    })
}

fn addon_package_lock_entry(package: &AddonMarketplacePackageEntry) -> AddonPackageLockEntry {
    AddonPackageLockEntry {
        package_id: package.package_id.clone(),
        addon_id: package.addon_id.clone(),
        addon_name: package.addon_name.clone(),
        addon_version: package.addon_version.clone(),
        repository: package.repository.clone(),
        channel: package.channel.clone(),
        status: package.status.clone(),
        install_allowed: package.policy.install_allowed,
        manifest_sha256: package.manifest_sha256.clone(),
        manifest_canonical_sha256: package.manifest_canonical_sha256.clone(),
        package_sha256: package.package_sha256.clone(),
        signature_status: package.signature_status.clone(),
        verification_status: package.verification_status.clone(),
        source: package.source.clone(),
        capability_count: package.summary.capability_count,
        capabilities: package.summary.capabilities.clone(),
        created_at: package.created_at.clone(),
        updated_at: package.updated_at.clone(),
    }
}

enum AddonVersionChange {
    Upgrade,
    Downgrade,
}

fn change_installed_addon_version(
    store: &ForgeStore,
    manifest_path: &Path,
    addon_dirs: &[PathBuf],
    change: AddonVersionChange,
    status: &str,
    action: &str,
) -> Result<AddonLifecycleReport> {
    let candidate = load_addon_manifest_from_path(manifest_path)?;
    let record = store.load_installed_addon(&candidate.id)?;
    let previous_manifest = installed_manifest_from_record(&record)?;
    let previous_version = previous_manifest.version.clone();
    let candidate_version = candidate.version.clone();
    match compare_addon_versions(&candidate_version, &previous_version).with_context(|| {
        format!("cannot compare addon versions `{candidate_version}` and `{previous_version}`")
    })? {
        std::cmp::Ordering::Greater if matches!(change, AddonVersionChange::Upgrade) => {}
        std::cmp::Ordering::Less if matches!(change, AddonVersionChange::Downgrade) => {}
        std::cmp::Ordering::Equal => {
            bail!(
                "addon {} is already at version {}",
                candidate.id,
                candidate.version
            );
        }
        std::cmp::Ordering::Greater => {
            bail!(
                "addon {} version {} is newer than installed {}; use upgrade",
                candidate.id,
                candidate.version,
                previous_version
            );
        }
        std::cmp::Ordering::Less => {
            bail!(
                "addon {} version {} is older than installed {}; use downgrade",
                candidate.id,
                candidate.version,
                previous_version
            );
        }
    }

    let mut manifest = candidate;
    manifest.lifecycle = record.status.clone();
    manifest.source = format!("file:{}", manifest_path.display());
    let source = manifest.source.clone();
    validate_candidate_catalog(store, addon_dirs, Some(manifest.clone()))?;
    ensure_version_change_migration_plan(&previous_manifest, &manifest)?;
    if addon_enabled(&manifest) {
        ensure_addon_permissions_authorized(store, &manifest)?;
    }
    let migration_workflow = create_version_change_migration_workflow_if_needed(
        store,
        &previous_manifest,
        &manifest,
        action,
        "forge_cli",
    )?;
    store.save_installed_addon(
        &manifest.id,
        &record.status,
        &source,
        &serde_json::to_value(&manifest)?,
    )?;
    let lifecycle = authorized_lifecycle_for_manifest(store, &manifest, &record.status)?;
    materialize_installed_addon_capabilities(store, &manifest, &lifecycle)?;
    let mut report = lifecycle_report(store, addon_dirs, &manifest.id, status, action)?;
    report.migration_workflow = migration_workflow;
    Ok(report)
}

fn update_addon_lifecycle(
    store: &ForgeStore,
    addon_id: &str,
    lifecycle: &str,
    addon_dirs: &[PathBuf],
    status: &str,
    action: &str,
) -> Result<AddonLifecycleReport> {
    let record = store.load_installed_addon(addon_id)?;
    let mut manifest = installed_manifest_from_record(&record)?;
    manifest.lifecycle = lifecycle.to_string();
    validate_candidate_catalog(store, addon_dirs, Some(manifest.clone()))?;
    if addon_enabled(&manifest) {
        ensure_addon_permissions_authorized(store, &manifest)?;
    }
    store.update_installed_addon_status(addon_id, lifecycle)?;
    materialize_installed_addon_capabilities(store, &manifest, lifecycle)?;
    lifecycle_report(store, addon_dirs, addon_id, status, action)
}

fn lifecycle_report(
    store: &ForgeStore,
    addon_dirs: &[PathBuf],
    addon_id: &str,
    status: &str,
    action: &str,
) -> Result<AddonLifecycleReport> {
    let record = store.load_installed_addon(addon_id)?;
    let catalog = load_addon_catalog_from_store(store, addon_dirs)?;
    Ok(AddonLifecycleReport {
        schema_version: addon_lifecycle_schema_version(),
        status: status.to_string(),
        action: action.to_string(),
        addon: installed_view_from_record(record)?,
        validation: validate_addon_catalog(&catalog),
        migration_workflow: None,
    })
}

fn installed_manifest_from_record(record: &StoredAddonRecord) -> Result<AddonManifest> {
    let mut manifest: AddonManifest = serde_json::from_value(record.manifest.clone())
        .with_context(|| format!("invalid installed addon manifest {}", record.id))?;
    manifest.lifecycle = record.status.clone();
    manifest.source = record.source.clone();
    Ok(manifest)
}

fn installed_view_from_record(record: StoredAddonRecord) -> Result<InstalledAddonView> {
    let manifest = installed_manifest_from_record(&record)?;
    Ok(InstalledAddonView {
        id: record.id,
        name: manifest.name,
        version: manifest.version,
        lifecycle: record.status,
        source: record.source,
        capability_count: manifest.capabilities.len(),
        installed_at: record.installed_at,
        updated_at: record.updated_at,
    })
}

fn sync_installed_addon_capability_index(store: &ForgeStore) -> Result<()> {
    for record in store.list_installed_addons()? {
        let manifest = installed_manifest_from_record(&record)?;
        let lifecycle = authorized_lifecycle_for_manifest(store, &manifest, &record.status)?;
        materialize_installed_addon_capabilities(store, &manifest, &lifecycle)?;
    }
    Ok(())
}

fn materialize_installed_addon_capabilities(
    store: &ForgeStore,
    manifest: &AddonManifest,
    lifecycle: &str,
) -> Result<()> {
    let capabilities = manifest
        .capabilities
        .iter()
        .map(|capability| {
            Ok(StoredAddonCapabilityWrite {
                capability_id: capability.id.clone(),
                title: capability.title.clone(),
                source: manifest.source.clone(),
                addon_version: manifest.version.clone(),
                domains: serde_json::to_value(&capability.domains)?,
                keywords: serde_json::to_value(&capability.keywords)?,
                workflow_extensions: serde_json::to_value(&capability.workflow_extensions)?,
                data: serde_json::to_value(capability)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    store.replace_addon_capabilities(&manifest.id, lifecycle, &capabilities)?;
    store.update_addon_capabilities_status(&manifest.id, lifecycle)?;
    Ok(())
}

fn capability_index_view_from_record(
    record: StoredAddonCapabilityRecord,
) -> Result<AddonCapabilityIndexView> {
    Ok(AddonCapabilityIndexView {
        addon_id: record.addon_id,
        capability_id: record.capability_id,
        title: record.title,
        lifecycle: record.status,
        source: record.source,
        addon_version: record.addon_version,
        domains: string_vec_from_value(record.domains)?,
        keywords: string_vec_from_value(record.keywords)?,
        workflow_extensions: string_vec_from_value(record.workflow_extensions)?,
        updated_at: record.updated_at,
    })
}

fn addon_permission_authorization_change_report(
    store: &ForgeStore,
    addon_id: &str,
    permission_id: &str,
    status: &str,
    action: &str,
) -> Result<AddonPermissionAuthorizationChangeReport> {
    let authorization = store
        .list_addon_permission_authorizations(Some(addon_id), Some(permission_id), Some(status))?
        .into_iter()
        .next()
        .with_context(|| {
            format!("addon permission authorization not found: {addon_id}:{permission_id}")
        })?;
    Ok(AddonPermissionAuthorizationChangeReport {
        schema_version: addon_permission_authorizations_schema_version(),
        status: status.to_string(),
        action: action.to_string(),
        authorization: addon_permission_authorization_view_from_record(authorization),
    })
}

fn addon_permission_authorization_view_from_record(
    record: StoredAddonPermissionAuthorizationRecord,
) -> AddonPermissionAuthorizationView {
    AddonPermissionAuthorizationView {
        addon_id: record.addon_id,
        permission_id: record.permission_id,
        status: record.status,
        risk: record.risk,
        approved_by: record.approved_by,
        source: record.source,
        granted_at: record.granted_at,
        updated_at: record.updated_at,
        data: record.data,
    }
}

fn runtime_contract_policy_entry(
    view: AddonRuntimeContractView,
) -> AddonRuntimeContractPolicyEntry {
    let mut issues = Vec::new();
    let mut status = "dispatch_ready".to_string();

    if !view.permission_gate.allowed {
        status = view.permission_gate.status.clone();
        issues.push(format!(
            "permission gate denied runtime contract: {}",
            view.permission_gate.status
        ));
    }
    if view.contract.runtime.trim().is_empty() {
        if status == "dispatch_ready" {
            status = "missing_runtime".to_string();
        }
        issues.push("runtime contract does not declare a runtime".to_string());
    }
    if view.contract.entrypoint.trim().is_empty() {
        if status == "dispatch_ready" {
            status = "missing_entrypoint".to_string();
        }
        issues.push("runtime contract does not declare an entrypoint".to_string());
    }

    let dispatch_allowed = issues.is_empty();
    AddonRuntimeContractPolicyEntry {
        addon_id: view.addon_id,
        addon_name: view.addon_name,
        addon_version: view.addon_version,
        addon_lifecycle: view.addon_lifecycle,
        contract_id: view.contract.id.clone(),
        contract_type: view.contract.contract_type.clone(),
        capability_id: view.contract.capability_id.clone(),
        runtime: view.contract.runtime.clone(),
        entrypoint: view.contract.entrypoint.clone(),
        dispatch_allowed,
        status,
        issues,
        permission_gate: view.permission_gate,
        contract: view.contract,
    }
}

fn planner_registration_from_policy_entry(
    entry: AddonRuntimeContractPolicyEntry,
) -> AddonPlannerRegistration {
    let source = planner_registration_source(&entry);
    let status = if !entry.dispatch_allowed {
        "blocked".to_string()
    } else if source == "internal_first_party_builder" {
        "core_builder_registered".to_string()
    } else {
        "external_planner_registered".to_string()
    };
    let (commands, mcp_tools) = planner_registration_actions(&entry, &source);
    AddonPlannerRegistration {
        id: format!("{}:{}", entry.addon_id, entry.contract_id),
        status,
        source,
        planner_kind: entry.contract_type.clone(),
        addon_id: entry.addon_id.clone(),
        addon_name: entry.addon_name.clone(),
        addon_version: entry.addon_version.clone(),
        addon_lifecycle: entry.addon_lifecycle.clone(),
        contract_id: entry.contract_id.clone(),
        capability_id: entry.capability_id.clone(),
        workflow_extension_id: entry.contract.workflow_extension_id.clone(),
        runtime: entry.runtime.clone(),
        entrypoint: entry.entrypoint.clone(),
        dispatch_allowed: entry.dispatch_allowed,
        permission_gate: entry.permission_gate.clone(),
        issues: entry.issues.clone(),
        commands,
        mcp_tools,
        reason: planner_registration_reason(&entry),
    }
}

fn planner_registration_source(entry: &AddonRuntimeContractPolicyEntry) -> String {
    if entry.runtime == "forge_core_builtin" && entry.entrypoint.starts_with("planner:") {
        "internal_first_party_builder".to_string()
    } else {
        "addon_runtime_contract".to_string()
    }
}

fn planner_registration_reason(entry: &AddonRuntimeContractPolicyEntry) -> String {
    if entry.runtime == "forge_core_builtin" && entry.entrypoint.starts_with("planner:") {
        "planner is currently implemented by Forge Core but declared as an Addon runtime contract"
            .to_string()
    } else if entry.dispatch_allowed {
        "planner is registered as an Addon runtime contract and can be dispatched through the runtime ledger".to_string()
    } else {
        "planner contract is registered but blocked by lifecycle, permission, runtime or entrypoint policy".to_string()
    }
}

fn planner_registration_actions(
    entry: &AddonRuntimeContractPolicyEntry,
    source: &str,
) -> (Vec<Vec<String>>, Vec<String>) {
    if source == "internal_first_party_builder" {
        return (
            vec![
                vec![
                    "forge".to_string(),
                    "addons".to_string(),
                    "resolve".to_string(),
                    "--goal".to_string(),
                    "<goal>".to_string(),
                    "--output".to_string(),
                    "json".to_string(),
                ],
                vec![
                    "forge".to_string(),
                    "plan".to_string(),
                    "--goal".to_string(),
                    "<goal>".to_string(),
                    "--output".to_string(),
                    "json".to_string(),
                ],
            ],
            vec!["forge.addons.resolve".to_string()],
        );
    }

    let mut commands = vec![vec![
        "forge".to_string(),
        "addons".to_string(),
        "contract-policy".to_string(),
        "--addon".to_string(),
        entry.addon_id.clone(),
        "--contract".to_string(),
        entry.contract_id.clone(),
        "--output".to_string(),
        "json".to_string(),
    ]];
    let mut mcp_tools = vec!["forge.addons.contract_policy".to_string()];
    if entry.dispatch_allowed {
        commands.push(vec![
            "forge".to_string(),
            "addons".to_string(),
            "dispatch-planner".to_string(),
            "--addon".to_string(),
            entry.addon_id.clone(),
            "--contract".to_string(),
            entry.contract_id.clone(),
            "--goal".to_string(),
            "<goal>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ]);
        mcp_tools.push("forge.addons.dispatch_planner".to_string());
    }
    (commands, mcp_tools)
}

fn stored_runtime_dispatch_entry(
    record: StoredRuntimeContractDispatchRecord,
) -> Result<AddonRuntimeContractDispatchEntry> {
    Ok(AddonRuntimeContractDispatchEntry {
        id: record.id,
        addon_id: record.addon_id,
        contract_id: record.contract_id,
        contract_type: record.contract_type,
        capability_id: record.capability_id,
        runtime: record.runtime,
        entrypoint: record.entrypoint,
        status: record.status,
        source: record.source,
        input: record.input,
        policy: serde_json::from_value(record.policy)?,
        data: record.data,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn load_runtime_dispatch_entry(
    store: &ForgeStore,
    dispatch_id: &str,
) -> Result<AddonRuntimeContractDispatchEntry> {
    let record = store
        .load_runtime_contract_dispatch(dispatch_id)?
        .with_context(|| format!("runtime contract dispatch not found: {dispatch_id}"))?;
    stored_runtime_dispatch_entry(record)
}

fn runtime_worker_entry(record: StoredRuntimeWorkerRecord) -> AddonRuntimeWorkerEntry {
    AddonRuntimeWorkerEntry {
        id: record.id,
        runtime: record.runtime,
        status: record.status,
        trust_level: record.trust_level,
        source: record.source,
        data: record.data,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

#[derive(Debug, Clone)]
struct LocalProcessWorkerConfig {
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
    timeout_seconds: u64,
}

#[derive(Debug, Clone)]
struct ExternalApiWorkerConfig {
    scheme: String,
    endpoint: String,
    host: String,
    port: u16,
    path: String,
    timeout_seconds: u64,
    max_response_bytes: usize,
    auth: ExternalApiWorkerAuthConfig,
}

#[derive(Debug, Clone)]
struct ExternalApiWorkerAuthConfig {
    scheme: String,
    signature_header: Option<String>,
    secret_env: Option<String>,
    secret_source: String,
    credential_vault: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct RuntimeWorkerExecution {
    status: String,
    result: serde_json::Value,
    signature: Option<String>,
    attestation: serde_json::Value,
}

fn execute_registered_runtime_worker(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
) -> Result<RuntimeWorkerExecution> {
    match worker_execution_mode(worker).as_deref() {
        Some("local_process") => {
            let config = local_process_worker_config(worker, entry)?;
            execute_local_process_worker(worker, entry, &config)
        }
        Some("external_api") => {
            let config = external_api_worker_config(worker, entry)?;
            execute_external_api_worker(worker, entry, &config)
        }
        Some(mode) => bail!("unsupported worker execution_mode {mode}"),
        None => bail!("worker must declare execution_mode local_process or external_api"),
    }
}

fn worker_execution_mode(worker: &AddonRuntimeWorkerEntry) -> Option<String> {
    worker
        .data
        .get("execution_mode")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn local_process_worker_config(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
) -> Result<LocalProcessWorkerConfig> {
    let execution_mode = worker
        .data
        .get("execution_mode")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if execution_mode != "local_process" {
        bail!("worker must declare execution_mode local_process");
    }
    ensure_worker_allowlist_contains(&worker.data, "allowed_entrypoints", &entry.entrypoint)?;
    ensure_worker_allowlist_contains(&worker.data, "allowed_contracts", &entry.contract_id)?;
    let command = worker
        .data
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("worker must declare an absolute command path")?
        .to_string();
    let command_path = Path::new(&command);
    if !command_path.is_absolute() {
        bail!("worker command must be an absolute path");
    }
    if !command_path.exists() {
        bail!("worker command does not exist: {command}");
    }
    let args = worker
        .data
        .get("args")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(|value| value.to_string())
                        .context("worker args must be strings")
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let cwd = worker
        .data
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    if let Some(cwd) = &cwd {
        let cwd_path = Path::new(cwd);
        if !cwd_path.is_absolute() {
            bail!("worker cwd must be an absolute path");
        }
        if !cwd_path.is_dir() {
            bail!("worker cwd does not exist or is not a directory: {cwd}");
        }
    }
    let timeout_seconds = worker
        .data
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30)
        .clamp(1, 300);
    Ok(LocalProcessWorkerConfig {
        command,
        args,
        cwd,
        timeout_seconds,
    })
}

fn ensure_worker_allowlist_contains(
    data: &serde_json::Value,
    field: &str,
    required: &str,
) -> Result<()> {
    let Some(values) = data.get(field).and_then(|value| value.as_array()) else {
        return Ok(());
    };
    if values.is_empty() {
        return Ok(());
    }
    if values.iter().any(|value| value.as_str() == Some(required)) {
        return Ok(());
    }
    bail!("worker {field} does not include {required}");
}

fn external_api_worker_config(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
) -> Result<ExternalApiWorkerConfig> {
    let execution_mode = worker
        .data
        .get("execution_mode")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if execution_mode != "external_api" {
        bail!("worker must declare execution_mode external_api");
    }
    ensure_worker_allowlist_contains(&worker.data, "allowed_entrypoints", &entry.entrypoint)?;
    ensure_worker_allowlist_contains(&worker.data, "allowed_contracts", &entry.contract_id)?;
    let endpoint = worker
        .data
        .get("endpoint")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external_api worker must declare endpoint")?
        .to_string();
    let (scheme, host, port, path) = parse_http_worker_endpoint(&endpoint)?;
    if !is_local_http_host(&host) && !worker_allowed_host(&worker.data, &host) {
        bail!("external_api worker endpoint host must be localhost or listed in allowed_hosts");
    }
    let timeout_seconds = worker
        .data
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30)
        .clamp(1, 300);
    let max_response_bytes = worker
        .data
        .get("max_response_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or(1024 * 1024)
        .clamp(1024, 5 * 1024 * 1024) as usize;
    let auth = external_api_worker_auth_config(&worker.data)?;
    Ok(ExternalApiWorkerConfig {
        scheme,
        endpoint,
        host,
        port,
        path,
        timeout_seconds,
        max_response_bytes,
        auth,
    })
}

fn parse_http_worker_endpoint(endpoint: &str) -> Result<(String, String, u16, String)> {
    let (scheme, rest, default_port) = if let Some(rest) = endpoint.strip_prefix("http://") {
        ("http".to_string(), rest, 80)
    } else if let Some(rest) = endpoint.strip_prefix("https://") {
        ("https".to_string(), rest, 443)
    } else {
        bail!("external_api worker endpoint must use explicit http:// or https://");
    };
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let authority = authority.trim();
    if authority.is_empty() {
        bail!("external_api worker endpoint host is required");
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(']') => {
            let port = port
                .parse::<u16>()
                .with_context(|| format!("invalid external_api endpoint port: {port}"))?;
            (host.to_string(), port)
        }
        _ => (authority.to_string(), default_port),
    };
    if host.trim().is_empty() {
        bail!("external_api worker endpoint host is required");
    }
    Ok((scheme, host, port, path))
}

fn is_local_http_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn worker_allowed_host(data: &serde_json::Value, host: &str) -> bool {
    data.get("allowed_hosts")
        .and_then(|value| value.as_array())
        .map(|values| values.iter().any(|value| value.as_str() == Some(host)))
        .unwrap_or(false)
}

fn external_api_worker_auth_config(
    data: &serde_json::Value,
) -> Result<ExternalApiWorkerAuthConfig> {
    let auth = data
        .get("auth")
        .or_else(|| data.get("auth_scheme"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none")
        .to_ascii_lowercase();
    let signature_header = data
        .get("signature_header")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_worker_http_header_name)
        .transpose()?;
    let secret_env = data
        .get("secret_env")
        .or_else(|| data.get("hmac_secret_env"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let credential_vault = data.get("credential_vault").cloned();
    let secret_source = if credential_vault.is_some() {
        "credential_vault"
    } else if secret_env.is_some() {
        "env"
    } else {
        "none"
    }
    .to_string();
    match auth.as_str() {
        "none" => Ok(ExternalApiWorkerAuthConfig {
            scheme: auth,
            signature_header: None,
            secret_env: None,
            secret_source: "none".to_string(),
            credential_vault: None,
        }),
        "bearer" | "hmac" | "hmac_sha256" => {
            if secret_source == "none" {
                bail!("external_api worker auth `{auth}` requires secret_env, hmac_secret_env or credential_vault");
            }
            Ok(ExternalApiWorkerAuthConfig {
                scheme: auth,
                signature_header,
                secret_env,
                secret_source,
                credential_vault,
            })
        }
        other => bail!("unsupported external_api worker auth `{other}`"),
    }
}

fn build_external_api_worker_auth_headers(
    config: &ExternalApiWorkerConfig,
    body: &[u8],
) -> Result<Vec<(String, String)>> {
    match config.auth.scheme.as_str() {
        "none" => Ok(Vec::new()),
        "bearer" => {
            let secret = resolve_external_api_worker_secret(&config.auth)?;
            validate_worker_http_header_value("external_api bearer secret", &secret)?;
            let header = config
                .auth
                .signature_header
                .clone()
                .unwrap_or_else(|| "Authorization".to_string());
            Ok(vec![(header, format!("Bearer {secret}"))])
        }
        "hmac" | "hmac_sha256" => {
            let secret = resolve_external_api_worker_secret(&config.auth)?;
            let header = config
                .auth
                .signature_header
                .clone()
                .unwrap_or_else(|| "X-Forge-Worker-Signature".to_string());
            let signature = format!(
                "sha256={}",
                hex_encode(&hmac_sha256(secret.as_bytes(), body))
            );
            Ok(vec![(header, signature)])
        }
        other => bail!("unsupported external_api worker auth `{other}`"),
    }
}

fn resolve_external_api_worker_secret(config: &ExternalApiWorkerAuthConfig) -> Result<String> {
    if let Some(vault) = &config.credential_vault {
        return resolve_external_api_worker_secret_from_vault(vault);
    }
    let secret_env = config.secret_env.as_deref().context(
        "external_api worker auth requires secret_env, hmac_secret_env or credential_vault",
    )?;
    let secret = env::var(secret_env)
        .with_context(|| format!("external_api worker secret env `{secret_env}` is not set"))?;
    if secret.is_empty() {
        bail!("external_api worker secret env `{secret_env}` is empty");
    }
    Ok(secret)
}

fn resolve_external_api_worker_secret_from_vault(vault: &serde_json::Value) -> Result<String> {
    let contract = required_worker_vault_value(vault, "contract")?;
    let data = required_worker_vault_value(vault, "data")?;
    let record = required_worker_vault_value(vault, "record")?;
    let field = required_worker_vault_value(vault, "field")?;
    let vault_bin = vault
        .get("vault_bin")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
        .with_context(|| "failed to run credential-vault for external_api worker secret")?;
    if !output.status.success() {
        let exit_code = output.status.code().unwrap_or(1);
        bail!(
            "credential-vault resolve failed for external_api worker secret `{record}:{field}` with exit code {exit_code}"
        );
    }
    let secret = String::from_utf8(output.stdout)
        .context("credential-vault returned non-UTF-8 external_api worker secret")?;
    if secret.is_empty() {
        bail!(
            "credential-vault returned an empty external_api worker secret for `{record}:{field}`"
        );
    }
    Ok(secret)
}

fn required_worker_vault_value(vault: &serde_json::Value, field: &str) -> Result<String> {
    vault
        .get(field)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .with_context(|| format!("external_api worker credential_vault.{field} is required"))
}

fn normalize_worker_http_header_name(value: &str) -> Result<String> {
    if value.is_empty() {
        bail!("external_api worker HTTP header name cannot be empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!("external_api worker HTTP header name must contain only ASCII letters, digits or hyphen");
    }
    Ok(value.to_string())
}

fn validate_worker_http_header_value(field: &str, value: &str) -> Result<()> {
    if value.contains('\r') || value.contains('\n') {
        bail!("{field} must not contain CR/LF characters");
    }
    Ok(())
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

fn runtime_worker_request(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": "forge.addon_runtime_worker_request.v1",
        "dispatch_id": entry.id,
        "worker_id": worker.id,
        "runtime": entry.runtime,
        "entrypoint": entry.entrypoint,
        "contract_id": entry.contract_id,
        "contract_type": entry.contract_type,
        "capability_id": entry.capability_id,
        "input": entry.input,
        "policy": entry.policy,
    })
}

fn execute_local_process_worker(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
    config: &LocalProcessWorkerConfig,
) -> Result<RuntimeWorkerExecution> {
    let request = runtime_worker_request(worker, entry);
    let request_bytes = serde_json::to_vec(&request)?;
    let started_at = Utc::now();
    let mut command = Command::new(&config.command);
    command
        .args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &config.cwd {
        command.current_dir(cwd);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Ok(RuntimeWorkerExecution {
                status: "failed".to_string(),
                result: serde_json::json!({
                    "outcome": "local_process_spawn_failed",
                    "error": error.to_string(),
                    "command": config.command,
                }),
                signature: None,
                attestation: local_process_attestation(
                    worker,
                    entry,
                    config,
                    started_at,
                    None,
                    "spawn_failed",
                ),
            });
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&request_bytes)?;
    }
    let timeout = Duration::from_secs(config.timeout_seconds);
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            return Ok(RuntimeWorkerExecution {
                status: "failed".to_string(),
                result: serde_json::json!({
                    "outcome": "local_process_timeout",
                    "timeout_seconds": config.timeout_seconds,
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                }),
                signature: None,
                attestation: local_process_attestation(
                    worker,
                    entry,
                    config,
                    started_at,
                    output.status.code(),
                    "timeout",
                ),
            });
        }
        thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let parsed = serde_json::from_str::<serde_json::Value>(&stdout).ok();
    let default_status = if output.status.success() {
        "completed"
    } else {
        "failed"
    };
    let status = parsed
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .unwrap_or(default_status)
        .to_string();
    let result = parsed
        .as_ref()
        .and_then(|value| value.get("result"))
        .cloned()
        .unwrap_or_else(|| {
            serde_json::json!({
                "outcome": "local_process_completed",
                "exit_code": output.status.code(),
                "stdout": stdout,
                "stderr": stderr,
            })
        });
    let signature = parsed
        .as_ref()
        .and_then(|value| value.get("signature"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let attestation = parsed
        .as_ref()
        .and_then(|value| value.get("attestation"))
        .cloned()
        .unwrap_or_else(|| {
            local_process_attestation(
                worker,
                entry,
                config,
                started_at,
                output.status.code(),
                "completed",
            )
        });
    Ok(RuntimeWorkerExecution {
        status,
        result,
        signature,
        attestation,
    })
}

fn execute_external_api_worker(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
    config: &ExternalApiWorkerConfig,
) -> Result<RuntimeWorkerExecution> {
    let request = runtime_worker_request(worker, entry);
    let request_bytes = serde_json::to_vec(&request)?;
    let started_at = Utc::now();
    let response = match post_external_api_worker_json(config, &request_bytes) {
        Ok(response) => response,
        Err(error) => {
            return Ok(RuntimeWorkerExecution {
                status: "failed".to_string(),
                result: serde_json::json!({
                    "outcome": "external_api_request_failed",
                    "error": error.to_string(),
                    "endpoint_host": config.host,
                    "endpoint_path": config.path,
                }),
                signature: None,
                attestation: external_api_attestation(
                    worker,
                    entry,
                    config,
                    ExternalApiAttestationInput {
                        started_at,
                        status_code: None,
                        response_bytes: None,
                        outcome: "request_failed",
                        response_sha256: None,
                    },
                ),
            });
        }
    };
    let response_sha256 = hex_sha256(&response.body);
    let parsed = serde_json::from_slice::<serde_json::Value>(&response.body).ok();
    if !(200..=299).contains(&response.status_code) {
        return Ok(RuntimeWorkerExecution {
            status: "failed".to_string(),
            result: serde_json::json!({
                "outcome": "external_api_http_error",
                "status_code": response.status_code,
                "response_body_sha256": response_sha256,
                "response_bytes": response.body.len(),
            }),
            signature: None,
            attestation: external_api_attestation(
                worker,
                entry,
                config,
                ExternalApiAttestationInput {
                    started_at,
                    status_code: Some(response.status_code),
                    response_bytes: Some(response.body.len()),
                    outcome: "http_error",
                    response_sha256: Some(&response_sha256),
                },
            ),
        });
    }
    let status = parsed
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .unwrap_or(if parsed.is_some() {
            "completed"
        } else {
            "failed"
        })
        .to_string();
    let result = parsed
        .as_ref()
        .and_then(|value| value.get("result"))
        .cloned()
        .unwrap_or_else(|| {
            serde_json::json!({
                "outcome": if parsed.is_some() { "external_api_completed" } else { "external_api_invalid_json" },
                "status_code": response.status_code,
                "response_body_sha256": response_sha256,
                "response_bytes": response.body.len(),
            })
        });
    let signature = parsed
        .as_ref()
        .and_then(|value| value.get("signature"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let attestation = parsed
        .as_ref()
        .and_then(|value| value.get("attestation"))
        .cloned()
        .unwrap_or_else(|| {
            external_api_attestation(
                worker,
                entry,
                config,
                ExternalApiAttestationInput {
                    started_at,
                    status_code: Some(response.status_code),
                    response_bytes: Some(response.body.len()),
                    outcome: if status == "completed" {
                        "completed"
                    } else {
                        "failed"
                    },
                    response_sha256: Some(&response_sha256),
                },
            )
        });
    Ok(RuntimeWorkerExecution {
        status,
        result,
        signature,
        attestation,
    })
}

struct ExternalApiHttpResponse {
    status_code: u16,
    body: Vec<u8>,
}

fn post_external_api_worker_json(
    config: &ExternalApiWorkerConfig,
    request_bytes: &[u8],
) -> Result<ExternalApiHttpResponse> {
    let extra_headers = build_external_api_worker_auth_headers(config, request_bytes)?;
    if config.scheme.eq_ignore_ascii_case("https") {
        return post_external_api_worker_https_curl(config, request_bytes, &extra_headers);
    }
    let timeout = Duration::from_secs(config.timeout_seconds);
    let mut addrs = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve external_api host {}", config.host))?;
    let addr = addrs
        .next()
        .with_context(|| format!("no socket address resolved for {}", config.host))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .with_context(|| format!("failed to connect to external_api worker {}", config.host))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let extra_headers = extra_headers
        .iter()
        .map(|(name, value)| {
            let name = normalize_worker_http_header_name(name)?;
            validate_worker_http_header_value("external_api worker header value", value)?;
            Ok(format!("{name}: {value}\r\n"))
        })
        .collect::<Result<Vec<_>>>()?
        .join("");
    let header = format!(
        "POST {} HTTP/1.1\r\nHost: {}:{}\r\nUser-Agent: forge-core/external-api-worker\r\nContent-Type: application/json\r\nAccept: application/json\r\n{extra_headers}Connection: close\r\nContent-Length: {}\r\n\r\n",
        config.path,
        config.host,
        config.port,
        request_bytes.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(request_bytes)?;
    let mut response = Vec::new();
    let max_total_bytes = config.max_response_bytes.saturating_add(16 * 1024);
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..read]);
        if response.len() > max_total_bytes {
            bail!("external_api response exceeded max_response_bytes");
        }
    }
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .context("external_api response missing HTTP header terminator")?;
    let headers = String::from_utf8_lossy(&response[..header_end]);
    let status_code = parse_http_status_code(&headers)?;
    let body = response[header_end..].to_vec();
    if body.len() > config.max_response_bytes {
        bail!("external_api response body exceeded max_response_bytes");
    }
    Ok(ExternalApiHttpResponse { status_code, body })
}

fn post_external_api_worker_https_curl(
    config: &ExternalApiWorkerConfig,
    request_bytes: &[u8],
    extra_headers: &[(String, String)],
) -> Result<ExternalApiHttpResponse> {
    let simulated = env::var("FORGE_EXTERNAL_API_WORKER_HTTPS_MODE")
        .map(|value| value.eq_ignore_ascii_case("simulate"))
        .unwrap_or(false);
    if simulated {
        let body = serde_json::json!({
            "status": "completed",
            "result": {
                "outcome": "external_api_https_simulated",
                "endpoint_sha256": hex_sha256(config.endpoint.as_bytes()),
                "request_sha256": hex_sha256(request_bytes),
                "auth_scheme": config.auth.scheme.as_str(),
                "secret_source": config.auth.secret_source.as_str(),
            },
            "attestation": {
                "schema_version": "forge.addon_runtime_worker_attestation.v1",
                "execution_mode": "external_api",
                "endpoint_scheme": "https",
                "simulated": true,
                "auth_scheme": config.auth.scheme.as_str(),
                "secret_source": config.auth.secret_source.as_str(),
            }
        })
        .to_string()
        .into_bytes();
        return Ok(ExternalApiHttpResponse {
            status_code: 202,
            body,
        });
    }

    let timeout_seconds = config.timeout_seconds.max(1).to_string();
    let mut command = Command::new("curl");
    command.args([
        "-sS",
        "--max-time",
        &timeout_seconds,
        "-X",
        "POST",
        &config.endpoint,
        "-H",
        "User-Agent: forge-core/external-api-worker",
        "-H",
        "Content-Type: application/json",
        "-H",
        "Accept: application/json",
    ]);
    for (name, value) in extra_headers {
        let name = normalize_worker_http_header_name(name)?;
        validate_worker_http_header_value("external_api worker header value", value)?;
        command.args(["-H", &format!("{name}: {value}")]);
    }
    command.args(["--data-binary", "@-", "-w", "\n%{http_code}"]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .context("failed to execute curl for external_api HTTPS worker")?;
    child
        .stdin
        .as_mut()
        .context("failed to open curl stdin for external_api HTTPS worker")?
        .write_all(request_bytes)?;
    let output = child
        .wait_with_output()
        .context("failed to wait for external_api HTTPS worker curl")?;
    let (status_code, response_body) = parse_worker_curl_http_status(&output.stdout)
        .unwrap_or_else(|| (if output.status.success() { 200 } else { 0 }, output.stdout));
    let body = if response_body.is_empty() && !output.status.success() {
        output.stderr
    } else {
        response_body
    };
    if body.len() > config.max_response_bytes {
        bail!("external_api response body exceeded max_response_bytes");
    }
    Ok(ExternalApiHttpResponse { status_code, body })
}

fn parse_worker_curl_http_status(output: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = output.iter().rposition(|byte| *byte == b'\n')?;
    let status_text = std::str::from_utf8(&output[split + 1..]).ok()?.trim();
    if status_text.len() != 3 || !status_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let status_code = status_text.parse::<u16>().ok()?;
    Some((status_code, output[..split].to_vec()))
}

fn parse_http_status_code(headers: &str) -> Result<u16> {
    let status_line = headers
        .lines()
        .next()
        .context("external_api response missing status line")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .context("external_api response status line missing code")?;
    status
        .parse::<u16>()
        .with_context(|| format!("invalid external_api HTTP status code: {status}"))
}

fn external_api_attestation(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
    config: &ExternalApiWorkerConfig,
    input: ExternalApiAttestationInput<'_>,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": "forge.addon_runtime_worker_attestation.v1",
        "outcome": input.outcome,
        "worker_id": worker.id,
        "runtime": worker.runtime,
        "trust_level": worker.trust_level,
        "dispatch_id": entry.id,
        "contract_id": entry.contract_id,
        "entrypoint": entry.entrypoint,
        "execution_mode": "external_api",
        "endpoint_scheme": config.scheme.as_str(),
        "endpoint_sha256": hex_sha256(config.endpoint.as_bytes()),
        "endpoint_host": config.host.as_str(),
        "endpoint_path": config.path.as_str(),
        "auth_scheme": config.auth.scheme.as_str(),
        "secret_source": config.auth.secret_source.as_str(),
        "secret_env": config.auth.secret_env.as_deref(),
        "credential_vault": config.auth.credential_vault.as_ref(),
        "timeout_seconds": config.timeout_seconds,
        "status_code": input.status_code,
        "response_bytes": input.response_bytes,
        "response_sha256": input.response_sha256,
        "started_at": input.started_at.to_rfc3339(),
        "finished_at": Utc::now().to_rfc3339(),
    })
}

fn local_process_attestation(
    worker: &AddonRuntimeWorkerEntry,
    entry: &AddonRuntimeContractDispatchEntry,
    config: &LocalProcessWorkerConfig,
    started_at: chrono::DateTime<Utc>,
    exit_code: Option<i32>,
    outcome: &str,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": "forge.addon_runtime_worker_attestation.v1",
        "outcome": outcome,
        "worker_id": worker.id,
        "runtime": worker.runtime,
        "trust_level": worker.trust_level,
        "dispatch_id": entry.id,
        "contract_id": entry.contract_id,
        "entrypoint": entry.entrypoint,
        "execution_mode": "local_process",
        "command_sha256": hex_sha256(config.command.as_bytes()),
        "args_sha256": hex_sha256(serde_json::to_string(&config.args).unwrap_or_default().as_bytes()),
        "timeout_seconds": config.timeout_seconds,
        "exit_code": exit_code,
        "started_at": started_at.to_rfc3339(),
        "finished_at": Utc::now().to_rfc3339(),
    })
}

fn runtime_worker_report(
    status: &str,
    runtime: Option<&str>,
    worker_status: Option<&str>,
    trust_level: Option<&str>,
    workers: Vec<AddonRuntimeWorkerEntry>,
) -> AddonRuntimeWorkerReport {
    let available_count = workers
        .iter()
        .filter(|worker| worker.status == "available")
        .count();
    AddonRuntimeWorkerReport {
        schema_version: addon_runtime_workers_schema_version(),
        status: status.to_string(),
        worker_count: workers.len(),
        available_count,
        filters: AddonRuntimeWorkerFilters {
            runtime: normalize_filter(runtime),
            status: normalize_filter(worker_status),
            trust_level: normalize_filter(trust_level),
        },
        workers,
    }
}

struct RuntimeDispatchPolicyRecheck {
    policy: AddonRuntimeContractPolicyEntry,
    allowed: bool,
    outcome: Option<serde_json::Value>,
}

fn recheck_runtime_dispatch_policy(
    catalog: &AddonCatalog,
    entry: &AddonRuntimeContractDispatchEntry,
) -> RuntimeDispatchPolicyRecheck {
    let policy = evaluate_addon_runtime_contract_policy(
        catalog,
        Some(&entry.addon_id),
        Some(&entry.contract_id),
        None,
        None,
        None,
    );
    if policy.contracts.len() != 1 {
        let detail = if policy.contracts.is_empty() {
            "runtime contract missing during dispatch policy recheck"
        } else {
            "runtime contract ambiguous during dispatch policy recheck"
        };
        return RuntimeDispatchPolicyRecheck {
            policy: entry.policy.clone(),
            allowed: false,
            outcome: Some(serde_json::json!({
                "outcome": "policy_recheck_failed",
                "reason": detail,
            })),
        };
    }
    let policy_entry = policy.contracts.into_iter().next().unwrap();
    if !policy_entry.dispatch_allowed {
        return RuntimeDispatchPolicyRecheck {
            policy: policy_entry,
            allowed: false,
            outcome: Some(serde_json::json!({
                "outcome": "policy_recheck_failed",
                "reason": "runtime contract is no longer dispatchable",
            })),
        };
    }
    if policy_entry.contract_type != entry.contract_type
        || policy_entry.capability_id != entry.capability_id
        || policy_entry.runtime != entry.runtime
        || policy_entry.entrypoint != entry.entrypoint
    {
        return RuntimeDispatchPolicyRecheck {
            policy: policy_entry,
            allowed: false,
            outcome: Some(serde_json::json!({
                "outcome": "contract_changed_after_enqueue",
                "reason": "runtime contract shape changed after the dispatch was queued",
            })),
        };
    }
    RuntimeDispatchPolicyRecheck {
        policy: policy_entry,
        allowed: true,
        outcome: None,
    }
}

fn runtime_dispatch_preview_report(
    mut entry: AddonRuntimeContractDispatchEntry,
    policy: Option<AddonRuntimeContractPolicyEntry>,
    report_status: &str,
    worker: &str,
    dry_run: bool,
    outcome: serde_json::Value,
) -> AddonRuntimeContractDispatchReport {
    let policy = policy.unwrap_or_else(|| entry.policy.clone());
    entry.data = runtime_processing_data(
        &entry.data,
        serde_json::json!({
            "worker": worker,
            "dry_run": dry_run,
            "previous_status": entry.status.clone(),
            "status": entry.status.clone(),
            "would_update_status": entry.status.clone(),
            "policy_status": policy.status.clone(),
            "dispatch_allowed": policy.dispatch_allowed,
            "outcome": outcome,
        }),
    );
    entry.policy = policy;
    dispatch_report(report_status, dry_run, vec![entry])
}

fn claimed_external_worker_id(entry: &AddonRuntimeContractDispatchEntry) -> Option<String> {
    entry
        .data
        .pointer("/runtime_processing/outcome/claim/worker_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn claimed_external_worker_snapshot(
    entry: &AddonRuntimeContractDispatchEntry,
) -> Option<AddonRuntimeWorkerEntry> {
    entry
        .data
        .pointer("/runtime_processing/outcome/claim/worker")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn verify_external_completion_signature(
    dispatch_id: &str,
    worker_id: &str,
    completion_status: &str,
    result_sha256: &str,
    attestation_sha256: &str,
    worker: &AddonRuntimeWorkerEntry,
    signature: &str,
) -> serde_json::Value {
    let scheme = worker
        .data
        .get("signature_scheme")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if signature.trim().is_empty() {
        return serde_json::json!({
            "status": "absent",
            "scheme": if scheme.is_empty() { serde_json::Value::Null } else { serde_json::json!(scheme) },
        });
    }
    if scheme != "ed25519" {
        return serde_json::json!({
            "status": "unconfigured",
            "scheme": if scheme.is_empty() { serde_json::Value::Null } else { serde_json::json!(scheme) },
            "reason": "worker does not declare signature_scheme ed25519",
        });
    }
    let public_key_hex = match worker
        .data
        .get("public_key_hex")
        .and_then(|value| value.as_str())
    {
        Some(value) => value,
        None => {
            return serde_json::json!({
                "status": "unconfigured",
                "scheme": "ed25519",
                "reason": "worker does not declare public_key_hex",
            });
        }
    };
    let public_key_bytes = match decode_hex_exact(public_key_hex, 32) {
        Ok(bytes) => bytes,
        Err(error) => {
            return serde_json::json!({
                "status": "invalid",
                "scheme": "ed25519",
                "reason": format!("invalid public_key_hex: {error}"),
            });
        }
    };
    let signature_bytes = match decode_hex_exact(signature, 64) {
        Ok(bytes) => bytes,
        Err(error) => {
            return serde_json::json!({
                "status": "invalid",
                "scheme": "ed25519",
                "reason": format!("invalid signature hex: {error}"),
            });
        }
    };
    let public_key_array: [u8; 32] = match public_key_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return serde_json::json!({
                "status": "invalid",
                "scheme": "ed25519",
                "reason": "public key must decode to 32 bytes",
            });
        }
    };
    let signature_array: [u8; 64] = match signature_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return serde_json::json!({
                "status": "invalid",
                "scheme": "ed25519",
                "reason": "signature must decode to 64 bytes",
            });
        }
    };
    let verifying_key = match VerifyingKey::from_bytes(&public_key_array) {
        Ok(key) => key,
        Err(error) => {
            return serde_json::json!({
                "status": "invalid",
                "scheme": "ed25519",
                "reason": format!("invalid Ed25519 public key: {error}"),
            });
        }
    };
    let signature = Signature::from_bytes(&signature_array);
    let payload = external_completion_signature_payload(
        dispatch_id,
        worker_id,
        completion_status,
        result_sha256,
        attestation_sha256,
    );
    match verifying_key.verify(payload.as_bytes(), &signature) {
        Ok(()) => serde_json::json!({
            "status": "verified",
            "scheme": "ed25519",
            "payload_sha256": hex_sha256(payload.as_bytes()),
        }),
        Err(error) => serde_json::json!({
            "status": "invalid",
            "scheme": "ed25519",
            "payload_sha256": hex_sha256(payload.as_bytes()),
            "reason": format!("signature verification failed: {error}"),
        }),
    }
}

fn external_completion_signature_payload(
    dispatch_id: &str,
    worker_id: &str,
    completion_status: &str,
    result_sha256: &str,
    attestation_sha256: &str,
) -> String {
    format!(
        "forge.addon_runtime_contract_completion.v1\ndispatch_id={dispatch_id}\nworker_id={worker_id}\nstatus={completion_status}\nresult_sha256={result_sha256}\nattestation_sha256={attestation_sha256}"
    )
}

fn decode_hex_exact(value: &str, expected_bytes: usize) -> Result<Vec<u8>> {
    let value = value.trim();
    if value.len() != expected_bytes * 2 {
        bail!(
            "expected {} hex chars for {} bytes, got {}",
            expected_bytes * 2,
            expected_bytes,
            value.len()
        );
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(expected_bytes);
    for index in (0..bytes.len()).step_by(2) {
        let high = hex_nibble(bytes[index])
            .with_context(|| format!("invalid hex char at offset {index}"))?;
        let low = hex_nibble(bytes[index + 1])
            .with_context(|| format!("invalid hex char at offset {}", index + 1))?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("not a hex digit"),
    }
}

fn execute_builtin_runtime_contract(
    entry: &AddonRuntimeContractDispatchEntry,
) -> Option<serde_json::Value> {
    match entry.entrypoint.as_str() {
        "builtin:echo" | "forge_core.echo" => Some(serde_json::json!({
            "kind": "forge_core_builtin_echo",
            "dispatch_id": entry.id.clone(),
            "addon_id": entry.addon_id.clone(),
            "contract_id": entry.contract_id.clone(),
            "capability_id": entry.capability_id.clone(),
            "input": entry.input.clone(),
        })),
        "builtin:ack" | "forge_core.ack" => Some(serde_json::json!({
            "kind": "forge_core_builtin_ack",
            "dispatch_id": entry.id.clone(),
            "addon_id": entry.addon_id.clone(),
            "contract_id": entry.contract_id.clone(),
            "capability_id": entry.capability_id.clone(),
            "accepted": true,
        })),
        _ => None,
    }
}

fn update_runtime_dispatch_entry(
    store: &ForgeStore,
    mut entry: AddonRuntimeContractDispatchEntry,
    input: RuntimeDispatchUpdateInput<'_>,
) -> Result<AddonRuntimeContractDispatchReport> {
    let reported_status = if input.dry_run {
        "dry_run"
    } else {
        input.status
    };
    let data = runtime_processing_data(
        &entry.data,
        serde_json::json!({
            "worker": input.worker,
            "dry_run": input.dry_run,
            "previous_status": entry.status.clone(),
            "status": reported_status,
            "would_update_status": input.status,
            "policy_status": input.policy.status.clone(),
            "dispatch_allowed": input.policy.dispatch_allowed,
            "outcome": input.outcome,
        }),
    );
    entry.status = reported_status.to_string();
    entry.policy = input.policy;
    entry.data = data;

    if !input.dry_run {
        let policy_value = serde_json::to_value(&entry.policy)?;
        store.update_runtime_contract_dispatch_state(
            &entry.id,
            input.status,
            &policy_value,
            &entry.data,
        )?;
        let refreshed = store
            .load_runtime_contract_dispatch(&entry.id)?
            .with_context(|| format!("runtime contract dispatch disappeared: {}", entry.id))?;
        entry = stored_runtime_dispatch_entry(refreshed)?;
    }

    Ok(dispatch_report(
        if input.dry_run {
            "runtime_contract_dispatch_dry_run"
        } else {
            input.report_status
        },
        input.dry_run,
        vec![entry],
    ))
}

fn runtime_processing_data(
    existing: &serde_json::Value,
    processing: serde_json::Value,
) -> serde_json::Value {
    let mut data = if let Some(map) = existing.as_object() {
        map.clone()
    } else {
        let mut map = serde_json::Map::new();
        map.insert("previous_data".to_string(), existing.clone());
        map
    };
    if let Some(previous_processing) = data.get("runtime_processing").cloned() {
        let mut history = data
            .remove("runtime_processing_history")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        history.push(previous_processing);
        data.insert(
            "runtime_processing_history".to_string(),
            serde_json::Value::Array(history),
        );
    }
    data.insert("runtime_processing".to_string(), processing);
    serde_json::Value::Object(data)
}

fn dispatch_report(
    status: &str,
    dry_run: bool,
    dispatches: Vec<AddonRuntimeContractDispatchEntry>,
) -> AddonRuntimeContractDispatchReport {
    let queued_count = dispatches
        .iter()
        .filter(|dispatch| dispatch.status == "queued")
        .count();
    let claimed_count = dispatches
        .iter()
        .filter(|dispatch| dispatch.status == "claimed_external_worker")
        .count();
    let completed_count = dispatches
        .iter()
        .filter(|dispatch| dispatch.status == "completed")
        .count();
    let failed_count = dispatches
        .iter()
        .filter(|dispatch| dispatch.status == "failed")
        .count();
    let needs_external_worker_count = dispatches
        .iter()
        .filter(|dispatch| dispatch.status == "needs_external_worker")
        .count();
    let blocked_count = dispatches
        .iter()
        .filter(|dispatch| dispatch.status == "blocked")
        .count();
    AddonRuntimeContractDispatchReport {
        schema_version: addon_runtime_contract_dispatch_schema_version(),
        status: status.to_string(),
        dispatch_count: dispatches.len(),
        queued_count,
        claimed_count,
        completed_count,
        failed_count,
        needs_external_worker_count,
        blocked_count,
        dry_run,
        dispatches,
    }
}

fn addon_observability_entry(
    store: &ForgeStore,
    addon: &AddonManifest,
    dispatch_limit: usize,
    runtime_events: &[StoredGlobalEventRecord],
) -> Result<AddonObservabilityEntry> {
    let dispatches =
        store.list_runtime_contract_dispatches(Some(&addon.id), None, None, dispatch_limit)?;
    let permission_ids = addon
        .permissions
        .iter()
        .map(|permission| permission.id.clone())
        .collect::<Vec<_>>();
    Ok(AddonObservabilityEntry {
        addon_id: addon.id.clone(),
        addon_name: addon.name.clone(),
        addon_version: addon.version.clone(),
        addon_lifecycle: addon.lifecycle.clone(),
        source: addon.source.clone(),
        capability_count: addon.capabilities.len(),
        dependency_count: addon.dependencies.len(),
        permission_count: addon.permissions.len(),
        runtime_contract_count: addon.runtime_contracts.len(),
        view_count: addon.views.len(),
        artifact_type_count: addon.artifact_types.len(),
        event_type_count: addon.event_types.len(),
        event_adapter_count: addon.event_adapters.len(),
        integration_count: addon.integrations.len(),
        context_provider_count: addon.context_providers.len(),
        memory_provider_count: addon.memory_providers.len(),
        permission_gate: addon_permission_gate(addon, &permission_ids),
        dependencies: addon.dependencies.clone(),
        capabilities: addon
            .capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect(),
        runtime_contracts: addon
            .runtime_contracts
            .iter()
            .map(|contract| contract.id.clone())
            .collect(),
        views: addon.views.iter().map(|view| view.id.clone()).collect(),
        artifact_types: addon
            .artifact_types
            .iter()
            .map(|artifact| artifact.id.clone())
            .collect(),
        event_types: addon
            .event_types
            .iter()
            .map(|event_type| event_type.id.clone())
            .collect(),
        integrations: addon
            .integrations
            .iter()
            .map(|integration| integration.id.clone())
            .collect(),
        event_flow: addon_event_flow_summary(addon, runtime_events),
        dispatches: addon_dispatch_observability(&dispatches),
    })
}

fn addon_observability_totals(addons: &[AddonObservabilityEntry]) -> AddonObservabilityTotals {
    let mut totals = AddonObservabilityTotals::default();
    for addon in addons {
        totals.capability_count += addon.capability_count;
        totals.dependency_count += addon.dependency_count;
        totals.permission_count += addon.permission_count;
        totals.runtime_contract_count += addon.runtime_contract_count;
        totals.view_count += addon.view_count;
        totals.artifact_type_count += addon.artifact_type_count;
        totals.event_type_count += addon.event_type_count;
        totals.event_adapter_count += addon.event_adapter_count;
        totals.integration_count += addon.integration_count;
        totals.dispatch_count += addon.dispatches.dispatch_count;
        totals.queued_dispatch_count += addon.dispatches.queued_count;
        totals.completed_dispatch_count += addon.dispatches.completed_count;
        totals.failed_dispatch_count += addon.dispatches.failed_count;
        totals.blocked_dispatch_count += addon.dispatches.blocked_count;
        totals.needs_external_worker_count += addon.dispatches.needs_external_worker_count;
        totals.runtime_event_count += addon.event_flow.runtime_event_count;
        totals.runtime_consumed_event_count += addon.event_flow.runtime_consumed_event_count;
        totals.runtime_emitted_event_count += addon.event_flow.runtime_emitted_event_count;
    }
    totals
}

fn addon_event_flow_summary(
    addon: &AddonManifest,
    runtime_events: &[StoredGlobalEventRecord],
) -> AddonEventFlowSummary {
    let mut summary = AddonEventFlowSummary::default();
    for adapter in &addon.event_adapters {
        let direction = adapter.direction.trim().to_lowercase();
        let event_types = if adapter.event_types.is_empty() {
            addon
                .event_types
                .iter()
                .map(|event_type| event_type.id.clone())
                .collect::<Vec<_>>()
        } else {
            adapter.event_types.clone()
        };
        if !adapter.transport.trim().is_empty() {
            extend_unique(&mut summary.transports, vec![adapter.transport.clone()]);
        }
        if direction == "ingress" {
            summary.ingress_adapter_count += 1;
            extend_unique(&mut summary.consumed_event_types, event_types);
        } else if direction == "egress" {
            summary.egress_adapter_count += 1;
            extend_unique(&mut summary.emitted_event_types, event_types);
        } else if direction == "bidirectional" || direction == "both" {
            summary.bidirectional_adapter_count += 1;
            extend_unique(&mut summary.consumed_event_types, event_types.clone());
            extend_unique(&mut summary.emitted_event_types, event_types);
        }
    }
    apply_addon_runtime_event_flow(&mut summary, &addon.id, runtime_events);
    summary
}

fn apply_addon_runtime_event_flow(
    summary: &mut AddonEventFlowSummary,
    addon_id: &str,
    events: &[StoredGlobalEventRecord],
) {
    for event in events {
        if addon_runtime_event_addon_id(event).as_deref() != Some(addon_id) {
            continue;
        }
        summary.runtime_event_count += 1;
        summary.latest_runtime_event_at = Some(event.created_at.clone());
        if let Some(event_type) = addon_runtime_event_type(event) {
            extend_unique(&mut summary.runtime_event_types, vec![event_type]);
        }
        if let Some(transport) = addon_runtime_event_transport(event) {
            extend_unique(&mut summary.runtime_transports, vec![transport]);
        }
        match addon_runtime_event_direction(event).as_deref() {
            Some("egress") | Some("outbound") | Some("emit") | Some("emitted") => {
                summary.runtime_emitted_event_count += 1;
            }
            Some("ingress") | Some("inbound") | Some("consume") | Some("consumed") => {
                summary.runtime_consumed_event_count += 1;
            }
            _ => {}
        }
    }
}

fn addon_runtime_event_addon_id(event: &StoredGlobalEventRecord) -> Option<String> {
    first_json_text(
        &event.data,
        &[
            &["addon_id"],
            &["addon"],
            &["request", "addon_id"],
            &["adapter_policy", "addon_id"],
            &["adapter_policy", "matched_adapter", "addon_id"],
        ],
    )
}

fn addon_runtime_event_direction(event: &StoredGlobalEventRecord) -> Option<String> {
    first_json_text(
        &event.data,
        &[
            &["direction"],
            &["event_direction"],
            &["request", "direction"],
            &["adapter_policy", "direction"],
            &["adapter_policy", "matched_adapter", "adapter", "direction"],
        ],
    )
    .map(|direction| direction.to_ascii_lowercase())
    .or_else(|| {
        let source = event.source.to_ascii_lowercase();
        let kind = event.kind.to_ascii_lowercase();
        if source.contains("egress") || kind.contains("egress") {
            Some("egress".to_string())
        } else if source.contains("ingress") || kind.contains("ingress") || kind.contains("inbound")
        {
            Some("ingress".to_string())
        } else {
            None
        }
    })
}

fn addon_runtime_event_type(event: &StoredGlobalEventRecord) -> Option<String> {
    first_json_text(
        &event.data,
        &[
            &["event_type"],
            &["type"],
            &["request", "event_type"],
            &["adapter_policy", "event_type"],
        ],
    )
}

fn addon_runtime_event_transport(event: &StoredGlobalEventRecord) -> Option<String> {
    first_json_text(
        &event.data,
        &[
            &["transport"],
            &["request", "transport"],
            &["adapter_policy", "transport"],
            &["adapter_policy", "matched_adapter", "adapter", "transport"],
        ],
    )
}

fn first_json_text(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| json_text_at_path(value, path))
}

fn json_text_at_path(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            current
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(ToString::to_string)
        })
}

fn addon_dispatch_observability(
    dispatches: &[StoredRuntimeContractDispatchRecord],
) -> AddonDispatchObservability {
    let mut summary = AddonDispatchObservability {
        dispatch_count: dispatches.len(),
        ..AddonDispatchObservability::default()
    };
    for dispatch in dispatches {
        match dispatch.status.as_str() {
            "queued" => summary.queued_count += 1,
            "claimed_external_worker" => summary.claimed_count += 1,
            "completed" => summary.completed_count += 1,
            "failed" => summary.failed_count += 1,
            "dry_run" => summary.dry_run_count += 1,
            "needs_external_worker" => summary.needs_external_worker_count += 1,
            status if status.contains("blocked") || status == "blocked" => {
                summary.blocked_count += 1;
            }
            _ => {}
        }
        if summary
            .latest_dispatch_at
            .as_deref()
            .map(|latest| dispatch.updated_at.as_str() > latest)
            .unwrap_or(true)
        {
            summary.latest_dispatch_at = Some(dispatch.updated_at.clone());
        }
    }
    summary
}

fn addon_permission_gate(
    addon: &AddonManifest,
    required_permissions: &[String],
) -> AddonPermissionGate {
    let declared_map = addon
        .permissions
        .iter()
        .map(|permission| (permission.id.clone(), permission))
        .collect::<BTreeMap<_, _>>();
    let required = required_permissions
        .iter()
        .filter(|permission| !permission.trim().is_empty())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut undeclared = Vec::new();
    let mut human_approval_required = Vec::new();
    let mut high_risk = Vec::new();
    let mut tools = Vec::new();
    let mut resources = Vec::new();
    let mut integrations = Vec::new();
    let mut actions = Vec::new();
    let mut tenant_scopes = Vec::new();

    for permission_id in &required {
        match declared_map.get(permission_id) {
            Some(permission) => {
                if permission.requires_human_approval {
                    human_approval_required.push(permission_id.clone());
                }
                if permission.risk == "high" {
                    high_risk.push(permission_id.clone());
                }
                extend_unique(&mut tools, permission.tools.clone());
                extend_unique(&mut resources, permission.resources.clone());
                extend_unique(&mut integrations, permission.integrations.clone());
                extend_unique(&mut actions, permission.actions.clone());
                extend_unique(&mut tenant_scopes, permission.tenant_scopes.clone());
            }
            None => undeclared.push(permission_id.clone()),
        }
    }

    let status = if !undeclared.is_empty() {
        "undeclared_permission".to_string()
    } else if addon.lifecycle == "unauthorized" && !human_approval_required.is_empty() {
        "missing_human_approval".to_string()
    } else if addon_enabled(addon) {
        "allowed".to_string()
    } else {
        "addon_not_enabled".to_string()
    };

    AddonPermissionGate {
        schema_version: addon_permission_gate_schema_version(),
        allowed: status == "allowed",
        status,
        required_permissions: required.into_iter().collect(),
        declared_permissions: declared_map.keys().cloned().collect(),
        undeclared_permissions: undeclared,
        human_approval_required,
        high_risk_permissions: high_risk,
        tools,
        resources,
        integrations,
        actions,
        tenant_scopes,
    }
}

fn validate_addon_permission_references(
    addon: &AddonManifest,
    issues: &mut Vec<AddonValidationIssue>,
) {
    let declared = addon
        .permissions
        .iter()
        .map(|permission| permission.id.as_str())
        .collect::<BTreeSet<_>>();

    for contract in &addon.runtime_contracts {
        validate_permission_reference_list(
            addon,
            "runtime_contract",
            &contract.id,
            &contract.permissions,
            &declared,
            issues,
        );
    }
    for adapter in &addon.event_adapters {
        validate_permission_reference_list(
            addon,
            "event_adapter",
            &adapter.id,
            &adapter.permissions,
            &declared,
            issues,
        );
    }
    for view in &addon.views {
        validate_permission_reference_list(
            addon,
            "view",
            &view.id,
            &view.permissions,
            &declared,
            issues,
        );
    }
}

fn validate_permission_reference_list(
    addon: &AddonManifest,
    owner_kind: &str,
    owner_id: &str,
    permissions: &[String],
    declared: &BTreeSet<&str>,
    issues: &mut Vec<AddonValidationIssue>,
) {
    for permission in permissions {
        if permission.trim().is_empty() || declared.contains(permission.as_str()) {
            continue;
        }
        issues.push(validation_issue(
            "error",
            "undeclared_permission_reference",
            &format!("{}:{}:{}", addon.id, owner_kind, owner_id),
            &format!(
                "{} references undeclared permission `{permission}`",
                owner_kind
            ),
        ));
    }
}

fn ensure_addon_permissions_authorized(store: &ForgeStore, manifest: &AddonManifest) -> Result<()> {
    let missing = missing_required_permission_authorizations(store, manifest)?;
    if missing.is_empty() {
        return Ok(());
    }
    bail!(
        "addon permission authorization required for {}: {}",
        manifest.id,
        missing.join(", ")
    );
}

fn authorized_lifecycle_for_manifest(
    store: &ForgeStore,
    manifest: &AddonManifest,
    lifecycle: &str,
) -> Result<String> {
    if lifecycle != "enabled" {
        return Ok(lifecycle.to_string());
    }
    if missing_required_permission_authorizations(store, manifest)?.is_empty() {
        Ok("enabled".to_string())
    } else {
        Ok("unauthorized".to_string())
    }
}

fn missing_required_permission_authorizations(
    store: &ForgeStore,
    manifest: &AddonManifest,
) -> Result<Vec<String>> {
    if manifest.permissions.is_empty() {
        return Ok(Vec::new());
    }
    let approved = store
        .list_addon_permission_authorizations(Some(&manifest.id), None, Some("approved"))?
        .into_iter()
        .map(|authorization| authorization.permission_id)
        .collect::<BTreeSet<_>>();
    Ok(manifest
        .permissions
        .iter()
        .filter(|permission| permission.requires_human_approval)
        .filter(|permission| !approved.contains(&permission.id))
        .map(|permission| format!("{}:{}", manifest.id, permission.id))
        .collect())
}

fn manifest_permissions_authorized(
    manifest: &AddonManifest,
    approved_permissions: &BTreeSet<(String, String)>,
) -> bool {
    manifest
        .permissions
        .iter()
        .filter(|permission| permission.requires_human_approval)
        .all(|permission| {
            approved_permissions.contains(&(manifest.id.clone(), permission.id.clone()))
        })
}

fn string_vec_from_value(value: serde_json::Value) -> Result<Vec<String>> {
    Ok(serde_json::from_value(value)?)
}

fn normalize_filter(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn filter_matches(filter: Option<&str>, value: &str) -> bool {
    filter
        .map(|filter| value.eq_ignore_ascii_case(filter))
        .unwrap_or(true)
}

fn upsert_addon(addons: &mut Vec<AddonManifest>, manifest: AddonManifest) {
    if let Some(existing) = addons.iter_mut().find(|addon| addon.id == manifest.id) {
        *existing = manifest;
    } else {
        addons.push(manifest);
    }
}

fn finalize_catalog(addons: Vec<AddonManifest>, addon_dirs: Vec<String>) -> AddonCatalog {
    let capability_count = addons
        .iter()
        .map(|addon| addon.capabilities.len())
        .sum::<usize>();
    AddonCatalog {
        schema_version: addon_catalog_schema_version(),
        status: "loaded".to_string(),
        addon_dirs,
        addon_count: addons.len(),
        capability_count,
        addons,
    }
}

fn push_need(
    required: &mut Vec<CapabilityNeed>,
    seen: &mut BTreeSet<String>,
    addon: &AddonManifest,
    capability: &CapabilityDeclaration,
    reason: String,
    matched_keywords: Vec<String>,
) {
    if !seen.insert(capability.id.clone()) {
        return;
    }
    required.push(CapabilityNeed {
        id: capability.id.clone(),
        title: capability.title.clone(),
        source_addon: addon.id.clone(),
        source_addon_version: addon.version.clone(),
        reason,
        required: true,
        domains: capability.domains.clone(),
        matched_keywords,
        workflow_extensions: capability.workflow_extensions.clone(),
    });
}

fn build_intent_overlay(
    catalog: &AddonCatalog,
    required: &[CapabilityNeed],
) -> CapabilityIntentOverlay {
    let mut deliverables = Vec::new();
    let mut constraints = Vec::new();
    let mut risks = Vec::new();
    let mut unknowns = Vec::new();

    for need in required {
        if let Some((_addon, capability)) = find_capability(catalog, &need.id) {
            extend_unique(&mut deliverables, capability.deliverables.clone());
            extend_unique(&mut constraints, capability.constraints.clone());
            extend_unique(&mut risks, capability.risks.clone());
            extend_unique(&mut unknowns, capability.unknowns.clone());
        }
    }

    CapabilityIntentOverlay {
        deliverables,
        constraints,
        risks,
        unknowns,
    }
}

fn build_capability_suggestions(
    catalog: &AddonCatalog,
    missing: &[MissingCapability],
) -> Vec<CapabilitySuggestion> {
    let mut seen = BTreeSet::new();
    let mut suggestions = Vec::new();
    for missing_capability in missing {
        for addon in &catalog.addons {
            let Some(capability) = addon
                .capabilities
                .iter()
                .find(|capability| capability.id == missing_capability.id)
            else {
                continue;
            };
            let key = format!(
                "{}:{}:{}",
                missing_capability.id, missing_capability.required_by, addon.id
            );
            if !seen.insert(key) {
                continue;
            }
            let (action, status, reason, commands, mcp_tools, permission_ids) =
                capability_suggestion_action(addon, capability, missing_capability);
            suggestions.push(CapabilitySuggestion {
                capability_id: missing_capability.id.clone(),
                required_by: missing_capability.required_by.clone(),
                action,
                status,
                addon_id: addon.id.clone(),
                addon_name: addon.name.clone(),
                addon_version: addon.version.clone(),
                addon_lifecycle: addon.lifecycle.clone(),
                reason,
                commands,
                mcp_tools,
                permission_ids,
                package_id: None,
                package_source: None,
                package_status: None,
                package_sha256: None,
                repository: None,
                channel: None,
            });
        }
    }
    suggestions.sort_by(|left, right| {
        suggestion_priority(&left.status)
            .cmp(&suggestion_priority(&right.status))
            .then_with(|| left.capability_id.cmp(&right.capability_id))
            .then_with(|| left.addon_id.cmp(&right.addon_id))
            .then_with(|| left.package_id.cmp(&right.package_id))
    });
    suggestions
}

fn append_marketplace_capability_suggestions(
    store: &ForgeStore,
    report: &mut CapabilityResolutionReport,
) -> Result<()> {
    if report.missing_capabilities.is_empty() {
        return Ok(());
    }

    let mut seen = report
        .capability_suggestions
        .iter()
        .map(|suggestion| {
            format!(
                "{}:{}:{}:{}",
                suggestion.capability_id,
                suggestion.required_by,
                suggestion.addon_id,
                suggestion.package_id.as_deref().unwrap_or("")
            )
        })
        .collect::<BTreeSet<_>>();
    let records =
        store.list_addon_marketplace_packages(None, None, None, Some("installable"), 200)?;
    for record in records {
        let package: AddonPackageReport = serde_json::from_value(record.package.clone())?;
        let entry = addon_marketplace_entry_from_record(store, record.clone())?;
        if !entry.policy.install_allowed {
            continue;
        }
        for missing in &report.missing_capabilities {
            if !entry
                .summary
                .capabilities
                .iter()
                .any(|capability| capability == &missing.id)
            {
                continue;
            }
            let key = format!(
                "{}:{}:{}:{}",
                missing.id, missing.required_by, entry.addon_id, entry.package_id
            );
            if !seen.insert(key) {
                continue;
            }
            report
                .capability_suggestions
                .push(marketplace_capability_suggestion(
                    missing,
                    &entry,
                    package.written_package_path.as_deref(),
                ));
        }
    }
    report.capability_suggestions.sort_by(|left, right| {
        suggestion_priority(&left.status)
            .cmp(&suggestion_priority(&right.status))
            .then_with(|| left.capability_id.cmp(&right.capability_id))
            .then_with(|| left.addon_id.cmp(&right.addon_id))
            .then_with(|| left.package_id.cmp(&right.package_id))
    });
    Ok(())
}

fn marketplace_capability_suggestion(
    missing: &MissingCapability,
    entry: &AddonMarketplacePackageEntry,
    written_package_path: Option<&str>,
) -> CapabilitySuggestion {
    let install_package = marketplace_install_package_path(entry, written_package_path);
    let fetch_package = if install_package.is_none() {
        marketplace_fetch_package_source(entry)
    } else {
        None
    };
    let mut commands = Vec::new();
    if let Some(package_path) = &install_package {
        commands.push(vec![
            "forge".to_string(),
            "addons".to_string(),
            "install-package".to_string(),
            "--package".to_string(),
            package_path.clone(),
            "--output".to_string(),
            "json".to_string(),
        ]);
    }
    if let Some(package_source) = &fetch_package {
        let mut command = vec![
            "forge".to_string(),
            "addons".to_string(),
            "fetch-package".to_string(),
            "--source".to_string(),
            package_source.clone(),
        ];
        if package_source.starts_with("http://") || package_source.starts_with("https://") {
            command.push("--allow-remote".to_string());
        }
        if !entry.package_sha256.trim().is_empty() {
            command.push("--expected-sha256".to_string());
            command.push(entry.package_sha256.clone());
        }
        command.push("--output".to_string());
        command.push("json".to_string());
        commands.push(command);
    }
    commands.push(vec![
        "forge".to_string(),
        "addons".to_string(),
        "marketplace".to_string(),
        "--repository".to_string(),
        entry.repository.clone(),
        "--channel".to_string(),
        entry.channel.clone(),
        "--addon".to_string(),
        entry.addon_id.clone(),
        "--status".to_string(),
        "installable".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]);

    CapabilitySuggestion {
        capability_id: missing.id.clone(),
        required_by: missing.required_by.clone(),
        action: if install_package.is_some() {
            "install_package".to_string()
        } else if fetch_package.is_some() {
            "fetch_package".to_string()
        } else {
            "inspect_marketplace_package".to_string()
        },
        status: "available_in_marketplace_package".to_string(),
        addon_id: entry.addon_id.clone(),
        addon_name: entry.addon_name.clone(),
        addon_version: entry.addon_version.clone(),
        addon_lifecycle: "marketplace".to_string(),
        reason: format!(
            "capability `{}` is available in trusted marketplace package `{}` from `{}`/`{}` and is required by `{}`",
            missing.id, entry.package_id, entry.repository, entry.channel, missing.required_by
        ),
        commands,
        mcp_tools: marketplace_suggestion_mcp_tools(install_package.is_some(), fetch_package.is_some()),
        permission_ids: Vec::new(),
        package_id: Some(entry.package_id.clone()),
        package_source: Some(entry.source.clone()),
        package_status: Some(entry.status.clone()),
        package_sha256: Some(entry.package_sha256.clone()),
        repository: Some(entry.repository.clone()),
        channel: Some(entry.channel.clone()),
    }
}

fn marketplace_install_package_path(
    entry: &AddonMarketplacePackageEntry,
    written_package_path: Option<&str>,
) -> Option<String> {
    written_package_path
        .and_then(local_package_source_argument)
        .or_else(|| local_package_source_argument(&entry.source))
}

fn marketplace_fetch_package_source(entry: &AddonMarketplacePackageEntry) -> Option<String> {
    let source = entry.source.trim();
    if source.starts_with("http://") || source.starts_with("https://") {
        return Some(source.to_string());
    }
    None
}

fn marketplace_suggestion_mcp_tools(can_install: bool, can_fetch: bool) -> Vec<String> {
    let mut tools = vec!["forge.addons.marketplace".to_string()];
    if can_fetch {
        tools.push("forge.addons.fetch_package".to_string());
    }
    if can_install {
        tools.push("forge.addons.install_package".to_string());
    }
    tools
}

fn marketplace_package_source(package_path: &Path, source: &str) -> String {
    let source = source.trim();
    if source.is_empty() || source == "cli" || source == "mcp" {
        package_path.display().to_string()
    } else {
        source.to_string()
    }
}

fn read_addon_package_source(
    source: &str,
    allow_remote: bool,
    max_bytes: u64,
) -> Result<(String, Vec<u8>)> {
    if source.starts_with("http://") || source.starts_with("https://") {
        if !allow_remote {
            bail!("remote addon package fetch requires --allow-remote");
        }
        return Ok((
            if source.starts_with("https://") {
                "https".to_string()
            } else {
                "http".to_string()
            },
            read_http_addon_package_source(source, max_bytes)?,
        ));
    }
    if let Some(path) = source.strip_prefix("file://") {
        return Ok((
            "file_uri".to_string(),
            read_limited_addon_package_file(Path::new(path), max_bytes)?,
        ));
    }
    Ok((
        "local_path".to_string(),
        read_limited_addon_package_file(Path::new(source), max_bytes)?,
    ))
}

fn parse_addon_registry_index(bytes: &[u8]) -> Result<AddonRegistryIndex> {
    serde_json::from_slice(bytes)
        .or_else(|json_error| {
            serde_yaml::from_slice(bytes).with_context(|| {
                format!("failed to parse registry index as JSON or YAML: {json_error}")
            })
        })
        .and_then(|index: AddonRegistryIndex| {
            if index.packages.is_empty() {
                bail!("registry index must include at least one package source");
            }
            for package in &index.packages {
                if package.source.trim().is_empty() {
                    bail!("registry index package source cannot be empty");
                }
            }
            Ok(index)
        })
}

fn read_limited_addon_package_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to stat addon package source {}", path.display()))?;
    if metadata.len() > max_bytes {
        bail!(
            "addon package source exceeded max bytes: {} > {}",
            metadata.len(),
            max_bytes
        );
    }
    fs::read(path)
        .with_context(|| format!("failed to read addon package source {}", path.display()))
}

fn read_http_addon_package_source(source: &str, max_bytes: u64) -> Result<Vec<u8>> {
    let max_filesize = max_bytes.to_string();
    let output = Command::new("curl")
        .args([
            "--proto",
            "=http,https",
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--max-time",
            "30",
            "--max-filesize",
            &max_filesize,
            source,
        ])
        .output()
        .with_context(|| "failed to start curl for remote addon package fetch")?;
    if !output.status.success() {
        bail!(
            "remote addon package fetch failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let bytes = output.stdout;
    if bytes.len() as u64 > max_bytes {
        bail!(
            "addon package source exceeded max bytes: {} > {}",
            bytes.len(),
            max_bytes
        );
    }
    Ok(bytes)
}

fn capability_suggestion_action(
    addon: &AddonManifest,
    capability: &CapabilityDeclaration,
    missing: &MissingCapability,
) -> CapabilitySuggestionAction {
    let permission_ids = addon
        .permissions
        .iter()
        .filter(|permission| permission.requires_human_approval)
        .map(|permission| permission.id.clone())
        .collect::<Vec<_>>();
    if addon.lifecycle == "disabled" {
        return (
            "enable_addon".to_string(),
            "available_disabled_addon".to_string(),
            format!(
                "capability `{}` is declared by disabled Addon `{}` and is required by `{}`",
                capability.id, addon.id, missing.required_by
            ),
            vec![vec![
                "forge".to_string(),
                "addons".to_string(),
                "enable".to_string(),
                addon.id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ]],
            vec!["forge.addons.enable".to_string()],
            permission_ids,
        );
    }
    if addon.lifecycle == "unauthorized" {
        let mut commands = Vec::new();
        for permission_id in &permission_ids {
            commands.push(vec![
                "forge".to_string(),
                "addons".to_string(),
                "authorize-permission".to_string(),
                "--addon".to_string(),
                addon.id.clone(),
                "--permission".to_string(),
                permission_id.clone(),
                "--approved-by".to_string(),
                "<operator>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]);
        }
        return (
            "authorize_permission".to_string(),
            "available_requires_authorization".to_string(),
            format!(
                "capability `{}` is declared by Addon `{}` but the Addon is unauthorized; approve required permissions before planning can use it",
                capability.id, addon.id
            ),
            commands,
            vec!["forge.addons.authorize_permission".to_string()],
            permission_ids,
        );
    }
    if addon_enabled(addon) {
        return (
            "already_available".to_string(),
            "available_enabled_addon".to_string(),
            format!(
                "capability `{}` is declared by enabled Addon `{}` but was still reported missing; inspect dependency resolution",
                capability.id, addon.id
            ),
            Vec::new(),
            vec!["forge.addons.resolve".to_string()],
            permission_ids,
        );
    }
    (
        "install_or_enable_addon".to_string(),
        "available_in_inactive_addon".to_string(),
        format!(
            "capability `{}` is declared by inactive Addon `{}` with lifecycle `{}`",
            capability.id, addon.id, addon.lifecycle
        ),
        vec![vec![
            "forge".to_string(),
            "addons".to_string(),
            "install".to_string(),
            "--manifest".to_string(),
            addon_manifest_source_argument(addon),
            "--output".to_string(),
            "json".to_string(),
        ]],
        vec!["forge.addons.install".to_string()],
        permission_ids,
    )
}

fn addon_manifest_source_argument(addon: &AddonManifest) -> String {
    addon
        .source
        .strip_prefix("file:")
        .unwrap_or(addon.source.as_str())
        .to_string()
}

fn local_package_source_argument(source: &str) -> Option<String> {
    let source = source.trim();
    if source.is_empty()
        || source == "cli"
        || source.starts_with("marketplace:")
        || source.starts_with("registry://")
        || source.starts_with("http://")
        || source.starts_with("https://")
    {
        return None;
    }
    let source = source
        .strip_prefix("file://")
        .or_else(|| source.strip_prefix("file:"))
        .unwrap_or(source);
    if source.ends_with(".json") || source.contains('/') || source.contains('\\') {
        return Some(source.to_string());
    }
    None
}

fn suggestion_priority(status: &str) -> u8 {
    match status {
        "available_requires_authorization" => 0,
        "available_disabled_addon" => 1,
        "available_in_marketplace_package" => 2,
        "available_in_inactive_addon" => 3,
        "available_enabled_addon" => 4,
        _ => 20,
    }
}

fn build_workflow_extension_activations(
    catalog: &AddonCatalog,
    required: &[CapabilityNeed],
) -> Vec<WorkflowExtensionActivation> {
    let mut seen = BTreeSet::new();
    let mut activations = Vec::new();
    for need in required {
        for extension_id in &need.workflow_extensions {
            let key = format!("{}:{}", need.id, extension_id);
            if !seen.insert(key) {
                continue;
            }
            let (kind, description) =
                match find_workflow_extension(catalog, &need.source_addon, extension_id) {
                    Some(extension) => (extension.kind.clone(), extension.description.clone()),
                    None => ("capability_declared".to_string(), String::new()),
                };
            activations.push(WorkflowExtensionActivation {
                id: extension_id.clone(),
                kind,
                description,
                source_addon: need.source_addon.clone(),
                source_addon_version: need.source_addon_version.clone(),
                source_capability: need.id.clone(),
                reason: need.reason.clone(),
            });
        }
    }
    activations
}

fn build_runtime_contract_activations(
    catalog: &AddonCatalog,
    required: &[CapabilityNeed],
) -> Vec<RuntimeContractActivation> {
    let mut seen = BTreeSet::new();
    let mut activations = Vec::new();

    for need in required {
        let Some(addon) = catalog
            .addons
            .iter()
            .find(|addon| addon.id == need.source_addon)
        else {
            continue;
        };
        for contract in &addon.runtime_contracts {
            let capability_matches = contract.capability_id == need.id;
            let extension_matches = !contract.workflow_extension_id.is_empty()
                && need
                    .workflow_extensions
                    .iter()
                    .any(|extension| extension == &contract.workflow_extension_id);
            if !capability_matches && !extension_matches {
                continue;
            }
            let key = format!("{}:{}", addon.id, contract.id);
            if !seen.insert(key) {
                continue;
            }
            activations.push(RuntimeContractActivation {
                id: contract.id.clone(),
                title: contract.title.clone(),
                contract_type: contract.contract_type.clone(),
                runtime: contract.runtime.clone(),
                entrypoint: contract.entrypoint.clone(),
                source_addon: addon.id.clone(),
                source_addon_version: addon.version.clone(),
                source_capability: need.id.clone(),
                workflow_extension_id: contract.workflow_extension_id.clone(),
                inputs: contract.inputs.clone(),
                outputs: contract.outputs.clone(),
                permissions: contract.permissions.clone(),
                constraints: contract.constraints.clone(),
                permission_gate: addon_permission_gate(addon, &contract.permissions),
                reason: if extension_matches {
                    "runtime contract matches activated workflow extension".to_string()
                } else {
                    "runtime contract matches required capability".to_string()
                },
            });
        }
    }

    activations.sort_by(|left, right| {
        left.source_addon
            .cmp(&right.source_addon)
            .then_with(|| left.source_capability.cmp(&right.source_capability))
            .then_with(|| left.id.cmp(&right.id))
    });
    activations
}

fn find_capability<'a>(
    catalog: &'a AddonCatalog,
    capability_id: &str,
) -> Option<(&'a AddonManifest, &'a CapabilityDeclaration)> {
    catalog.addons.iter().find_map(|addon| {
        addon
            .capabilities
            .iter()
            .find(|capability| capability.id == capability_id)
            .map(|capability| (addon, capability))
    })
}

fn find_workflow_extension<'a>(
    catalog: &'a AddonCatalog,
    addon_id: &str,
    extension_id: &str,
) -> Option<&'a WorkflowExtensionDeclaration> {
    catalog
        .addons
        .iter()
        .find(|addon| addon.id == addon_id)
        .and_then(|addon| {
            addon
                .workflows
                .iter()
                .find(|extension| extension.id == extension_id)
        })
}

fn extend_unique(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target.iter().any(|existing| existing == &value) {
            target.push(value);
        }
    }
}

fn validate_addon_compatibility(addon: &AddonManifest, issues: &mut Vec<AddonValidationIssue>) {
    let compatibility = &addon.compatibility;
    if !compatibility.forge_version_req.trim().is_empty()
        && !addon_version_req_satisfied(&compatibility.forge_version_req, env!("CARGO_PKG_VERSION"))
    {
        issues.push(validation_issue(
            "error",
            "unsupported_forge_version_requirement",
            &addon.id,
            &format!(
                "addon requires Forge version `{}` but current version is {}",
                compatibility.forge_version_req,
                env!("CARGO_PKG_VERSION")
            ),
        ));
    }

    let supported_apis = supported_addon_api_versions();
    for api_version in &compatibility.api_versions {
        if !supported_apis.contains(api_version.as_str()) {
            issues.push(validation_issue(
                "error",
                "unsupported_addon_api_version",
                &format!("{}:{api_version}", addon.id),
                "addon declares an API version this Forge build does not support",
            ));
        }
    }

    let supported_features = supported_addon_features();
    for feature in &compatibility.features {
        if !supported_features.contains(feature.as_str()) {
            issues.push(validation_issue(
                "error",
                "unsupported_addon_feature",
                &format!("{}:{feature}", addon.id),
                "addon declares a required feature this Forge build does not support",
            ));
        }
    }

    let supported_runtimes = supported_addon_runtimes();
    let declared_runtimes = compatibility
        .runtimes
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for runtime in &compatibility.runtimes {
        if !supported_runtimes.contains(runtime.as_str()) {
            issues.push(validation_issue(
                "error",
                "unsupported_addon_runtime",
                &format!("{}:{runtime}", addon.id),
                "addon declares a runtime this Forge build cannot route",
            ));
        }
    }
    for contract in &addon.runtime_contracts {
        if !contract.runtime.trim().is_empty()
            && !supported_runtimes.contains(contract.runtime.as_str())
        {
            issues.push(validation_issue(
                "error",
                "unsupported_runtime_contract_runtime",
                &format!("{}:{}", addon.id, contract.id),
                &format!(
                    "runtime contract declares unsupported runtime `{}`",
                    contract.runtime
                ),
            ));
        }
        if !declared_runtimes.is_empty()
            && !contract.runtime.trim().is_empty()
            && !declared_runtimes.contains(contract.runtime.as_str())
        {
            issues.push(validation_issue(
                "error",
                "runtime_contract_outside_declared_compatibility",
                &format!("{}:{}", addon.id, contract.id),
                &format!(
                    "runtime contract uses `{}` but compatibility.runtimes does not declare it",
                    contract.runtime
                ),
            ));
        }
    }

    if !compatibility.platforms.is_empty() {
        let current_platforms = current_platform_tags();
        let matches_current = compatibility
            .platforms
            .iter()
            .any(|platform| current_platforms.contains(platform.as_str()));
        if !matches_current {
            issues.push(validation_issue(
                "error",
                "unsupported_addon_platform",
                &addon.id,
                &format!(
                    "addon declares platforms {:?}, current platform tags are {:?}",
                    compatibility.platforms, current_platforms
                ),
            ));
        }
    }

    for migration in &compatibility.migrations {
        if migration.from_version.trim().is_empty()
            || migration.to_version.trim().is_empty()
            || migration.strategy.trim().is_empty()
        {
            issues.push(validation_issue(
                "error",
                "invalid_addon_migration_plan",
                &addon.id,
                "migration plans must declare from_version, to_version and strategy",
            ));
        }
        if migration.requires_backup && migration.rollback.trim().is_empty() {
            issues.push(validation_issue(
                "error",
                "missing_addon_migration_rollback",
                &addon.id,
                "migration plans that require backup must declare rollback evidence",
            ));
        }
    }
}

fn supported_addon_api_versions() -> BTreeSet<&'static str> {
    [
        "forge.addon_manifest.v1",
        ADDON_CATALOG_SCHEMA_VERSION,
        ADDON_PACKAGE_SCHEMA_VERSION,
        ADDON_RUNTIME_CONTRACTS_SCHEMA_VERSION,
        ADDON_VIEWS_SCHEMA_VERSION,
        ADDON_EVENT_ADAPTERS_SCHEMA_VERSION,
    ]
    .into_iter()
    .collect()
}

fn supported_addon_features() -> BTreeSet<&'static str> {
    [
        "addon_catalog",
        "addon_lifecycle",
        "addon_marketplace",
        "addon_permissions",
        "addon_runtime_contracts",
        "addon_runtime_workers",
        "addon_views",
        "event_adapters",
        "memory_providers",
        "context_providers",
    ]
    .into_iter()
    .collect()
}

fn supported_addon_runtimes() -> BTreeSet<&'static str> {
    ["forge_core_builtin", "wasm", "external_api"]
        .into_iter()
        .collect()
}

fn current_platform_tags() -> BTreeSet<String> {
    [
        "any".to_string(),
        std::env::consts::OS.to_string(),
        std::env::consts::ARCH.to_string(),
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    ]
    .into_iter()
    .collect()
}

fn ensure_version_change_migration_plan(
    previous: &AddonManifest,
    candidate: &AddonManifest,
) -> Result<()> {
    if let Some(migration) = required_addon_migration_plan(previous, candidate)? {
        if migration.strategy.trim().is_empty() || migration.rollback.trim().is_empty() {
            bail!(
                "addon {} migration {} -> {} requires strategy and rollback evidence",
                candidate.id,
                previous.version,
                candidate.version
            );
        }
    }
    Ok(())
}

fn required_addon_migration_plan<'a>(
    previous: &AddonManifest,
    candidate: &'a AddonManifest,
) -> Result<Option<&'a AddonMigrationPlan>> {
    if previous.id != candidate.id {
        return Ok(None);
    }
    let previous_major = parse_addon_version(&previous.version).map(|version| version.0);
    let candidate_major = parse_addon_version(&candidate.version).map(|version| version.0);
    if previous_major.is_none()
        || candidate_major.is_none()
        || previous_major == candidate_major
        || previous.version == candidate.version
    {
        return Ok(None);
    }
    let migration = candidate.compatibility.migrations.iter().find(|migration| {
        migration.from_version == previous.version && migration.to_version == candidate.version
    });
    let Some(migration) = migration else {
        bail!(
            "addon {} major version change {} -> {} requires compatibility.migrations entry",
            candidate.id,
            previous.version,
            candidate.version
        );
    };
    Ok(Some(migration))
}

fn ensure_candidate_migration_against_installed(
    store: &ForgeStore,
    candidate: &AddonManifest,
) -> Result<()> {
    match store.load_installed_addon(&candidate.id) {
        Ok(record) => {
            let previous = installed_manifest_from_record(&record)?;
            ensure_version_change_migration_plan(&previous, candidate)
        }
        Err(error) if error.to_string().contains("installed addon not found") => Ok(()),
        Err(error) => Err(error),
    }
}

fn create_candidate_migration_workflow_if_needed(
    store: &ForgeStore,
    candidate: &AddonManifest,
    action: &str,
    origin: &str,
) -> Result<Option<AddonMigrationWorkflowReport>> {
    match store.load_installed_addon(&candidate.id) {
        Ok(record) => {
            let previous = installed_manifest_from_record(&record)?;
            create_version_change_migration_workflow_if_needed(
                store, &previous, candidate, action, origin,
            )
        }
        Err(error) if error.to_string().contains("installed addon not found") => Ok(None),
        Err(error) => Err(error),
    }
}

fn create_version_change_migration_workflow_if_needed(
    store: &ForgeStore,
    previous: &AddonManifest,
    candidate: &AddonManifest,
    action: &str,
    origin: &str,
) -> Result<Option<AddonMigrationWorkflowReport>> {
    if required_addon_migration_plan(previous, candidate)?.is_some() {
        Ok(Some(create_and_save_addon_migration_workflow(
            store, previous, candidate, action, origin,
        )?))
    } else {
        Ok(None)
    }
}

fn create_and_save_addon_migration_workflow(
    store: &ForgeStore,
    previous: &AddonManifest,
    candidate: &AddonManifest,
    action: &str,
    origin: &str,
) -> Result<AddonMigrationWorkflowReport> {
    let migration = required_addon_migration_plan(previous, candidate)?
        .with_context(|| "addon migration workflow requires a matching migration plan")?;
    let mut workflow = build_addon_migration_workflow(previous, candidate, migration, action);
    workflow.revisions.push(WorkflowRevision {
        revision: 1,
        origin: origin.trim().to_string(),
        change_type: "addon_migration_workflow_created".to_string(),
        summary: format!(
            "Created Addon migration workflow for {} {} -> {}",
            candidate.id, previous.version, candidate.version
        ),
        created_at: Utc::now(),
    });
    store.save_workflow(&workflow)?;
    store.record_event(
        &workflow.id,
        "addon_migration_workflow_created",
        &serde_json::json!({
            "schema_version": ADDON_MIGRATION_WORKFLOW_SCHEMA_VERSION,
            "action": action,
            "addon_id": candidate.id,
            "from_version": previous.version,
            "to_version": candidate.version,
            "strategy": migration.strategy,
            "rollback": migration.rollback,
            "requires_backup": migration.requires_backup,
        }),
    )?;
    Ok(addon_migration_workflow_report(
        &workflow, previous, candidate, migration, action,
    ))
}

fn build_addon_migration_workflow(
    previous: &AddonManifest,
    candidate: &AddonManifest,
    migration: &AddonMigrationPlan,
    action: &str,
) -> Workflow {
    let goal = format!(
        "Run audited Addon migration for {} from {} to {} using strategy {}",
        candidate.id, previous.version, candidate.version, migration.strategy
    );
    let mut workflow = create_workflow(parse_intent(&goal));
    workflow.status = "pending".to_string();
    workflow.runtime.lifecycle_kind = "persistent_workflow".to_string();
    workflow.runtime.expected_lifetime = "until_migration_audit_complete".to_string();
    workflow.runtime.persistent = true;
    workflow.runtime.ephemeral = false;
    workflow.runtime.can_become_persistent = false;
    workflow.runtime.scale_to_zero_policy = "scale_to_zero_after_migration_audit".to_string();
    workflow.tasks = vec![
        addon_migration_task(AddonMigrationTaskInput {
            id: "task-001",
            title: "Snapshot installed Addon state",
            dependencies: &[],
            context_requirements: &[
                "installed Addon manifest",
                "capability index",
                "permission authorizations",
                "marketplace package policy",
            ],
            validation_rules: vec![migration_validation_rule(
                "addon_migration_backup",
                "previous manifest, source and capability index snapshot are recorded before migration",
            )],
            expected_output: "pre-migration backup snapshot",
            executor: ExecutorKind::Command,
            human_required: true,
        }),
        addon_migration_task(AddonMigrationTaskInput {
            id: "task-002",
            title: "Apply declared Addon migration plan",
            dependencies: &["task-001"],
            context_requirements: &[
                "compatibility.migrations entry",
                "data migration plan",
                "tenant policy",
                "human approval evidence",
            ],
            validation_rules: vec![migration_validation_rule(
                "addon_migration_apply",
                &format!(
                    "strategy `{}` and data migration `{}` are applied with execution evidence",
                    migration.strategy, migration.data_migration
                ),
            )],
            expected_output: "migration execution trace",
            executor: ExecutorKind::Mixed,
            human_required: true,
        }),
        addon_migration_task(AddonMigrationTaskInput {
            id: "task-003",
            title: "Validate migrated Addon state",
            dependencies: &["task-002"],
            context_requirements: &[
                "candidate Addon manifest",
                "catalog validation",
                "capability index rebuild evidence",
            ],
            validation_rules: vec![migration_validation_rule(
                "addon_migration_validation",
                "candidate catalog validates and materialized capabilities match the target version",
            )],
            expected_output: "post-migration validation evidence",
            executor: ExecutorKind::Command,
            human_required: false,
        }),
        addon_migration_task(AddonMigrationTaskInput {
            id: "task-004",
            title: "Prepare Addon rollback path",
            dependencies: &["task-001"],
            context_requirements: &[
                "rollback instructions",
                "backup snapshot",
                "previous Addon manifest",
            ],
            validation_rules: vec![migration_validation_rule(
                "addon_migration_rollback_ready",
                &format!("rollback path `{}` is executable from the backup snapshot", migration.rollback),
            )],
            expected_output: "rollback readiness evidence",
            executor: ExecutorKind::Command,
            human_required: true,
        }),
        addon_migration_task(AddonMigrationTaskInput {
            id: "task-005",
            title: "Package Addon migration audit",
            dependencies: &["task-003", "task-004"],
            context_requirements: &[
                "migration execution trace",
                "validation evidence",
                "rollback readiness evidence",
            ],
            validation_rules: vec![migration_validation_rule(
                "addon_migration_audit_package",
                "migration audit package is ready for operator review",
            )],
            expected_output: "auditable Addon migration package",
            executor: ExecutorKind::Notification,
            human_required: false,
        }),
    ];
    for task in &mut workflow.tasks {
        task.context_requirements
            .push(format!("source Addon {}", candidate.id));
        task.context_requirements
            .push(format!("migration action {action}"));
        task.context_requirements
            .push(format!("from version {}", previous.version));
        task.context_requirements
            .push(format!("to version {}", candidate.version));
    }
    workflow
}

fn addon_migration_task(input: AddonMigrationTaskInput<'_>) -> crate::graph::AtomicTask {
    let mut task = workflow_task(
        input.id,
        input.title,
        input.dependencies,
        input.context_requirements,
        input.validation_rules,
        input.expected_output,
        (input.executor, 0.0),
    );
    task.human_required = input.human_required;
    task
}

fn migration_validation_rule(kind: &str, expected: &str) -> ValidationRule {
    ValidationRule {
        kind: kind.to_string(),
        command: None,
        expected: expected.to_string(),
    }
}

fn addon_migration_workflow_report(
    workflow: &Workflow,
    previous: &AddonManifest,
    candidate: &AddonManifest,
    migration: &AddonMigrationPlan,
    action: &str,
) -> AddonMigrationWorkflowReport {
    let tasks = workflow
        .tasks
        .iter()
        .map(|task| AddonMigrationWorkflowTask {
            id: task.id.clone(),
            title: task.title.clone(),
            executor: executor_kind_label(&task.executor).to_string(),
            human_required: task.human_required,
            expected_output: task.expected_output.clone(),
        })
        .collect::<Vec<_>>();
    AddonMigrationWorkflowReport {
        schema_version: addon_migration_workflow_schema_version(),
        status: "addon_migration_workflow_created".to_string(),
        action: action.to_string(),
        workflow_id: workflow.id.clone(),
        from_addon_id: previous.id.clone(),
        to_addon_id: candidate.id.clone(),
        from_version: previous.version.clone(),
        to_version: candidate.version.clone(),
        migration_strategy: migration.strategy.clone(),
        data_migration: migration.data_migration.clone(),
        rollback: migration.rollback.clone(),
        requires_backup: migration.requires_backup,
        task_count: tasks.len(),
        tasks,
    }
}

fn executor_kind_label(executor: &ExecutorKind) -> &'static str {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
}

fn validate_addon_version_change_direction(
    previous: &AddonManifest,
    candidate: &AddonManifest,
    action: &str,
) -> Result<()> {
    if previous.id != candidate.id {
        bail!(
            "addon migration workflow requires the same addon id, got {} and {}",
            previous.id,
            candidate.id
        );
    }
    let ordering = compare_addon_versions(&candidate.version, &previous.version)
        .with_context(|| "cannot compare addon migration versions")?;
    match action.trim() {
        "upgrade" if ordering == std::cmp::Ordering::Greater => Ok(()),
        "downgrade" if ordering == std::cmp::Ordering::Less => Ok(()),
        "install" | "install_package" => Ok(()),
        other => bail!(
            "addon migration action `{}` does not match version change {} -> {}",
            other,
            previous.version,
            candidate.version
        ),
    }
}

fn addon_version_req_satisfied(requirement: &str, installed_version: &str) -> bool {
    let requirement = requirement.trim();
    if requirement.is_empty() || requirement == "*" {
        return true;
    }
    requirement
        .split(',')
        .map(str::trim)
        .filter(|clause| !clause.is_empty())
        .all(|clause| addon_version_clause_satisfied(clause, installed_version))
}

fn addon_version_clause_satisfied(clause: &str, installed_version: &str) -> bool {
    let Some(installed) = parse_addon_version(installed_version) else {
        return false;
    };
    for operator in [">=", "<=", "==", ">", "<", "=", "^", "~"] {
        if let Some(version) = clause.strip_prefix(operator) {
            let Some(required) = parse_addon_version(version.trim()) else {
                return false;
            };
            return match operator {
                ">=" => installed >= required,
                "<=" => installed <= required,
                ">" => installed > required,
                "<" => installed < required,
                "=" | "==" => installed == required,
                "^" => installed >= required && installed < caret_upper_bound(required),
                "~" => installed >= required && installed < tilde_upper_bound(required),
                _ => false,
            };
        }
    }
    parse_addon_version(clause)
        .map(|required| installed == required)
        .unwrap_or(false)
}

fn caret_upper_bound(version: (u64, u64, u64)) -> (u64, u64, u64) {
    if version.0 > 0 {
        (version.0 + 1, 0, 0)
    } else {
        (0, version.1 + 1, 0)
    }
}

fn tilde_upper_bound(version: (u64, u64, u64)) -> (u64, u64, u64) {
    (version.0, version.1 + 1, 0)
}

fn parse_addon_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(parse_version_part);
    Some((
        parts.next().unwrap_or(Some(0))?,
        parts.next().unwrap_or(Some(0))?,
        parts.next().unwrap_or(Some(0))?,
    ))
}

fn compare_addon_versions(candidate: &str, installed: &str) -> Option<std::cmp::Ordering> {
    Some(parse_addon_version(candidate)?.cmp(&parse_addon_version(installed)?))
}

fn parse_version_part(part: &str) -> Option<u64> {
    let digits = part
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn validation_issue(
    severity: &str,
    code: &str,
    subject: &str,
    message: &str,
) -> AddonValidationIssue {
    AddonValidationIssue {
        severity: severity.to_string(),
        code: code.to_string(),
        subject: subject.to_string(),
        message: message.to_string(),
    }
}

fn addon_enabled(addon: &AddonManifest) -> bool {
    addon.lifecycle.trim() == "enabled"
}

fn is_manifest_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("json" | "yaml" | "yml")
    )
}

fn core_kernel_addon() -> AddonManifest {
    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.core.kernel".to_string(),
        name: "Forge Core Kernel".to_string(),
        version: "0.1.0".to_string(),
        description: "Capacidades universais do runtime Forge, sem domínio operacional específico."
            .to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin".to_string(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
        capabilities: vec![
            capability(
                CAP_WORKFLOW_RUNTIME,
                "Workflow runtime",
                &["workflow", "workflows", "fluxo", "fluxos", "objetivo"],
                &["core"],
            ),
            capability(
                CAP_DYNAMIC_WORKFLOW,
                "Dynamic workflow engine",
                &[
                    "dinâmico",
                    "dynamic",
                    "replanejar",
                    "subworkflow",
                    "subworkflows",
                ],
                &["core"],
            ),
            capability(
                CAP_EVENT_ENGINE,
                "Event engine",
                &["evento", "event", "webhook", "cron", "api"],
                &["core"],
            ),
            capability(
                CAP_CONTEXT_ROUTING,
                "Context routing",
                &["contexto", "context", "roteamento"],
                &["core"],
            ),
            capability(
                CAP_MEMORY_GOVERNANCE,
                "Memory governance",
                &["memória", "memory", "retenção", "privacidade"],
                &["core"],
            ),
            capability(
                CAP_IDENTITY_ROUTING,
                "Identity routing",
                &["identidade", "identity", "usuário", "permissão"],
                &["core"],
            ),
            capability(
                CAP_PERSONALITY_ROUTING,
                "Personality routing",
                &["personalidade", "persona", "personality", "tom de voz"],
                &["core"],
            ),
            capability(
                CAP_HUMAN_COLLABORATION,
                "Human collaboration",
                &["humano", "human", "aprovação", "copilot", "colaboração"],
                &["core"],
            ),
            capability(
                CAP_OBSERVABILITY,
                "Observability",
                &["observabilidade", "logs", "timeline", "custos", "métricas"],
                &["core"],
            ),
            capability(
                CAP_ADDON_REGISTRY,
                "Addon registry",
                &["addon", "addons", "capabilidade", "capability", "plugin"],
                &["core"],
            ),
        ],
        workflows: Vec::new(),
        runtime_contracts: Vec::new(),
        views: Vec::new(),
        artifact_types: Vec::new(),
        event_types: Vec::new(),
        event_adapters: vec![event_adapter(
            "forge.core.event_inbox",
            "forge_inbox",
            "ingress",
            &["api", "webhook", "cron", "manual"],
            &[
                "start_workflow",
                "continue_workflow",
                "modify_workflow",
                "pause_workflow",
                "resume_workflow",
                "complete_workflow",
            ],
            &["forge.inbound_event"],
            "forge.event_ingest.v1",
            "forge_policy",
        )],
        context_providers: vec![context_provider(
            "forge.core.operating_context",
            "forge_core_context_router",
            &["global", "organization", "project", "processing"],
            &[
                "operating_context",
                "workflow_goal",
                "validation_rules",
                "dependencies",
            ],
        )],
        memory_providers: vec![memory_provider(
            "forge.core.file_memory",
            "file_first_markdown",
            &["global", "organization", "project", "processing"],
            &[
                "MEMORY_NONE",
                "MEMORY_SESSION",
                "MEMORY_SHORT_TERM",
                "MEMORY_STANDARD",
                "MEMORY_FULL",
                "MEMORY_ADMIN",
            ],
        )],
        integrations: Vec::new(),
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::new(),
    }
}

fn workflow_automation_addon() -> AddonManifest {
    let mut capability = capability(
        CAP_WORKFLOW_AUTOMATION_RESEARCH,
        "Workflow automation benchmark",
        &[
            "n8n",
            "n8n automation",
            "workflow automation benchmark",
            "workflow automation research",
        ],
        &["workflow_automation"],
    );
    capability.workflow_extensions = vec!["n8n_primitive_research".to_string()];
    capability.deliverables = vec![
        "n8n primitive research catalog".to_string(),
        "Forge primitive promotion recommendation".to_string(),
    ];
    capability.risks = vec![
        "external workflow concepts must not be copied blindly or promoted without Forge validation value".to_string(),
    ];
    capability.unknowns = vec![
        "current n8n source and documentation must be checked during research execution"
            .to_string(),
    ];
    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.addon.workflow_automation".to_string(),
        name: "Workflow Automation Addon".to_string(),
        version: "0.1.0".to_string(),
        description:
            "Pesquisa e integração de padrões de plataformas de automação visual como n8n."
                .to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin_compat".to_string(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
        capabilities: vec![capability],
        workflows: vec![workflow_extension("n8n_primitive_research", "research")],
        runtime_contracts: vec![runtime_contract(
            "n8n_primitive_research.planning_strategy",
            "planning_strategy",
            CAP_WORKFLOW_AUTOMATION_RESEARCH,
            "n8n_primitive_research",
            "forge_core_builtin",
            "planner:n8n_primitive_research",
            &["goal", "capability_resolution", "operating_context"],
            &["workflow_tasks", "validation_rules"],
            &[],
        )],
        views: Vec::new(),
        artifact_types: Vec::new(),
        event_types: Vec::new(),
        event_adapters: Vec::new(),
        context_providers: Vec::new(),
        memory_providers: Vec::new(),
        integrations: Vec::new(),
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::new(),
    }
}

fn visual_workspace_addon() -> AddonManifest {
    let mut capability = capability(
        CAP_VISUAL_WORKSPACE,
        "Collaborative visual workspace",
        &[
            "visual",
            "whiteboard",
            "figma",
            "wireframe",
            "tokens",
            "componentes",
            "components",
            "design system",
            "sistema de design",
        ],
        &["design", "collaboration"],
    );
    capability.workflow_extensions = vec!["creative_workspace".to_string()];
    capability.constraints = vec![
        "visual artifacts remain Forge-owned workflow state before external export".to_string(),
        "human and AI collaboration events are auditable in the workflow".to_string(),
    ];
    capability.deliverables = vec![
        "collaborative AI and human whiteboard".to_string(),
        "design token system".to_string(),
        "component, page, wireframe and flow artifacts".to_string(),
    ];
    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.addon.visual_workspace".to_string(),
        name: "Visual Workspace Addon".to_string(),
        version: "0.1.0".to_string(),
        description:
            "Whiteboards, tokens, componentes, fluxos e artefatos visuais controlados pelo Forge."
                .to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin_compat".to_string(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
        capabilities: vec![capability],
        workflows: vec![workflow_extension("creative_workspace", "artifact")],
        runtime_contracts: vec![runtime_contract(
            "creative_workspace.planning_strategy",
            "planning_strategy",
            CAP_VISUAL_WORKSPACE,
            "creative_workspace",
            "forge_core_builtin",
            "planner:creative_workspace",
            &["goal", "capability_resolution", "design_system"],
            &["visual_artifact_tasks", "collaboration_events"],
            &[],
        )],
        views: vec![AddonView {
            id: "visual.workspace".to_string(),
            title: "Visual Workspace".to_string(),
            surface: "ops_console".to_string(),
            view_type: "dashboard".to_string(),
            component: "forge.ops.visual_workspace".to_string(),
            route: "/ops/visual-workspace".to_string(),
            layout: AddonViewLayout {
                zone: "main".to_string(),
                order: 20,
                width: "full".to_string(),
                height: "auto".to_string(),
                density: "comfortable".to_string(),
            },
            data_bindings: vec![AddonViewDataBinding {
                id: "visual_artifacts".to_string(),
                source: "forge.ops.snapshot.visual_workflows".to_string(),
                query: "workflow.visual_artifacts".to_string(),
                scope: "workflow".to_string(),
                refresh_seconds: 5,
                required_capability: CAP_VISUAL_WORKSPACE.to_string(),
            }],
            actions: vec![AddonViewAction {
                id: "visual.create_artifact".to_string(),
                label: "Criar artefato visual".to_string(),
                action_type: "form".to_string(),
                target: "/api/visual/create-artifact".to_string(),
                method: "POST".to_string(),
                permission: "workflow:mutate".to_string(),
                requires_confirmation: false,
                payload_schema: vec!["workflow_id".to_string(), "kind".to_string()],
            }],
            permissions: Vec::new(),
            props: BTreeMap::new(),
        }],
        artifact_types: vec![
            artifact_type("whiteboard"),
            artifact_type("wireframe"),
            artifact_type("design_tokens"),
        ],
        event_types: vec![event_type("visual.collaboration_event", "local")],
        event_adapters: Vec::new(),
        context_providers: Vec::new(),
        memory_providers: Vec::new(),
        integrations: Vec::new(),
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::new(),
    }
}

fn software_development_addon() -> AddonManifest {
    let mut capability = capability(
        CAP_SOURCE_CODE_PATCH_LIFECYCLE,
        "Source code patch lifecycle",
        &[
            "código fonte",
            "codigo fonte",
            "source code",
            "edição de código",
            "edicao de codigo",
            "file editing",
            "patch review",
            "patch",
            "rollback",
        ],
        &["software_development"],
    );
    capability.description =
        "Planejamento, revisão, aplicação e restauração auditável de patches em repositórios."
            .to_string();
    capability.workflow_extensions = vec!["source_code_patch_lifecycle".to_string()];
    capability.deliverables = vec![
        "bounded patch plan with repo-relative targets".to_string(),
        "diff review artifact before apply approval".to_string(),
        "apply artifact with validation evidence".to_string(),
        "guarded revert and explicit restore receipts".to_string(),
    ];
    capability.constraints = vec![
        "software-specific behavior stays in this Addon, not in the universal Core kernel"
            .to_string(),
        "current runtime delegates to Forge Core builtin patch commands for compatibility"
            .to_string(),
        "human approval gates are enforced at patch operation level".to_string(),
    ];
    capability.risks = vec![
        "source edits can mutate repository state and require diff review before apply".to_string(),
        "restore operations must remain explicit and approval-gated".to_string(),
    ];
    capability.view_ids = vec!["software.patch_workbench".to_string()];

    let mut patch_executor = runtime_contract(
        "source_code_patch_lifecycle.executor",
        "executor",
        CAP_SOURCE_CODE_PATCH_LIFECYCLE,
        "source_code_patch_lifecycle",
        "forge_core_builtin",
        "forge.patch.lifecycle",
        &[
            "workflow_id",
            "task_id",
            "repository_path",
            "target_paths",
            "operator_intent",
            "validation_commands",
        ],
        &[
            "patch_plan_artifact",
            "patch_review_artifact",
            "patch_diff_artifact",
            "patch_apply_artifact",
            "patch_revert_artifact",
            "patch_restore_artifact",
        ],
        &["source_code.patch"],
    );
    patch_executor.constraints = vec![
        "run forge patch plan before bounded source edits".to_string(),
        "run forge patch review and forge patch diff before apply approval".to_string(),
        "record rollback proposal before explicit restore execution".to_string(),
    ];

    let mut props = BTreeMap::new();
    props.insert(
        "kernel_boundary".to_string(),
        serde_json::json!(
            "software-specific patch UX is owned by this Addon; Core only hosts the builtin compatibility executor"
        ),
    );
    props.insert(
        "operation_contract".to_string(),
        serde_json::json!("forge.interactive.patch_operation_plan.v1"),
    );

    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.addon.software_development".to_string(),
        name: "Software Development Addon".to_string(),
        version: "0.1.0".to_string(),
        description:
            "Capacidades específicas de desenvolvimento de software, incluindo ciclo auditável de patches."
                .to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin_compat".to_string(),
        dependencies: Vec::new(),
        permissions: vec![AddonPermission {
            id: "source_code.patch".to_string(),
            description:
                "Planejar, revisar, aplicar e restaurar alterações de código-fonte em repositórios sob workflow."
                    .to_string(),
            risk: "medium".to_string(),
            requires_human_approval: false,
            tools: vec![
                "git".to_string(),
                "forge.patch.plan".to_string(),
                "forge.patch.review".to_string(),
                "forge.patch.diff".to_string(),
                "forge.patch.apply".to_string(),
                "forge.patch.revert".to_string(),
                "forge.patch.restore".to_string(),
            ],
            resources: vec![
                "repository.source".to_string(),
                "workflow_artifact".to_string(),
                "diff_snapshot".to_string(),
            ],
            integrations: Vec::new(),
            actions: vec![
                "source_code:plan_patch".to_string(),
                "source_code:review_diff".to_string(),
                "source_code:apply_patch".to_string(),
                "source_code:revert_patch".to_string(),
                "source_code:restore_files".to_string(),
            ],
            tenant_scopes: vec![
                "organization".to_string(),
                "project".to_string(),
                "workflow".to_string(),
            ],
        }],
        capabilities: vec![capability],
        workflows: vec![workflow_extension("source_code_patch_lifecycle", "software")],
        runtime_contracts: vec![patch_executor],
        views: vec![AddonView {
            id: "software.patch_workbench".to_string(),
            title: "Software Patch Workbench".to_string(),
            surface: "tui".to_string(),
            view_type: "workbench".to_string(),
            component: "forge.interactive.patch_workbench".to_string(),
            route: "/interactive/patch-workbench".to_string(),
            layout: AddonViewLayout {
                zone: "main".to_string(),
                order: 30,
                width: "full".to_string(),
                height: "auto".to_string(),
                density: "dense".to_string(),
            },
            data_bindings: vec![AddonViewDataBinding {
                id: "patch_workbench".to_string(),
                source: "forge.interactive.patch_workbench".to_string(),
                query: "dashboard.patch_workbench_panel".to_string(),
                scope: "repository".to_string(),
                refresh_seconds: 2,
                required_capability: CAP_SOURCE_CODE_PATCH_LIFECYCLE.to_string(),
            }],
            actions: vec![
                AddonViewAction {
                    id: "patch.plan".to_string(),
                    label: "Plan patch".to_string(),
                    action_type: "command".to_string(),
                    target: "forge patch plan".to_string(),
                    method: "CLI".to_string(),
                    permission: "source_code.patch".to_string(),
                    requires_confirmation: false,
                    payload_schema: vec![
                        "workflow_id".to_string(),
                        "task_id".to_string(),
                        "target_paths".to_string(),
                    ],
                },
                AddonViewAction {
                    id: "patch.review".to_string(),
                    label: "Review patch".to_string(),
                    action_type: "command".to_string(),
                    target: "forge patch review".to_string(),
                    method: "CLI".to_string(),
                    permission: "source_code.patch".to_string(),
                    requires_confirmation: false,
                    payload_schema: vec!["artifact_ref".to_string()],
                },
                AddonViewAction {
                    id: "patch.apply".to_string(),
                    label: "Apply patch".to_string(),
                    action_type: "command".to_string(),
                    target: "forge patch apply".to_string(),
                    method: "CLI".to_string(),
                    permission: "source_code.patch".to_string(),
                    requires_confirmation: true,
                    payload_schema: vec![
                        "artifact_ref".to_string(),
                        "approved_by".to_string(),
                        "validation_commands".to_string(),
                    ],
                },
                AddonViewAction {
                    id: "patch.restore".to_string(),
                    label: "Restore files".to_string(),
                    action_type: "command".to_string(),
                    target: "forge patch restore".to_string(),
                    method: "CLI".to_string(),
                    permission: "source_code.patch".to_string(),
                    requires_confirmation: true,
                    payload_schema: vec![
                        "revert_artifact_ref".to_string(),
                        "approved_by".to_string(),
                        "confirm_restore".to_string(),
                    ],
                },
            ],
            permissions: vec!["source_code.patch".to_string()],
            props,
        }],
        artifact_types: vec![
            artifact_type("patch_plan"),
            artifact_type("patch_review"),
            artifact_type("patch_diff"),
            artifact_type("patch_apply"),
            artifact_type("patch_revert"),
            artifact_type("patch_restore"),
        ],
        event_types: vec![
            event_type("source_code.patch_planned", "local"),
            event_type("source_code.patch_reviewed", "local"),
            event_type("source_code.patch_applied", "local"),
            event_type("source_code.patch_restored", "local"),
        ],
        event_adapters: Vec::new(),
        context_providers: Vec::new(),
        memory_providers: Vec::new(),
        integrations: Vec::new(),
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::from([(
            "core_boundary".to_string(),
            "software-specific Addon; Core runtime adapter remains builtin compatibility"
                .to_string(),
        )]),
    }
}

fn hackathon_factory_addon() -> AddonManifest {
    let mut capability = capability(
        CAP_HACKATHON_FACTORY,
        "Hackathon factory",
        &[
            "hackathon mvp",
            "hackathon software factory",
            "ideathon mvp",
            "ideathon software factory",
            "maratona mvp",
            "maratona software factory",
            "maratona fábrica",
            "fábrica de mvp",
        ],
        &["software", "product"],
    );
    capability.workflow_extensions = vec!["hackathon_factory".to_string()];
    capability.deliverables = vec![
        "hackathon regulation compliance matrix".to_string(),
        "idea viability decision".to_string(),
        "final idea PDF artifact".to_string(),
        "MVP backlog and software factory plan".to_string(),
        "pitch package".to_string(),
        "buffered deadline improvement loop".to_string(),
        "Telegram delivery payload".to_string(),
    ];
    capability.constraints = vec![
        "regulation-first feasibility gate".to_string(),
        "final package deadline buffer before official submission".to_string(),
        "PDF and explanation artifact delivered to Telegram".to_string(),
    ];
    capability.risks = vec![
        "user idea may be strategically useful but off-theme unless reframed against the regulation".to_string(),
        "deadline buffer can be insufficient if the final pitch package is left too late".to_string(),
        "MVP complexity must not crowd out pitch quality and judging criteria".to_string(),
    ];
    capability.unknowns = vec![
        "exact final regulation deadline and preferred buffer hours are supplied per run".to_string(),
        "team size, skills and available implementation time must be confirmed before build scope is locked".to_string(),
    ];
    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.addon.hackathon_factory".to_string(),
        name: "Hackathon Factory Addon".to_string(),
        version: "0.1.0".to_string(),
        description: "Workflow específico para maratonas, ideathons e escopo de MVP.".to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin_compat".to_string(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
        capabilities: vec![capability],
        workflows: vec![workflow_extension("hackathon_factory", "planning")],
        runtime_contracts: vec![runtime_contract(
            "hackathon_factory.planning_strategy",
            "planning_strategy",
            CAP_HACKATHON_FACTORY,
            "hackathon_factory",
            "forge_core_builtin",
            "planner:hackathon_factory",
            &["goal", "deadline", "regulation_context"],
            &["feasibility_gate", "mvp_backlog", "pitch_package_tasks"],
            &[],
        )],
        views: Vec::new(),
        artifact_types: vec![
            artifact_type("regulation_matrix"),
            artifact_type("pitch_package"),
        ],
        event_types: Vec::new(),
        event_adapters: Vec::new(),
        context_providers: Vec::new(),
        memory_providers: Vec::new(),
        integrations: Vec::new(),
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::new(),
    }
}

fn daily_goal_research_addon() -> AddonManifest {
    let mut capability = capability(
        CAP_DAILY_GOAL_RESEARCH,
        "Daily goal research",
        &[
            "daily goal research",
            "daily goal",
            "goal research workflow",
        ],
        &["research"],
    );
    capability.workflow_extensions = vec!["daily_goal_research".to_string()];
    capability.deliverables = vec![
        "durable daily Goal research schedule".to_string(),
        "explicit Goal loop node".to_string(),
        "per-Goal research subflow lineage".to_string(),
        "Markdown and PDF Goal reports".to_string(),
        "Telegram delivery record".to_string(),
    ];
    capability.constraints = vec![
        "cron and loop semantics remain native Forge graph state".to_string(),
        "deterministic code handles stable repeated work".to_string(),
        "AI is reserved for judgment and summarization".to_string(),
    ];
    capability.risks = vec![
        "recurring research must remain Forge-owned instead of becoming an ad hoc terminal loop"
            .to_string(),
        "Telegram delivery records must not expose raw secrets".to_string(),
    ];
    capability.unknowns =
        vec!["live DuckDuckGo and Playwright page availability can vary per daily run".to_string()];
    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.addon.research".to_string(),
        name: "Research Addon".to_string(),
        version: "0.1.0".to_string(),
        description: "Loops de pesquisa recorrente e empacotamento de relatórios.".to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin_compat".to_string(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
        capabilities: vec![capability],
        workflows: vec![workflow_extension("daily_goal_research", "persistent")],
        runtime_contracts: vec![runtime_contract(
            "daily_goal_research.planning_strategy",
            "planning_strategy",
            CAP_DAILY_GOAL_RESEARCH,
            "daily_goal_research",
            "forge_core_builtin",
            "planner:daily_goal_research",
            &["goal", "cron", "timezone"],
            &["loop_node", "research_subflows", "report_artifacts"],
            &[],
        )],
        views: Vec::new(),
        artifact_types: vec![artifact_type("research_report")],
        event_types: vec![event_type("cron.daily_goal_research", "cron")],
        event_adapters: Vec::new(),
        context_providers: Vec::new(),
        memory_providers: Vec::new(),
        integrations: Vec::new(),
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::new(),
    }
}

fn notification_addon() -> AddonManifest {
    let mut capability = capability(
        CAP_TELEGRAM_NOTIFICATION,
        "Telegram notification",
        &["telegram"],
        &["notification"],
    );
    capability.workflow_extensions = vec!["telegram_notification".to_string()];
    let mut telegram_updates_adapter = event_adapter(
        "telegram.bot_updates",
        "telegram",
        "ingress",
        &["telegram"],
        &["start_workflow", "continue_workflow"],
        &["telegram.message"],
        "telegram.update.v1",
        "bot_token",
    );
    telegram_updates_adapter.permissions = vec!["telegram.send_message".to_string()];
    let mut telegram_message_egress = event_adapter(
        "telegram.bot_send_message",
        "telegram",
        "egress",
        &["forge", "codex", "opencode", "gemini", "claude"],
        &["send_message", "send_report", "notify_user"],
        &["telegram.message"],
        "telegram.send_message.v1",
        "bot_token",
    );
    telegram_message_egress.permissions = vec!["telegram.send_message".to_string()];
    telegram_message_egress.secret_env = Some("TELEGRAM_BOT_TOKEN".to_string());
    let mut telegram_document_egress = event_adapter(
        "telegram.bot_send_document",
        "telegram",
        "egress",
        &["forge", "codex", "opencode", "gemini", "claude"],
        &["send_document", "send_report", "send_final_report"],
        &["telegram.document", "telegram.report"],
        "telegram.send_document.v1",
        "bot_token",
    );
    telegram_document_egress.permissions = vec!["telegram.send_message".to_string()];
    telegram_document_egress.secret_env = Some("TELEGRAM_BOT_TOKEN".to_string());
    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.addon.notification".to_string(),
        name: "Notification Addon".to_string(),
        version: "0.1.0".to_string(),
        description: "Canais de notificação acionados por workflows.".to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin_compat".to_string(),
        dependencies: Vec::new(),
        permissions: vec![AddonPermission {
            id: "telegram.send_message".to_string(),
            description: "Enviar mensagens e documentos para chat autorizado.".to_string(),
            risk: "medium".to_string(),
            requires_human_approval: true,
            tools: vec!["telegram_bot_api".to_string()],
            resources: vec![
                "authorized_chat".to_string(),
                "telegram_document".to_string(),
            ],
            integrations: vec!["telegram.bot_api".to_string()],
            actions: vec!["send_message".to_string(), "send_document".to_string()],
            tenant_scopes: vec!["organization".to_string(), "channel".to_string()],
        }],
        capabilities: vec![capability],
        workflows: vec![workflow_extension("telegram_notification", "notification")],
        runtime_contracts: vec![runtime_contract(
            "telegram_notification.handoff",
            "handoff",
            CAP_TELEGRAM_NOTIFICATION,
            "telegram_notification",
            "external_api",
            "telegram.bot_api.send_message",
            &["message", "document_ref", "authorized_chat"],
            &["telegram_delivery_record"],
            &["telegram.send_message"],
        )],
        views: Vec::new(),
        artifact_types: vec![
            artifact_type("telegram_delivery_record"),
            artifact_type("telegram_document_delivery"),
        ],
        event_types: vec![
            event_type("telegram.message", "telegram"),
            event_type("telegram.document", "telegram"),
            event_type("telegram.report", "telegram"),
        ],
        event_adapters: vec![
            telegram_updates_adapter,
            telegram_message_egress,
            telegram_document_egress,
        ],
        context_providers: Vec::new(),
        memory_providers: Vec::new(),
        integrations: vec![IntegrationDeclaration {
            id: "telegram.bot_api".to_string(),
            title: "Telegram Bot API".to_string(),
            integration_type: "messaging".to_string(),
        }],
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::new(),
    }
}

fn async_runtime_addon() -> AddonManifest {
    let mut capability = capability(
        CAP_ASYNC_RUNTIME,
        "Async runtime",
        &[
            "async",
            "assíncrono",
            "kubernetes",
            "knative",
            "docker",
            "long-running",
            "longa duração",
        ],
        &["runtime"],
    );
    capability.workflow_extensions = vec!["async_runtime_policy".to_string()];
    AddonManifest {
        schema_version: addon_manifest_schema_version(),
        id: "forge.addon.runtime".to_string(),
        name: "Runtime Addon".to_string(),
        version: "0.1.0".to_string(),
        description: "Políticas de execução assíncrona e substratos de runtime.".to_string(),
        lifecycle: "enabled".to_string(),
        source: "builtin_compat".to_string(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
        capabilities: vec![capability],
        workflows: vec![workflow_extension("async_runtime_policy", "runtime")],
        runtime_contracts: vec![runtime_contract(
            "async_runtime_policy.planning_strategy",
            "planning_strategy",
            CAP_ASYNC_RUNTIME,
            "async_runtime_policy",
            "forge_core_builtin",
            "planner:async_runtime_policy",
            &["goal", "runtime_availability", "execution_policy"],
            &["async_policy", "scale_to_zero_plan"],
            &[],
        )],
        views: Vec::new(),
        artifact_types: Vec::new(),
        event_types: Vec::new(),
        event_adapters: Vec::new(),
        context_providers: Vec::new(),
        memory_providers: Vec::new(),
        integrations: Vec::new(),
        compatibility: AddonCompatibility::default(),
        metadata: BTreeMap::new(),
    }
}

fn capability(id: &str, title: &str, keywords: &[&str], domains: &[&str]) -> CapabilityDeclaration {
    CapabilityDeclaration {
        id: id.to_string(),
        title: title.to_string(),
        description: String::new(),
        domains: domains.iter().map(|domain| (*domain).to_string()).collect(),
        keywords: keywords
            .iter()
            .map(|keyword| (*keyword).to_string())
            .collect(),
        requires_capabilities: Vec::new(),
        workflow_extensions: Vec::new(),
        deliverables: Vec::new(),
        constraints: Vec::new(),
        risks: Vec::new(),
        unknowns: Vec::new(),
        event_triggers: Vec::new(),
        artifact_types: Vec::new(),
        view_ids: Vec::new(),
    }
}

fn workflow_extension(id: &str, kind: &str) -> WorkflowExtensionDeclaration {
    WorkflowExtensionDeclaration {
        id: id.to_string(),
        kind: kind.to_string(),
        description: String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn runtime_contract(
    id: &str,
    contract_type: &str,
    capability_id: &str,
    workflow_extension_id: &str,
    runtime: &str,
    entrypoint: &str,
    inputs: &[&str],
    outputs: &[&str],
    permissions: &[&str],
) -> AddonRuntimeContractDeclaration {
    AddonRuntimeContractDeclaration {
        id: id.to_string(),
        title: id.replace('.', " "),
        contract_type: contract_type.to_string(),
        capability_id: capability_id.to_string(),
        workflow_extension_id: workflow_extension_id.to_string(),
        runtime: runtime.to_string(),
        entrypoint: entrypoint.to_string(),
        inputs: inputs.iter().map(|input| (*input).to_string()).collect(),
        outputs: outputs.iter().map(|output| (*output).to_string()).collect(),
        permissions: permissions
            .iter()
            .map(|permission| (*permission).to_string())
            .collect(),
        constraints: Vec::new(),
    }
}

fn artifact_type(id: &str) -> ArtifactTypeDeclaration {
    ArtifactTypeDeclaration {
        id: id.to_string(),
        title: id.replace('_', " "),
        generic_kind: "artifact".to_string(),
    }
}

fn event_type(id: &str, transport: &str) -> EventTypeDeclaration {
    EventTypeDeclaration {
        id: id.to_string(),
        title: id.replace('.', " "),
        transport: transport.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn event_adapter(
    id: &str,
    transport: &str,
    direction: &str,
    origins: &[&str],
    actions: &[&str],
    event_types: &[&str],
    schema: &str,
    auth: &str,
) -> EventAdapterDeclaration {
    EventAdapterDeclaration {
        id: id.to_string(),
        title: id.replace('.', " "),
        transport: transport.to_string(),
        direction: direction.to_string(),
        origins: origins.iter().map(|origin| (*origin).to_string()).collect(),
        actions: actions.iter().map(|action| (*action).to_string()).collect(),
        event_types: event_types
            .iter()
            .map(|event_type| (*event_type).to_string())
            .collect(),
        schema: schema.to_string(),
        auth: auth.to_string(),
        secret_env: None,
        credential_vault: None,
        signature_header: None,
        permissions: Vec::new(),
        endpoint: None,
        allowed_hosts: Vec::new(),
        timeout_seconds: None,
        max_response_bytes: None,
    }
}

fn context_provider(
    id: &str,
    source: &str,
    scopes: &[&str],
    provides_sections: &[&str],
) -> ContextProviderDeclaration {
    ContextProviderDeclaration {
        id: id.to_string(),
        title: id.replace('.', " "),
        source: source.to_string(),
        scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
        provides_sections: provides_sections
            .iter()
            .map(|section| (*section).to_string())
            .collect(),
    }
}

fn memory_provider(
    id: &str,
    provider_type: &str,
    scopes: &[&str],
    memory_levels: &[&str],
) -> MemoryProviderDeclaration {
    MemoryProviderDeclaration {
        id: id.to_string(),
        title: id.replace('.', " "),
        provider_type: provider_type.to_string(),
        scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
        memory_levels: memory_levels
            .iter()
            .map(|level| (*level).to_string())
            .collect(),
    }
}

fn addon_catalog_schema_version() -> String {
    ADDON_CATALOG_SCHEMA_VERSION.to_string()
}

fn addon_manifest_schema_version() -> String {
    "forge.addon_manifest.v1".to_string()
}

fn capability_resolution_schema_version() -> String {
    CAPABILITY_RESOLUTION_SCHEMA_VERSION.to_string()
}

fn addon_validation_schema_version() -> String {
    ADDON_VALIDATION_SCHEMA_VERSION.to_string()
}

fn installed_addons_schema_version() -> String {
    INSTALLED_ADDONS_SCHEMA_VERSION.to_string()
}

fn addon_lifecycle_schema_version() -> String {
    ADDON_LIFECYCLE_SCHEMA_VERSION.to_string()
}

fn addon_capability_index_schema_version() -> String {
    ADDON_CAPABILITY_INDEX_SCHEMA_VERSION.to_string()
}

fn addon_event_adapters_schema_version() -> String {
    ADDON_EVENT_ADAPTERS_SCHEMA_VERSION.to_string()
}

fn addon_observability_schema_version() -> String {
    ADDON_OBSERVABILITY_SCHEMA_VERSION.to_string()
}

fn addon_runtime_contracts_schema_version() -> String {
    ADDON_RUNTIME_CONTRACTS_SCHEMA_VERSION.to_string()
}

fn addon_planner_registry_schema_version() -> String {
    ADDON_PLANNER_REGISTRY_SCHEMA_VERSION.to_string()
}

fn addon_runtime_contract_policy_schema_version() -> String {
    ADDON_RUNTIME_CONTRACT_POLICY_SCHEMA_VERSION.to_string()
}

fn addon_runtime_contract_dispatch_schema_version() -> String {
    ADDON_RUNTIME_CONTRACT_DISPATCH_SCHEMA_VERSION.to_string()
}

fn addon_runtime_workers_schema_version() -> String {
    ADDON_RUNTIME_WORKERS_SCHEMA_VERSION.to_string()
}

fn addon_views_schema_version() -> String {
    ADDON_VIEWS_SCHEMA_VERSION.to_string()
}

fn addon_package_schema_version() -> String {
    ADDON_PACKAGE_SCHEMA_VERSION.to_string()
}

fn addon_marketplace_schema_version() -> String {
    ADDON_MARKETPLACE_SCHEMA_VERSION.to_string()
}

fn addon_package_fetch_schema_version() -> String {
    ADDON_PACKAGE_FETCH_SCHEMA_VERSION.to_string()
}

fn addon_registry_sync_schema_version() -> String {
    ADDON_REGISTRY_SYNC_SCHEMA_VERSION.to_string()
}

fn addon_package_lock_schema_version() -> String {
    ADDON_PACKAGE_LOCK_SCHEMA_VERSION.to_string()
}

fn addon_package_lock_enforcement_schema_version() -> String {
    ADDON_PACKAGE_LOCK_ENFORCEMENT_SCHEMA_VERSION.to_string()
}

fn addon_trust_store_schema_version() -> String {
    ADDON_TRUST_STORE_SCHEMA_VERSION.to_string()
}

fn addon_package_policy_schema_version() -> String {
    ADDON_PACKAGE_POLICY_SCHEMA_VERSION.to_string()
}

fn addon_package_install_schema_version() -> String {
    ADDON_PACKAGE_INSTALL_SCHEMA_VERSION.to_string()
}

fn addon_migration_workflow_schema_version() -> String {
    ADDON_MIGRATION_WORKFLOW_SCHEMA_VERSION.to_string()
}

fn addon_permission_authorizations_schema_version() -> String {
    ADDON_PERMISSION_AUTHORIZATIONS_SCHEMA_VERSION.to_string()
}

fn addon_permission_gate_schema_version() -> String {
    "forge.addon_permission_gate.v1".to_string()
}

fn default_addon_lifecycle() -> String {
    "enabled".to_string()
}

fn default_addon_source() -> String {
    "external".to_string()
}

fn default_addon_view_type() -> String {
    "panel".to_string()
}

fn default_addon_view_action_type() -> String {
    "command".to_string()
}

fn default_permission_risk() -> String {
    "medium".to_string()
}
