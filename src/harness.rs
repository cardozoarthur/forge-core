use crate::artifact::hex_sha256;
use crate::intent::OperatingContextSpec;
use crate::storage::{ForgeStore, GlobalEventWrite, HeadroomBlobWrite, StoredHeadroomBlobRecord};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TOKEN_HEADROOM_SCHEMA_VERSION: &str = "forge.harness.token_headroom.v1";
pub const CLI_WRAPPER_PLAN_SCHEMA_VERSION: &str = "forge.harness.cli_wrapper_plan.v1";
pub const HEADROOM_RETRIEVAL_SCHEMA_VERSION: &str = "forge.harness.headroom_retrieval.v1";
pub const HEADROOM_STATS_SCHEMA_VERSION: &str = "forge.harness.headroom_stats.v1";
pub const CLI_HARNESS_EXEC_SCHEMA_VERSION: &str = "forge.harness.exec_receipt.v1";
pub const CLI_HARNESS_EXEC_EVENT_SCHEMA_VERSION: &str = "forge.harness.exec_event.v1";
pub const CLI_HARNESS_MODE_SCHEMA_VERSION: &str = "forge.harness.mode.v1";
pub const CLI_HARNESS_DOCTOR_SCHEMA_VERSION: &str = "forge.harness.doctor.v1";
pub const CLI_HARNESS_HEADROOM_PLAN_SCHEMA_VERSION: &str = "forge.harness.headroom_plan.v1";
pub const CLI_HARNESS_HEADROOM_RUNTIME_PLAN_SCHEMA_VERSION: &str =
    "forge.harness.headroom_runtime_plan.v1";
pub const CLI_HARNESS_ADOPTION_PLAN_SCHEMA_VERSION: &str = "forge.harness.adoption_plan.v1";
pub const CLI_HARNESS_ACTIVATION_PROFILE_SCHEMA_VERSION: &str =
    "forge.harness.activation_profile.v1";
pub const CLI_HARNESS_EXECUTOR_COMPATIBILITY_SCHEMA_VERSION: &str =
    "forge.harness.executor_compatibility.v1";
pub const CLI_HARNESS_BOOTSTRAP_SCHEMA_VERSION: &str = "forge.harness.bootstrap.v1";
pub const CLI_HARNESS_ORCHESTRATION_CONTRACT_SCHEMA_VERSION: &str =
    "forge.harness.orchestration_contract.v1";
pub const CLI_HARNESS_BOOTSTRAP_CONFIG_WRITE_SCHEMA_VERSION: &str =
    "forge.harness.bootstrap_config_write.v1";
pub const CLI_HARNESS_SESSION_LIFECYCLE_PLAN_SCHEMA_VERSION: &str =
    "forge.harness.session_lifecycle_plan.v1";
pub const CLI_SHIM_INSTALL_SCHEMA_VERSION: &str = "forge.harness.shim_install.v1";
pub const CLI_SHIM_STATUS_SCHEMA_VERSION: &str = "forge.harness.shim_status.v1";
pub const CLI_SHIM_ACTIVATION_DIAGNOSTIC_SCHEMA_VERSION: &str =
    "forge.harness.shim_activation_diagnostic.v1";
const CLI_SHIM_MARKER: &str = "# forge-harness-shim:v1";
const CLI_HARNESS_ACTIVATION_BEGIN: &str = "# >>> forge harness activation profile";
const CLI_HARNESS_ACTIVATION_END: &str = "# <<< forge harness activation profile";

#[derive(Debug, Clone, Serialize)]
pub struct TokenHeadroomReport {
    pub schema_version: String,
    pub status: String,
    pub source: String,
    pub content_kind: String,
    pub strategy: String,
    pub reversible: bool,
    pub original_sha256: String,
    pub original_bytes: usize,
    pub compressed_sha256: String,
    pub compressed_bytes: usize,
    pub estimated_original_tokens: usize,
    pub estimated_compressed_tokens: usize,
    pub estimated_saved_tokens: usize,
    pub savings_percent: f64,
    pub budget_tokens: usize,
    pub budget_status: String,
    pub retrieval_ref: String,
    pub persisted: bool,
    pub retrieval_available: bool,
    pub store_status: String,
    pub routing: Vec<String>,
    pub compressed_content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliWrapperPlanReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub command: Vec<String>,
    pub forge_first: bool,
    pub forge_first_source: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub wrapper_strategy: String,
    pub context_budget: usize,
    pub context_budget_source: String,
    pub token_headroom_enabled: bool,
    pub token_headroom_source: String,
    pub require_token_headroom_for_forge_first: bool,
    pub env: Vec<CliWrapperEnvVar>,
    pub launch_command: Vec<String>,
    pub orchestration_contract: HarnessOrchestrationContract,
    pub headroom_runtime_plan: HarnessHeadroomRuntimePlan,
    pub session_lifecycle_plan: HarnessSessionLifecyclePlan,
    pub harness_checks: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessHeadroomRuntimePlan {
    pub schema_version: String,
    pub status: String,
    pub enabled: bool,
    pub executor: String,
    pub mode: String,
    pub context_budget: usize,
    pub require_for_forge_first: bool,
    pub interception_points: Vec<HarnessHeadroomInterceptionPoint>,
    pub content_routes: Vec<HarnessHeadroomContentRoute>,
    pub reversible_store: HarnessHeadroomReversibleStore,
    pub mcp_tools: Vec<String>,
    pub env: Vec<CliWrapperEnvVar>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessHeadroomInterceptionPoint {
    pub point_id: String,
    pub source: String,
    pub target: String,
    pub direction: String,
    pub required: bool,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessHeadroomContentRoute {
    pub content_kind: String,
    pub detector: String,
    pub strategy: String,
    pub reversible: bool,
    pub persistence: String,
    pub retrieval_hint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessHeadroomReversibleStore {
    pub backend: String,
    pub uri_scheme: String,
    pub persistence_mode: String,
    pub retrieval_command: Vec<String>,
    pub ttl_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessOrchestrationContract {
    pub schema_version: String,
    pub status: String,
    pub control_plane: String,
    pub executor: String,
    pub forge_first: bool,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub required_env: Vec<CliWrapperEnvVar>,
    pub routing_stages: Vec<HarnessOrchestrationStage>,
    pub gates: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessOrchestrationStage {
    pub id: String,
    pub owner: String,
    pub source: String,
    pub target: String,
    pub required: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessSessionLifecyclePlan {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub session_id: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub lineage_complete: bool,
    pub missing_lineage: Vec<String>,
    pub gates: Vec<HarnessSessionLifecycleGate>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessSessionLifecycleGate {
    pub gate_id: String,
    pub title: String,
    pub state: String,
    pub status: String,
    pub command: Vec<String>,
    pub mutates_workflow: bool,
    pub records_event: bool,
    pub rationale: String,
}

pub struct CliWrapperPlanOptions<'a> {
    pub executor: &'a str,
    pub command: &'a [String],
    pub forge_first: bool,
    pub forge_first_source: &'a str,
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub context_budget_source: &'a str,
    pub token_headroom: bool,
    pub token_headroom_source: &'a str,
    pub require_token_headroom_for_forge_first: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessModeReport {
    pub schema_version: String,
    pub status: String,
    pub forge_first: bool,
    pub effective_mode: String,
    pub forge_first_source: String,
    pub env_default_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_default_value: Option<String>,
    pub project_config_path: String,
    pub project_config_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_default_mode: Option<String>,
    pub project_exec_policy_path: String,
    pub project_exec_policy_status: String,
    pub require_lineage_for_exec: bool,
    pub default_context_budget: usize,
    pub context_budget_source: String,
    pub default_token_headroom: bool,
    pub token_headroom_source: String,
    pub require_token_headroom_for_forge_first: bool,
    pub precedence: Vec<String>,
    pub safety_checks: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessDoctorReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub project_root: String,
    pub shim_dir: String,
    pub forge_first_ready: bool,
    pub token_headroom_ready: bool,
    pub shim_ready: bool,
    pub lineage_policy_ready: bool,
    pub lineage_context_ready: bool,
    pub mode: HarnessModeReport,
    pub shim_status: CliShimStatusReport,
    pub wrapper_plan: CliWrapperPlanReport,
    pub readiness_checks: Vec<String>,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessHeadroomPlanReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub project_root: String,
    pub forge_first: bool,
    pub forge_first_source: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub context_budget: usize,
    pub context_budget_source: String,
    pub token_headroom_enabled: bool,
    pub token_headroom_source: String,
    pub require_token_headroom_for_forge_first: bool,
    pub wrapper_env: Vec<CliWrapperEnvVar>,
    pub orchestration_contract: HarnessOrchestrationContract,
    pub wrapper_plan: CliWrapperPlanReport,
    pub session_lifecycle_plan: HarnessSessionLifecyclePlan,
    pub compression_pipeline: Vec<String>,
    pub reserve_strategy: Vec<String>,
    pub retrieval_policy: Vec<String>,
    pub mcp_tools: Vec<String>,
    pub next_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessAdoptionPlanReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub project_root: String,
    pub shim_dir: String,
    pub mutates_state: bool,
    pub executes_child: bool,
    pub recommended_project_config: HarnessRecommendedProjectConfig,
    pub mode: HarnessModeReport,
    pub headroom_plan: HarnessHeadroomPlanReport,
    pub doctor: HarnessDoctorReport,
    pub adoption_steps: Vec<HarnessAdoptionStep>,
    pub commands: HarnessAdoptionCommands,
    pub mcp_tools: Vec<String>,
    pub next_action: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessRecommendedProjectConfig {
    pub default_mode: String,
    pub default_context_budget: usize,
    pub default_token_headroom: bool,
    pub require_token_headroom_for_forge_first: bool,
    pub require_lineage_for_exec: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessAdoptionStep {
    pub id: String,
    pub title: String,
    pub status: String,
    pub command_key: String,
    pub risk_level: String,
    pub mutates_state: bool,
    pub executes_child: bool,
    pub requires_approval: bool,
    pub approval_reason: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessAdoptionCommands {
    pub write_project_harness_config: Vec<String>,
    pub bootstrap_project_harness: Vec<String>,
    pub mode: Vec<String>,
    pub headroom_plan: Vec<String>,
    pub doctor: Vec<String>,
    pub activation_profile: Vec<String>,
    pub install_shims: Vec<String>,
    pub sync_executors: Vec<String>,
    pub wrap_plan: Vec<String>,
    pub exec_with_lineage_dry_run: Vec<String>,
    pub exec_with_lineage: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessActivationProfileReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub shim_dir: String,
    pub project_root: String,
    pub apply: bool,
    pub applied: bool,
    pub mutates_state: bool,
    pub executes_child: bool,
    pub writes_shell_rc: bool,
    pub would_write_shell_rc: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_rc: Option<String>,
    pub shell_rc_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    pub forge_first: bool,
    pub context_budget: usize,
    pub context_budget_source: String,
    pub token_headroom: bool,
    pub token_headroom_source: String,
    pub path_prepend: String,
    pub path_precedence_before_activation: String,
    pub current_shell_activation_status: String,
    pub activation_required: bool,
    pub one_shot_activation_test_command: String,
    pub verification_commands: Vec<String>,
    pub env: Vec<CliWrapperEnvVar>,
    pub activation_commands: Vec<String>,
    pub deactivation_commands: Vec<String>,
    pub activation_script: String,
    pub deactivation_script: String,
    pub managed_block: String,
    pub rollback_commands: Vec<String>,
    pub next_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessExecutorCompatibilityReport {
    pub schema_version: String,
    pub status: String,
    pub selected_executor: String,
    pub selected_adapter_family: String,
    pub selected_compatibility: HarnessExecutorCompatibility,
    pub canonical_executor_families: Vec<HarnessExecutorFamily>,
    pub compatibility_matrix: Vec<HarnessExecutorCompatibility>,
    pub next_action: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessExecutorFamily {
    pub executor: String,
    pub display_name: String,
    pub adapter_family: String,
    pub native_entrypoint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessExecutorCompatibility {
    pub executor: String,
    pub display_name: String,
    pub adapter_family: String,
    pub native_entrypoint: String,
    pub selected: bool,
    pub compatibility_status: String,
    pub adoption_posture: String,
    pub ready_as_forge_first_default: bool,
    pub readiness_score_percent: usize,
    pub ready_surfaces: Vec<String>,
    pub blocked_surfaces: Vec<String>,
    pub recommended_surfaces: Vec<String>,
    pub disabled_surfaces: Vec<String>,
    pub supported_surfaces: Vec<String>,
    pub readiness: Vec<HarnessExecutorSurfaceReadiness>,
    pub next_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessExecutorSurfaceReadiness {
    pub surface: String,
    pub status: String,
    pub source: String,
    pub reason: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessBootstrapReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub project_root: String,
    pub shim_dir: String,
    pub apply: bool,
    pub applied: bool,
    pub mutates_state: bool,
    pub would_mutate_state: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    pub config_write: HarnessBootstrapConfigWrite,
    pub adoption_plan: HarnessAdoptionPlanReport,
    pub shim_install: Option<CliShimInstallReport>,
    pub next_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessBootstrapConfigWrite {
    pub schema_version: String,
    pub status: String,
    pub path: String,
    pub existed_before: bool,
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    pub config: HarnessRecommendedProjectConfig,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessModeOptions<'a> {
    pub forge_first: bool,
    pub observe_only: bool,
    pub project_root: Option<&'a Path>,
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessDoctorOptions<'a> {
    pub shim_dir: &'a Path,
    pub executor: &'a str,
    pub forge_first: bool,
    pub observe_only: bool,
    pub project_root: Option<&'a Path>,
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub context_budget_source: &'a str,
    pub token_headroom: bool,
    pub token_headroom_source: &'a str,
    pub require_token_headroom_for_forge_first: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessHeadroomPlanOptions<'a> {
    pub executor: &'a str,
    pub command: &'a [String],
    pub forge_first: bool,
    pub forge_first_source: &'a str,
    pub project_root: Option<&'a Path>,
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub context_budget_source: &'a str,
    pub token_headroom: bool,
    pub token_headroom_source: &'a str,
    pub require_token_headroom_for_forge_first: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessAdoptionPlanOptions<'a> {
    pub shim_dir: &'a Path,
    pub executor: &'a str,
    pub forge_first: bool,
    pub observe_only: bool,
    pub project_root: Option<&'a Path>,
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub context_budget_source: &'a str,
    pub token_headroom: bool,
    pub token_headroom_source: &'a str,
    pub require_token_headroom_for_forge_first: bool,
}

pub struct HarnessActivationProfileOptions<'a> {
    pub shim_dir: &'a Path,
    pub executor: &'a str,
    pub project_root: Option<&'a Path>,
    pub context_budget: usize,
    pub context_budget_source: &'a str,
    pub token_headroom: bool,
    pub token_headroom_source: &'a str,
    pub apply: bool,
    pub shell_rc: Option<&'a Path>,
    pub approved_by: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessBootstrapOptions<'a> {
    pub shim_dir: &'a Path,
    pub executor: &'a str,
    pub project_root: &'a Path,
    pub store_path: Option<&'a Path>,
    pub context_budget: usize,
    pub context_budget_source: &'a str,
    pub token_headroom: bool,
    pub token_headroom_source: &'a str,
    pub apply: bool,
    pub approved_by: Option<&'a str>,
    pub force: bool,
}

struct HarnessForgeFirstMode {
    forge_first: bool,
    source: &'static str,
}

struct HarnessProjectDefaultMode {
    path: PathBuf,
    status: &'static str,
    forge_first: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct HarnessRuntimePolicy {
    pub context_budget: usize,
    pub context_budget_source: String,
    pub token_headroom: bool,
    pub token_headroom_source: String,
    pub require_token_headroom_for_forge_first: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessRuntimePolicyOptions<'a> {
    pub project_root: Option<&'a Path>,
    pub context_budget: Option<usize>,
    pub context_budget_source: &'a str,
    pub token_headroom: Option<bool>,
    pub token_headroom_source: &'a str,
    pub forge_first: bool,
    pub default_context_budget: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliWrapperEnvVar {
    pub name: String,
    pub value: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliHarnessExecReceipt {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub command: Vec<String>,
    pub command_sha256: String,
    pub cwd: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub forge_first: bool,
    pub forge_first_source: String,
    pub context_budget: usize,
    pub context_budget_source: String,
    pub token_headroom_source: String,
    pub require_token_headroom_for_forge_first: bool,
    pub dry_run: bool,
    pub allow_exec: bool,
    pub execution_mode: String,
    pub project_policy_path: String,
    pub project_policy_status: String,
    pub require_lineage_for_exec: bool,
    pub resolved_executable: Option<String>,
    pub resolution_status: String,
    pub wrapper_plan: CliWrapperPlanReport,
    pub safety_checks: Vec<String>,
    pub executed: bool,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
    pub stdout_bytes: Option<usize>,
    pub stderr_bytes: Option<usize>,
    pub stdout_sha256: Option<String>,
    pub stderr_sha256: Option<String>,
    pub stdout_excerpt: Option<String>,
    pub stderr_excerpt: Option<String>,
    pub output_headroom_enabled: bool,
    pub stdout_headroom: Option<TokenHeadroomReport>,
    pub stderr_headroom: Option<TokenHeadroomReport>,
    pub event_recorded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliShimInstallReport {
    pub schema_version: String,
    pub status: String,
    pub shim_dir: String,
    pub store_path: Option<String>,
    pub forge_binary: String,
    pub forge_first: bool,
    pub forge_first_source: String,
    pub context_budget: usize,
    pub token_headroom: bool,
    pub force: bool,
    pub installed_count: usize,
    pub updated_count: usize,
    pub blocked_count: usize,
    pub shims: Vec<CliShimReport>,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliShimReport {
    pub executor: String,
    pub shim_path: String,
    pub real_command: String,
    pub real_command_source: String,
    pub real_command_resolution_status: String,
    pub store_path: Option<String>,
    pub forge_binary: String,
    pub forge_first: bool,
    pub forge_first_source: String,
    pub context_budget: usize,
    pub token_headroom: bool,
    pub status: String,
    pub script_sha256: Option<String>,
    pub argv_policy: String,
    pub safety_checks: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliShimStatusReport {
    pub schema_version: String,
    pub status: String,
    pub shim_dir: String,
    pub executor: String,
    pub shim_path: String,
    pub shim_exists: bool,
    pub forge_owned: bool,
    pub executable: bool,
    pub path_precedence: String,
    pub path_entry_index: Option<usize>,
    pub resolved_path_from_path: Option<String>,
    pub real_command: Option<String>,
    pub real_command_source: String,
    pub real_command_resolution_status: String,
    pub store_path: Option<String>,
    pub forge_binary: Option<String>,
    pub would_recurse: bool,
    pub activation_diagnostic: CliShimActivationDiagnostic,
    pub checks: Vec<String>,
    pub instructions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliShimActivationDiagnostic {
    pub schema_version: String,
    pub status: String,
    pub activation_required: bool,
    pub activation_possible: bool,
    pub reason: String,
    pub path_precedence: String,
    pub shim_ready_for_activation: bool,
    pub one_shot_activation_command: String,
    pub activation_profile_command: Vec<String>,
    pub verification_commands: Vec<String>,
    pub rollback_hints: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct CliShimInstallOptions<'a> {
    pub shim_dir: &'a Path,
    pub executor: &'a str,
    pub real_cmd: Option<&'a str>,
    pub store_path: Option<&'a Path>,
    pub forge_first: bool,
    pub forge_first_source: &'a str,
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub token_headroom: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CliShimStatusOptions<'a> {
    pub shim_dir: &'a Path,
    pub executor: &'a str,
}

#[derive(Clone, Copy)]
pub struct CliHarnessExecOptions<'a> {
    pub store: Option<&'a ForgeStore>,
    pub executor: &'a str,
    pub command: &'a [String],
    pub forge_first: bool,
    pub forge_first_source: &'a str,
    pub workflow_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub context_budget_source: &'a str,
    pub token_headroom: bool,
    pub token_headroom_source: &'a str,
    pub require_token_headroom_for_forge_first: bool,
    pub dry_run: bool,
    pub allow_exec: bool,
    pub project_root: Option<&'a Path>,
    pub cwd: Option<&'a Path>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadroomRetrievalReport {
    pub schema_version: String,
    pub status: String,
    pub retrieval_ref: String,
    pub original_sha256: String,
    pub found: bool,
    pub include_content: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_original_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_compressed_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_saved_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

pub struct HeadroomStatsOptions<'a> {
    pub source: Option<&'a str>,
    pub content_kind: Option<&'a str>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadroomStatsReport {
    pub schema_version: String,
    pub status: String,
    pub operational_status: String,
    pub recommended_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind_filter: Option<String>,
    pub total_blobs: usize,
    pub total_original_bytes: i64,
    pub total_compressed_bytes: i64,
    pub total_estimated_original_tokens: i64,
    pub total_estimated_compressed_tokens: i64,
    pub total_estimated_saved_tokens: i64,
    pub average_savings_percent: f64,
    pub over_budget_after_headroom_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_savings_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_savings_content_kind: Option<String>,
    pub by_content_kind: Vec<HeadroomStatsContentKindBucket>,
    pub by_source: Vec<HeadroomStatsSourceBucket>,
    pub top_saved_blobs: Vec<HeadroomStatsSavedBlob>,
    pub next_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadroomStatsContentKindBucket {
    pub content_kind: String,
    pub blob_count: usize,
    pub estimated_original_tokens: i64,
    pub estimated_compressed_tokens: i64,
    pub estimated_saved_tokens: i64,
    pub savings_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadroomStatsSourceBucket {
    pub source: String,
    pub blob_count: usize,
    pub estimated_original_tokens: i64,
    pub estimated_compressed_tokens: i64,
    pub estimated_saved_tokens: i64,
    pub savings_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadroomStatsSavedBlob {
    pub retrieval_ref: String,
    pub source: String,
    pub content_kind: String,
    pub strategy: String,
    pub original_sha256: String,
    pub original_bytes: i64,
    pub compressed_bytes: i64,
    pub estimated_original_tokens: i64,
    pub estimated_compressed_tokens: i64,
    pub estimated_saved_tokens: i64,
    pub savings_percent: f64,
    pub budget_status: String,
    pub updated_at: String,
}

pub fn analyze_token_headroom(
    content: &str,
    content_kind_hint: Option<&str>,
    budget_tokens: usize,
    source: &str,
    reversible: bool,
) -> TokenHeadroomReport {
    let content_kind = detect_content_kind(content, content_kind_hint);
    let (strategy, routing, compressed_content) = compress_for_headroom(content, &content_kind);
    let original_bytes = content.len();
    let compressed_bytes = compressed_content.len();
    let estimated_original_tokens = estimate_tokens(content);
    let estimated_compressed_tokens = estimate_tokens(&compressed_content);
    let estimated_saved_tokens =
        estimated_original_tokens.saturating_sub(estimated_compressed_tokens);
    let savings_percent = if estimated_original_tokens == 0 {
        0.0
    } else {
        ((estimated_saved_tokens as f64 / estimated_original_tokens as f64) * 10000.0).round()
            / 100.0
    };
    let budget_status = if budget_tokens == 0 {
        "budget_not_requested"
    } else if estimated_compressed_tokens <= budget_tokens {
        "fits_budget_after_headroom"
    } else if estimated_original_tokens <= budget_tokens {
        "already_fit_budget"
    } else {
        "still_over_budget"
    };
    let original_sha256 = hex_sha256(content.as_bytes());
    let compressed_sha256 = hex_sha256(compressed_content.as_bytes());
    TokenHeadroomReport {
        schema_version: TOKEN_HEADROOM_SCHEMA_VERSION.to_string(),
        status: "token_headroom_ready".to_string(),
        source: source.to_string(),
        content_kind,
        strategy,
        reversible,
        original_sha256: original_sha256.clone(),
        original_bytes,
        compressed_sha256,
        compressed_bytes,
        estimated_original_tokens,
        estimated_compressed_tokens,
        estimated_saved_tokens,
        savings_percent,
        budget_tokens,
        budget_status: budget_status.to_string(),
        retrieval_ref: format!("forge://harness/headroom/{original_sha256}"),
        persisted: false,
        retrieval_available: false,
        store_status: "not_persisted".to_string(),
        routing,
        compressed_content,
    }
}

pub fn persist_token_headroom_report(
    store: &ForgeStore,
    mut report: TokenHeadroomReport,
    original_content: &str,
) -> Result<TokenHeadroomReport> {
    let write = HeadroomBlobWrite {
        source: report.source.clone(),
        content_kind: report.content_kind.clone(),
        strategy: report.strategy.clone(),
        reversible: report.reversible,
        original_sha256: report.original_sha256.clone(),
        original_bytes: usize_to_i64(report.original_bytes),
        compressed_sha256: report.compressed_sha256.clone(),
        compressed_bytes: usize_to_i64(report.compressed_bytes),
        estimated_original_tokens: usize_to_i64(report.estimated_original_tokens),
        estimated_compressed_tokens: usize_to_i64(report.estimated_compressed_tokens),
        estimated_saved_tokens: usize_to_i64(report.estimated_saved_tokens),
        budget_tokens: usize_to_i64(report.budget_tokens),
        budget_status: report.budget_status.clone(),
        routing: json!(report.routing),
        original_content: original_content.to_string(),
        compressed_content: report.compressed_content.clone(),
    };
    store.save_headroom_blob(&write)?;
    report.persisted = true;
    report.retrieval_available = true;
    report.store_status = "stored_local_sqlite".to_string();
    Ok(report)
}

pub fn retrieve_headroom_blob(
    store: &ForgeStore,
    retrieval_ref: &str,
    include_content: bool,
) -> Result<HeadroomRetrievalReport> {
    let original_sha256 = parse_headroom_ref(retrieval_ref)?;
    let retrieval_ref = format!("forge://harness/headroom/{original_sha256}");
    let Some(record) = store.load_headroom_blob_by_sha(&original_sha256)? else {
        return Ok(HeadroomRetrievalReport {
            schema_version: HEADROOM_RETRIEVAL_SCHEMA_VERSION.to_string(),
            status: "headroom_blob_missing".to_string(),
            retrieval_ref,
            original_sha256,
            found: false,
            include_content,
            source: None,
            content_kind: None,
            strategy: None,
            reversible: None,
            original_bytes: None,
            compressed_sha256: None,
            compressed_bytes: None,
            estimated_original_tokens: None,
            estimated_compressed_tokens: None,
            estimated_saved_tokens: None,
            budget_tokens: None,
            budget_status: None,
            routing: None,
            original_content: None,
            compressed_content: None,
            created_at: None,
            updated_at: None,
        });
    };
    Ok(headroom_retrieval_report(
        record,
        retrieval_ref,
        include_content,
    ))
}

pub fn build_headroom_stats_report(
    store: &ForgeStore,
    options: HeadroomStatsOptions<'_>,
) -> Result<HeadroomStatsReport> {
    let source_filter = normalize_optional_text(options.source);
    let content_kind_filter = normalize_optional_text(options.content_kind);
    let records =
        store.load_headroom_blobs(source_filter.as_deref(), content_kind_filter.as_deref())?;

    let mut by_content_kind = BTreeMap::<String, HeadroomStatsAccumulator>::new();
    let mut by_source = BTreeMap::<String, HeadroomStatsAccumulator>::new();
    let mut total = HeadroomStatsAccumulator::default();
    for record in &records {
        total.add(record);
        by_content_kind
            .entry(record.content_kind.clone())
            .or_default()
            .add(record);
        by_source
            .entry(record.source.clone())
            .or_default()
            .add(record);
    }

    let mut top_records = records.clone();
    top_records.sort_by(|left, right| {
        right
            .estimated_saved_tokens
            .cmp(&left.estimated_saved_tokens)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.original_sha256.cmp(&right.original_sha256))
    });
    let top_limit = if options.limit == 0 {
        10
    } else {
        options.limit.min(50)
    };
    let top_saved_blobs = top_records
        .into_iter()
        .take(top_limit)
        .map(headroom_stats_saved_blob)
        .collect::<Vec<_>>();
    let over_budget_after_headroom_count = records
        .iter()
        .filter(|record| record.budget_status == "still_over_budget")
        .count();
    let (operational_status, recommended_action) = headroom_stats_operator_decision(
        &records,
        total.saved_tokens,
        over_budget_after_headroom_count,
    );
    let primary_savings_source = top_saved_blobs.first().map(|blob| blob.source.clone());
    let primary_savings_content_kind = top_saved_blobs
        .first()
        .map(|blob| blob.content_kind.clone());

    let mut next_commands = vec![
        "forge harness token-headroom --content <payload> --kind log --budget-tokens <n> --persist --output json".to_string(),
        "forge harness headroom-stats --output json".to_string(),
    ];
    if let Some(top) = top_saved_blobs.first() {
        next_commands.push(format!(
            "forge harness retrieve-headroom --ref {} --include-content --output json",
            shell_quote(&top.retrieval_ref)
        ));
    }

    Ok(HeadroomStatsReport {
        schema_version: HEADROOM_STATS_SCHEMA_VERSION.to_string(),
        status: if records.is_empty() {
            "headroom_stats_empty".to_string()
        } else {
            "headroom_stats_ready".to_string()
        },
        operational_status: operational_status.to_string(),
        recommended_action: recommended_action.to_string(),
        source_filter,
        content_kind_filter,
        total_blobs: records.len(),
        total_original_bytes: total.original_bytes,
        total_compressed_bytes: total.compressed_bytes,
        total_estimated_original_tokens: total.original_tokens,
        total_estimated_compressed_tokens: total.compressed_tokens,
        total_estimated_saved_tokens: total.saved_tokens,
        average_savings_percent: headroom_savings_percent(total.original_tokens, total.saved_tokens),
        over_budget_after_headroom_count,
        primary_savings_source,
        primary_savings_content_kind,
        by_content_kind: by_content_kind
            .into_iter()
            .map(|(content_kind, aggregate)| HeadroomStatsContentKindBucket {
                content_kind,
                blob_count: aggregate.blob_count,
                estimated_original_tokens: aggregate.original_tokens,
                estimated_compressed_tokens: aggregate.compressed_tokens,
                estimated_saved_tokens: aggregate.saved_tokens,
                savings_percent: headroom_savings_percent(
                    aggregate.original_tokens,
                    aggregate.saved_tokens,
                ),
            })
            .collect(),
        by_source: by_source
            .into_iter()
            .map(|(source, aggregate)| HeadroomStatsSourceBucket {
                source,
                blob_count: aggregate.blob_count,
                estimated_original_tokens: aggregate.original_tokens,
                estimated_compressed_tokens: aggregate.compressed_tokens,
                estimated_saved_tokens: aggregate.saved_tokens,
                savings_percent: headroom_savings_percent(
                    aggregate.original_tokens,
                    aggregate.saved_tokens,
                ),
            })
            .collect(),
        top_saved_blobs,
        next_commands,
        notes: vec![
            "Headroom stats are read-only and aggregate only blobs already persisted in the local Forge store.".to_string(),
            "Use filters to inspect noisy sources before routing large tool outputs or CLI stdout back to a brain.".to_string(),
        ],
    })
}

fn headroom_stats_operator_decision(
    records: &[StoredHeadroomBlobRecord],
    total_saved_tokens: i64,
    over_budget_after_headroom_count: usize,
) -> (&'static str, &'static str) {
    if records.is_empty() {
        return ("headroom_no_data", "persist_headroom_samples");
    }
    if over_budget_after_headroom_count > 0 {
        return (
            "headroom_budget_attention_required",
            "inspect_over_budget_headroom_blobs",
        );
    }
    if total_saved_tokens > 0 {
        return (
            "headroom_savings_available",
            "route_large_tool_outputs_through_headroom",
        );
    }
    (
        "headroom_no_material_savings",
        "inspect_sources_before_enforcing_headroom",
    )
}

pub fn build_harness_mode_report(options: HarnessModeOptions<'_>) -> HarnessModeReport {
    let HarnessModeOptions {
        forge_first,
        observe_only,
        project_root,
    } = options;
    let mode = resolve_harness_forge_first(forge_first, observe_only, project_root);
    let env_default_value = env::var("FORGE_HARNESS_DEFAULT_MODE").ok();
    let project_root = project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let project = read_harness_project_mode(&project_root);
    let project_exec_policy = read_harness_project_exec_policy(&project_root);
    let project_exec_policy_status =
        harness_project_exec_policy_status(&project_exec_policy, false, None, None, None);
    let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
        project_root: Some(&project_root),
        context_budget: None,
        context_budget_source: "default",
        token_headroom: None,
        token_headroom_source: "default",
        forge_first: mode.forge_first,
        default_context_budget: 1200,
    });
    let mut safety_checks = vec![
        "mode report is read-only and never launches child processes".to_string(),
        "exec policy should be inspected before running external brain CLIs".to_string(),
    ];
    if project_exec_policy.require_lineage_for_exec {
        safety_checks.push("project_require_lineage_for_exec".to_string());
    }
    if runtime_policy.require_token_headroom_for_forge_first {
        safety_checks.push("project_require_token_headroom_for_forge_first".to_string());
    }
    HarnessModeReport {
        schema_version: CLI_HARNESS_MODE_SCHEMA_VERSION.to_string(),
        status: "harness_mode_resolved".to_string(),
        forge_first: mode.forge_first,
        effective_mode: harness_effective_mode(mode.forge_first).to_string(),
        forge_first_source: mode.source.to_string(),
        env_default_present: env_default_value.is_some(),
        env_default_value,
        project_config_path: project.path.display().to_string(),
        project_config_status: project.status.to_string(),
        project_default_mode: project
            .forge_first
            .map(harness_effective_mode)
            .map(ToString::to_string),
        project_exec_policy_path: project_exec_policy.path.display().to_string(),
        project_exec_policy_status: project_exec_policy_status.to_string(),
        require_lineage_for_exec: project_exec_policy.require_lineage_for_exec,
        default_context_budget: runtime_policy.context_budget,
        context_budget_source: runtime_policy.context_budget_source,
        default_token_headroom: runtime_policy.token_headroom,
        token_headroom_source: runtime_policy.token_headroom_source,
        require_token_headroom_for_forge_first: runtime_policy
            .require_token_headroom_for_forge_first,
        precedence: vec![
            "observe_only_flag".to_string(),
            "explicit_flag".to_string(),
            "env_default".to_string(),
            "project_config".to_string(),
            "default_observe_only".to_string(),
        ],
        safety_checks,
        notes: vec![
            "This report is read-only and does not install shims or execute brain CLIs."
                .to_string(),
            "Use it before wrap-plan, install-shims or exec when the active Forge-first policy is unclear.".to_string(),
        ],
    }
}

pub fn build_harness_doctor_report(
    options: HarnessDoctorOptions<'_>,
) -> Result<HarnessDoctorReport> {
    let HarnessDoctorOptions {
        shim_dir,
        executor,
        forge_first,
        observe_only,
        project_root,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    } = options;
    let executor = normalize_executor(executor);
    let project_root_path = project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mode = build_harness_mode_report(HarnessModeOptions {
        forge_first,
        observe_only,
        project_root: Some(&project_root_path),
    });
    let shim_status = inspect_cli_harness_shim_status(CliShimStatusOptions {
        shim_dir,
        executor: &executor,
    })?;
    let wrapper_plan = build_cli_wrapper_plan(CliWrapperPlanOptions {
        executor: &executor,
        command: &[],
        forge_first: mode.forge_first,
        forge_first_source: &mode.forge_first_source,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    });
    let forge_first_ready = mode.forge_first;
    let token_headroom_ready = token_headroom && wrapper_plan.token_headroom_enabled;
    let shim_ready = shim_status.status == "shim_status_ready";
    let lineage_context_ready = !mode.require_lineage_for_exec
        || harness_exec_has_required_lineage(workflow_id, task_id, run_id);
    let lineage_policy_ready = true;
    let mut readiness_checks = vec!["read_only_no_child_process".to_string()];
    readiness_checks.push(if forge_first_ready {
        "forge_first_enabled".to_string()
    } else {
        "forge_first_not_enabled".to_string()
    });
    readiness_checks.push(if token_headroom_ready {
        "token_headroom_enabled".to_string()
    } else {
        "token_headroom_disabled".to_string()
    });
    readiness_checks.push(if shim_ready {
        "shim_ready".to_string()
    } else if !shim_status.shim_exists {
        "shim_missing".to_string()
    } else {
        shim_status.status.clone()
    });
    if mode.require_lineage_for_exec && !lineage_context_ready {
        readiness_checks.push("lineage_required_for_real_exec".to_string());
    } else if mode.require_lineage_for_exec {
        readiness_checks.push("lineage_required_satisfied".to_string());
    } else {
        readiness_checks.push("lineage_not_required".to_string());
    }

    let mut next_actions = vec![format!(
        "forge harness mode --project-root {} --output json",
        shell_quote(&project_root_path.display().to_string())
    )];
    if !shim_ready {
        next_actions.push(format!(
            "forge harness install-shims --shim-dir {} --executor {} --project-root {} --output json",
            shell_quote(&shim_dir.display().to_string()),
            shell_quote(&executor),
            shell_quote(&project_root_path.display().to_string())
        ));
    }
    if mode.require_lineage_for_exec && !lineage_context_ready {
        next_actions.push(
            "pass --workflow <workflow-id> --task <task-id> --run <run-id> before real harness exec"
                .to_string(),
        );
    }
    next_actions.push(format!(
        "forge sync executors --shim-dir {} --output json",
        shell_quote(&shim_dir.display().to_string())
    ));

    let status = if forge_first_ready && token_headroom_ready && shim_ready {
        "harness_doctor_ready"
    } else {
        "harness_doctor_degraded"
    };
    Ok(HarnessDoctorReport {
        schema_version: CLI_HARNESS_DOCTOR_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        executor,
        project_root: project_root_path.display().to_string(),
        shim_dir: shim_dir.display().to_string(),
        forge_first_ready,
        token_headroom_ready,
        shim_ready,
        lineage_policy_ready,
        lineage_context_ready,
        mode,
        shim_status,
        wrapper_plan,
        readiness_checks,
        next_actions,
        notes: vec![
            "Harness doctor is read-only: it never installs shims or launches child processes."
                .to_string(),
            "Use it before handing Codex, Claude, Gemini or OpenCode to Forge-first execution."
                .to_string(),
        ],
    })
}

pub fn build_harness_headroom_plan(
    options: HarnessHeadroomPlanOptions<'_>,
) -> HarnessHeadroomPlanReport {
    let HarnessHeadroomPlanOptions {
        executor,
        command,
        forge_first,
        forge_first_source,
        project_root,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    } = options;
    let executor = normalize_executor(executor);
    let project_root_path = project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let wrapper_plan = build_cli_wrapper_plan(CliWrapperPlanOptions {
        executor: &executor,
        command,
        forge_first,
        forge_first_source,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    });
    let mut wrapper_env = wrapper_plan.env.clone();
    wrapper_env.push(env_var(
        "FORGE_TOKEN_HEADROOM_SOURCE",
        token_headroom_source,
        "records which flag, project policy or API input selected token-headroom",
    ));
    wrapper_env.push(env_var(
        "FORGE_HEADROOM_PLAN_SCHEMA",
        CLI_HARNESS_HEADROOM_PLAN_SCHEMA_VERSION,
        "lets downstream wrappers identify the Forge headroom planning contract",
    ));

    let project_root_display = project_root_path.display().to_string();
    let token_headroom_flag = if token_headroom {
        "--token-headroom"
    } else {
        "--no-token-headroom"
    };
    let mut next_commands = vec![
        format!(
            "forge harness wrap-plan --executor {} --project-root {} --context-budget {} {} --output json",
            shell_quote(&executor),
            shell_quote(&project_root_display),
            context_budget,
            token_headroom_flag,
        ),
        format!(
            "forge harness token-headroom --content <payload> --kind log --budget-tokens {} --output json",
            context_budget,
        ),
        "forge interactive harness --output json".to_string(),
    ];
    if require_token_headroom_for_forge_first {
        next_commands.push(
            "keep require_token_headroom_for_forge_first enabled for Forge-first child CLIs"
                .to_string(),
        );
    }

    HarnessHeadroomPlanReport {
        schema_version: CLI_HARNESS_HEADROOM_PLAN_SCHEMA_VERSION.to_string(),
        status: "harness_headroom_plan_ready".to_string(),
        executor,
        project_root: project_root_display,
        forge_first,
        forge_first_source: normalize_harness_mode_source(forge_first_source, forge_first),
        workflow_id: normalize_optional_text(workflow_id),
        task_id: normalize_optional_text(task_id),
        run_id: normalize_optional_text(run_id),
        context_budget,
        context_budget_source: context_budget_source.to_string(),
        token_headroom_enabled: token_headroom,
        token_headroom_source: token_headroom_source.to_string(),
        require_token_headroom_for_forge_first,
        wrapper_env,
        orchestration_contract: wrapper_plan.orchestration_contract.clone(),
        session_lifecycle_plan: wrapper_plan.session_lifecycle_plan.clone(),
        wrapper_plan,
        compression_pipeline: vec![
            "content_router".to_string(),
            "deterministic_log_json_code_text_compressors".to_string(),
            "reversible_retrieval_refs".to_string(),
            "local_sqlite_persistence_when_requested".to_string(),
            "guarded_stdout_stderr_headroom_on_exec".to_string(),
        ],
        reserve_strategy: vec![
            "reserve_context_budget_for_prompt_packet".to_string(),
            "reserve_headroom_for_tool_output_refs".to_string(),
            "prefer_retrieval_ref_over_large_inline_payload".to_string(),
            "keep_lineage_and_policy_env_uncompressed".to_string(),
        ],
        retrieval_policy: vec![
            "original_and_compressed_hashes_are_reported".to_string(),
            "persisted_refs_use_forge_harness_headroom_uri".to_string(),
            "retrieval_requires_forge_harness_retrieve_headroom".to_string(),
        ],
        mcp_tools: vec![
            "forge.harness.headroom_plan".to_string(),
            "forge.harness.token_headroom".to_string(),
            "forge.harness.retrieve_headroom".to_string(),
            "forge.harness.wrap_plan".to_string(),
        ],
        next_commands,
        notes: vec![
            "This is a read-only plan: it does not install shims or execute child CLIs."
                .to_string(),
            "Headroom benchmark ideas absorbed here are local-first compression, wrapper env shaping, reversible refs and MCP-visible readiness without copying external code."
                .to_string(),
        ],
    }
}

pub fn build_harness_activation_profile(
    options: HarnessActivationProfileOptions<'_>,
) -> Result<HarnessActivationProfileReport> {
    let executor = normalize_executor(options.executor);
    let shim_dir = options
        .shim_dir
        .canonicalize()
        .unwrap_or_else(|_| options.shim_dir.to_path_buf());
    let project_root = options
        .project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .canonicalize()
        .unwrap_or_else(|_| {
            options
                .project_root
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
    let shim_dir_display = shim_dir.display().to_string();
    let project_root_display = project_root.display().to_string();
    let shim_status = inspect_cli_harness_shim_status(CliShimStatusOptions {
        shim_dir: &shim_dir,
        executor: &executor,
    })?;
    let activation_required = shim_status.path_precedence != "shim_first";
    let current_shell_activation_status = if shim_status.path_precedence == "shim_first" {
        "activation_active"
    } else if shim_status.shim_exists
        && shim_status.forge_owned
        && shim_status.executable
        && !shim_status.would_recurse
    {
        "activation_required"
    } else {
        "shim_not_ready_for_activation"
    }
    .to_string();
    let path_activation_prefix = format!("PATH={}:$PATH", shell_quote(&shim_dir_display));
    let one_shot_activation_test_command = format!(
        "{} forge harness shim-status --shim-dir {} --executor {} --output json",
        path_activation_prefix,
        shell_quote(&shim_dir_display),
        shell_quote(&executor)
    );
    let verification_commands = vec![
        one_shot_activation_test_command.clone(),
        format!(
            "{} forge harness doctor --shim-dir {} --executor {} --project-root {} --output json",
            path_activation_prefix,
            shell_quote(&shim_dir_display),
            shell_quote(&executor),
            shell_quote(&project_root_display)
        ),
    ];
    let token_headroom_value = if options.token_headroom {
        "enabled"
    } else {
        "disabled"
    };
    let env = vec![
        env_var(
            "FORGE_HARNESS",
            "enabled",
            "marks the shell as intentionally routed through Forge harness controls",
        ),
        env_var(
            "FORGE_HARNESS_DEFAULT_MODE",
            "forge_first",
            "makes compatible CLI wrappers prefer Forge infrastructure by default",
        ),
        env_var(
            "FORGE_HARNESS_PROJECT_ROOT",
            &project_root_display,
            "binds wrapper and mode resolution to this project policy root",
        ),
        env_var(
            "FORGE_HARNESS_CONTEXT_BUDGET",
            &options.context_budget.to_string(),
            "sets the default bounded context budget for wrapper planning",
        ),
        env_var(
            "FORGE_HARNESS_TOKEN_HEADROOM",
            token_headroom_value,
            "documents whether token-headroom should be active in this shell",
        ),
        env_var(
            "FORGE_HARNESS_EXECUTOR",
            &executor,
            "records which brain CLI this activation profile is meant to inspect first",
        ),
    ];
    let activation_commands = vec![
        r#"export FORGE_HARNESS_PREV_PATH="${PATH}""#.to_string(),
        format!("export PATH={}:$PATH", shell_quote(&shim_dir_display)),
        "export FORGE_HARNESS=enabled".to_string(),
        "export FORGE_HARNESS_DEFAULT_MODE=forge_first".to_string(),
        format!(
            "export FORGE_HARNESS_PROJECT_ROOT={}",
            shell_quote(&project_root_display)
        ),
        format!(
            "export FORGE_HARNESS_CONTEXT_BUDGET={}",
            options.context_budget
        ),
        format!("export FORGE_HARNESS_TOKEN_HEADROOM={token_headroom_value}"),
        format!("export FORGE_HARNESS_EXECUTOR={}", shell_quote(&executor)),
    ];
    let deactivation_commands = vec![
        r#"if [ -n "${FORGE_HARNESS_PREV_PATH:-}" ]; then export PATH="${FORGE_HARNESS_PREV_PATH}"; fi"#.to_string(),
        "unset FORGE_HARNESS_PREV_PATH".to_string(),
        "unset FORGE_HARNESS".to_string(),
        "unset FORGE_HARNESS_DEFAULT_MODE".to_string(),
        "unset FORGE_HARNESS_PROJECT_ROOT".to_string(),
        "unset FORGE_HARNESS_CONTEXT_BUDGET".to_string(),
        "unset FORGE_HARNESS_TOKEN_HEADROOM".to_string(),
        "unset FORGE_HARNESS_EXECUTOR".to_string(),
    ];
    let activation_script = format!("{}\n", activation_commands.join("\n"));
    let deactivation_script = format!("{}\n", deactivation_commands.join("\n"));
    let managed_block = harness_activation_managed_block(
        &executor,
        &shim_dir_display,
        &project_root_display,
        &activation_script,
    );
    let shell_rc_path = options.shell_rc.map(Path::to_path_buf);
    let shell_rc_display = shell_rc_path
        .as_ref()
        .map(|path| path.display().to_string());
    let approved_by = options
        .approved_by
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let mut status = "harness_activation_profile_ready".to_string();
    let mut applied = false;
    let mut mutates_state = false;
    let mut writes_shell_rc = false;
    let mut shell_rc_status = if options.apply {
        "planned_apply_requested".to_string()
    } else {
        "not_requested".to_string()
    };
    let mut backup_path = None;
    let mut notes = vec![
        "Activation profile is read-only unless --apply, --shell-rc and --approved-by are all supplied.".to_string(),
        "Source the activation script only in shells where selected CLIs should prefer Forge infrastructure.".to_string(),
        "Deactivate by running the deactivation commands or starting a fresh shell.".to_string(),
    ];

    if options.apply {
        if approved_by.is_none() {
            status = "harness_activation_profile_blocked_missing_approval".to_string();
            shell_rc_status = "blocked_missing_approval".to_string();
            notes.push(
                "Shell startup files are not modified without an explicit approver.".to_string(),
            );
        } else if shell_rc_path.is_none() {
            status = "harness_activation_profile_blocked_missing_shell_rc".to_string();
            shell_rc_status = "blocked_missing_shell_rc".to_string();
            notes.push(
                "Supply --shell-rc <path> to choose the exact startup file Forge may update."
                    .to_string(),
            );
        } else if let Some(shell_rc) = shell_rc_path.as_ref() {
            let existed_before = shell_rc.exists();
            let existing = match fs::read_to_string(shell_rc) {
                Ok(content) => content,
                Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to read {}", shell_rc.display()));
                }
            };
            if let Some(parent) = shell_rc.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create shell rc parent {}", parent.display())
                })?;
            }
            if existed_before {
                let backup = shell_rc_backup_path(shell_rc);
                fs::write(&backup, existing.as_bytes())
                    .with_context(|| format!("failed to write {}", backup.display()))?;
                backup_path = Some(backup.display().to_string());
            }
            let updated = replace_harness_activation_block(&existing, &managed_block);
            fs::write(shell_rc, updated.as_bytes())
                .with_context(|| format!("failed to write {}", shell_rc.display()))?;
            status = "harness_activation_profile_applied".to_string();
            applied = true;
            mutates_state = true;
            writes_shell_rc = true;
            shell_rc_status = if existing.contains(CLI_HARNESS_ACTIVATION_BEGIN) {
                "updated_managed_block".to_string()
            } else if existed_before {
                "appended_managed_block".to_string()
            } else {
                "created_shell_rc".to_string()
            };
            notes.push(
                "A Forge-managed shell block was written; user shell activation still requires opening a new shell or sourcing the file.".to_string(),
            );
        }
    }

    let shell_rc_arg = shell_rc_display
        .clone()
        .unwrap_or_else(|| "<shell-rc>".to_string());
    let rollback_commands = vec![
        format!(
            "sed -i '/{}/,/{}/d' {}",
            CLI_HARNESS_ACTIVATION_BEGIN,
            CLI_HARNESS_ACTIVATION_END,
            shell_quote(&shell_rc_arg)
        ),
        backup_path
            .as_ref()
            .map(|backup| format!("cp {} {}", shell_quote(backup), shell_quote(&shell_rc_arg)))
            .unwrap_or_else(|| {
                "no backup was created because the shell rc did not exist before apply".to_string()
            }),
    ];
    let mut next_commands = vec![
        one_shot_activation_test_command.clone(),
        format!(
            "forge harness shim-status --shim-dir {} --executor {} --output json",
            shell_quote(&shim_dir_display),
            shell_quote(&executor)
        ),
        format!(
            "forge harness doctor --shim-dir {} --executor {} --project-root {} --output json",
            shell_quote(&shim_dir_display),
            shell_quote(&executor),
            shell_quote(&project_root_display)
        ),
        format!(
            "forge sync executors --shim-dir {} --allow {} --output json",
            shell_quote(&shim_dir_display),
            shell_quote(&executor)
        ),
    ];
    if !applied {
        next_commands.insert(
            0,
            format!(
                "forge harness activation-profile --shim-dir {} --executor {} --project-root {} --shell-rc {} --apply --approved-by <operator> --output json",
                shell_quote(&shim_dir_display),
                shell_quote(&executor),
                shell_quote(&project_root_display),
                shell_quote(&shell_rc_arg)
            ),
        );
    }

    Ok(HarnessActivationProfileReport {
        schema_version: CLI_HARNESS_ACTIVATION_PROFILE_SCHEMA_VERSION.to_string(),
        status,
        executor: executor.clone(),
        shim_dir: shim_dir_display.clone(),
        project_root: project_root_display.clone(),
        apply: options.apply,
        applied,
        mutates_state,
        executes_child: false,
        writes_shell_rc,
        would_write_shell_rc: true,
        shell_rc: shell_rc_display,
        shell_rc_status,
        approved_by,
        backup_path,
        forge_first: true,
        context_budget: options.context_budget,
        context_budget_source: options.context_budget_source.to_string(),
        token_headroom: options.token_headroom,
        token_headroom_source: options.token_headroom_source.to_string(),
        path_prepend: shim_dir_display.clone(),
        path_precedence_before_activation: shim_status.path_precedence,
        current_shell_activation_status,
        activation_required,
        one_shot_activation_test_command,
        verification_commands,
        env,
        activation_commands,
        deactivation_commands,
        activation_script,
        deactivation_script,
        managed_block,
        rollback_commands,
        next_commands,
        notes,
    })
}

fn harness_activation_managed_block(
    executor: &str,
    shim_dir: &str,
    project_root: &str,
    activation_script: &str,
) -> String {
    format!(
        "{begin}\n# forge-harness-activation:v1\n# executor={executor}\n# shim_dir={shim_dir}\n# project_root={project_root}\n{activation_script}{end}\n",
        begin = CLI_HARNESS_ACTIVATION_BEGIN,
        end = CLI_HARNESS_ACTIVATION_END,
    )
}

fn shell_rc_backup_path(shell_rc: &Path) -> PathBuf {
    let file_name = shell_rc
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shellrc");
    shell_rc.with_file_name(format!("{file_name}.forge-backup"))
}

fn replace_harness_activation_block(existing: &str, managed_block: &str) -> String {
    if let Some(start) = existing.find(CLI_HARNESS_ACTIVATION_BEGIN) {
        if let Some(end_relative) = existing[start..].find(CLI_HARNESS_ACTIVATION_END) {
            let end_marker_end = start + end_relative + CLI_HARNESS_ACTIVATION_END.len();
            let end = existing[end_marker_end..]
                .find('\n')
                .map(|offset| end_marker_end + offset + 1)
                .unwrap_or(end_marker_end);
            let mut updated = String::new();
            updated.push_str(&existing[..start]);
            if !updated.ends_with('\n') && !updated.is_empty() {
                updated.push('\n');
            }
            updated.push_str(managed_block);
            updated.push_str(&existing[end..]);
            return updated;
        }
    }

    let mut updated = existing.to_string();
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(managed_block);
    updated
}

pub fn build_harness_adoption_plan(
    options: HarnessAdoptionPlanOptions<'_>,
) -> Result<HarnessAdoptionPlanReport> {
    let HarnessAdoptionPlanOptions {
        shim_dir,
        executor,
        forge_first,
        observe_only,
        project_root,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    } = options;
    let executor = normalize_executor(executor);
    let project_root_path = project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mode = build_harness_mode_report(HarnessModeOptions {
        forge_first,
        observe_only,
        project_root: Some(&project_root_path),
    });
    let headroom_plan = build_harness_headroom_plan(HarnessHeadroomPlanOptions {
        executor: &executor,
        command: &[],
        forge_first: mode.forge_first,
        forge_first_source: &mode.forge_first_source,
        project_root: Some(&project_root_path),
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    });
    let doctor = build_harness_doctor_report(HarnessDoctorOptions {
        shim_dir,
        executor: &executor,
        forge_first,
        observe_only,
        project_root: Some(&project_root_path),
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    })?;

    let project_root_display = project_root_path.display().to_string();
    let shim_dir_display = shim_dir.display().to_string();
    let recommended_project_config = HarnessRecommendedProjectConfig {
        default_mode: "forge_first".to_string(),
        default_context_budget: context_budget,
        default_token_headroom: true,
        require_token_headroom_for_forge_first: true,
        require_lineage_for_exec: true,
    };
    let bootstrap_project_harness = vec![
        "forge".to_string(),
        "harness".to_string(),
        "bootstrap".to_string(),
        "--executor".to_string(),
        executor.clone(),
        "--shim-dir".to_string(),
        shim_dir_display.clone(),
        "--project-root".to_string(),
        project_root_display.clone(),
        "--apply".to_string(),
        "--approved-by".to_string(),
        "<operator>".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];
    let token_headroom_flag = if token_headroom {
        "--token-headroom"
    } else {
        "--no-token-headroom"
    };
    let lineage_workflow_id = workflow_id.unwrap_or("<workflow-id>").to_string();
    let lineage_task_id = task_id.unwrap_or("<task-id>").to_string();
    let lineage_run_id = run_id.unwrap_or("<run-id>").to_string();
    let exec_with_lineage_dry_run = vec![
        "forge".to_string(),
        "harness".to_string(),
        "exec".to_string(),
        "--executor".to_string(),
        executor.clone(),
        "--forge-first".to_string(),
        "--project-root".to_string(),
        project_root_display.clone(),
        "--workflow".to_string(),
        lineage_workflow_id.clone(),
        "--task".to_string(),
        lineage_task_id.clone(),
        "--run".to_string(),
        lineage_run_id.clone(),
        "--context-budget".to_string(),
        context_budget.to_string(),
        token_headroom_flag.to_string(),
        "--output".to_string(),
        "json".to_string(),
        "--".to_string(),
        executor.clone(),
    ];
    let exec_with_lineage = {
        let mut command = exec_with_lineage_dry_run.clone();
        let output_index = command
            .iter()
            .position(|part| part == "--output")
            .unwrap_or(command.len());
        command.insert(output_index, "--allow-exec".to_string());
        command.insert(output_index, "--execute".to_string());
        command
    };
    let commands = HarnessAdoptionCommands {
        write_project_harness_config: bootstrap_project_harness.clone(),
        bootstrap_project_harness,
        mode: vec![
            "forge".to_string(),
            "harness".to_string(),
            "mode".to_string(),
            "--project-root".to_string(),
            project_root_display.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        headroom_plan: vec![
            "forge".to_string(),
            "harness".to_string(),
            "headroom-plan".to_string(),
            "--executor".to_string(),
            executor.clone(),
            "--project-root".to_string(),
            project_root_display.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        doctor: vec![
            "forge".to_string(),
            "harness".to_string(),
            "doctor".to_string(),
            "--executor".to_string(),
            executor.clone(),
            "--shim-dir".to_string(),
            shim_dir_display.clone(),
            "--project-root".to_string(),
            project_root_display.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        activation_profile: vec![
            "forge".to_string(),
            "harness".to_string(),
            "activation-profile".to_string(),
            "--shim-dir".to_string(),
            shim_dir_display.clone(),
            "--executor".to_string(),
            executor.clone(),
            "--project-root".to_string(),
            project_root_display.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        install_shims: vec![
            "forge".to_string(),
            "harness".to_string(),
            "install-shims".to_string(),
            "--shim-dir".to_string(),
            shim_dir_display.clone(),
            "--executor".to_string(),
            executor.clone(),
            "--project-root".to_string(),
            project_root_display.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        sync_executors: vec![
            "forge".to_string(),
            "sync".to_string(),
            "executors".to_string(),
            "--shim-dir".to_string(),
            shim_dir_display.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        wrap_plan: vec![
            "forge".to_string(),
            "harness".to_string(),
            "wrap-plan".to_string(),
            "--executor".to_string(),
            executor.clone(),
            "--project-root".to_string(),
            project_root_display.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        exec_with_lineage_dry_run,
        exec_with_lineage,
    };
    let shim_file_ready = doctor.shim_status.shim_exists
        && doctor.shim_status.forge_owned
        && doctor.shim_status.executable;
    let shim_installed_but_not_active =
        shim_file_ready && doctor.shim_status.path_precedence != "shim_first";
    let next_action = if mode.project_config_status != "loaded" {
        format!(
            "forge harness bootstrap --executor {} --shim-dir {} --project-root {} --apply --approved-by <operator> --output json",
            shell_quote(&executor),
            shell_quote(&shim_dir_display),
            shell_quote(&project_root_display)
        )
    } else if shim_installed_but_not_active {
        format!(
            "forge harness activation-profile --shim-dir {} --executor {} --project-root {} --output json",
            shell_quote(&shim_dir_display),
            shell_quote(&executor),
            shell_quote(&project_root_display)
        )
    } else if !doctor.shim_ready {
        format!(
            "forge harness install-shims --shim-dir {} --executor {} --project-root {} --output json",
            shell_quote(&shim_dir_display),
            shell_quote(&executor),
            shell_quote(&project_root_display)
        )
    } else if !doctor.lineage_context_ready {
        shell_join(&commands.exec_with_lineage_dry_run)
    } else {
        format!(
            "forge sync executors --shim-dir {} --output json",
            shell_quote(&shim_dir_display)
        )
    };

    Ok(HarnessAdoptionPlanReport {
        schema_version: CLI_HARNESS_ADOPTION_PLAN_SCHEMA_VERSION.to_string(),
        status: "harness_adoption_plan_ready".to_string(),
        executor,
        project_root: project_root_display,
        shim_dir: shim_dir_display,
        mutates_state: false,
        executes_child: false,
        recommended_project_config,
        adoption_steps: vec![
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "write_project_harness_config",
                title: "Write project harness policy",
                status: if mode.project_config_status == "loaded" {
                    "already_configured"
                } else {
                    "recommended"
                },
                command_key: "write_project_harness_config",
                risk_level: "medium",
                mutates_state: true,
                executes_child: false,
                requires_approval: true,
                approval_reason: "Project harness config changes Forge-first, token-headroom and lineage policy for future CLI execution.",
                rationale: "Project policy makes Forge-first, headroom and lineage requirements explicit instead of relying on operator memory.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "bootstrap_project_harness",
                title: "Bootstrap project harness",
                status: if mode.project_config_status == "loaded" && doctor.shim_ready {
                    "already_ready"
                } else {
                    "recommended"
                },
                command_key: "bootstrap_project_harness",
                risk_level: "medium",
                mutates_state: true,
                executes_child: false,
                requires_approval: true,
                approval_reason: "Project harness bootstrap writes Forge-first policy and CLI shims for future CLI execution.",
                rationale: "Bootstrap gives UI and agent surfaces one explicit setup action before using Forge-first child CLI execution.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "inspect_headroom_plan",
                title: "Inspect headroom and wrapper policy",
                status: "ready",
                command_key: "headroom_plan",
                risk_level: "low",
                mutates_state: false,
                executes_child: false,
                requires_approval: false,
                approval_reason: "",
                rationale: "Headroom planning keeps large logs and child output bounded while preserving reversible retrieval refs.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "install_forge_first_shims",
                title: "Install Forge-first shims",
                status: if doctor.shim_ready {
                    "already_ready"
                } else if shim_file_ready {
                    "already_installed_needs_activation"
                } else {
                    "recommended"
                },
                command_key: "install_shims",
                risk_level: "medium",
                mutates_state: true,
                executes_child: false,
                requires_approval: true,
                approval_reason: "PATH shims alter how selected CLIs enter Forge infrastructure and must be approved before writing files.",
                rationale: "Shims make the selected CLI enter through Forge harness controls without replacing the native executable.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "activate_shell_profile",
                title: "Activate Forge-first shell profile",
                status: if doctor.shim_ready {
                    "already_active"
                } else if shim_file_ready {
                    "recommended"
                } else {
                    "ready_after_shim_install"
                },
                command_key: "activation_profile",
                risk_level: "low",
                mutates_state: false,
                executes_child: false,
                requires_approval: false,
                approval_reason: "",
                rationale: "Activation profile prints reversible shell exports so the operator can make selected CLIs prefer Forge infrastructure without editing shell startup files automatically.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "sync_executor_inventory",
                title: "Sync executor inventory",
                status: "ready",
                command_key: "sync_executors",
                risk_level: "low",
                mutates_state: true,
                executes_child: false,
                requires_approval: false,
                approval_reason: "",
                rationale: "Executor sync projects shim readiness into brains, sessions and shell launch plans.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "verify_harness_doctor",
                title: "Verify harness doctor",
                status: if doctor.status == "harness_doctor_ready" {
                    "already_ready"
                } else {
                    "recommended"
                },
                command_key: "doctor",
                risk_level: "low",
                mutates_state: false,
                executes_child: false,
                requires_approval: false,
                approval_reason: "",
                rationale: "Doctor confirms Forge-first, shim, token-headroom and lineage readiness before real handoff.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "validate_harness_exec_dry_run_with_lineage",
                title: "Validate harness exec dry-run with workflow lineage",
                status: if doctor.lineage_context_ready {
                    "ready"
                } else {
                    "blocked_until_lineage"
                },
                command_key: "exec_with_lineage_dry_run",
                risk_level: "medium",
                mutates_state: true,
                executes_child: false,
                requires_approval: false,
                approval_reason: "",
                rationale: "Dry-run receipts validate Forge-first env, context budget, token headroom and workflow/task/run lineage before any child CLI execution.",
            }),
            harness_adoption_step(HarnessAdoptionStepInput {
                id: "use_harness_exec_with_lineage",
                title: "Use harness exec with workflow lineage",
                status: if doctor.lineage_context_ready {
                    "ready_after_dry_run"
                } else {
                    "blocked_until_lineage"
                },
                command_key: "exec_with_lineage",
                risk_level: "high",
                mutates_state: true,
                executes_child: true,
                requires_approval: true,
                approval_reason: "Real harness exec starts an external child process and must be explicitly allowed with workflow/task/run lineage.",
                rationale: "Lineage binds child CLI work to workflow, task and run records before execution.",
            }),
        ],
        commands,
        mcp_tools: vec![
            "forge.harness.adoption_plan".to_string(),
            "forge.harness.bootstrap".to_string(),
            "forge.harness.mode".to_string(),
            "forge.harness.headroom_plan".to_string(),
            "forge.harness.doctor".to_string(),
            "forge.harness.activation_profile".to_string(),
            "forge.harness.install_shims".to_string(),
            "forge.harness.exec".to_string(),
        ],
        next_action,
        mode,
        headroom_plan,
        doctor,
        notes: vec![
            "This adoption plan is read-only: it does not write project config, install shims, sync executors or execute child CLIs.".to_string(),
            "Use it when an operator wants Codex, Claude, Gemini or OpenCode to prefer Forge infrastructure without hiding the native CLI boundary.".to_string(),
        ],
    })
}

pub fn build_harness_executor_compatibility_report(
    executor: &str,
    project_root: &Path,
    shim_dir: &Path,
    doctor: &HarnessDoctorReport,
    wrapper_plan: &CliWrapperPlanReport,
    session_lifecycle_plan: &HarnessSessionLifecyclePlan,
) -> HarnessExecutorCompatibilityReport {
    let selected_executor = normalize_executor(executor);
    let canonical_executor_families = canonical_harness_executor_families(&selected_executor);
    let selected_family = canonical_executor_families
        .iter()
        .find(|family| family.executor == selected_executor)
        .cloned()
        .unwrap_or_else(|| harness_executor_family(&selected_executor));
    let selected_compatibility = harness_executor_compatibility(
        selected_family.clone(),
        true,
        project_root,
        shim_dir,
        Some(doctor),
        Some(wrapper_plan),
        Some(session_lifecycle_plan),
    );
    let compatibility_matrix = canonical_executor_families
        .iter()
        .cloned()
        .map(|family| {
            if family.executor == selected_executor {
                selected_compatibility.clone()
            } else {
                harness_executor_compatibility(
                    family,
                    false,
                    project_root,
                    shim_dir,
                    None,
                    None,
                    None,
                )
            }
        })
        .collect::<Vec<_>>();
    let status = if selected_compatibility.compatibility_status == "ready" {
        "executor_compatibility_ready"
    } else {
        "executor_compatibility_degraded"
    };
    let next_action = selected_compatibility
        .next_commands
        .first()
        .cloned()
        .unwrap_or_else(|| "forge interactive harness --output json".to_string());

    HarnessExecutorCompatibilityReport {
        schema_version: CLI_HARNESS_EXECUTOR_COMPATIBILITY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        selected_executor,
        selected_adapter_family: selected_family.adapter_family,
        selected_compatibility,
        canonical_executor_families,
        compatibility_matrix,
        next_action,
        notes: vec![
            "This report is read-only and keeps Codex, Claude, Gemini and OpenCode as replaceable execution brains under Forge-owned routing.".to_string(),
            "Forge owns workflow state, context, memory, skills, MCP, credential-vault references, session lifecycle, token headroom and receipts; the native CLI remains the executor entrypoint.".to_string(),
        ],
    }
}

fn harness_executor_compatibility(
    family: HarnessExecutorFamily,
    selected: bool,
    project_root: &Path,
    shim_dir: &Path,
    doctor: Option<&HarnessDoctorReport>,
    wrapper_plan: Option<&CliWrapperPlanReport>,
    session_lifecycle_plan: Option<&HarnessSessionLifecyclePlan>,
) -> HarnessExecutorCompatibility {
    let supported_surfaces = harness_executor_supported_surfaces();
    let readiness = if selected {
        selected_harness_executor_readiness(
            &family,
            project_root,
            shim_dir,
            doctor.expect("selected executor compatibility requires doctor report"),
            wrapper_plan.expect("selected executor compatibility requires wrapper plan"),
            session_lifecycle_plan
                .expect("selected executor compatibility requires session lifecycle plan"),
        )
    } else {
        non_selected_harness_executor_readiness(&family, project_root, shim_dir)
    };
    let compatibility_status = harness_executor_compatibility_status(selected, &readiness);
    let surface_summary = harness_executor_surface_summary(&readiness);
    let adoption_posture = harness_executor_adoption_posture(
        selected,
        &readiness,
        &surface_summary,
        &compatibility_status,
    );
    let ready_as_forge_first_default =
        selected && adoption_posture == "ready_as_forge_first_default";
    let readiness_score_percent = harness_executor_readiness_score(&readiness);
    let next_commands =
        harness_executor_next_commands(&family, project_root, shim_dir, selected, &readiness);
    let notes = if selected {
        vec![
            "Selected executor readiness is derived from harness doctor, wrapper plan, shim status, lineage policy and headroom policy.".to_string(),
            "A blocked surface does not remove the executor; it tells the TUI/operator which Forge-first capability must be enabled before real child execution.".to_string(),
            "adoption_posture is the compact operator decision for whether this brain can become the Forge-first default without more setup.".to_string(),
        ]
    } else {
        vec![
            "Canonical executor family is known to the harness but was not selected for this readiness run.".to_string(),
            format!(
                "Run `forge interactive harness --executor {}` to inspect live readiness for this brain.",
                family.executor
            ),
        ]
    };

    HarnessExecutorCompatibility {
        executor: family.executor,
        display_name: family.display_name,
        adapter_family: family.adapter_family,
        native_entrypoint: family.native_entrypoint,
        selected,
        compatibility_status,
        adoption_posture,
        ready_as_forge_first_default,
        readiness_score_percent,
        ready_surfaces: surface_summary.ready,
        blocked_surfaces: surface_summary.blocked,
        recommended_surfaces: surface_summary.recommended,
        disabled_surfaces: surface_summary.disabled,
        supported_surfaces,
        readiness,
        next_commands,
        notes,
    }
}

#[derive(Debug, Default)]
struct HarnessExecutorSurfaceSummary {
    ready: Vec<String>,
    blocked: Vec<String>,
    recommended: Vec<String>,
    disabled: Vec<String>,
}

fn harness_executor_surface_summary(
    readiness: &[HarnessExecutorSurfaceReadiness],
) -> HarnessExecutorSurfaceSummary {
    let mut summary = HarnessExecutorSurfaceSummary::default();
    for item in readiness {
        match item.status.as_str() {
            "ready" => summary.ready.push(item.surface.clone()),
            "blocked" => summary.blocked.push(item.surface.clone()),
            "recommended" => summary.recommended.push(item.surface.clone()),
            "disabled" => summary.disabled.push(item.surface.clone()),
            _ => {}
        }
    }
    summary
}

fn harness_executor_readiness_score(readiness: &[HarnessExecutorSurfaceReadiness]) -> usize {
    if readiness.is_empty() {
        return 0;
    }
    let ready_count = readiness
        .iter()
        .filter(|item| item.status == "ready")
        .count();
    ((ready_count * 100) + (readiness.len() / 2)) / readiness.len()
}

fn harness_executor_adoption_posture(
    selected: bool,
    readiness: &[HarnessExecutorSurfaceReadiness],
    summary: &HarnessExecutorSurfaceSummary,
    compatibility_status: &str,
) -> String {
    if !selected {
        return "inspect_required".to_string();
    }
    if summary.blocked.iter().any(|surface| surface == "path_shim") {
        if readiness.iter().any(|item| {
            item.surface == "path_shim"
                && item.status == "blocked"
                && item.source == "harness_doctor.shim_status.path_precedence"
        }) {
            return "needs_path_activation".to_string();
        }
        return "needs_forge_owned_path_shim".to_string();
    }
    if summary
        .blocked
        .iter()
        .any(|surface| surface == "harness_exec" || surface == "session_lifecycle")
    {
        return "needs_workflow_lineage".to_string();
    }
    if summary
        .blocked
        .iter()
        .any(|surface| surface == "token_headroom")
    {
        return "needs_token_headroom".to_string();
    }
    if !summary.blocked.is_empty() {
        return "blocked".to_string();
    }
    if !summary.disabled.is_empty() {
        return "ready_with_optional_surfaces_disabled".to_string();
    }
    if summary
        .recommended
        .iter()
        .any(|surface| surface == "project_policy")
    {
        return "ready_but_project_policy_recommended".to_string();
    }
    if compatibility_status == "ready" {
        "ready_as_forge_first_default".to_string()
    } else {
        "ready_with_recommendations".to_string()
    }
}

fn selected_harness_executor_readiness(
    family: &HarnessExecutorFamily,
    project_root: &Path,
    shim_dir: &Path,
    doctor: &HarnessDoctorReport,
    wrapper_plan: &CliWrapperPlanReport,
    session_lifecycle_plan: &HarnessSessionLifecyclePlan,
) -> Vec<HarnessExecutorSurfaceReadiness> {
    let project_root_display = project_root.display().to_string();
    let shim_dir_display = shim_dir.display().to_string();
    let executor = &family.executor;
    let env_overlay_ready = wrapper_plan
        .env
        .iter()
        .any(|item| item.name == "FORGE_HARNESS" && item.value == "enabled");
    let routing_env_ready = |name: &str| {
        wrapper_plan
            .env
            .iter()
            .any(|item| item.name == name && item.value == "forge_controlled")
    };
    let credential_boundary_ready = wrapper_plan.env.iter().any(|item| {
        item.name == "FORGE_CREDENTIAL_VAULT_BOUNDARY" && item.value == "reference_only"
    });
    let event_receipts_ready = wrapper_plan
        .env
        .iter()
        .any(|item| item.name == "FORGE_EVENT_RECEIPTS" && item.value == "required");
    let shim_file_ready_for_activation = doctor.shim_status.shim_exists
        && doctor.shim_status.forge_owned
        && doctor.shim_status.executable
        && !doctor.shim_status.would_recurse;

    vec![
        executor_surface_readiness(
            "env_overlay",
            if env_overlay_ready { "ready" } else { "blocked" },
            "wrapper_plan.env",
            "Forge harness env overlay marks child CLI execution as Forge-controlled.",
            vec![
                "forge".to_string(),
                "harness".to_string(),
                "wrap-plan".to_string(),
                "--executor".to_string(),
                executor.clone(),
                "--project-root".to_string(),
                project_root_display.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        executor_surface_readiness(
            "path_shim",
            if doctor.shim_ready { "ready" } else { "blocked" },
            if doctor.shim_ready {
                "harness_doctor.shim_ready"
            } else if shim_file_ready_for_activation {
                "harness_doctor.shim_status.path_precedence"
            } else {
                "harness_doctor.shim_ready"
            },
            if doctor.shim_ready {
                "PATH resolves to the Forge-owned wrapper before native CLI defaults."
            } else if shim_file_ready_for_activation {
                "Forge-owned shim is installed and executable, but the shim directory is not first on PATH for this shell."
            } else {
                "PATH shim must prefer the Forge-owned wrapper before native CLI defaults can be intercepted."
            },
            vec![
                "forge".to_string(),
                "harness".to_string(),
                if shim_file_ready_for_activation {
                    "activation-profile".to_string()
                } else {
                    "install-shims".to_string()
                },
                "--shim-dir".to_string(),
                shim_dir_display.clone(),
                "--executor".to_string(),
                executor.clone(),
                "--project-root".to_string(),
                project_root_display.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        executor_surface_readiness(
            "harness_exec",
            if doctor.lineage_context_ready {
                "ready"
            } else {
                "blocked"
            },
            "harness_doctor.lineage_context_ready",
            "Real harness exec requires workflow, task and run lineage when project policy requires it.",
            vec![
                "forge".to_string(),
                "harness".to_string(),
                "exec".to_string(),
                "--executor".to_string(),
                executor.clone(),
                "--project-root".to_string(),
                project_root_display.clone(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--task".to_string(),
                "<task-id>".to_string(),
                "--run".to_string(),
                "<run-id>".to_string(),
                "--execute".to_string(),
                "--allow-exec".to_string(),
                "--".to_string(),
                family.native_entrypoint.clone(),
            ],
        ),
        executor_surface_readiness(
            "token_headroom",
            if doctor.token_headroom_ready {
                "ready"
            } else if wrapper_plan.require_token_headroom_for_forge_first && wrapper_plan.forge_first {
                "blocked"
            } else {
                "disabled"
            },
            "harness_doctor.token_headroom_ready",
            "Token headroom keeps tool output, logs and large context reversible without flooding the child brain.",
            vec![
                "forge".to_string(),
                "harness".to_string(),
                "headroom-plan".to_string(),
                "--executor".to_string(),
                executor.clone(),
                "--project-root".to_string(),
                project_root_display.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        executor_surface_readiness(
            "session_lifecycle",
            if session_lifecycle_plan.lineage_complete {
                "ready"
            } else {
                "blocked"
            },
            "session_lifecycle_plan.lineage_complete",
            "Opening or attaching brain shells should be recorded through Forge session lifecycle with workflow lineage.",
            vec![
                "forge".to_string(),
                "sessions".to_string(),
                "lifecycle".to_string(),
                "--session".to_string(),
                session_lifecycle_plan.session_id.clone(),
                "--state".to_string(),
                "opened".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        executor_surface_readiness(
            "project_policy",
            if doctor.mode.project_config_status == "loaded" {
                "ready"
            } else {
                "recommended"
            },
            "harness_mode.project_config_status",
            "Project .forge/harness.json makes Forge-first mode, token headroom and lineage requirements explicit.",
            vec![
                "forge".to_string(),
                "harness".to_string(),
                "bootstrap".to_string(),
                "--executor".to_string(),
                executor.clone(),
                "--shim-dir".to_string(),
                shim_dir_display,
                "--project-root".to_string(),
                project_root_display.clone(),
                "--apply".to_string(),
                "--approved-by".to_string(),
                "<operator>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        executor_surface_readiness(
            "context_routing",
            if routing_env_ready("FORGE_CONTEXT_ROUTING") {
                "ready"
            } else {
                "blocked"
            },
            "wrapper_plan.env.FORGE_CONTEXT_ROUTING",
            "Task context should be built by Forge context policy instead of an implicit CLI project scan.",
            vec!["forge".to_string(), "context".to_string(), "--output".to_string(), "json".to_string()],
        ),
        executor_surface_readiness(
            "memory_routing",
            if routing_env_ready("FORGE_MEMORY_ROUTING") {
                "ready"
            } else {
                "blocked"
            },
            "wrapper_plan.env.FORGE_MEMORY_ROUTING",
            "Memory reads remain scoped by Forge visibility, tenant and project policy.",
            vec![
                "forge".to_string(),
                "memory".to_string(),
                "policy".to_string(),
                "--project-root".to_string(),
                project_root_display.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        executor_surface_readiness(
            "skill_routing",
            if routing_env_ready("FORGE_SKILL_ROUTING") {
                "ready"
            } else {
                "blocked"
            },
            "wrapper_plan.env.FORGE_SKILL_ROUTING",
            "Skills are selected through Forge workflow/node capability context rather than hidden CLI defaults.",
            vec!["forge".to_string(), "interactive".to_string(), "readiness".to_string(), "--output".to_string(), "json".to_string()],
        ),
        executor_surface_readiness(
            "mcp_routing",
            if routing_env_ready("FORGE_MCP_ROUTING") {
                "ready"
            } else {
                "blocked"
            },
            "wrapper_plan.env.FORGE_MCP_ROUTING",
            "MCP tools remain routed by Forge capabilities, permissions and workflow state.",
            vec!["forge".to_string(), "mcp".to_string(), "tools".to_string(), "--output".to_string(), "json".to_string()],
        ),
        executor_surface_readiness(
            "credential_vault_boundary",
            if credential_boundary_ready {
                "ready"
            } else {
                "blocked"
            },
            "wrapper_plan.env.FORGE_CREDENTIAL_VAULT_BOUNDARY",
            "Credential vault values stay out of prompts; child CLIs receive only governed references or injected env at execution time.",
            vec!["forge".to_string(), "harness".to_string(), "doctor".to_string(), "--executor".to_string(), executor.clone(), "--output".to_string(), "json".to_string()],
        ),
        executor_surface_readiness(
            "event_receipts",
            if event_receipts_ready { "ready" } else { "blocked" },
            "wrapper_plan.env.FORGE_EVENT_RECEIPTS",
            "Guarded child work should emit Forge receipts for workflow, task and run lineage.",
            vec!["forge".to_string(), "events".to_string(), "tail".to_string(), "--output".to_string(), "json".to_string()],
        ),
        executor_surface_readiness(
            "cost_and_headroom_accounting",
            "ready",
            "headroom_stats",
            "Forge can account token-headroom savings and execution cost signals separately from the brain CLI.",
            vec![
                "forge".to_string(),
                "harness".to_string(),
                "headroom-stats".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
    ]
}

fn non_selected_harness_executor_readiness(
    family: &HarnessExecutorFamily,
    project_root: &Path,
    shim_dir: &Path,
) -> Vec<HarnessExecutorSurfaceReadiness> {
    let project_root_display = project_root.display().to_string();
    let shim_dir_display = shim_dir.display().to_string();
    harness_executor_supported_surfaces()
        .into_iter()
        .map(|surface| {
            let command = match surface.as_str() {
                "path_shim" => vec![
                    "forge".to_string(),
                    "harness".to_string(),
                    "doctor".to_string(),
                    "--executor".to_string(),
                    family.executor.clone(),
                    "--shim-dir".to_string(),
                    shim_dir_display.clone(),
                    "--project-root".to_string(),
                    project_root_display.clone(),
                    "--output".to_string(),
                    "json".to_string(),
                ],
                _ => vec![
                    "forge".to_string(),
                    "interactive".to_string(),
                    "harness".to_string(),
                    "--executor".to_string(),
                    family.executor.clone(),
                    "--project-root".to_string(),
                    project_root_display.clone(),
                    "--output".to_string(),
                    "json".to_string(),
                ],
            };
            executor_surface_readiness(
                &surface,
                "inspect_required",
                "not_selected_executor",
                "Select this executor to compute live Forge-first readiness for its shim, policy and lineage state.",
                command,
            )
        })
        .collect()
}

fn harness_executor_compatibility_status(
    selected: bool,
    readiness: &[HarnessExecutorSurfaceReadiness],
) -> String {
    if !selected {
        return "not_selected_inspect_required".to_string();
    }
    if readiness.iter().any(|item| item.status == "blocked") {
        "degraded".to_string()
    } else if readiness
        .iter()
        .any(|item| item.status == "recommended" || item.status == "disabled")
    {
        "ready_with_recommendations".to_string()
    } else {
        "ready".to_string()
    }
}

fn harness_executor_next_commands(
    family: &HarnessExecutorFamily,
    project_root: &Path,
    shim_dir: &Path,
    selected: bool,
    readiness: &[HarnessExecutorSurfaceReadiness],
) -> Vec<String> {
    let project_root_display = project_root.display().to_string();
    let shim_dir_display = shim_dir.display().to_string();
    if !selected {
        return vec![format!(
            "forge interactive harness --executor {} --shim-dir {} --project-root {} --output json",
            shell_quote(&family.executor),
            shell_quote(&shim_dir_display),
            shell_quote(&project_root_display)
        )];
    }

    let mut commands = Vec::new();
    if readiness.iter().any(|item| {
        item.surface == "path_shim"
            && item.status == "blocked"
            && item.source == "harness_doctor.shim_status.path_precedence"
    }) {
        commands.push(format!(
            "forge harness activation-profile --shim-dir {} --executor {} --project-root {} --output json",
            shell_quote(&shim_dir_display),
            shell_quote(&family.executor),
            shell_quote(&project_root_display)
        ));
    } else if readiness
        .iter()
        .any(|item| item.surface == "path_shim" && item.status == "blocked")
    {
        commands.push(format!(
            "forge harness install-shims --shim-dir {} --executor {} --project-root {} --output json",
            shell_quote(&shim_dir_display),
            shell_quote(&family.executor),
            shell_quote(&project_root_display)
        ));
    }
    if readiness
        .iter()
        .any(|item| item.surface == "harness_exec" && item.status == "blocked")
    {
        commands.push(
            "pass --workflow <workflow-id> --task <task-id> --run <run-id> before real harness exec"
                .to_string(),
        );
    }
    commands.push(format!(
        "forge harness wrap-plan --executor {} --project-root {} --output json",
        shell_quote(&family.executor),
        shell_quote(&project_root_display)
    ));
    commands.push(format!(
        "forge harness headroom-plan --executor {} --project-root {} --output json",
        shell_quote(&family.executor),
        shell_quote(&project_root_display)
    ));
    commands.push(format!(
        "forge sync executors --shim-dir {} --allow {} --output json",
        shell_quote(&shim_dir_display),
        shell_quote(&family.executor)
    ));
    commands
}

fn executor_surface_readiness(
    surface: &str,
    status: &str,
    source: &str,
    reason: &str,
    command: Vec<String>,
) -> HarnessExecutorSurfaceReadiness {
    HarnessExecutorSurfaceReadiness {
        surface: surface.to_string(),
        status: status.to_string(),
        source: source.to_string(),
        reason: reason.to_string(),
        command,
    }
}

fn canonical_harness_executor_families(selected_executor: &str) -> Vec<HarnessExecutorFamily> {
    let mut families = vec![
        harness_executor_family("codex"),
        harness_executor_family("claude"),
        harness_executor_family("gemini"),
        harness_executor_family("opencode"),
    ];
    if !families
        .iter()
        .any(|family| family.executor == selected_executor)
    {
        families.push(harness_executor_family(selected_executor));
    }
    families
}

fn harness_executor_family(executor: &str) -> HarnessExecutorFamily {
    let executor = normalize_executor(executor);
    HarnessExecutorFamily {
        display_name: harness_executor_display_name(&executor).to_string(),
        adapter_family: harness_executor_adapter_family(&executor).to_string(),
        native_entrypoint: harness_executor_native_entrypoint(&executor).to_string(),
        executor,
    }
}

fn harness_executor_display_name(executor: &str) -> &'static str {
    match executor {
        "codex" => "Codex CLI",
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "opencode" => "OpenCode CLI",
        _ => "Generic CLI",
    }
}

fn harness_executor_adapter_family(executor: &str) -> &'static str {
    match executor {
        "codex" => "codex_cli",
        "claude" => "claude_cli",
        "gemini" => "gemini_cli",
        "opencode" => "opencode_cli",
        _ => "generic_cli",
    }
}

fn harness_executor_native_entrypoint(executor: &str) -> &str {
    match executor {
        "codex" => "codex",
        "claude" => "claude",
        "gemini" => "gemini",
        "opencode" => "opencode",
        value => value,
    }
}

fn harness_executor_supported_surfaces() -> Vec<String> {
    [
        "env_overlay",
        "path_shim",
        "harness_exec",
        "token_headroom",
        "session_lifecycle",
        "project_policy",
        "context_routing",
        "memory_routing",
        "skill_routing",
        "mcp_routing",
        "credential_vault_boundary",
        "event_receipts",
        "cost_and_headroom_accounting",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

struct HarnessAdoptionStepInput<'a> {
    id: &'a str,
    title: &'a str,
    status: &'a str,
    command_key: &'a str,
    risk_level: &'a str,
    mutates_state: bool,
    executes_child: bool,
    requires_approval: bool,
    approval_reason: &'a str,
    rationale: &'a str,
}

fn harness_adoption_step(input: HarnessAdoptionStepInput<'_>) -> HarnessAdoptionStep {
    HarnessAdoptionStep {
        id: input.id.to_string(),
        title: input.title.to_string(),
        status: input.status.to_string(),
        command_key: input.command_key.to_string(),
        risk_level: input.risk_level.to_string(),
        mutates_state: input.mutates_state,
        executes_child: input.executes_child,
        requires_approval: input.requires_approval,
        approval_reason: input.approval_reason.to_string(),
        rationale: input.rationale.to_string(),
    }
}

pub fn build_harness_bootstrap_report(
    options: HarnessBootstrapOptions<'_>,
) -> Result<HarnessBootstrapReport> {
    let HarnessBootstrapOptions {
        shim_dir,
        executor,
        project_root,
        store_path,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        apply,
        approved_by,
        force,
    } = options;
    let executor = normalize_executor(executor);
    let project_root_path = project_root.to_path_buf();
    let shim_dir_path = shim_dir.to_path_buf();
    let project_root_display = project_root_path.display().to_string();
    let shim_dir_display = shim_dir_path.display().to_string();
    let recommended_project_config = HarnessRecommendedProjectConfig {
        default_mode: "forge_first".to_string(),
        default_context_budget: context_budget,
        default_token_headroom: true,
        require_token_headroom_for_forge_first: true,
        require_lineage_for_exec: true,
    };
    let adoption_plan = build_harness_adoption_plan(HarnessAdoptionPlanOptions {
        shim_dir: &shim_dir_path,
        executor: &executor,
        forge_first: true,
        observe_only: false,
        project_root: Some(&project_root_path),
        workflow_id: None,
        task_id: None,
        run_id: None,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first: true,
    })?;
    let config_path = project_root_path.join(".forge/harness.json");
    let existed_before = config_path.exists();
    let approved_by = approved_by
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    if apply && approved_by.is_none() {
        return Ok(HarnessBootstrapReport {
            schema_version: CLI_HARNESS_BOOTSTRAP_SCHEMA_VERSION.to_string(),
            status: "harness_bootstrap_blocked_missing_approval".to_string(),
            executor,
            project_root: project_root_display,
            shim_dir: shim_dir_display,
            apply,
            applied: false,
            mutates_state: false,
            would_mutate_state: true,
            approved_by,
            config_write: bootstrap_config_write_report(
                "blocked_missing_approval",
                &config_path,
                existed_before,
                false,
                None,
                recommended_project_config.clone(),
            ),
            adoption_plan,
            shim_install: None,
            next_commands: vec![
                "rerun with --apply --approved-by <operator> after reviewing the adoption plan"
                    .to_string(),
            ],
            notes: vec![
                "Bootstrap does not mutate project files or shims unless an approver is recorded."
                    .to_string(),
            ],
        });
    }

    if !apply {
        return Ok(HarnessBootstrapReport {
            schema_version: CLI_HARNESS_BOOTSTRAP_SCHEMA_VERSION.to_string(),
            status: "harness_bootstrap_planned".to_string(),
            executor,
            project_root: project_root_display,
            shim_dir: shim_dir_display,
            apply,
            applied: false,
            mutates_state: false,
            would_mutate_state: true,
            approved_by,
            config_write: bootstrap_config_write_report(
                "planned",
                &config_path,
                existed_before,
                false,
                None,
                recommended_project_config,
            ),
            adoption_plan,
            shim_install: None,
            next_commands: vec![
                "review adoption_plan before applying".to_string(),
                "forge harness bootstrap --executor <executor> --shim-dir <dir> --project-root <project-root> --apply --approved-by <operator> --output json".to_string(),
            ],
            notes: vec![
                "Dry-run is the default; no project config, shim or executor state is changed."
                    .to_string(),
            ],
        });
    }

    let config_write = write_harness_bootstrap_project_config(
        &project_root_path,
        &recommended_project_config,
        approved_by.as_deref(),
    )?;
    let shim_install = install_cli_harness_shim(CliShimInstallOptions {
        shim_dir: &shim_dir_path,
        executor: &executor,
        real_cmd: None,
        store_path,
        forge_first: true,
        forge_first_source: "bootstrap_default",
        workflow_id: None,
        task_id: None,
        run_id: None,
        context_budget,
        token_headroom,
        force,
    })?;
    let status = if shim_install.blocked_count > 0 {
        "harness_bootstrap_partially_applied"
    } else {
        "harness_bootstrap_applied"
    };
    Ok(HarnessBootstrapReport {
        schema_version: CLI_HARNESS_BOOTSTRAP_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        executor,
        project_root: project_root_display,
        shim_dir: shim_dir_display.clone(),
        apply,
        applied: shim_install.blocked_count == 0,
        mutates_state: true,
        would_mutate_state: true,
        approved_by,
        config_write,
        adoption_plan,
        shim_install: Some(shim_install),
        next_commands: vec![
            format!("export PATH={}:$PATH", shell_quote(&shim_dir_display)),
            format!(
                "forge sync executors --shim-dir {} --output json",
                shell_quote(&shim_dir_display)
            ),
            "forge harness doctor --executor <executor> --shim-dir <dir> --project-root <project-root> --output json".to_string(),
        ],
        notes: vec![
            "Bootstrap wrote the project harness policy before installing Forge-owned shims."
                .to_string(),
            "The native CLI remains the executable behind the shim; Forge only controls entry, context, lineage and headroom policy.".to_string(),
        ],
    })
}

fn write_harness_bootstrap_project_config(
    project_root: &Path,
    config: &HarnessRecommendedProjectConfig,
    approved_by: Option<&str>,
) -> Result<HarnessBootstrapConfigWrite> {
    let forge_dir = project_root.join(".forge");
    fs::create_dir_all(&forge_dir).with_context(|| {
        format!(
            "failed to create Forge project dir `{}`",
            forge_dir.display()
        )
    })?;
    let path = forge_dir.join("harness.json");
    let existed_before = path.exists();
    let mut existing = if existed_before {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read harness config `{}`", path.display()))?;
        serde_json::from_str::<serde_json::Map<String, Value>>(&content)
            .with_context(|| format!("failed to parse harness config `{}`", path.display()))?
    } else {
        serde_json::Map::new()
    };
    existing.insert(
        "default_mode".to_string(),
        Value::String(config.default_mode.clone()),
    );
    existing.insert(
        "default_context_budget".to_string(),
        json!(config.default_context_budget),
    );
    existing.insert(
        "default_token_headroom".to_string(),
        json!(config.default_token_headroom),
    );
    existing.insert(
        "require_token_headroom_for_forge_first".to_string(),
        json!(config.require_token_headroom_for_forge_first),
    );
    existing.insert(
        "require_lineage_for_exec".to_string(),
        json!(config.require_lineage_for_exec),
    );
    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(&Value::Object(existing))?
    );
    fs::write(&path, content)
        .with_context(|| format!("failed to write harness config `{}`", path.display()))?;
    Ok(bootstrap_config_write_report(
        "written",
        &path,
        existed_before,
        true,
        approved_by,
        config.clone(),
    ))
}

fn bootstrap_config_write_report(
    status: &str,
    path: &Path,
    existed_before: bool,
    applied: bool,
    approved_by: Option<&str>,
    config: HarnessRecommendedProjectConfig,
) -> HarnessBootstrapConfigWrite {
    HarnessBootstrapConfigWrite {
        schema_version: CLI_HARNESS_BOOTSTRAP_CONFIG_WRITE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        path: path.display().to_string(),
        existed_before,
        applied,
        approved_by: approved_by.map(ToString::to_string),
        config,
        notes: vec![
            "Project harness config controls Forge-first defaults, token headroom and lineage policy."
                .to_string(),
            "Bootstrap preserves unrelated JSON keys when updating an existing harness config."
                .to_string(),
        ],
    }
}

pub fn resolve_harness_forge_first_source(
    flag_forge_first: bool,
    flag_observe_only: bool,
) -> (bool, &'static str) {
    resolve_harness_forge_first_source_for_project(flag_forge_first, flag_observe_only, None)
}

pub fn resolve_harness_forge_first_source_for_project(
    flag_forge_first: bool,
    flag_observe_only: bool,
    project_root: Option<&Path>,
) -> (bool, &'static str) {
    let mode = resolve_harness_forge_first(flag_forge_first, flag_observe_only, project_root);
    (mode.forge_first, mode.source)
}

pub fn resolve_harness_runtime_policy(
    options: HarnessRuntimePolicyOptions<'_>,
) -> HarnessRuntimePolicy {
    let project_policy = options
        .project_root
        .map(read_harness_project_runtime_policy);
    let (context_budget, context_budget_source) = options
        .context_budget
        .filter(|budget| *budget > 0)
        .map(|budget| (budget, options.context_budget_source.to_string()))
        .or_else(|| {
            project_policy
                .as_ref()
                .and_then(|policy| policy.context_budget)
                .map(|budget| (budget, "project_config".to_string()))
        })
        .unwrap_or_else(|| (options.default_context_budget, "default".to_string()));
    let (mut token_headroom, mut token_headroom_source) = options
        .token_headroom
        .map(|enabled| (enabled, options.token_headroom_source.to_string()))
        .or_else(|| {
            project_policy
                .as_ref()
                .and_then(|policy| policy.token_headroom)
                .map(|enabled| (enabled, "project_config".to_string()))
        })
        .unwrap_or_else(|| (true, "default_enabled".to_string()));
    let require_token_headroom_for_forge_first = project_policy
        .as_ref()
        .is_some_and(|policy| policy.require_token_headroom_for_forge_first);
    if options.forge_first && require_token_headroom_for_forge_first && !token_headroom {
        token_headroom = true;
        token_headroom_source = "project_policy_required_for_forge_first".to_string();
    }
    HarnessRuntimePolicy {
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    }
}

pub fn build_cli_wrapper_plan(options: CliWrapperPlanOptions<'_>) -> CliWrapperPlanReport {
    let CliWrapperPlanOptions {
        executor,
        command,
        forge_first,
        forge_first_source,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    } = options;
    let executor = normalize_executor(executor);
    let forge_first_source = normalize_harness_mode_source(forge_first_source, forge_first);
    let command = if command.is_empty() {
        vec![executor.clone()]
    } else {
        command.to_vec()
    };
    let headroom_runtime_plan = build_harness_headroom_runtime_plan(
        &executor,
        token_headroom,
        context_budget,
        require_token_headroom_for_forge_first,
    );
    let mut env =
        vec![
        env_var(
            "FORGE_HARNESS",
            "enabled",
            "marks the child process as running under Forge harness control",
        ),
        env_var(
            "FORGE_HARNESS_MODE",
            if forge_first { "forge_first" } else { "observe_only" },
            "controls whether Forge context routing is preferred before native CLI defaults",
        ),
        env_var(
            "FORGE_HARNESS_MODE_SOURCE",
            &forge_first_source,
            "records which CLI/API input selected the harness mode",
        ),
        env_var(
            "FORGE_CONTEXT_BUDGET",
            &context_budget.to_string(),
            "bounds task-local context before a brain CLI receives it",
        ),
        env_var(
            "FORGE_TOKEN_HEADROOM",
            if token_headroom { "enabled" } else { "disabled" },
            "enables Forge's local token-headroom contract for tool output and context payloads",
        ),
    ];
    env.extend(headroom_runtime_plan.env.clone());
    if let Some(workflow_id) = workflow_id.filter(|value| !value.trim().is_empty()) {
        env.push(env_var(
            "FORGE_WORKFLOW_ID",
            workflow_id,
            "binds CLI execution to a Forge workflow lineage",
        ));
    }
    if let Some(task_id) = task_id.filter(|value| !value.trim().is_empty()) {
        env.push(env_var(
            "FORGE_TASK_ID",
            task_id,
            "binds CLI execution to a Forge task/node lineage",
        ));
    }
    if let Some(run_id) = run_id.filter(|value| !value.trim().is_empty()) {
        env.push(env_var(
            "FORGE_RUN_ID",
            run_id,
            "binds CLI execution to a Forge async run lineage",
        ));
    }
    if executor == "claude" {
        env.push(env_var(
            "ENABLE_TOOL_SEARCH",
            "true",
            "keeps Claude tool loading deferred when a wrapper changes its environment",
        ));
    }
    let orchestration_env = harness_orchestration_env(token_headroom);
    env.extend(orchestration_env.clone());

    let mut launch_command = vec![
        "forge".to_string(),
        "harness".to_string(),
        "exec".to_string(),
        "--executor".to_string(),
        executor.clone(),
    ];
    if forge_first {
        launch_command.push("--forge-first".to_string());
    }
    if let Some(workflow_id) = workflow_id.filter(|value| !value.trim().is_empty()) {
        launch_command.push("--workflow".to_string());
        launch_command.push(workflow_id.to_string());
    }
    if let Some(task_id) = task_id.filter(|value| !value.trim().is_empty()) {
        launch_command.push("--task".to_string());
        launch_command.push(task_id.to_string());
    }
    if let Some(run_id) = run_id.filter(|value| !value.trim().is_empty()) {
        launch_command.push("--run".to_string());
        launch_command.push(run_id.to_string());
    }
    launch_command.push("--context-budget".to_string());
    launch_command.push(context_budget.to_string());
    launch_command.push(if token_headroom {
        "--token-headroom".to_string()
    } else {
        "--no-token-headroom".to_string()
    });
    launch_command.push("--".to_string());
    launch_command.extend(command.clone());

    let mut harness_checks = vec![
        "resolve real CLI before PATH shim precedence".to_string(),
        "prepend Forge shim directory only for the child process".to_string(),
        "record argv, cwd, workflow/task/run lineage, token-headroom metrics and timeline event evidence".to_string(),
        "plan shell session lifecycle events before a brain CLI is opened, attached or closed".to_string(),
        "persist reversible headroom blobs in the Forge store when compression is applied".to_string(),
        "fall back to observe_only when Forge context is unavailable".to_string(),
    ];
    if require_token_headroom_for_forge_first {
        harness_checks.push(
            "project policy requires token headroom for Forge-first CLI execution".to_string(),
        );
    }
    let session_lifecycle_plan =
        build_harness_session_lifecycle_plan(&executor, workflow_id, task_id, run_id);
    let orchestration_contract = build_harness_orchestration_contract(
        &executor,
        forge_first,
        workflow_id,
        task_id,
        run_id,
        orchestration_env,
    );

    CliWrapperPlanReport {
        schema_version: CLI_WRAPPER_PLAN_SCHEMA_VERSION.to_string(),
        status: "cli_wrapper_plan_ready".to_string(),
        executor: executor.clone(),
        command,
        forge_first,
        forge_first_source,
        workflow_id: normalize_optional_text(workflow_id),
        task_id: normalize_optional_text(task_id),
        run_id: normalize_optional_text(run_id),
        wrapper_strategy: "env_overlay_with_forge_context_and_token_headroom".to_string(),
        context_budget,
        context_budget_source: context_budget_source.to_string(),
        token_headroom_enabled: token_headroom,
        token_headroom_source: token_headroom_source.to_string(),
        require_token_headroom_for_forge_first,
        env,
        launch_command,
        orchestration_contract,
        headroom_runtime_plan,
        session_lifecycle_plan,
        harness_checks,
        notes: vec![
            "Headroom-inspired ideas absorbed: local-first compression, reversible retrieval refs, CLI wrapper env shaping, tool-search preservation and shim-based harness tests".to_string(),
            "This plan is non-destructive; actual exec remains a separate guarded harness action".to_string(),
            "Session lifecycle commands are plan-only and must be recorded through Forge before relying on external brain shell state.".to_string(),
        ],
    }
}

fn harness_orchestration_env(token_headroom: bool) -> Vec<CliWrapperEnvVar> {
    vec![
        env_var(
            "FORGE_PROMPT_PACKET_REQUIRED",
            "true",
            "requires Forge-owned prompt packets with organization, personality and company-work decisions before brain execution",
        ),
        env_var(
            "FORGE_CONTEXT_ROUTING",
            "forge_controlled",
            "routes task context through Forge context policy instead of the child CLI's implicit project scan",
        ),
        env_var(
            "FORGE_MEMORY_ROUTING",
            "forge_controlled",
            "routes memory lookup through Forge memory governance and visibility policy",
        ),
        env_var(
            "FORGE_SKILL_ROUTING",
            "forge_controlled",
            "routes skill selection through Forge-owned workflow and node capability context",
        ),
        env_var(
            "FORGE_MCP_ROUTING",
            "forge_controlled",
            "routes MCP/tool availability through Forge capability, permission and workflow state",
        ),
        env_var(
            "FORGE_CREDENTIAL_VAULT_BOUNDARY",
            "reference_only",
            "keeps credential-vault values outside prompts and passes only governed references to child CLIs",
        ),
        env_var(
            "FORGE_TOKEN_HEADROOM_REQUIRED",
            if token_headroom { "true" } else { "false" },
            "declares whether large context, logs and tool output must use Forge token-headroom routing",
        ),
        env_var(
            "FORGE_SESSION_LIFECYCLE",
            "audited",
            "requires shell launch/opened/attached/closed state to be recorded through Forge session lifecycle",
        ),
        env_var(
            "FORGE_EVENT_RECEIPTS",
            "required",
            "requires workflow/task/run lineage and receipt events for guarded child process execution",
        ),
    ]
}

fn build_harness_headroom_runtime_plan(
    executor: &str,
    enabled: bool,
    context_budget: usize,
    require_for_forge_first: bool,
) -> HarnessHeadroomRuntimePlan {
    let mode = if enabled {
        "compress_reference_and_retrieve"
    } else {
        "observe_only"
    };
    HarnessHeadroomRuntimePlan {
        schema_version: CLI_HARNESS_HEADROOM_RUNTIME_PLAN_SCHEMA_VERSION.to_string(),
        status: if enabled {
            "headroom_runtime_plan_ready".to_string()
        } else {
            "headroom_runtime_plan_disabled".to_string()
        },
        enabled,
        executor: executor.to_string(),
        mode: mode.to_string(),
        context_budget,
        require_for_forge_first,
        interception_points: vec![
            headroom_interception_point(
                "prompt_packet",
                "forge_context_packet",
                "child_cli_prompt",
                "ingress",
                enabled,
                "preserve_policy_fields_and_compress_large_context",
            ),
            headroom_interception_point(
                "tool_output",
                "child_cli_tool_output",
                "brain_context",
                "egress",
                enabled,
                "compress_then_return_retrieval_ref",
            ),
            headroom_interception_point(
                "stdout_stderr",
                "guarded_harness_exec",
                "forge_event_receipt",
                "egress",
                enabled,
                "compress_output_and_store_reversible_blob",
            ),
            headroom_interception_point(
                "retrieval_request",
                "brain_or_operator",
                "forge_headroom_store",
                "on_demand",
                enabled,
                "retrieve_original_by_forge_headroom_ref",
            ),
        ],
        content_routes: vec![
            headroom_content_route(
                "log",
                "error_warning_panic_detector",
                "signal_log_compressor",
                "local_sqlite_when_persisted",
            ),
            headroom_content_route(
                "json",
                "serde_json_shape_detector",
                "smart_json_shape_summary",
                "local_sqlite_when_persisted",
            ),
            headroom_content_route(
                "search",
                "colon_dense_search_result_detector",
                "search_result_compressor",
                "local_sqlite_when_persisted",
            ),
            headroom_content_route(
                "code",
                "signature_keyword_detector",
                "code_signature_compressor",
                "local_sqlite_when_persisted",
            ),
            headroom_content_route(
                "text",
                "fallback_text_detector",
                "text_head_tail_summary",
                "local_sqlite_when_persisted",
            ),
        ],
        reversible_store: HarnessHeadroomReversibleStore {
            backend: "forge_store_headroom_blobs".to_string(),
            uri_scheme: "forge://harness/headroom/".to_string(),
            persistence_mode: "explicit_persist_or_guarded_exec_receipt".to_string(),
            retrieval_command: vec![
                "forge".to_string(),
                "harness".to_string(),
                "retrieve-headroom".to_string(),
                "--ref".to_string(),
                "<retrieval-ref>".to_string(),
                "--include-content".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            ttl_policy: "store_retention_policy_controls_lifetime".to_string(),
        },
        mcp_tools: vec![
            "forge.harness.token_headroom".to_string(),
            "forge.harness.retrieve_headroom".to_string(),
            "forge.harness.headroom_stats".to_string(),
        ],
        env: vec![
            env_var(
                "FORGE_HEADROOM_RUNTIME_PLAN",
                CLI_HARNESS_HEADROOM_RUNTIME_PLAN_SCHEMA_VERSION,
                "declares the structured wrapper interception and retrieval contract",
            ),
            env_var(
                "FORGE_HEADROOM_INTERCEPT",
                if enabled { "enabled" } else { "disabled" },
                "controls whether wrapper integrations must compress large payloads before returning them to the brain",
            ),
            env_var(
                "FORGE_HEADROOM_RETRIEVAL_TOOL",
                "forge.harness.retrieve_headroom",
                "names the MCP/CLI retrieval surface for reversible headroom refs",
            ),
        ],
        notes: vec![
            "This plan is declarative: it does not proxy network traffic or start a child CLI by itself."
                .to_string(),
            "It adapts Headroom-style wrapper and reversible retrieval concepts to Forge-owned workflow lineage, memory policy and event receipts.".to_string(),
        ],
    }
}

fn headroom_interception_point(
    point_id: &str,
    source: &str,
    target: &str,
    direction: &str,
    required: bool,
    action: &str,
) -> HarnessHeadroomInterceptionPoint {
    HarnessHeadroomInterceptionPoint {
        point_id: point_id.to_string(),
        source: source.to_string(),
        target: target.to_string(),
        direction: direction.to_string(),
        required,
        action: action.to_string(),
    }
}

fn headroom_content_route(
    content_kind: &str,
    detector: &str,
    strategy: &str,
    persistence: &str,
) -> HarnessHeadroomContentRoute {
    HarnessHeadroomContentRoute {
        content_kind: content_kind.to_string(),
        detector: detector.to_string(),
        strategy: strategy.to_string(),
        reversible: true,
        persistence: persistence.to_string(),
        retrieval_hint: "return forge://harness/headroom/<sha256> when original content is needed"
            .to_string(),
    }
}

fn build_harness_orchestration_contract(
    executor: &str,
    forge_first: bool,
    workflow_id: Option<&str>,
    task_id: Option<&str>,
    run_id: Option<&str>,
    required_env: Vec<CliWrapperEnvVar>,
) -> HarnessOrchestrationContract {
    HarnessOrchestrationContract {
        schema_version: CLI_HARNESS_ORCHESTRATION_CONTRACT_SCHEMA_VERSION.to_string(),
        status: "orchestration_contract_ready".to_string(),
        control_plane: "forge_core".to_string(),
        executor: executor.to_string(),
        forge_first,
        workflow_id: normalize_optional_text(workflow_id),
        task_id: normalize_optional_text(task_id),
        run_id: normalize_optional_text(run_id),
        required_env,
        routing_stages: vec![
            orchestration_stage(
                "prompt_packet",
                "goal/workflow/node context",
                "child brain prompt",
                "Forge supplies organization, brand, product, personality and company-work decisions before the CLI sees task context.",
            ),
            orchestration_stage(
                "context_router",
                "Forge context engine",
                "bounded task packet",
                "Context is selected by workflow/task policy rather than by unrestricted project scanning.",
            ),
            orchestration_stage(
                "memory_router",
                "Forge memory governance",
                "scoped memory snippets",
                "Memory access stays tenant-bound, audience-gated and retrieval-based.",
            ),
            orchestration_stage(
                "skill_router",
                "Forge capability registry",
                "executor skill surface",
                "Skills are selected from workflow and node capability needs, not from global CLI defaults.",
            ),
            orchestration_stage(
                "mcp_router",
                "Forge tool registry",
                "allowed tool surface",
                "MCP/tool access remains permissioned, auditable and replaceable per node brain.",
            ),
            orchestration_stage(
                "credential_vault_boundary",
                "Forge credential references",
                "child process environment",
                "Secrets cross the boundary only through approved vault references or injected env, never through prompt text.",
            ),
            orchestration_stage(
                "token_headroom",
                "Forge harness headroom",
                "logs, context and stdout/stderr",
                "Large payloads are compressed locally with reversible retrieval refs before returning to a brain.",
            ),
            orchestration_stage(
                "session_lifecycle",
                "Forge session registry",
                "brain shell lifecycle",
                "Shell launch/opened/attached/closed transitions stay auditable even when a human drives the CLI.",
            ),
            orchestration_stage(
                "event_receipt",
                "Forge event timeline",
                "workflow observability",
                "Guarded CLI work emits receipts tied to workflow, task and run lineage.",
            ),
        ],
        gates: vec![
            "prompt_packet_required".to_string(),
            "context_budget_enforced".to_string(),
            "memory_policy_respected".to_string(),
            "credential_values_not_prompted".to_string(),
            "token_headroom_applied_when_required".to_string(),
            "session_lifecycle_recordable".to_string(),
            "event_receipts_recordable".to_string(),
        ],
        notes: vec![
            "This contract makes external CLIs execution brains; Forge remains the workflow, routing, memory, permission and observability control plane.".to_string(),
            "The contract is read-only planning data and does not launch or intercept a child process by itself.".to_string(),
        ],
    }
}

fn orchestration_stage(
    id: &str,
    source: &str,
    target: &str,
    rationale: &str,
) -> HarnessOrchestrationStage {
    HarnessOrchestrationStage {
        id: id.to_string(),
        owner: "forge".to_string(),
        source: source.to_string(),
        target: target.to_string(),
        required: true,
        rationale: rationale.to_string(),
    }
}

fn build_harness_session_lifecycle_plan(
    executor: &str,
    workflow_id: Option<&str>,
    task_id: Option<&str>,
    run_id: Option<&str>,
) -> HarnessSessionLifecyclePlan {
    let workflow_id = normalize_optional_text(workflow_id);
    let task_id = normalize_optional_text(task_id);
    let run_id = normalize_optional_text(run_id);
    let lineage_complete = workflow_id.is_some() && task_id.is_some() && run_id.is_some();
    let mut missing_lineage = Vec::new();
    if workflow_id.is_none() {
        missing_lineage.push("workflow_id".to_string());
    }
    if task_id.is_none() {
        missing_lineage.push("task_id".to_string());
    }
    if run_id.is_none() {
        missing_lineage.push("run_id".to_string());
    }
    let session_id = harness_session_id(executor);
    let workflow_arg = workflow_id
        .clone()
        .unwrap_or_else(|| "<workflow-id>".to_string());
    let task_arg = task_id.clone().unwrap_or_else(|| "<task-id>".to_string());
    let run_arg = run_id.clone().unwrap_or_else(|| "<run-id>".to_string());
    let lifecycle_status = if lineage_complete {
        "available_with_lineage"
    } else {
        "blocked_until_lineage"
    };
    let lifecycle_context = HarnessSessionLifecycleCommandContext {
        session_id: &session_id,
        workflow_id: &workflow_arg,
        task_id: &task_arg,
        run_id: &run_arg,
    };

    HarnessSessionLifecyclePlan {
        schema_version: CLI_HARNESS_SESSION_LIFECYCLE_PLAN_SCHEMA_VERSION.to_string(),
        status: "session_lifecycle_plan_ready".to_string(),
        executor: executor.to_string(),
        session_id: session_id.clone(),
        workflow_id,
        task_id,
        run_id,
        lineage_complete,
        missing_lineage,
        gates: vec![
            HarnessSessionLifecycleGate {
                gate_id: "record_launch_plan".to_string(),
                title: "Record shell launch intent".to_string(),
                state: "planned".to_string(),
                status: "available".to_string(),
                command: vec![
                    "forge".to_string(),
                    "shells".to_string(),
                    "--executor".to_string(),
                    executor.to_string(),
                    "--workflow".to_string(),
                    workflow_arg.clone(),
                    "--task".to_string(),
                    task_arg.clone(),
                    "--run".to_string(),
                    run_arg.clone(),
                    "--record-session".to_string(),
                    "--origin".to_string(),
                    "forge_harness".to_string(),
                    "--output".to_string(),
                    "json".to_string(),
                ],
                mutates_workflow: false,
                records_event: true,
                rationale: "External brain shells should have auditable launch intent before a human or workflow opens them."
                    .to_string(),
            },
            harness_session_lifecycle_gate(
                "record_opened",
                "Record shell opened",
                "opened",
                lifecycle_status,
                &lifecycle_context,
            ),
            harness_session_lifecycle_gate(
                "record_attached",
                "Record shell attached",
                "attached",
                lifecycle_status,
                &lifecycle_context,
            ),
            harness_session_lifecycle_gate(
                "record_closed",
                "Record shell closed",
                "closed",
                lifecycle_status,
                &lifecycle_context,
            ),
        ],
        notes: vec![
            "Lifecycle plan is read-only and does not open, attach or close child shells by itself."
                .to_string(),
            "Use concrete workflow/task/run lineage before recording lifecycle transitions for production handoff."
                .to_string(),
        ],
    }
}

struct HarnessSessionLifecycleCommandContext<'a> {
    session_id: &'a str,
    workflow_id: &'a str,
    task_id: &'a str,
    run_id: &'a str,
}

fn harness_session_lifecycle_gate(
    gate_id: &str,
    title: &str,
    state: &str,
    status: &str,
    lifecycle_context: &HarnessSessionLifecycleCommandContext<'_>,
) -> HarnessSessionLifecycleGate {
    HarnessSessionLifecycleGate {
        gate_id: gate_id.to_string(),
        title: title.to_string(),
        state: state.to_string(),
        status: status.to_string(),
        command: vec![
            "forge".to_string(),
            "sessions".to_string(),
            "lifecycle".to_string(),
            "--session".to_string(),
            lifecycle_context.session_id.to_string(),
            "--state".to_string(),
            state.to_string(),
            "--workflow".to_string(),
            lifecycle_context.workflow_id.to_string(),
            "--task".to_string(),
            lifecycle_context.task_id.to_string(),
            "--run".to_string(),
            lifecycle_context.run_id.to_string(),
            "--origin".to_string(),
            "forge_harness".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        mutates_workflow: false,
        records_event: true,
        rationale: format!(
            "Record the `{state}` lifecycle transition in Forge-owned session history without starting child processes."
        ),
    }
}

fn harness_session_id(executor: &str) -> String {
    if executor == "forge" {
        "forge-tui".to_string()
    } else {
        format!("{executor}-shell")
    }
}

pub fn install_cli_harness_shim(
    options: CliShimInstallOptions<'_>,
) -> Result<CliShimInstallReport> {
    let CliShimInstallOptions {
        shim_dir,
        executor,
        real_cmd,
        store_path,
        forge_first,
        forge_first_source,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        token_headroom,
        force,
    } = options;
    let executor = normalize_executor(executor);
    let forge_first_source = normalize_harness_mode_source(forge_first_source, forge_first);
    fs::create_dir_all(shim_dir)
        .with_context(|| format!("failed to create shim dir `{}`", shim_dir.display()))?;
    let shim_dir = shim_dir
        .canonicalize()
        .unwrap_or_else(|_| shim_dir.to_path_buf());
    let real_command = resolve_real_command_for_shim(&executor, real_cmd, &shim_dir)?;
    let current_exe = env::current_exe().context("failed to resolve current forge binary")?;
    let forge_binary = current_exe
        .canonicalize()
        .unwrap_or(current_exe)
        .display()
        .to_string();
    let shim_path = shim_dir.join(shim_binary_name(&executor));
    let script = build_cli_shim_script(CliShimScriptOptions {
        forge_binary: &forge_binary,
        executor: &executor,
        real_cmd: &real_command.command,
        store_path,
        forge_first,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        token_headroom,
    });
    let script_sha256 = hex_sha256(script.as_bytes());
    let mut installed_count = 0usize;
    let mut updated_count = 0usize;
    let mut blocked_count = 0usize;
    let status = if shim_path.exists() {
        let existing = fs::read_to_string(&shim_path).unwrap_or_default();
        if force || existing.contains(CLI_SHIM_MARKER) {
            fs::write(&shim_path, script.as_bytes())
                .with_context(|| format!("failed to update shim `{}`", shim_path.display()))?;
            make_executable(&shim_path)?;
            updated_count += 1;
            "updated"
        } else {
            blocked_count += 1;
            "blocked_existing_file"
        }
    } else {
        fs::write(&shim_path, script.as_bytes())
            .with_context(|| format!("failed to write shim `{}`", shim_path.display()))?;
        make_executable(&shim_path)?;
        installed_count += 1;
        "installed"
    };
    let overall_status = if blocked_count > 0 {
        "shim_install_blocked"
    } else {
        "shim_install_ready"
    };
    let shim = CliShimReport {
        executor,
        shim_path: shim_path.display().to_string(),
        real_command: real_command.command,
        real_command_source: real_command.source,
        real_command_resolution_status: real_command.status,
        store_path: store_path.map(|path| path.display().to_string()),
        forge_binary: forge_binary.clone(),
        forge_first,
        forge_first_source: forge_first_source.clone(),
        context_budget,
        token_headroom,
        status: status.to_string(),
        script_sha256: (status != "blocked_existing_file").then_some(script_sha256),
        argv_policy: "preserve_user_argv_after_resolved_real_cli".to_string(),
        safety_checks: vec![
            "shim directory is explicit and must be added to PATH by the caller".to_string(),
            "existing non-Forge shim files are not overwritten unless --force is used".to_string(),
            "real CLI command is captured before the shim can take PATH precedence".to_string(),
            "shim delegates to forge harness exec with --execute and --allow-exec".to_string(),
        ],
        notes: vec![
            "This installs a Forge-owned CLI shim, not a replacement for the native CLI binary."
                .to_string(),
            "Put the shim directory before the native CLI directory only in shells that should prefer Forge infrastructure.".to_string(),
        ],
    };

    Ok(CliShimInstallReport {
        schema_version: CLI_SHIM_INSTALL_SCHEMA_VERSION.to_string(),
        status: overall_status.to_string(),
        shim_dir: shim_dir.display().to_string(),
        store_path: store_path.map(|path| path.display().to_string()),
        forge_binary,
        forge_first,
        forge_first_source,
        context_budget,
        token_headroom,
        force,
        installed_count,
        updated_count,
        blocked_count,
        shims: vec![shim],
        instructions: vec![
            format!(
                "export PATH={}:$PATH",
                shell_quote(&shim_dir.display().to_string())
            ),
            "verify the real CLI path before putting the shim directory first in PATH".to_string(),
            "rerun with --force only when the existing file is disposable or Forge-owned"
                .to_string(),
        ],
    })
}

pub fn inspect_cli_harness_shim_status(
    options: CliShimStatusOptions<'_>,
) -> Result<CliShimStatusReport> {
    let executor = normalize_executor(options.executor);
    let shim_dir = options
        .shim_dir
        .canonicalize()
        .unwrap_or_else(|_| options.shim_dir.to_path_buf());
    let shim_path = shim_dir.join(shim_binary_name(&executor));
    let shim_exists = shim_path.is_file();
    let shim_content = if shim_exists {
        Some(
            fs::read_to_string(&shim_path)
                .with_context(|| format!("failed to read shim `{}`", shim_path.display()))?,
        )
    } else {
        None
    };
    let forge_owned = shim_content
        .as_deref()
        .is_some_and(|content| content.contains(CLI_SHIM_MARKER));
    let executable = shim_exists && is_executable(&shim_path);
    let path_entry_index = path_entry_index(&shim_dir);
    let path_resolution = resolve_executable_from_path(&shim_binary_name(&executor));
    let resolved_is_shim = path_resolution
        .path
        .as_deref()
        .is_some_and(|path| same_path(Path::new(path), &shim_path));
    let path_precedence = match (&path_resolution.path, path_entry_index, resolved_is_shim) {
        (None, _, _) => "missing_from_path",
        (Some(_), Some(_), true) if forge_owned => "shim_first",
        (Some(_), Some(_), true) => "manual_shim_first",
        (Some(_), Some(_), false) => "native_first",
        (Some(_), None, _) => "shim_not_on_path",
    }
    .to_string();

    let parsed_script = shim_content.as_deref().and_then(parse_cli_shim_script);
    let fallback_real_command = if parsed_script
        .as_ref()
        .and_then(|script| script.real_command.as_ref())
        .is_none()
    {
        resolve_real_command_for_status(&executor, &shim_dir)
    } else {
        None
    };
    let real_command = parsed_script
        .as_ref()
        .and_then(|script| script.real_command.clone())
        .or_else(|| {
            fallback_real_command
                .as_ref()
                .map(|resolution| resolution.command.clone())
        });
    let (real_command_source, real_command_resolution_status) = if parsed_script
        .as_ref()
        .and_then(|script| script.real_command.as_ref())
        .is_some()
    {
        (
            "shim_script".to_string(),
            "parsed_from_forge_shim".to_string(),
        )
    } else if let Some(resolution) = &fallback_real_command {
        (resolution.source.clone(), resolution.status.clone())
    } else if shim_exists {
        (
            "unresolved".to_string(),
            "real_command_unresolved".to_string(),
        )
    } else {
        ("unresolved".to_string(), "shim_missing".to_string())
    };
    let real_command_is_shim = real_command
        .as_deref()
        .is_some_and(|command| same_path(Path::new(command), &shim_path));
    let would_recurse = real_command_is_shim || (resolved_is_shim && !forge_owned);
    let status = if !shim_exists {
        "shim_status_missing"
    } else if would_recurse {
        "shim_status_blocked"
    } else if forge_owned && executable && resolved_is_shim {
        "shim_status_ready"
    } else {
        "shim_status_degraded"
    };
    let activation_diagnostic = shim_activation_diagnostic(ShimActivationDiagnosticInput {
        status,
        path_precedence: &path_precedence,
        shim_dir: &shim_dir,
        executor: &executor,
        shim_exists,
        forge_owned,
        executable,
        would_recurse,
    });

    Ok(CliShimStatusReport {
        schema_version: CLI_SHIM_STATUS_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        shim_dir: shim_dir.display().to_string(),
        executor: executor.clone(),
        shim_path: shim_path.display().to_string(),
        shim_exists,
        forge_owned,
        executable,
        path_precedence,
        path_entry_index,
        resolved_path_from_path: path_resolution.path,
        real_command,
        real_command_source,
        real_command_resolution_status,
        store_path: parsed_script
            .as_ref()
            .and_then(|script| script.store_path.clone()),
        forge_binary: parsed_script
            .as_ref()
            .and_then(|script| script.forge_binary.clone()),
        would_recurse,
        activation_diagnostic,
        checks: shim_status_checks(
            shim_exists,
            forge_owned,
            executable,
            resolved_is_shim,
            would_recurse,
            path_resolution.status.as_str(),
        ),
        instructions: shim_status_instructions(status, &shim_dir, &executor),
        notes: vec![
            "Shim status is an audit report; it does not create, overwrite or execute CLI binaries."
                .to_string(),
            "Use this before relying on PATH precedence for Forge-first brain CLI operation."
                .to_string(),
        ],
    })
}

pub fn run_cli_harness_exec(options: CliHarnessExecOptions<'_>) -> Result<CliHarnessExecReceipt> {
    let CliHarnessExecOptions {
        store,
        executor,
        command,
        forge_first,
        forge_first_source,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
        dry_run,
        allow_exec,
        project_root,
        cwd,
    } = options;
    let wrapper_plan = build_cli_wrapper_plan(CliWrapperPlanOptions {
        executor,
        command,
        forge_first,
        forge_first_source,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        context_budget_source,
        token_headroom,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    });
    let command = wrapper_plan.command.clone();
    let cwd_path = cwd
        .map(Path::to_path_buf)
        .unwrap_or(env::current_dir().context("failed to read current directory")?);
    let policy_root = project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cwd_path.clone());
    let cwd_display = cwd_path.display().to_string();
    let (resolved_executable, resolution_status) = resolve_executable(command.first(), &cwd_path);
    let command_sha256 = hex_sha256(command.join("\0").as_bytes());
    let project_policy = read_harness_project_exec_policy(&policy_root);
    let project_policy_status =
        harness_project_exec_policy_status(&project_policy, dry_run, workflow_id, task_id, run_id);
    let mut safety_checks = vec![
        "dry_run is the default; real execution requires --execute and --allow-exec".to_string(),
        "resolved executable is recorded before running the child process".to_string(),
        "Forge env overlay is applied only to the child process".to_string(),
        "stdout and stderr are summarized by bytes, sha256 and bounded excerpts".to_string(),
        "workflow/task/run lineage, token-headroom settings and harness events stay explicit in the receipt".to_string(),
    ];
    if project_policy.require_lineage_for_exec {
        safety_checks.push("project_require_lineage_for_exec".to_string());
    }

    if dry_run {
        let mut receipt = exec_receipt(CliExecReceiptInput {
            wrapper_plan,
            command,
            command_sha256,
            cwd: cwd_display,
            forge_first,
            dry_run,
            allow_exec,
            execution_mode: "dry_run".to_string(),
            project_policy_path: project_policy.path.display().to_string(),
            project_policy_status: project_policy_status.to_string(),
            require_lineage_for_exec: project_policy.require_lineage_for_exec,
            resolved_executable,
            resolution_status,
            status: "harness_exec_dry_run".to_string(),
            safety_checks,
            executed: false,
            success: None,
            exit_code: None,
            stdout_bytes: None,
            stderr_bytes: None,
            stdout_sha256: None,
            stderr_sha256: None,
            stdout_excerpt: None,
            stderr_excerpt: None,
            output_headroom_enabled: token_headroom,
            stdout_headroom: None,
            stderr_headroom: None,
        });
        record_harness_exec_event_if_possible(store, workflow_id, task_id, run_id, &mut receipt)?;
        return Ok(receipt);
    }
    if !allow_exec {
        let mut receipt = exec_receipt(CliExecReceiptInput {
            wrapper_plan,
            command,
            command_sha256,
            cwd: cwd_display,
            forge_first,
            dry_run,
            allow_exec,
            execution_mode: "blocked".to_string(),
            project_policy_path: project_policy.path.display().to_string(),
            project_policy_status: project_policy_status.to_string(),
            require_lineage_for_exec: project_policy.require_lineage_for_exec,
            resolved_executable,
            resolution_status,
            status: "harness_exec_blocked_without_allow_exec".to_string(),
            safety_checks,
            executed: false,
            success: None,
            exit_code: None,
            stdout_bytes: None,
            stderr_bytes: None,
            stdout_sha256: None,
            stderr_sha256: None,
            stdout_excerpt: None,
            stderr_excerpt: None,
            output_headroom_enabled: token_headroom,
            stdout_headroom: None,
            stderr_headroom: None,
        });
        record_harness_exec_event_if_possible(store, workflow_id, task_id, run_id, &mut receipt)?;
        return Ok(receipt);
    }
    if project_policy_status == "lineage_required_missing" {
        let mut receipt = exec_receipt(CliExecReceiptInput {
            wrapper_plan,
            command,
            command_sha256,
            cwd: cwd_display,
            forge_first,
            dry_run,
            allow_exec,
            execution_mode: "blocked".to_string(),
            project_policy_path: project_policy.path.display().to_string(),
            project_policy_status: project_policy_status.to_string(),
            require_lineage_for_exec: project_policy.require_lineage_for_exec,
            resolved_executable,
            resolution_status,
            status: "harness_exec_blocked_by_project_policy".to_string(),
            safety_checks,
            executed: false,
            success: None,
            exit_code: None,
            stdout_bytes: None,
            stderr_bytes: None,
            stdout_sha256: None,
            stderr_sha256: None,
            stdout_excerpt: None,
            stderr_excerpt: None,
            output_headroom_enabled: token_headroom,
            stdout_headroom: None,
            stderr_headroom: None,
        });
        receipt.notes.push(
            "Project harness policy requires workflow, task and run lineage before real execution."
                .to_string(),
        );
        record_harness_exec_event_if_possible(store, workflow_id, task_id, run_id, &mut receipt)?;
        return Ok(receipt);
    }
    let Some(executable) = resolved_executable.clone() else {
        let mut receipt = exec_receipt(CliExecReceiptInput {
            wrapper_plan,
            command,
            command_sha256,
            cwd: cwd_display,
            forge_first,
            dry_run,
            allow_exec,
            execution_mode: "blocked".to_string(),
            project_policy_path: project_policy.path.display().to_string(),
            project_policy_status: project_policy_status.to_string(),
            require_lineage_for_exec: project_policy.require_lineage_for_exec,
            resolved_executable,
            resolution_status,
            status: "harness_exec_blocked_missing_executable".to_string(),
            safety_checks,
            executed: false,
            success: None,
            exit_code: None,
            stdout_bytes: None,
            stderr_bytes: None,
            stdout_sha256: None,
            stderr_sha256: None,
            stdout_excerpt: None,
            stderr_excerpt: None,
            output_headroom_enabled: token_headroom,
            stdout_headroom: None,
            stderr_headroom: None,
        });
        record_harness_exec_event_if_possible(store, workflow_id, task_id, run_id, &mut receipt)?;
        return Ok(receipt);
    };

    let mut child = Command::new(&executable);
    child.args(command.iter().skip(1));
    child.current_dir(&cwd_path);
    for env_var in &wrapper_plan.env {
        child.env(&env_var.name, &env_var.value);
    }
    let output = child
        .output()
        .with_context(|| format!("failed to execute harness child `{executable}`"))?;
    let success = output.status.success();
    let stdout_excerpt = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr_excerpt = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout_headroom = build_output_headroom_report(
        store,
        &wrapper_plan.executor,
        "stdout",
        &stdout_excerpt,
        context_budget,
        token_headroom,
    )?;
    let stderr_headroom = build_output_headroom_report(
        store,
        &wrapper_plan.executor,
        "stderr",
        &stderr_excerpt,
        context_budget,
        token_headroom,
    )?;
    let mut receipt = exec_receipt(CliExecReceiptInput {
        wrapper_plan,
        command,
        command_sha256,
        cwd: cwd_display,
        forge_first,
        dry_run,
        allow_exec,
        execution_mode: "guarded_exec".to_string(),
        project_policy_path: project_policy.path.display().to_string(),
        project_policy_status: project_policy_status.to_string(),
        require_lineage_for_exec: project_policy.require_lineage_for_exec,
        resolved_executable,
        resolution_status,
        status: if success {
            "harness_exec_completed"
        } else {
            "harness_exec_failed"
        }
        .to_string(),
        safety_checks,
        executed: true,
        success: Some(success),
        exit_code: output.status.code(),
        stdout_bytes: Some(output.stdout.len()),
        stderr_bytes: Some(output.stderr.len()),
        stdout_sha256: Some(hex_sha256(&output.stdout)),
        stderr_sha256: Some(hex_sha256(&output.stderr)),
        stdout_excerpt: Some(bounded_excerpt(&stdout_excerpt, 4000)),
        stderr_excerpt: Some(bounded_excerpt(&stderr_excerpt, 4000)),
        output_headroom_enabled: token_headroom,
        stdout_headroom,
        stderr_headroom,
    });
    record_harness_exec_event_if_possible(store, workflow_id, task_id, run_id, &mut receipt)?;
    Ok(receipt)
}

fn record_harness_exec_event_if_possible(
    store: Option<&ForgeStore>,
    workflow_id: Option<&str>,
    task_id: Option<&str>,
    run_id: Option<&str>,
    receipt: &mut CliHarnessExecReceipt,
) -> Result<()> {
    let Some(store) = store else {
        return Ok(());
    };
    if workflow_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
        && task_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        && run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Ok(());
    }
    let data = json!({
        "schema_version": CLI_HARNESS_EXEC_EVENT_SCHEMA_VERSION,
        "status": harness_event_status(&receipt.status),
        "task_id": task_id,
        "run_id": run_id,
        "executor": receipt.executor,
        "command_sha256": receipt.command_sha256,
        "receipt": receipt,
    });
    let source_id = format!(
        "harness_{}",
        &hex_sha256(serde_json::to_string(&data)?.as_bytes())[..16]
    );
    let tenant_context = harness_tenant_context(store, workflow_id)?;
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "forge_harness",
        source_id: &source_id,
        workflow_id,
        kind: &receipt.status,
        origin: "forge_harness",
        status: harness_event_status(&receipt.status),
        data: &data,
        tenant_context: &tenant_context,
    })?;
    receipt.event_recorded = true;
    receipt.global_event_id = Some(global_event_id);
    Ok(())
}

fn harness_tenant_context(store: &ForgeStore, workflow_id: Option<&str>) -> Result<Value> {
    if let Some(workflow_id) = workflow_id {
        if let Ok(workflow) = store.load_workflow(workflow_id) {
            return Ok(serde_json::to_value(&workflow.intent.operating_context)?);
        }
    }
    Ok(serde_json::to_value(OperatingContextSpec::default())?)
}

fn harness_event_status(status: &str) -> &'static str {
    match status {
        "harness_exec_completed" => "completed",
        "harness_exec_failed" => "failed",
        "harness_exec_dry_run" => "planned",
        status if status.starts_with("harness_exec_blocked") => "blocked",
        _ => "recorded",
    }
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_harness_mode_source(value: &str, forge_first: bool) -> String {
    let value = value.trim();
    if !value.is_empty() {
        return value.to_string();
    }
    if forge_first {
        "unspecified_forge_first".to_string()
    } else {
        "default_observe_only".to_string()
    }
}

fn resolve_harness_forge_first(
    flag_forge_first: bool,
    flag_observe_only: bool,
    project_root: Option<&Path>,
) -> HarnessForgeFirstMode {
    if flag_observe_only {
        return HarnessForgeFirstMode {
            forge_first: false,
            source: "observe_only_flag",
        };
    }
    if flag_forge_first {
        return HarnessForgeFirstMode {
            forge_first: true,
            source: "explicit_flag",
        };
    }
    if harness_default_mode_prefers_forge_first() {
        return HarnessForgeFirstMode {
            forge_first: true,
            source: "env_default",
        };
    }
    if let Some(forge_first) = harness_project_default_mode(project_root) {
        return HarnessForgeFirstMode {
            forge_first,
            source: "project_config",
        };
    }
    HarnessForgeFirstMode {
        forge_first: false,
        source: "default_observe_only",
    }
}

fn harness_default_mode_prefers_forge_first() -> bool {
    env::var("FORGE_HARNESS_DEFAULT_MODE")
        .ok()
        .map(|value| harness_mode_prefers_forge_first(&value))
        .unwrap_or(false)
}

fn harness_project_default_mode(project_root: Option<&Path>) -> Option<bool> {
    let project_root = match project_root {
        Some(path) => path.to_path_buf(),
        None => env::current_dir().ok()?,
    };
    read_harness_project_mode(&project_root).forge_first
}

fn read_harness_project_mode(project_root: &Path) -> HarnessProjectDefaultMode {
    let path = project_root.join(".forge/harness.json");
    let Ok(content) = fs::read_to_string(&path) else {
        return HarnessProjectDefaultMode {
            path,
            status: "missing",
            forge_first: None,
        };
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return HarnessProjectDefaultMode {
            path,
            status: "invalid_json",
            forge_first: None,
        };
    };
    let forge_first = match config.get("default_mode") {
        Some(Value::String(value)) => Some(harness_mode_prefers_forge_first(value)),
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    };
    HarnessProjectDefaultMode {
        path,
        status: if forge_first.is_some() {
            "loaded"
        } else {
            "missing_default_mode"
        },
        forge_first,
    }
}

#[derive(Debug, Clone)]
struct HarnessProjectExecPolicy {
    path: PathBuf,
    status: &'static str,
    require_lineage_for_exec: bool,
}

#[derive(Debug, Clone)]
struct HarnessProjectRuntimePolicy {
    context_budget: Option<usize>,
    token_headroom: Option<bool>,
    require_token_headroom_for_forge_first: bool,
}

fn read_harness_project_exec_policy(project_root: &Path) -> HarnessProjectExecPolicy {
    let path = project_root.join(".forge/harness.json");
    let Ok(content) = fs::read_to_string(&path) else {
        return HarnessProjectExecPolicy {
            path,
            status: "missing",
            require_lineage_for_exec: false,
        };
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return HarnessProjectExecPolicy {
            path,
            status: "invalid_json",
            require_lineage_for_exec: false,
        };
    };
    let require_lineage_for_exec = config
        .get("require_lineage_for_exec")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    HarnessProjectExecPolicy {
        path,
        status: if config.get("require_lineage_for_exec").is_some() {
            "loaded"
        } else {
            "missing_require_lineage_for_exec"
        },
        require_lineage_for_exec,
    }
}

fn read_harness_project_runtime_policy(project_root: &Path) -> HarnessProjectRuntimePolicy {
    let path = project_root.join(".forge/harness.json");
    let Ok(content) = fs::read_to_string(&path) else {
        return HarnessProjectRuntimePolicy {
            context_budget: None,
            token_headroom: None,
            require_token_headroom_for_forge_first: false,
        };
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return HarnessProjectRuntimePolicy {
            context_budget: None,
            token_headroom: None,
            require_token_headroom_for_forge_first: false,
        };
    };
    let context_budget = config
        .get("context_budget")
        .or_else(|| config.get("default_context_budget"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0);
    let token_headroom = config
        .get("default_token_headroom")
        .or_else(|| config.get("token_headroom"))
        .and_then(Value::as_bool);
    let require_token_headroom_for_forge_first = config
        .get("require_token_headroom_for_forge_first")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    HarnessProjectRuntimePolicy {
        context_budget,
        token_headroom,
        require_token_headroom_for_forge_first,
    }
}

fn harness_project_exec_policy_status(
    policy: &HarnessProjectExecPolicy,
    dry_run: bool,
    workflow_id: Option<&str>,
    task_id: Option<&str>,
    run_id: Option<&str>,
) -> &'static str {
    if !policy.require_lineage_for_exec {
        return match policy.status {
            "missing" => "missing",
            "invalid_json" => "invalid_json",
            _ => "lineage_not_required",
        };
    }
    if dry_run {
        return "lineage_required_dry_run";
    }
    if harness_exec_has_required_lineage(workflow_id, task_id, run_id) {
        "lineage_required_satisfied"
    } else {
        "lineage_required_missing"
    }
}

fn harness_exec_has_required_lineage(
    workflow_id: Option<&str>,
    task_id: Option<&str>,
    run_id: Option<&str>,
) -> bool {
    [workflow_id, task_id, run_id]
        .into_iter()
        .all(|value| value.is_some_and(|value| !value.trim().is_empty()))
}

fn harness_effective_mode(forge_first: bool) -> &'static str {
    if forge_first {
        "forge_first"
    } else {
        "observe_only"
    }
}

fn harness_mode_prefers_forge_first(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "forge_first" | "forge-first" | "forgefirst" | "1" | "true" | "yes" | "on"
    )
}

struct CliShimScriptOptions<'a> {
    forge_binary: &'a str,
    executor: &'a str,
    real_cmd: &'a str,
    store_path: Option<&'a Path>,
    forge_first: bool,
    workflow_id: Option<&'a str>,
    task_id: Option<&'a str>,
    run_id: Option<&'a str>,
    context_budget: usize,
    token_headroom: bool,
}

fn build_cli_shim_script(options: CliShimScriptOptions<'_>) -> String {
    let CliShimScriptOptions {
        forge_binary,
        executor,
        real_cmd,
        store_path,
        forge_first,
        workflow_id,
        task_id,
        run_id,
        context_budget,
        token_headroom,
    } = options;
    let mut parts = vec!["exec".to_string(), shell_quote(forge_binary)];
    if let Some(store_path) = store_path {
        parts.push("--store".to_string());
        parts.push(shell_quote(&store_path.display().to_string()));
    }
    parts.extend([
        "harness".to_string(),
        "exec".to_string(),
        "--executor".to_string(),
        shell_quote(executor),
    ]);
    if forge_first {
        parts.push("--forge-first".to_string());
    }
    if let Some(workflow_id) = workflow_id.filter(|value| !value.trim().is_empty()) {
        parts.push("--workflow".to_string());
        parts.push(shell_quote(workflow_id));
    }
    if let Some(task_id) = task_id.filter(|value| !value.trim().is_empty()) {
        parts.push("--task".to_string());
        parts.push(shell_quote(task_id));
    }
    if let Some(run_id) = run_id.filter(|value| !value.trim().is_empty()) {
        parts.push("--run".to_string());
        parts.push(shell_quote(run_id));
    }
    parts.push("--context-budget".to_string());
    parts.push(context_budget.to_string());
    if token_headroom {
        parts.push("--token-headroom".to_string());
    }
    parts.push("--execute".to_string());
    parts.push("--allow-exec".to_string());
    parts.push("--".to_string());
    parts.push(shell_quote(real_cmd));
    parts.push("\"$@\"".to_string());
    format!(
        "#!/bin/sh\n{CLI_SHIM_MARKER}\n# Generated by Forge. Edit through `forge harness install-shims`.\n{}\n",
        parts.join(" ")
    )
}

fn shim_binary_name(executor: &str) -> String {
    normalize_executor(executor)
}

struct RealCommandResolution {
    command: String,
    source: String,
    status: String,
}

#[derive(Default)]
struct ParsedCliShimScript {
    forge_binary: Option<String>,
    store_path: Option<String>,
    real_command: Option<String>,
}

fn resolve_real_command_for_shim(
    executor: &str,
    explicit_real_cmd: Option<&str>,
    shim_dir: &Path,
) -> Result<RealCommandResolution> {
    if let Some(real_cmd) = explicit_real_cmd
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(RealCommandResolution {
            command: real_cmd.to_string(),
            source: "explicit".to_string(),
            status: "explicit_real_command".to_string(),
        });
    }

    let binary_name = shim_binary_name(executor);
    let Some(path_var) = env::var_os("PATH") else {
        bail!("real CLI command was not provided and PATH is not available");
    };
    for dir in env::split_paths(&path_var) {
        if same_path(&dir, shim_dir) {
            continue;
        }
        let candidate = dir.join(&binary_name);
        if candidate.is_file() {
            return Ok(RealCommandResolution {
                command: canonical_or_display(candidate),
                source: "path_discovery".to_string(),
                status: "resolved_from_path_excluding_shim_dir".to_string(),
            });
        }
    }
    bail!(
        "real CLI command was not provided and `{binary_name}` was not found in PATH outside `{}`",
        shim_dir.display()
    );
}

fn resolve_real_command_for_status(
    executor: &str,
    shim_dir: &Path,
) -> Option<RealCommandResolution> {
    let binary_name = shim_binary_name(executor);
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        if same_path(&dir, shim_dir) {
            continue;
        }
        let candidate = dir.join(&binary_name);
        if candidate.is_file() {
            return Some(RealCommandResolution {
                command: canonical_or_display(candidate),
                source: "path_discovery".to_string(),
                status: "resolved_from_path_excluding_shim_dir".to_string(),
            });
        }
    }
    None
}

struct PathResolution {
    path: Option<String>,
    status: String,
}

fn resolve_executable_from_path(binary_name: &str) -> PathResolution {
    let Some(path_var) = env::var_os("PATH") else {
        return PathResolution {
            path: None,
            status: "path_unavailable".to_string(),
        };
    };
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(binary_name);
        if candidate.is_file() {
            return PathResolution {
                path: Some(canonical_or_display(candidate)),
                status: "resolved_from_path".to_string(),
            };
        }
    }
    PathResolution {
        path: None,
        status: "not_found_in_path".to_string(),
    }
}

fn path_entry_index(path: &Path) -> Option<usize> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .enumerate()
        .find_map(|(index, entry)| same_path(&entry, path).then_some(index))
}

fn parse_cli_shim_script(script: &str) -> Option<ParsedCliShimScript> {
    let exec_line = script
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("exec "))?;
    let words = split_shell_words(exec_line);
    if words.len() < 2 || words.first()? != "exec" {
        return None;
    }
    let store_path = words
        .windows(2)
        .find(|window| window.first().is_some_and(|value| value == "--store"))
        .and_then(|window| window.get(1))
        .cloned();
    let real_command = words
        .iter()
        .position(|word| word == "--")
        .and_then(|index| words.get(index + 1))
        .cloned();
    Some(ParsedCliShimScript {
        forge_binary: words.get(1).cloned(),
        store_path,
        real_command,
    })
}

fn split_shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut in_word = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match quote {
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            Some('"') => {
                if ch == '"' {
                    quote = None;
                } else if ch == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(ch);
                }
            }
            Some(_) => {}
            None if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                in_word = true;
            }
            None if ch == '\\' => {
                in_word = true;
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            None => {
                in_word = true;
                current.push(ch);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

fn shim_status_checks(
    shim_exists: bool,
    forge_owned: bool,
    executable: bool,
    resolved_is_shim: bool,
    would_recurse: bool,
    path_resolution_status: &str,
) -> Vec<String> {
    let mut checks = Vec::new();
    checks.push(if shim_exists {
        "shim file exists".to_string()
    } else {
        "shim file is missing".to_string()
    });
    checks.push(if forge_owned {
        "shim has Forge ownership marker".to_string()
    } else {
        "shim does not have Forge ownership marker".to_string()
    });
    checks.push(if executable {
        "shim file is executable".to_string()
    } else {
        "shim file is not executable".to_string()
    });
    checks.push(if resolved_is_shim && forge_owned {
        "PATH resolves to the Forge-owned shim".to_string()
    } else if resolved_is_shim {
        "PATH resolves to a non-Forge shim".to_string()
    } else {
        format!("PATH resolution status: {path_resolution_status}")
    });
    checks.push(if would_recurse {
        "recursion risk detected before execution".to_string()
    } else {
        "no shim recursion risk detected".to_string()
    });
    checks
}

struct ShimActivationDiagnosticInput<'a> {
    status: &'a str,
    path_precedence: &'a str,
    shim_dir: &'a Path,
    executor: &'a str,
    shim_exists: bool,
    forge_owned: bool,
    executable: bool,
    would_recurse: bool,
}

fn shim_activation_diagnostic(
    input: ShimActivationDiagnosticInput<'_>,
) -> CliShimActivationDiagnostic {
    let shim_ready_for_activation =
        input.shim_exists && input.forge_owned && input.executable && !input.would_recurse;
    let activation_profile_command = vec![
        "forge".to_string(),
        "harness".to_string(),
        "activation-profile".to_string(),
        "--shim-dir".to_string(),
        input.shim_dir.display().to_string(),
        "--executor".to_string(),
        input.executor.to_string(),
        "--project-root".to_string(),
        ".".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];
    let shim_status_command = format!(
        "forge harness shim-status --shim-dir {} --executor {} --output json",
        shell_quote(&input.shim_dir.display().to_string()),
        shell_quote(input.executor)
    );
    let one_shot_activation_command = if shim_ready_for_activation {
        format!(
            "export PATH={}:$PATH && {shim_status_command}",
            shell_quote(&input.shim_dir.display().to_string())
        )
    } else {
        String::new()
    };
    let (status, activation_required, activation_possible, reason) =
        if input.status == "shim_status_ready" {
            (
                "shim_activation_active",
                false,
                false,
                input.path_precedence.to_string(),
            )
        } else if !input.shim_exists {
            (
                "shim_activation_unavailable",
                true,
                false,
                "shim_missing".to_string(),
            )
        } else if input.would_recurse {
            (
                "shim_activation_blocked",
                true,
                false,
                "recursion_risk".to_string(),
            )
        } else if !input.forge_owned {
            (
                "shim_activation_blocked",
                true,
                false,
                "shim_not_forge_owned".to_string(),
            )
        } else if !input.executable {
            (
                "shim_activation_blocked",
                true,
                false,
                "shim_not_executable".to_string(),
            )
        } else {
            (
                "shim_activation_recommended",
                true,
                true,
                input.path_precedence.to_string(),
            )
        };

    let mut verification_commands = vec![shim_status_command];
    if activation_possible {
        verification_commands.push(one_shot_activation_command.clone());
    }

    CliShimActivationDiagnostic {
        schema_version: CLI_SHIM_ACTIVATION_DIAGNOSTIC_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        activation_required,
        activation_possible,
        reason,
        path_precedence: input.path_precedence.to_string(),
        shim_ready_for_activation,
        one_shot_activation_command,
        activation_profile_command,
        verification_commands,
        rollback_hints: vec![
            "This report is read-only; undo any manual PATH export by opening a new shell or restoring the previous PATH."
                .to_string(),
            "Use the activation-profile rollback commands only when you applied a managed shell profile block."
                .to_string(),
        ],
        notes: vec![
            "Activation means PATH prefers the Forge-owned shim before the native brain CLI."
                .to_string(),
            "A recommended activation still requires the operator to opt into the PATH change."
                .to_string(),
        ],
    }
}

fn shim_status_instructions(status: &str, shim_dir: &Path, executor: &str) -> Vec<String> {
    match status {
        "shim_status_ready" => vec![
            "no action required; PATH currently prefers the Forge-owned shim".to_string(),
            "run `forge harness exec` directly when you need a one-off guarded receipt".to_string(),
        ],
        "shim_status_missing" => vec![format!(
            "run `forge harness install-shims --shim-dir {} --executor {executor}`",
            shell_quote(&shim_dir.display().to_string())
        )],
        "shim_status_blocked" => vec![
            format!(
                "run `forge harness install-shims --shim-dir {} --executor {executor} --force` only if the existing file is disposable",
                shell_quote(&shim_dir.display().to_string())
            ),
            "move the non-Forge shim later in PATH or replace it through Forge before enabling Forge-first shells".to_string(),
        ],
        _ => vec![
            format!(
                "export PATH={}:$PATH when this shell should prefer the Forge shim",
                shell_quote(&shim_dir.display().to_string())
            ),
            "rerun `forge harness shim-status` after changing PATH or reinstalling the shim"
                .to_string(),
        ],
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_join(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.chars().any(|ch| {
                ch.is_whitespace() || matches!(ch, '\'' | '"' | '(' | ')' | '&' | ';' | '|')
            }) {
                shell_quote(part)
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to read permissions for `{}`", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to mark shim executable `{}`", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn build_output_headroom_report(
    store: Option<&ForgeStore>,
    executor: &str,
    stream: &str,
    content: &str,
    context_budget: usize,
    token_headroom: bool,
) -> Result<Option<TokenHeadroomReport>> {
    if !token_headroom || content.is_empty() {
        return Ok(None);
    }
    let source = format!("harness-exec:{executor}:{stream}");
    let report = analyze_token_headroom(content, None, context_budget, &source, true);
    if let Some(store) = store {
        return persist_token_headroom_report(store, report, content).map(Some);
    }
    Ok(Some(report))
}

fn headroom_retrieval_report(
    record: StoredHeadroomBlobRecord,
    retrieval_ref: String,
    include_content: bool,
) -> HeadroomRetrievalReport {
    HeadroomRetrievalReport {
        schema_version: HEADROOM_RETRIEVAL_SCHEMA_VERSION.to_string(),
        status: "headroom_blob_retrieved".to_string(),
        retrieval_ref,
        original_sha256: record.original_sha256,
        found: true,
        include_content,
        source: Some(record.source),
        content_kind: Some(record.content_kind),
        strategy: Some(record.strategy),
        reversible: Some(record.reversible),
        original_bytes: Some(record.original_bytes),
        compressed_sha256: Some(record.compressed_sha256),
        compressed_bytes: Some(record.compressed_bytes),
        estimated_original_tokens: Some(record.estimated_original_tokens),
        estimated_compressed_tokens: Some(record.estimated_compressed_tokens),
        estimated_saved_tokens: Some(record.estimated_saved_tokens),
        budget_tokens: Some(record.budget_tokens),
        budget_status: Some(record.budget_status),
        routing: Some(record.routing),
        original_content: include_content.then_some(record.original_content),
        compressed_content: include_content.then_some(record.compressed_content),
        created_at: Some(record.created_at),
        updated_at: Some(record.updated_at),
    }
}

#[derive(Default)]
struct HeadroomStatsAccumulator {
    blob_count: usize,
    original_bytes: i64,
    compressed_bytes: i64,
    original_tokens: i64,
    compressed_tokens: i64,
    saved_tokens: i64,
}

impl HeadroomStatsAccumulator {
    fn add(&mut self, record: &StoredHeadroomBlobRecord) {
        self.blob_count += 1;
        self.original_bytes += record.original_bytes;
        self.compressed_bytes += record.compressed_bytes;
        self.original_tokens += record.estimated_original_tokens;
        self.compressed_tokens += record.estimated_compressed_tokens;
        self.saved_tokens += record.estimated_saved_tokens;
    }
}

fn headroom_stats_saved_blob(record: StoredHeadroomBlobRecord) -> HeadroomStatsSavedBlob {
    HeadroomStatsSavedBlob {
        retrieval_ref: format!("forge://harness/headroom/{}", record.original_sha256),
        source: record.source,
        content_kind: record.content_kind,
        strategy: record.strategy,
        original_sha256: record.original_sha256,
        original_bytes: record.original_bytes,
        compressed_bytes: record.compressed_bytes,
        estimated_original_tokens: record.estimated_original_tokens,
        estimated_compressed_tokens: record.estimated_compressed_tokens,
        estimated_saved_tokens: record.estimated_saved_tokens,
        savings_percent: headroom_savings_percent(
            record.estimated_original_tokens,
            record.estimated_saved_tokens,
        ),
        budget_status: record.budget_status,
        updated_at: record.updated_at,
    }
}

fn headroom_savings_percent(original_tokens: i64, saved_tokens: i64) -> f64 {
    if original_tokens <= 0 || saved_tokens <= 0 {
        0.0
    } else {
        ((saved_tokens as f64 / original_tokens as f64) * 10000.0).round() / 100.0
    }
}

fn parse_headroom_ref(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("headroom retrieval ref cannot be empty");
    }
    let sha = value
        .strip_prefix("forge://harness/headroom/")
        .unwrap_or(value)
        .trim();
    if sha.is_empty() {
        bail!("headroom retrieval ref does not include a hash");
    }
    Ok(sha.to_string())
}

struct CliExecReceiptInput {
    wrapper_plan: CliWrapperPlanReport,
    command: Vec<String>,
    command_sha256: String,
    cwd: String,
    forge_first: bool,
    dry_run: bool,
    allow_exec: bool,
    execution_mode: String,
    project_policy_path: String,
    project_policy_status: String,
    require_lineage_for_exec: bool,
    resolved_executable: Option<String>,
    resolution_status: String,
    status: String,
    safety_checks: Vec<String>,
    executed: bool,
    success: Option<bool>,
    exit_code: Option<i32>,
    stdout_bytes: Option<usize>,
    stderr_bytes: Option<usize>,
    stdout_sha256: Option<String>,
    stderr_sha256: Option<String>,
    stdout_excerpt: Option<String>,
    stderr_excerpt: Option<String>,
    output_headroom_enabled: bool,
    stdout_headroom: Option<TokenHeadroomReport>,
    stderr_headroom: Option<TokenHeadroomReport>,
}

fn exec_receipt(input: CliExecReceiptInput) -> CliHarnessExecReceipt {
    let executor = input.wrapper_plan.executor.clone();
    let workflow_id = input.wrapper_plan.workflow_id.clone();
    let task_id = input.wrapper_plan.task_id.clone();
    let run_id = input.wrapper_plan.run_id.clone();
    CliHarnessExecReceipt {
        schema_version: CLI_HARNESS_EXEC_SCHEMA_VERSION.to_string(),
        status: input.status,
        executor,
        command: input.command,
        command_sha256: input.command_sha256,
        cwd: input.cwd,
        workflow_id,
        task_id,
        run_id,
        forge_first: input.forge_first,
        forge_first_source: input.wrapper_plan.forge_first_source.clone(),
        context_budget: input.wrapper_plan.context_budget,
        context_budget_source: input.wrapper_plan.context_budget_source.clone(),
        token_headroom_source: input.wrapper_plan.token_headroom_source.clone(),
        require_token_headroom_for_forge_first: input
            .wrapper_plan
            .require_token_headroom_for_forge_first,
        dry_run: input.dry_run,
        allow_exec: input.allow_exec,
        execution_mode: input.execution_mode,
        project_policy_path: input.project_policy_path,
        project_policy_status: input.project_policy_status,
        require_lineage_for_exec: input.require_lineage_for_exec,
        resolved_executable: input.resolved_executable,
        resolution_status: input.resolution_status,
        wrapper_plan: input.wrapper_plan,
        safety_checks: input.safety_checks,
        executed: input.executed,
        success: input.success,
        exit_code: input.exit_code,
        stdout_bytes: input.stdout_bytes,
        stderr_bytes: input.stderr_bytes,
        stdout_sha256: input.stdout_sha256,
        stderr_sha256: input.stderr_sha256,
        stdout_excerpt: input.stdout_excerpt,
        stderr_excerpt: input.stderr_excerpt,
        output_headroom_enabled: input.output_headroom_enabled,
        stdout_headroom: input.stdout_headroom,
        stderr_headroom: input.stderr_headroom,
        event_recorded: false,
        global_event_id: None,
        notes: vec![
            "Harness exec is a Forge-owned receipt for brain CLI invocation, not process interception.".to_string(),
            "Use dry-run receipts to validate wrapper shape before opting into guarded execution.".to_string(),
        ],
    }
}

fn resolve_executable(command: Option<&String>, cwd: &Path) -> (Option<String>, String) {
    let Some(command) = command
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return (None, "command_empty".to_string());
    };
    let candidate = Path::new(command);
    if candidate.components().count() > 1 {
        let path = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            cwd.join(candidate)
        };
        if path.is_file() {
            return (
                Some(canonical_or_display(path)),
                "executable_resolved_by_path".to_string(),
            );
        }
        return (None, "executable_missing".to_string());
    }
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let path = dir.join(command);
            if path.is_file() {
                return (
                    Some(canonical_or_display(path)),
                    "executable_resolved_from_path".to_string(),
                );
            }
        }
    }
    (None, "executable_missing".to_string())
}

fn canonical_or_display(path: PathBuf) -> String {
    path.canonicalize().unwrap_or(path).display().to_string()
}

fn bounded_excerpt(value: &str, max_chars: usize) -> String {
    let mut excerpt = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        excerpt.push_str("\n[forge excerpt truncated]");
    }
    excerpt
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn detect_content_kind(content: &str, hint: Option<&str>) -> String {
    if let Some(hint) = hint.map(str::trim).filter(|value| !value.is_empty()) {
        return hint.to_lowercase().replace('_', "-");
    }
    if serde_json::from_str::<Value>(content).is_ok() {
        return "json".to_string();
    }
    let lower = content.to_lowercase();
    if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("panic")
        || lower.contains("warning")
    {
        return "log".to_string();
    }
    if content
        .lines()
        .take(20)
        .any(|line| line.contains(':') && line.matches(':').count() >= 2)
    {
        return "search".to_string();
    }
    if lower.contains("fn ")
        || lower.contains("class ")
        || lower.contains("struct ")
        || lower.contains("impl ")
        || lower.contains("import ")
    {
        return "code".to_string();
    }
    "text".to_string()
}

fn compress_for_headroom(content: &str, content_kind: &str) -> (String, Vec<String>, String) {
    match content_kind {
        "json" => (
            "smart_json_shape_summary".to_string(),
            vec!["json_detected".to_string(), "shape_summary".to_string()],
            compress_json_shape(content),
        ),
        "log" => (
            "signal_log_compressor".to_string(),
            vec!["log_detected".to_string(), "error_warning_tail".to_string()],
            compress_signal_lines(content, true),
        ),
        "search" => (
            "search_result_compressor".to_string(),
            vec![
                "search_detected".to_string(),
                "top_matches_grouped".to_string(),
            ],
            compress_signal_lines(content, false),
        ),
        "code" => (
            "code_signature_compressor".to_string(),
            vec!["code_detected".to_string(), "signature_lines".to_string()],
            compress_code_signatures(content),
        ),
        _ => (
            "text_head_tail_summary".to_string(),
            vec!["text_detected".to_string(), "head_tail_summary".to_string()],
            compress_text(content),
        ),
    }
}

fn compress_json_shape(content: &str) -> String {
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Array(items)) => format!(
            "json array: len={} sample_types={}",
            items.len(),
            items
                .iter()
                .take(8)
                .map(json_kind)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Ok(Value::Object(map)) => format!(
            "json object: keys={} key_list={}",
            map.len(),
            map.keys().take(32).cloned().collect::<Vec<_>>().join(",")
        ),
        Ok(value) => format!("json scalar: {}", json_kind(&value)),
        Err(_) => compress_text(content),
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn compress_signal_lines(content: &str, include_tail: bool) -> String {
    let mut selected = content
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("error")
                || lower.contains("failed")
                || lower.contains("panic")
                || lower.contains("warning")
                || lower.contains("fatal")
        })
        .take(40)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if include_tail {
        selected.extend(
            content
                .lines()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(str::to_string),
        );
    } else if selected.is_empty() {
        selected.extend(content.lines().take(40).map(str::to_string));
    }
    selected.dedup();
    if selected.is_empty() {
        compress_text(content)
    } else {
        selected.join("\n")
    }
}

fn compress_code_signatures(content: &str) -> String {
    let selected = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("pub ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("impl ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("import ")
                || trimmed.starts_with("use ")
        })
        .take(120)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        compress_text(content)
    } else {
        selected.join("\n")
    }
}

fn compress_text(content: &str) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    if lines.len() <= 20 {
        return content.to_string();
    }
    let mut selected = lines.iter().take(10).copied().collect::<Vec<_>>();
    selected.push("[... omitted middle content; retrieve by original_sha256 ...]");
    selected.extend(
        lines
            .iter()
            .rev()
            .take(10)
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev(),
    );
    selected.join("\n")
}

fn estimate_tokens(content: &str) -> usize {
    if content.trim().is_empty() {
        return 0;
    }
    let char_estimate = content.chars().count().div_ceil(4);
    let word_estimate = content.split_whitespace().count();
    char_estimate.max(word_estimate).max(1)
}

fn normalize_executor(executor: &str) -> String {
    match executor.trim().to_lowercase().as_str() {
        "claude-code" => "claude".to_string(),
        "open-code" => "opencode".to_string(),
        "gemini-cli" => "gemini".to_string(),
        "codex-cli" => "codex".to_string(),
        value if !value.is_empty() => value.to_string(),
        _ => "codex".to_string(),
    }
}

fn env_var(name: &str, value: &str, reason: &str) -> CliWrapperEnvVar {
    CliWrapperEnvVar {
        name: name.to_string(),
        value: value.to_string(),
        reason: reason.to_string(),
    }
}
