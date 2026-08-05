use crate::artifact::hex_sha256;
use crate::cli_integration::{
    build_cli_wrapper_plan, build_harness_bootstrap_report, build_headroom_stats_report,
    run_cli_harness_exec, CliHarnessExecOptions, CliWrapperPlanOptions, CliWrapperPlanReport,
    HarnessBootstrapOptions, HeadroomStatsOptions, HeadroomStatsReport,
    CLI_HARNESS_HEADROOM_RUNTIME_PLAN_SCHEMA_VERSION, CLI_WRAPPER_PLAN_SCHEMA_VERSION,
};
use crate::executor::{
    load_executors, record_brain_session_lifecycle, record_shell_session_plan, BrainCandidate,
    BrainRouterReport, BrainSessionLifecycleOptions, BrainShellSessionSpec, ShellLaunchPlanOptions,
};
use crate::graph::{
    create_workflow, task, ExecutorKind, NodeBrainAgentSlotSpec, NodeBrainRoutingSpec,
};
use crate::handoff::build_task_handoff_with_project;
use crate::intent::parse_intent;
use crate::interactive::{build_interactive_harness, InteractiveHarnessOptions};
use crate::ir::{
    ir_schema_version, CreativeArtifact, DesignToken, DocumentSection, DocumentSpec, ScreenSpec,
    SemanticAlias, TokenCollection, TokenType,
};
use crate::mission::{
    load_mission, AgentHandoff, MissionDriveReport, MissionMode, MissionRecord, MissionStatus,
    MissionSubmission, MissionSubmitReport,
};
use crate::mission_executor::{
    load_mission_execution_receipt, verify_mission_execution_receipt, MissionExecutionReceipt,
    MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION,
};
use crate::mission_platform::{
    mission_platform_catalog, MissionPlatformCatalog, MISSION_PLATFORM_BOUNDED_SIMULATION,
    MISSION_PLATFORM_CAPABILITY_COUNT, MISSION_PLATFORM_CATALOG_SCHEMA_VERSION,
    MISSION_PLATFORM_CONTRACT_ONLY, MISSION_PLATFORM_RUNTIME_REAL,
};
use crate::multimodal::{
    build_multimodal_runtime_benchmark, resolve_multimodal_feature_flag,
    MultimodalRuntimeBenchmarkOptions,
};
use crate::patch::{
    build_patch_apply, build_patch_diff, build_patch_plan, build_patch_restore, build_patch_revert,
    build_patch_review, PatchApplyArtifactRef, PatchDiffOptions, PatchPlanArtifactRef,
};
use crate::request::{heartbeat_request, start_async_request, RunActivity};
use crate::schedule::{create_daily_goal_research_workflow, run_daily_goal_research_smoke};
use crate::security::{sanitize_prompt_secrets, SecretSanitizationOptions};
use crate::storage::{FoundryStore, GlobalEventWrite};
use crate::validation::validate_workflow;
use crate::workflow::{
    attach_creative_artifact, attach_workflow_artifact, set_workflow_token_collection,
    update_workflow_node_brain_routing, WorkflowNodeBrainRoutingUpdateInput,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const MILESTONE_STATUS_SCHEMA_VERSION: &str = "foundry.milestone.status.v1";
const MILESTONE_MANIFEST_SCHEMA_VERSION: &str = "foundry.milestone.manifest.v1";
const MILESTONE_ATTACHED_EVIDENCE_SCHEMA_VERSION: &str = "foundry.milestone.attached_evidence.v1";
const MILESTONE_ATTACHED_EVIDENCE_EVENT_KIND: &str = "milestone_evidence_attached";
const SUPPORTED_MILESTONE: &str = "0.6";
const LEGACY_MILESTONE_QUERY_ALIAS: &str = "0.5";
pub const PRODUCTION_READINESS_MANIFEST_SCHEMA_VERSION: &str =
    "foundry.milestone.production_readiness_manifest.v1";
pub const PRODUCTION_READINESS_REPORT_SCHEMA_VERSION: &str =
    "foundry.milestone.production_readiness.v1";
pub const PRODUCTION_READINESS_PLAN_SCHEMA_VERSION: &str =
    "foundry.milestone.production_readiness_plan.v1";
pub const PRODUCTION_EVIDENCE_DRAFT_SCHEMA_VERSION: &str =
    "foundry.milestone.production_evidence_draft.v1";
pub const PRODUCTION_EVIDENCE_TEMPLATE_REPORT_SCHEMA_VERSION: &str =
    "foundry.milestone.production_evidence_template.v1";
pub const PRODUCTION_EVIDENCE_ASSEMBLY_REPORT_SCHEMA_VERSION: &str =
    "foundry.milestone.production_evidence_assembly.v1";
pub const PRODUCTION_SOURCE_EVIDENCE_SCHEMA_PREFIX: &str =
    "foundry.milestone.production_source_evidence";
pub const PRODUCTION_MISSION_LIFECYCLE_RECEIPT_SCHEMA_VERSION: &str =
    "foundry.milestone.mission_lifecycle.v1";
pub const PRODUCTION_READINESS_REQUIRED_GATE_COUNT: usize = 11;
pub const PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT: usize = 14;
const PRODUCTION_PROFILE: &str = "single_host_linux_v0.6";
const LAST_LEGACY_PRODUCTION_RECEIPT_VERSION: &str = "0.5.2";
const MAX_PRODUCTION_EVIDENCE_AGE_SECONDS: u64 = 86_400;
const MAX_PRODUCTION_EVIDENCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RESTORE_RPO_SECONDS: u64 = 86_400;
const MAX_RESTORE_RTO_SECONDS: u64 = 1_800;
const MAX_BOUNDED_LOAD_SECONDS: u64 = 300;
const MAX_BOUNDED_LOAD_CONCURRENCY: u64 = 64;
const MAX_BOUNDED_LOAD_P95_MILLIS: u64 = 2_000;
const MIN_BOUNDED_LOAD_OPERATIONS: u64 = 100;
const REQUIRED_RELEASE_TARGETS: [&str; 5] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
];
const PRODUCTION_EVIDENCE_ASSEMBLY_KINDS: [&str; 13] = [
    "release_matrix",
    "release_artifacts",
    "release_sbom",
    "release_checksums",
    "release_sigstore",
    "release_provenance",
    "installation",
    "off_host_recovery",
    "key_escrow",
    "alerting",
    "restore_drill",
    "upgrade_rollback",
    "bounded_load",
];

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneStatusReport {
    pub schema_version: String,
    pub milestone: String,
    pub requested_milestone: String,
    pub compatibility_mode: String,
    pub release_line_boundary: String,
    pub status_vocabulary: Vec<String>,
    pub summary: MilestoneStatusSummary,
    pub capabilities: Vec<MilestoneCapability>,
    pub promotion_decision: MilestonePromotionDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneStatusSummary {
    pub implemented: usize,
    pub validated: usize,
    pub groundwork: usize,
    pub planned: usize,
    pub blocked: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCapability {
    pub id: String,
    pub title: String,
    pub status: String,
    pub required_for_promotion: bool,
    pub evidence: String,
    pub gap_before_promotion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePromotionDecision {
    pub decision: String,
    pub promotable: bool,
    pub readiness_scope: String,
    pub capability_ready: bool,
    pub production_ready: bool,
    pub production_evidence_evaluated: bool,
    pub blocked_by: Vec<String>,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionEvidenceRef {
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub observed_at_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionReleaseEvidence {
    pub matrix: ProductionEvidenceRef,
    pub successful_targets: Vec<String>,
    pub artifacts: ProductionEvidenceRef,
    pub binary_sha256_by_target: BTreeMap<String, String>,
    pub sbom: ProductionEvidenceRef,
    pub sbom_format: String,
    pub sbom_component_count: u64,
    pub checksums: ProductionEvidenceRef,
    pub checksum_entry_count: u64,
    pub checksums_verified: bool,
    pub sigstore: ProductionEvidenceRef,
    pub sigstore_verified: bool,
    pub provenance: ProductionEvidenceRef,
    pub provenance_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionInstallationEvidence {
    pub evidence: ProductionEvidenceRef,
    pub target: String,
    pub installed_version: String,
    pub installed_binary_sha256: String,
    pub ops_service_active: bool,
    pub runtime_service_active: bool,
    pub request_supervisor_service_active: bool,
    pub store_check_passed: bool,
    pub ops_authenticated_probe_passed: bool,
    pub ops_http_status: u16,
    pub ops_loopback_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionOffHostBackupEvidence {
    pub evidence: ProductionEvidenceRef,
    pub recovery_challenge_epoch: u64,
    pub immutable_upload_passed: bool,
    pub remote_digest_verified: bool,
    pub download_digest_verified: bool,
    pub downloaded_store_check_passed: bool,
    pub disposable_restore_passed: bool,
    pub restored_store_check_passed: bool,
    pub off_host_retention_enabled: bool,
    #[serde(alias = "forge_key_isolated_from_uploader")] // foundry-brand-allow: legacy-compat
    pub foundry_key_isolated_from_uploader: bool,
    pub uploader_credentials_isolated_from_foundry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionKeyEscrowEvidence {
    pub evidence: ProductionEvidenceRef,
    pub encrypted: bool,
    pub separate_access_control: bool,
    pub recovery_key_available: bool,
    pub restore_with_escrowed_key_tested: bool,
    pub excluded_from_database_backup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionAlertEvidence {
    pub evidence: ProductionEvidenceRef,
    pub ops_service_failure_alert: bool,
    pub runtime_service_failure_alert: bool,
    pub request_supervisor_service_failure_alert: bool,
    pub store_check_alert: bool,
    pub backup_timer_alert: bool,
    pub off_host_failure_alert: bool,
    pub disk_space_alert: bool,
    pub backup_age_alert: bool,
    pub delivery_route_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionRestoreDrillEvidence {
    pub evidence: ProductionEvidenceRef,
    pub drill_epoch: u64,
    pub disposable_recovery_host: bool,
    pub downloaded_store_check_passed: bool,
    pub restored_store_check_passed: bool,
    pub canary_workflow_verified: bool,
    pub ops_authenticated_probe_passed: bool,
    pub rpo_seconds: u64,
    pub rto_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionUpgradeRollbackEvidence {
    pub evidence: ProductionEvidenceRef,
    pub target_version: String,
    pub simulation_completed: bool,
    pub pre_upgrade_backup_verified: bool,
    pub upgraded_store_check_passed: bool,
    pub upgraded_ops_health_passed: bool,
    pub rollback_completed: bool,
    pub previous_version_store_check_passed: bool,
    pub previous_version_ops_health_passed: bool,
    pub target_reinstalled_and_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionBoundedLoadEvidence {
    pub evidence: ProductionEvidenceRef,
    pub duration_seconds: u64,
    pub concurrency: u64,
    pub operation_count: u64,
    pub error_count: u64,
    pub p95_latency_millis: u64,
    pub max_rss_bytes: u64,
    pub max_rss_limit_bytes: u64,
    pub timeout_enforced: bool,
    pub resource_limit_enforced: bool,
    pub store_check_passed: bool,
    pub crash_restart_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionMissionOperationalEvidence {
    pub evidence: ProductionEvidenceRef,
    pub capability_inventory_schema_version: String,
    pub capability_inventory_sha256: String,
    pub capability_numbers: Vec<u8>,
    pub mission_id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub execute_receipt_schema_version: String,
    pub execute_receipt_id: String,
    pub execute_receipt_sha256: String,
    pub execute_status: String,
    pub execution_attempted: bool,
    pub executed: bool,
    pub execute_exit_code: Option<i32>,
    pub submit_receipt_schema_version: String,
    pub submit_receipt_sha256: String,
    pub submit_status: String,
    pub submit_queued: bool,
    pub submitted_execute_receipt_sha256: String,
    pub handoff_id: String,
    pub inbox_id: String,
    pub resume_receipt_schema_version: String,
    pub resume_receipt_sha256: String,
    pub resume_status: String,
    pub resume_action: String,
    pub resumed_handoff_id: String,
    pub resume_consumed: bool,
    pub execute_observed_at_epoch: u64,
    pub submit_observed_at_epoch: u64,
    pub resume_observed_at_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionMissionLifecycleReceipt {
    pub schema_version: String,
    pub kind: String,
    pub status: String,
    pub subject_version: String,
    pub claims_sha256: String,
    pub capability_inventory_schema_version: String,
    pub capability_inventory_sha256: String,
    pub capability_numbers: Vec<u8>,
    pub execution_receipt: MissionExecutionReceipt,
    pub submission: MissionSubmission,
    pub submit_report: MissionSubmitReport,
    pub resume_report: MissionDriveReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionMissionLifecycleEvidencePackage {
    pub schema_version: String,
    pub status: String,
    pub manifest_section: ProductionMissionOperationalEvidence,
    pub artifact: ProductionMissionLifecycleReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionReadinessManifest {
    pub schema_version: String,
    pub milestone: String,
    pub profile: String,
    pub release_version: String,
    pub generated_at_epoch: u64,
    pub release: ProductionReleaseEvidence,
    pub installation: ProductionInstallationEvidence,
    pub off_host_backup: ProductionOffHostBackupEvidence,
    pub key_escrow: ProductionKeyEscrowEvidence,
    pub alerts: ProductionAlertEvidence,
    pub restore_drill: ProductionRestoreDrillEvidence,
    pub upgrade_rollback: ProductionUpgradeRollbackEvidence,
    pub bounded_load: ProductionBoundedLoadEvidence,
    pub mission_operational_lifecycle: ProductionMissionOperationalEvidence,
}

#[derive(Debug, Clone)]
pub struct ProductionEvidenceTemplateOptions<'a> {
    pub version: &'a str,
    pub release_version: &'a str,
    pub evidence_root: &'a Path,
    pub template_path: &'a Path,
}

#[derive(Debug, Clone)]
pub struct ProductionEvidenceAssemblyOptions<'a> {
    pub version: &'a str,
    pub release_version: &'a str,
    pub evidence_root: &'a Path,
    pub draft_path: &'a Path,
    pub receipt_directory: &'a Path,
    pub manifest_path: &'a Path,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionEvidenceTemplateReport {
    pub schema_version: String,
    pub status: String,
    pub template_path: String,
    pub release_version: String,
    pub unresolved_field_count: usize,
    pub files_written: u64,
    pub infrastructure_commands_executed: u64,
    pub infrastructure_mutations_performed: bool,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionEvidenceAssemblyReceiptReport {
    pub kind: String,
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub source_artifact_path: String,
    pub source_artifact_sha256: String,
    pub observed_at_epoch: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionEvidenceAssemblyReport {
    pub schema_version: String,
    pub status: String,
    pub manifest_path: String,
    pub manifest_sha256: String,
    pub release_version: String,
    pub receipt_count: usize,
    pub source_artifact_count: usize,
    pub receipts: Vec<ProductionEvidenceAssemblyReceiptReport>,
    pub mission_operational_lifecycle_artifact: String,
    pub files_written: u64,
    pub infrastructure_commands_executed: u64,
    pub infrastructure_mutations_performed: bool,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionEvidenceDraftSource {
    artifact_path: String,
    observed_at_epoch: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionSourceEvidence {
    schema_version: String,
    kind: String,
    status: String,
    subject_version: String,
    observed_at_epoch: u64,
    execution_mode: String,
    producer: String,
    claims: serde_json::Value,
    evidence: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ProductionReadinessOptions<'a> {
    pub version: &'a str,
    pub manifest_path: &'a Path,
    pub evidence_root: &'a Path,
    pub store_path: &'a Path,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionReadinessCheck {
    pub id: String,
    pub passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionReadinessGate {
    pub id: String,
    pub title: String,
    pub status: String,
    pub checks: Vec<ProductionReadinessCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionReadinessReport {
    pub schema_version: String,
    pub milestone: String,
    pub profile: String,
    pub release_version: String,
    pub evaluation_mode: String,
    pub capability_ready: bool,
    pub capability_inventory_count: usize,
    pub capability_inventory_sha256: String,
    pub capability_proof_kind_counts: BTreeMap<String, usize>,
    pub required_gate_count: usize,
    pub required_receipt_count: usize,
    pub production_ready: bool,
    pub decision: String,
    pub blocked_by: Vec<String>,
    pub gates: Vec<ProductionReadinessGate>,
    pub manifest_sha256: String,
    pub commands_executed: u64,
    pub mutations_performed: bool,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionReadinessRequirement {
    pub gate_id: String,
    pub required_evidence: Vec<String>,
    pub blocking: bool,
    pub max_evidence_age_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionReadinessPlanReport {
    pub schema_version: String,
    pub milestone: String,
    pub profile: String,
    pub evaluation_mode: String,
    pub capability_ready: bool,
    pub capability_inventory_count: usize,
    pub capability_inventory_sha256: String,
    pub capability_proof_kind_counts: BTreeMap<String, usize>,
    pub required_gate_count: usize,
    pub required_receipt_count: usize,
    pub production_ready: bool,
    pub requirements: Vec<ProductionReadinessRequirement>,
    pub commands_executed: u64,
    pub mutations_performed: bool,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionEvidenceReceipt {
    pub schema_version: String,
    pub kind: String,
    pub status: String,
    pub subject_version: String,
    pub claims_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_artifact_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_artifact_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_observed_at_epoch: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestReport {
    pub schema_version: String,
    pub milestone: String,
    pub requested_milestone: String,
    pub compatibility_mode: String,
    pub release_line_boundary: String,
    pub requirements: Vec<MilestoneRequirement>,
    pub completed_capabilities: Vec<MilestoneManifestCapability>,
    pub missing_capabilities: Vec<MilestoneManifestCapability>,
    pub validation_evidence: Vec<MilestoneManifestEvidence>,
    pub attached_evidence: Vec<MilestoneAttachedEvidence>,
    pub evidence_provenance: Vec<MilestoneEvidenceProvenance>,
    pub demos: Vec<MilestoneManifestDemo>,
    pub known_gaps: Vec<MilestoneManifestGap>,
    pub promotion_decision: MilestonePromotionDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneEvidenceProvenance {
    pub source_milestone: String,
    pub evidence_count: usize,
    pub usage: String,
    pub auto_migrated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneRequirement {
    pub capability_id: String,
    pub title: String,
    pub status: String,
    pub required_evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestCapability {
    pub id: String,
    pub title: String,
    pub status: String,
    pub promotion_ready: bool,
    pub evidence: String,
    pub gap_before_promotion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestEvidence {
    pub capability_id: String,
    pub status: String,
    pub summary: String,
    pub validation_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestDemo {
    pub capability_id: String,
    pub status: String,
    pub summary: String,
    pub required_for_promotion: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestGap {
    pub capability_id: String,
    pub status: String,
    pub gap: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneAttachedEvidence {
    pub schema_version: String,
    pub milestone: String,
    pub capability_id: String,
    pub evidence_id: String,
    pub kind: String,
    pub status: String,
    pub summary: String,
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub artifact_bytes: u64,
    pub approved_by: String,
    pub origin: String,
    pub promotion_impact: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_event_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct MilestoneAttachEvidenceOptions<'a> {
    pub version: &'a str,
    pub capability_id: &'a str,
    pub kind: &'a str,
    pub summary: &'a str,
    pub artifact_path: &'a Path,
    pub approved_by: &'a str,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct MilestoneEvidencePlanOptions<'a> {
    pub version: &'a str,
    pub capability_id: &'a str,
    pub project_root: Option<&'a Path>,
    pub connected_brain: Option<&'a str>,
    pub connected_runtime: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneEvidencePlanReport {
    pub schema_version: String,
    pub milestone: String,
    pub capability_id: String,
    pub status: String,
    pub project_root: Option<String>,
    pub ready_to_collect_evidence: bool,
    pub required_attached_evidence_kinds: Vec<String>,
    pub attached_evidence_kinds: Vec<String>,
    pub missing_attached_evidence_kinds: Vec<String>,
    pub promotion_gate_templates: Vec<MilestonePromotionGateTemplate>,
    pub config_checks: Vec<MilestoneEvidencePlanConfigCheck>,
    pub manifest_templates: Vec<MilestoneEvidencePlanManifestTemplate>,
    pub provider_candidates: Vec<MilestoneEvidenceProviderCandidate>,
    pub configured_evidence_sources: Vec<String>,
    pub evidence_collection_commands: Vec<String>,
    pub attach_commands: Vec<String>,
    pub next_action: String,
    pub promotion_impact: String,
}

#[derive(Debug, Clone, Copy)]
pub struct MilestonePrepareEvidenceInputsOptions<'a> {
    pub version: &'a str,
    pub capability_id: &'a str,
    pub project_root: Option<&'a Path>,
    pub connected_brain: Option<&'a str>,
    pub connected_runtime: Option<&'a str>,
    pub provider_command: Option<&'a Path>,
    pub model_id: Option<&'a str>,
    pub approval_ref: Option<&'a str>,
    pub apply: bool,
    pub approved_by: Option<&'a str>,
    pub force: bool,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePrepareEvidenceInputsReport {
    pub schema_version: String,
    pub milestone: String,
    pub capability_id: String,
    pub status: String,
    pub project_root: String,
    pub apply: bool,
    pub mutates_files: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    pub force: bool,
    pub origin: String,
    pub template_count: usize,
    pub written_count: usize,
    pub skipped_count: usize,
    pub prepared_files: Vec<MilestonePreparedEvidenceInputFile>,
    pub evidence_plan: MilestoneEvidencePlanReport,
    pub next_commands: Vec<String>,
    pub next_action: String,
    pub promotion_impact: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePreparedEvidenceInputFile {
    pub template_id: String,
    pub target_path: String,
    pub secret_free: bool,
    pub existed_before: bool,
    pub created_parent_dir: bool,
    pub write_status: String,
    pub bytes: usize,
    pub sha256: String,
    pub summary: String,
    pub validation_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneEvidencePlanConfigCheck {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_id: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePromotionGateTemplate {
    pub evidence_kind: String,
    pub gate_ids: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneEvidencePlanManifestTemplate {
    pub schema_version: String,
    pub id: String,
    pub status: String,
    pub target_path: String,
    pub secret_free: bool,
    pub template_json: serde_json::Value,
    pub preparation_commands: Vec<String>,
    pub validation_commands: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneEvidenceProviderCandidate {
    pub schema_version: String,
    pub provider_id: String,
    pub brain_id: String,
    pub binary: String,
    pub detected_path: Option<String>,
    pub installed: bool,
    pub version_command: Vec<String>,
    pub version_status: String,
    pub version_output: String,
    pub readiness: String,
    pub manifest_provider_template: serde_json::Value,
    pub evidence_blocker: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Copy)]
pub struct MilestoneCollectEvidenceOptions<'a> {
    pub version: &'a str,
    pub capability_id: &'a str,
    pub kind: Option<&'a str>,
    pub project_root: Option<&'a Path>,
    pub connected_brain: Option<&'a str>,
    pub connected_runtime: Option<&'a str>,
    pub approved_by: &'a str,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct MilestoneCollectReadyEvidenceOptions<'a> {
    pub version: &'a str,
    pub project_root: Option<&'a Path>,
    pub connected_brain: Option<&'a str>,
    pub connected_runtime: Option<&'a str>,
    pub approved_by: &'a str,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCollectEvidenceReport {
    pub schema_version: String,
    pub milestone: String,
    pub capability_id: String,
    pub kind: String,
    pub status: String,
    pub project_root: String,
    pub configured_evidence_source: String,
    pub collection_promotion_ready: bool,
    pub promotion_gates: Vec<MilestonePromotionGate>,
    pub collection_artifact_path: String,
    pub collection_artifact_sha256: String,
    pub collection_summary: String,
    pub attached_evidence: MilestoneAttachedEvidence,
    pub promotion_impact: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCollectReadyEvidenceReport {
    pub schema_version: String,
    pub milestone: String,
    pub status: String,
    pub project_root: String,
    pub approved_by: String,
    pub origin: String,
    pub required_count: usize,
    pub collected_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub promotion_ready_after_collection: bool,
    pub promotion_decision_after_collection: MilestonePromotionDecision,
    pub collected_evidence: Vec<MilestoneCollectReadyEvidenceCollected>,
    pub skipped_evidence: Vec<MilestoneCollectReadyEvidenceSkipped>,
    pub failed_evidence: Vec<MilestoneCollectReadyEvidenceFailed>,
    pub next_commands: Vec<String>,
    pub next_action: String,
    pub promotion_impact: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCollectReadyEvidenceCollected {
    pub capability_id: String,
    pub kind: String,
    pub status: String,
    pub configured_evidence_source: String,
    pub collection_promotion_ready: bool,
    pub collection_artifact_path: String,
    pub collection_artifact_sha256: String,
    pub attached_evidence_id: String,
    pub attached_artifact_path: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCollectReadyEvidenceSkipped {
    pub capability_id: String,
    pub kind: String,
    pub status: String,
    pub reason: String,
    pub evidence_plan: MilestoneEvidencePlanReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCollectReadyEvidenceFailed {
    pub capability_id: String,
    pub kind: String,
    pub status: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchReport {
    pub schema_version: String,
    pub status: String,
    pub milestone: String,
    pub artifact_path: String,
    pub source_count: usize,
    pub sources: Vec<MilestoneResearchSource>,
    pub local_skill_inputs: Vec<MilestoneResearchSource>,
    pub findings: Vec<MilestoneResearchFinding>,
    pub validation_gates: Vec<MilestoneResearchGate>,
    pub workflow_templates: Vec<MilestoneResearchTemplate>,
    pub lean_governance: Vec<MilestoneLeanDecision>,
    pub promotion_impact: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchSource {
    pub label: String,
    pub url_or_path: String,
    pub evidence: String,
    pub foundry_implication: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchFinding {
    pub id: String,
    pub title: String,
    pub source_labels: Vec<String>,
    pub finding: String,
    pub foundry_runtime_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchGate {
    pub id: String,
    pub title: String,
    pub validates: String,
    pub failure_condition: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchTemplate {
    pub id: String,
    pub title: String,
    pub stages: Vec<String>,
    pub deterministic_nodes: Vec<String>,
    pub ai_nodes: Vec<String>,
    pub human_gates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneLeanDecision {
    pub id: String,
    pub decision: String,
    pub accepted_complexity: String,
    pub rejected_complexity: String,
    pub evidence_metric: String,
}

fn canonical_milestone_version(version: &str) -> Result<&'static str> {
    match version.trim() {
        SUPPORTED_MILESTONE => Ok(SUPPORTED_MILESTONE),
        LEGACY_MILESTONE_QUERY_ALIAS => Ok(SUPPORTED_MILESTONE),
        version => bail!(
            "unsupported milestone {version}; currently supported: {SUPPORTED_MILESTONE} (legacy query alias: {LEGACY_MILESTONE_QUERY_ALIAS})"
        ),
    }
}

fn milestone_compatibility_mode(requested_version: &str) -> &'static str {
    if requested_version.trim() == LEGACY_MILESTONE_QUERY_ALIAS {
        "legacy_query_alias"
    } else {
        "canonical"
    }
}

pub fn build_milestone_status(version: &str) -> Result<MilestoneStatusReport> {
    let requested_milestone = version.trim().to_string();
    let version = canonical_milestone_version(&requested_milestone)?;

    let capabilities = foundry_06_capabilities();
    let summary = summarize_capabilities(&capabilities);
    let blocked_by = capabilities
        .iter()
        .filter(|capability| {
            capability.required_for_promotion && !is_promotion_ready_status(&capability.status)
        })
        .map(|capability| capability.id.clone())
        .collect::<Vec<_>>();
    let promotable = blocked_by.is_empty();

    Ok(MilestoneStatusReport {
        schema_version: MILESTONE_STATUS_SCHEMA_VERSION.to_string(),
        milestone: version.to_string(),
        requested_milestone: requested_milestone.clone(),
        compatibility_mode: milestone_compatibility_mode(&requested_milestone).to_string(),
        release_line_boundary:
            "0.6 is the canonical Foundry Core production line. Valid 0.5 evidence may be consumed read-only with its source milestone preserved; no receipt is rewritten or auto-promoted. Replacement-grade CLI and multimodal runtimes remain optional adoption capabilities and do not block the secure orchestration core."
                .to_string(),
        status_vocabulary: status_vocabulary(),
        summary,
        capabilities,
        promotion_decision: MilestonePromotionDecision {
            decision: if promotable { "promote" } else { "fail" }.to_string(),
            promotable,
            readiness_scope: "capability".to_string(),
            capability_ready: promotable,
            production_ready: false,
            production_evidence_evaluated: false,
            blocked_by,
            reason: if promotable {
                "All required Foundry 0.6 capabilities have implementation and validation evidence. This capability decision does not assert operational production readiness."
                    .to_string()
            } else {
                "Foundry 0.6 promotion is blocked while any required capability remains planned, blocked or only groundwork."
                    .to_string()
            },
            next_action: if promotable {
                "Evaluate the separate fail-closed production-readiness manifest before publishing or installing the release."
                    .to_string()
            } else {
                "Close the next required core capability with tests and milestone evidence before reconsidering 0.6 promotion."
                    .to_string()
            },
        },
    })
}

fn mission_platform_inventory_is_exact(catalog: &MissionPlatformCatalog) -> bool {
    let expected_numbers = (1..=u8::try_from(MISSION_PLATFORM_CAPABILITY_COUNT).unwrap_or(u8::MAX))
        .collect::<Vec<_>>();
    let actual_numbers = catalog
        .capabilities
        .iter()
        .map(|capability| capability.number)
        .collect::<Vec<_>>();
    let unique_ids = catalog
        .capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        == MISSION_PLATFORM_CAPABILITY_COUNT;
    let proof_kinds = catalog
        .capabilities
        .iter()
        .map(|capability| capability.proof_kind.as_str())
        .collect::<BTreeSet<_>>();
    let expected_proof_kinds = BTreeSet::from([
        MISSION_PLATFORM_RUNTIME_REAL,
        MISSION_PLATFORM_BOUNDED_SIMULATION,
        MISSION_PLATFORM_CONTRACT_ONLY,
    ]);
    let expected_proof_kind_counts = BTreeMap::from([
        (MISSION_PLATFORM_RUNTIME_REAL.to_string(), 20_usize),
        (MISSION_PLATFORM_BOUNDED_SIMULATION.to_string(), 14_usize),
        (MISSION_PLATFORM_CONTRACT_ONLY.to_string(), 6_usize),
    ]);
    let inventory_hash_matches = serde_json::to_vec(&catalog.capabilities)
        .ok()
        .is_some_and(|bytes| hex_sha256(&bytes) == catalog.inventory_sha256);
    catalog.schema_version == MISSION_PLATFORM_CATALOG_SCHEMA_VERSION
        && catalog.capability_count == MISSION_PLATFORM_CAPABILITY_COUNT
        && catalog.capabilities.len() == MISSION_PLATFORM_CAPABILITY_COUNT
        && actual_numbers == expected_numbers
        && unique_ids
        && proof_kinds == expected_proof_kinds
        && catalog.proof_kind_counts == expected_proof_kind_counts
        && valid_sha256(&catalog.inventory_sha256)
        && inventory_hash_matches
        && !catalog.production_ready
        && catalog
            .capabilities
            .iter()
            .all(|capability| !capability.production_ready)
}

pub fn build_production_readiness_plan(version: &str) -> Result<ProductionReadinessPlanReport> {
    let capability_status = build_milestone_status(version)?;
    let mission_platform_catalog = mission_platform_catalog();
    let capability_ready = capability_status.promotion_decision.capability_ready
        && mission_platform_inventory_is_exact(&mission_platform_catalog);
    let requirement = |gate_id: &str, evidence: &[&str]| ProductionReadinessRequirement {
        gate_id: gate_id.to_string(),
        required_evidence: evidence.iter().map(|value| (*value).to_string()).collect(),
        blocking: true,
        max_evidence_age_seconds: MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
    };

    Ok(ProductionReadinessPlanReport {
        schema_version: PRODUCTION_READINESS_PLAN_SCHEMA_VERSION.to_string(),
        milestone: capability_status.milestone,
        profile: PRODUCTION_PROFILE.to_string(),
        evaluation_mode: "plan_only".to_string(),
        capability_ready,
        capability_inventory_count: mission_platform_catalog.capability_count,
        capability_inventory_sha256: mission_platform_catalog.inventory_sha256,
        capability_proof_kind_counts: mission_platform_catalog.proof_kind_counts,
        required_gate_count: PRODUCTION_READINESS_REQUIRED_GATE_COUNT,
        required_receipt_count: PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT,
        production_ready: false,
        requirements: vec![
            requirement("capability_readiness", &["milestone capability status"]),
            requirement(
                "manifest_integrity",
                &["fresh secret-free production readiness manifest"],
            ),
            requirement(
                "release_integrity",
                &[
                    "successful release matrix",
                    "published artifact manifest",
                    "CycloneDX SBOM",
                    "checksum manifest",
                    "Sigstore verification",
                    "build provenance",
                ],
            ),
            requirement(
                "installed_ops_health",
                &[
                    "installed version and binary digest",
                    "Ops, runtime and request-supervisor services active",
                    "authenticated Ops health",
                ],
            ),
            requirement(
                "off_host_recovery",
                &["complete off-host upload/verify/download/restore challenge"],
            ),
            requirement(
                "key_escrow",
                &["separately protected encrypted vault-key escrow recovery"],
            ),
            requirement(
                "alerting",
                &[
                    "Ops, runtime and request-supervisor service-failure alerts",
                    "store, backup, off-host, disk and backup-age alerts",
                ],
            ),
            requirement(
                "restore_drill",
                &["disposable restore drill proving RPO <= 24h and RTO <= 30m"],
            ),
            requirement(
                "upgrade_rollback",
                &["bounded upgrade and rollback simulation for the target version"],
            ),
            requirement(
                "bounded_load",
                &["bounded load, resource, crash-restart and store-check simulation"],
            ),
            requirement(
                "mission_operational_lifecycle",
                &[
                    "exact canonical capability inventory 1-40",
                    "real execution receipt",
                    "queued submission receipt linked to execution",
                    "resume receipt consuming the same handoff",
                ],
            ),
        ],
        commands_executed: 0,
        mutations_performed: false,
        next_action:
            "Collect the listed evidence into the secret-free manifest, then run the read-only evaluator; no command or infrastructure mutation is authorized by this plan."
                .to_string(),
    })
}

pub fn write_production_evidence_template(
    options: ProductionEvidenceTemplateOptions<'_>,
) -> Result<ProductionEvidenceTemplateReport> {
    validate_production_evidence_release_identity(options.version, options.release_version)?;
    let evidence_root = canonical_production_evidence_root(options.evidence_root)?;
    let template_path =
        contained_production_relative_path(options.template_path, "production evidence template")?;
    let template = production_evidence_draft_template(options.version, options.release_version);
    let unresolved_field_count = count_json_nulls(&template);
    let mut bytes = serde_json::to_vec_pretty(&template)
        .context("failed serialize production evidence template")?;
    bytes.push(b'\n');
    let persisted_path = write_new_contained_production_file(
        &evidence_root,
        &template_path,
        &bytes,
        "production evidence template",
    )?;

    Ok(ProductionEvidenceTemplateReport {
        schema_version: PRODUCTION_EVIDENCE_TEMPLATE_REPORT_SCHEMA_VERSION.to_string(),
        status: "template_written".to_string(),
        template_path: relative_production_path_string(&template_path, "template path")?,
        release_version: options.release_version.to_string(),
        unresolved_field_count,
        files_written: 1,
        infrastructure_commands_executed: 0,
        infrastructure_mutations_performed: false,
        next_action: format!(
            "Fill every null with observed claims and source artifacts, paste the exact production-mission-evidence manifest_section, then assemble {}.",
            persisted_path.display()
        ),
    })
}

pub fn assemble_production_evidence(
    options: ProductionEvidenceAssemblyOptions<'_>,
) -> Result<ProductionEvidenceAssemblyReport> {
    validate_production_evidence_release_identity(options.version, options.release_version)?;
    let evidence_root = canonical_production_evidence_root(options.evidence_root)?;
    let draft_path =
        contained_production_relative_path(options.draft_path, "production evidence draft")?;
    let receipt_directory = contained_production_relative_path(
        options.receipt_directory,
        "production evidence receipt directory",
    )?;
    let manifest_path =
        contained_production_relative_path(options.manifest_path, "production readiness manifest")?;
    if draft_path == manifest_path {
        bail!("production evidence draft and output manifest paths must differ");
    }

    let draft_path_value = relative_production_path_string(&draft_path, "draft path")?;
    let draft_file = resolve_production_evidence_path(&evidence_root, &draft_path_value)?;
    let draft_bytes =
        read_bounded_production_text_artifact(&draft_file, "production evidence draft")?;
    let mut draft: serde_json::Value =
        serde_json::from_slice(&draft_bytes).context("failed parse production evidence draft")?;
    if let Some(path) = first_json_null_path(&draft) {
        bail!("production evidence draft contains unresolved null at {path}");
    }
    validate_production_draft_top_level_keys(&draft)?;

    let draft_object = draft
        .as_object_mut()
        .context("production evidence draft must be a JSON object")?;
    let schema_version = draft_object
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .context("production evidence draft schema_version must be a string")?;
    if schema_version != PRODUCTION_EVIDENCE_DRAFT_SCHEMA_VERSION {
        bail!(
            "unsupported production evidence draft schema `{schema_version}`; expected `{PRODUCTION_EVIDENCE_DRAFT_SCHEMA_VERSION}`"
        );
    }
    let sources_value = draft_object
        .remove("sources")
        .context("production evidence draft is missing sources")?;
    let sources: BTreeMap<String, ProductionEvidenceDraftSource> =
        serde_json::from_value(sources_value)
            .context("failed parse production evidence draft sources")?;
    validate_production_draft_source_kinds(&sources)?;
    inject_production_draft_evidence_refs(draft_object, &sources)?;
    draft_object.insert(
        "schema_version".to_string(),
        serde_json::Value::String(PRODUCTION_READINESS_MANIFEST_SCHEMA_VERSION.to_string()),
    );

    let mut manifest: ProductionReadinessManifest = serde_json::from_value(draft)
        .context("failed parse completed production evidence claims")?;
    validate_completed_production_draft(&manifest, options.version, options.release_version)?;
    let now_epoch =
        u64::try_from(Utc::now().timestamp()).context("system clock is before Unix epoch")?;

    struct PreparedReceipt {
        report: ProductionEvidenceAssemblyReceiptReport,
        artifact_path: PathBuf,
        bytes: Vec<u8>,
        evidence: ProductionEvidenceRef,
    }

    let mut prepared = Vec::with_capacity(PRODUCTION_EVIDENCE_ASSEMBLY_KINDS.len());
    let mut output_paths = BTreeSet::new();
    let mut canonical_source_paths = BTreeSet::new();
    let draft_path_string = relative_production_path_string(&draft_path, "draft path")?;
    let manifest_path_string = relative_production_path_string(&manifest_path, "manifest path")?;
    output_paths.insert(draft_path_string.clone());
    if !output_paths.insert(manifest_path_string.clone()) {
        bail!("production evidence output paths must be unique");
    }

    for kind in PRODUCTION_EVIDENCE_ASSEMBLY_KINDS {
        let source = sources
            .get(kind)
            .with_context(|| format!("production evidence source `{kind}` is missing"))?;
        if !evidence_epoch_is_fresh(
            source.observed_at_epoch,
            now_epoch,
            MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
        ) {
            bail!("production evidence source `{kind}` is future-dated, zero or stale");
        }
        let source_path =
            contained_production_relative_path(Path::new(&source.artifact_path), kind)?;
        let source_path_string =
            relative_production_path_string(&source_path, "source artifact path")?;
        let canonical_source =
            resolve_production_evidence_path(&evidence_root, &source_path_string)
                .with_context(|| format!("failed resolve source artifact for `{kind}`"))?;
        if !canonical_source_paths.insert(canonical_source.clone()) {
            bail!("production evidence source `{kind}` reuses another source artifact");
        }
        let source_bytes = read_bounded_production_text_artifact(&canonical_source, kind)?;
        let claims = production_readiness_claims_value(&manifest, kind)?;
        validate_production_source_evidence(
            &source_bytes,
            kind,
            options.release_version,
            source.observed_at_epoch,
            &claims,
        )?;
        let source_artifact_sha256 = hex_sha256(&source_bytes);

        let receipt_path = receipt_directory.join(format!("{kind}.json"));
        let receipt_path =
            contained_production_relative_path(&receipt_path, "production evidence receipt")?;
        let receipt_path_string =
            relative_production_path_string(&receipt_path, "receipt artifact path")?;
        if !output_paths.insert(receipt_path_string.clone()) {
            bail!("production evidence output path `{receipt_path_string}` is duplicated");
        }
        if receipt_path_string == source_path_string {
            bail!("production evidence receipt `{kind}` must not overwrite its source artifact");
        }

        let claims_sha256 = hex_sha256(
            &serde_json::to_vec(&claims)
                .context("failed to serialize canonical production evidence claims")?,
        );
        let receipt = ProductionEvidenceReceipt {
            schema_version: production_source_bound_receipt_schema(kind),
            kind: kind.to_string(),
            status: "passed".to_string(),
            subject_version: options.release_version.to_string(),
            claims_sha256,
            source_artifact_path: Some(source_path_string.clone()),
            source_artifact_sha256: Some(source_artifact_sha256.clone()),
            source_observed_at_epoch: Some(source.observed_at_epoch),
        };
        let bytes = serde_json::to_vec(&receipt)
            .with_context(|| format!("failed serialize production evidence receipt `{kind}`"))?;
        let artifact_sha256 = hex_sha256(&bytes);
        let evidence = ProductionEvidenceRef {
            artifact_path: receipt_path_string.clone(),
            artifact_sha256: artifact_sha256.clone(),
            observed_at_epoch: source.observed_at_epoch,
        };
        prepared.push(PreparedReceipt {
            report: ProductionEvidenceAssemblyReceiptReport {
                kind: kind.to_string(),
                artifact_path: receipt_path_string,
                artifact_sha256,
                source_artifact_path: source_path_string,
                source_artifact_sha256,
                observed_at_epoch: source.observed_at_epoch,
            },
            artifact_path: receipt_path,
            bytes,
            evidence,
        });
    }

    for receipt in &prepared {
        set_production_evidence_ref(
            &mut manifest,
            &receipt.report.kind,
            receipt.evidence.clone(),
        )?;
    }

    let mission_artifact_path = manifest
        .mission_operational_lifecycle
        .evidence
        .artifact_path
        .clone();
    if output_paths.contains(&mission_artifact_path) {
        bail!("mission operational lifecycle artifact path conflicts with assembly output");
    }
    bind_existing_mission_lifecycle_artifact(
        &mut manifest,
        &evidence_root,
        now_epoch,
        options.release_version,
    )?;

    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .context("failed serialize production readiness manifest")?;
    manifest_bytes.push(b'\n');
    if manifest_bytes.is_empty()
        || u64::try_from(manifest_bytes.len()).unwrap_or(u64::MAX) > MAX_PRODUCTION_EVIDENCE_BYTES
    {
        bail!(
            "production readiness manifest must contain 1..={MAX_PRODUCTION_EVIDENCE_BYTES} bytes"
        );
    }
    let manifest_sha256 = hex_sha256(&manifest_bytes);

    let receipt_states = prepared
        .iter()
        .map(|receipt| {
            inspect_exact_production_output(
                &evidence_root,
                &receipt.artifact_path,
                &receipt.bytes,
                "production evidence receipt",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let manifest_state = inspect_exact_production_output(
        &evidence_root,
        &manifest_path,
        &manifest_bytes,
        "production readiness manifest",
    )?;
    if manifest_state == ExactProductionOutputState::Exact
        && receipt_states.contains(&ExactProductionOutputState::Missing)
    {
        bail!(
            "committed production evidence manifest exists but its receipt set is incomplete; refusing to repair files behind a published commit marker"
        );
    }

    let mut files_written = 0_u64;
    if manifest_state == ExactProductionOutputState::Missing {
        for receipt in &prepared {
            if write_or_reuse_exact_production_output(
                &evidence_root,
                &receipt.artifact_path,
                &receipt.bytes,
                "production evidence receipt",
            )? {
                files_written = files_written.saturating_add(1);
            }
        }

        // The manifest is the assembly commit marker. Recheck and durably sync
        // every receipt immediately before publishing it so an interrupted run
        // can resume from exact receipt bytes without exposing a partial assembly.
        for receipt in &prepared {
            sync_exact_production_output(
                &evidence_root,
                &receipt.artifact_path,
                &receipt.bytes,
                "production evidence receipt",
            )?;
        }
        if write_or_reuse_exact_production_output(
            &evidence_root,
            &manifest_path,
            &manifest_bytes,
            "production readiness manifest",
        )? {
            files_written = files_written.saturating_add(1);
        }
    } else {
        for receipt in &prepared {
            sync_exact_production_output(
                &evidence_root,
                &receipt.artifact_path,
                &receipt.bytes,
                "production evidence receipt",
            )?;
        }
        sync_exact_production_output(
            &evidence_root,
            &manifest_path,
            &manifest_bytes,
            "production readiness manifest",
        )?;
    }

    for receipt in &prepared {
        if inspect_exact_production_output(
            &evidence_root,
            &receipt.artifact_path,
            &receipt.bytes,
            "production evidence receipt",
        )? != ExactProductionOutputState::Exact
        {
            bail!("production evidence receipt is missing after manifest commit");
        }
    }
    if inspect_exact_production_output(
        &evidence_root,
        &manifest_path,
        &manifest_bytes,
        "production readiness manifest",
    )? != ExactProductionOutputState::Exact
    {
        bail!("production readiness manifest commit marker is missing after assembly");
    }

    Ok(ProductionEvidenceAssemblyReport {
        schema_version: PRODUCTION_EVIDENCE_ASSEMBLY_REPORT_SCHEMA_VERSION.to_string(),
        status: "assembled".to_string(),
        manifest_path: manifest_path_string.clone(),
        manifest_sha256,
        release_version: options.release_version.to_string(),
        receipt_count: prepared.len(),
        source_artifact_count: sources.len(),
        receipts: prepared.into_iter().map(|receipt| receipt.report).collect(),
        mission_operational_lifecycle_artifact: mission_artifact_path,
        files_written,
        infrastructure_commands_executed: 0,
        infrastructure_mutations_performed: false,
        next_action: format!(
            "Run `foundry milestone production-readiness --version {} --manifest {} --evidence-root <same-root> --output json`; assembly does not promote readiness.",
            options.version, manifest_path_string
        ),
    })
}

fn production_evidence_draft_template(version: &str, release_version: &str) -> serde_json::Value {
    let sources = PRODUCTION_EVIDENCE_ASSEMBLY_KINDS
        .into_iter()
        .map(|kind| {
            (
                kind.to_string(),
                serde_json::json!({
                    "artifact_path": null,
                    "observed_at_epoch": null,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({
        "schema_version": PRODUCTION_EVIDENCE_DRAFT_SCHEMA_VERSION,
        "milestone": version,
        "profile": PRODUCTION_PROFILE,
        "release_version": release_version,
        "generated_at_epoch": null,
        "release": {
            "successful_targets": null,
            "binary_sha256_by_target": null,
            "sbom_format": null,
            "sbom_component_count": null,
            "checksum_entry_count": null,
            "checksums_verified": null,
            "sigstore_verified": null,
            "provenance_verified": null
        },
        "installation": {
            "target": null,
            "installed_version": null,
            "installed_binary_sha256": null,
            "ops_service_active": null,
            "runtime_service_active": null,
            "request_supervisor_service_active": null,
            "store_check_passed": null,
            "ops_authenticated_probe_passed": null,
            "ops_http_status": null,
            "ops_loopback_only": null
        },
        "off_host_backup": {
            "recovery_challenge_epoch": null,
            "immutable_upload_passed": null,
            "remote_digest_verified": null,
            "download_digest_verified": null,
            "downloaded_store_check_passed": null,
            "disposable_restore_passed": null,
            "restored_store_check_passed": null,
            "off_host_retention_enabled": null,
            "foundry_key_isolated_from_uploader": null,
            "uploader_credentials_isolated_from_foundry": null
        },
        "key_escrow": {
            "encrypted": null,
            "separate_access_control": null,
            "recovery_key_available": null,
            "restore_with_escrowed_key_tested": null,
            "excluded_from_database_backup": null
        },
        "alerts": {
            "ops_service_failure_alert": null,
            "runtime_service_failure_alert": null,
            "request_supervisor_service_failure_alert": null,
            "store_check_alert": null,
            "backup_timer_alert": null,
            "off_host_failure_alert": null,
            "disk_space_alert": null,
            "backup_age_alert": null,
            "delivery_route_verified": null
        },
        "restore_drill": {
            "drill_epoch": null,
            "disposable_recovery_host": null,
            "downloaded_store_check_passed": null,
            "restored_store_check_passed": null,
            "canary_workflow_verified": null,
            "ops_authenticated_probe_passed": null,
            "rpo_seconds": null,
            "rto_seconds": null
        },
        "upgrade_rollback": {
            "target_version": null,
            "simulation_completed": null,
            "pre_upgrade_backup_verified": null,
            "upgraded_store_check_passed": null,
            "upgraded_ops_health_passed": null,
            "rollback_completed": null,
            "previous_version_store_check_passed": null,
            "previous_version_ops_health_passed": null,
            "target_reinstalled_and_healthy": null
        },
        "bounded_load": {
            "duration_seconds": null,
            "concurrency": null,
            "operation_count": null,
            "error_count": null,
            "p95_latency_millis": null,
            "max_rss_bytes": null,
            "max_rss_limit_bytes": null,
            "timeout_enforced": null,
            "resource_limit_enforced": null,
            "store_check_passed": null,
            "crash_restart_verified": null
        },
        "mission_operational_lifecycle": null,
        "sources": serde_json::Value::Object(sources)
    })
}

fn validate_production_evidence_release_identity(
    version: &str,
    release_version: &str,
) -> Result<()> {
    let version = canonical_milestone_version(version)?;
    if release_version != env!("CARGO_PKG_VERSION") {
        bail!(
            "production evidence release version `{release_version}` must match running Foundry version `{}`",
            env!("CARGO_PKG_VERSION")
        );
    }
    if !release_version_matches_milestone(release_version, version) {
        bail!(
            "production evidence release version `{release_version}` does not belong to milestone `{version}`"
        );
    }
    Ok(())
}

fn canonical_production_evidence_root(evidence_root: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(evidence_root).with_context(|| {
        format!(
            "failed inspect production evidence root {}",
            evidence_root.display()
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("production evidence root must be a non-symlink directory");
    }
    fs::canonicalize(evidence_root).with_context(|| {
        format!(
            "failed resolve production evidence root {}",
            evidence_root.display()
        )
    })
}

fn contained_production_relative_path(value: &Path, label: &str) -> Result<PathBuf> {
    if value.as_os_str().is_empty()
        || value.is_absolute()
        || !value
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("{label} must be a contained relative path");
    }
    Ok(value.to_path_buf())
}

fn relative_production_path_string(value: &Path, label: &str) -> Result<String> {
    value
        .to_str()
        .map(ToOwned::to_owned)
        .with_context(|| format!("{label} must be valid UTF-8"))
}

fn count_json_nulls(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Null => 1,
        serde_json::Value::Array(values) => values.iter().map(count_json_nulls).sum(),
        serde_json::Value::Object(values) => values.values().map(count_json_nulls).sum(),
        _ => 0,
    }
}

fn first_json_null_path(value: &serde_json::Value) -> Option<String> {
    fn visit(value: &serde_json::Value, path: &str) -> Option<String> {
        match value {
            serde_json::Value::Null => Some(path.to_string()),
            serde_json::Value::Array(values) => values
                .iter()
                .enumerate()
                .find_map(|(index, value)| visit(value, &format!("{path}[{index}]"))),
            serde_json::Value::Object(values) => values
                .iter()
                .find_map(|(key, value)| visit(value, &format!("{path}.{key}"))),
            _ => None,
        }
    }
    visit(value, "$")
}

fn validate_production_draft_top_level_keys(draft: &serde_json::Value) -> Result<()> {
    let object = draft
        .as_object()
        .context("production evidence draft must be a JSON object")?;
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = [
        "schema_version",
        "milestone",
        "profile",
        "release_version",
        "generated_at_epoch",
        "release",
        "installation",
        "off_host_backup",
        "key_escrow",
        "alerts",
        "restore_drill",
        "upgrade_rollback",
        "bounded_load",
        "mission_operational_lifecycle",
        "sources",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let unknown = actual.difference(&expected).copied().collect::<Vec<_>>();
        bail!(
            "production evidence draft top-level keys differ: missing={missing:?} unknown={unknown:?}"
        );
    }
    Ok(())
}

fn validate_production_draft_source_kinds(
    sources: &BTreeMap<String, ProductionEvidenceDraftSource>,
) -> Result<()> {
    let actual = sources.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = PRODUCTION_EVIDENCE_ASSEMBLY_KINDS
        .into_iter()
        .collect::<BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let unknown = actual.difference(&expected).copied().collect::<Vec<_>>();
        bail!(
            "production evidence draft must contain all 13 source kinds exactly: missing={missing:?} unknown={unknown:?}"
        );
    }
    Ok(())
}

fn draft_evidence_ref(source: &ProductionEvidenceDraftSource) -> Result<serde_json::Value> {
    serde_json::to_value(ProductionEvidenceRef {
        artifact_path: source.artifact_path.clone(),
        artifact_sha256: String::new(),
        observed_at_epoch: source.observed_at_epoch,
    })
    .context("failed serialize production evidence draft source reference")
}

fn draft_object_section_mut<'a>(
    draft: &'a mut serde_json::Map<String, serde_json::Value>,
    section: &str,
) -> Result<&'a mut serde_json::Map<String, serde_json::Value>> {
    draft
        .get_mut(section)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("production evidence draft section `{section}` must be an object"))
}

fn inject_production_draft_evidence_refs(
    draft: &mut serde_json::Map<String, serde_json::Value>,
    sources: &BTreeMap<String, ProductionEvidenceDraftSource>,
) -> Result<()> {
    let source = |kind: &str| {
        sources
            .get(kind)
            .with_context(|| format!("production evidence source `{kind}` is missing"))
            .and_then(draft_evidence_ref)
    };
    let release = draft_object_section_mut(draft, "release")?;
    release.insert("matrix".to_string(), source("release_matrix")?);
    release.insert("artifacts".to_string(), source("release_artifacts")?);
    release.insert("sbom".to_string(), source("release_sbom")?);
    release.insert("checksums".to_string(), source("release_checksums")?);
    release.insert("sigstore".to_string(), source("release_sigstore")?);
    release.insert("provenance".to_string(), source("release_provenance")?);
    for (section, kind) in [
        ("installation", "installation"),
        ("off_host_backup", "off_host_recovery"),
        ("key_escrow", "key_escrow"),
        ("alerts", "alerting"),
        ("restore_drill", "restore_drill"),
        ("upgrade_rollback", "upgrade_rollback"),
        ("bounded_load", "bounded_load"),
    ] {
        draft_object_section_mut(draft, section)?.insert("evidence".to_string(), source(kind)?);
    }
    Ok(())
}

fn validate_completed_production_draft(
    manifest: &ProductionReadinessManifest,
    version: &str,
    release_version: &str,
) -> Result<()> {
    if manifest.schema_version != PRODUCTION_READINESS_MANIFEST_SCHEMA_VERSION {
        bail!("completed production evidence draft produced unsupported manifest schema");
    }
    if manifest.milestone != version {
        bail!("production evidence draft milestone does not match --version");
    }
    if manifest.profile != PRODUCTION_PROFILE {
        bail!("production evidence draft profile is unsupported");
    }
    if manifest.release_version != release_version {
        bail!("production evidence draft release_version does not match --release-version");
    }
    let now_epoch =
        u64::try_from(Utc::now().timestamp()).context("system clock is before Unix epoch")?;
    if !evidence_epoch_is_fresh(
        manifest.generated_at_epoch,
        now_epoch,
        MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
    ) {
        bail!("production evidence draft generated_at_epoch is future-dated, zero or stale");
    }
    Ok(())
}

fn read_bounded_production_text_artifact(path: &Path, label: &str) -> Result<Vec<u8>> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed inspect {label} {}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_PRODUCTION_EVIDENCE_BYTES
    {
        bail!(
            "{label} must be a non-empty file no larger than {MAX_PRODUCTION_EVIDENCE_BYTES} bytes"
        );
    }
    let bytes =
        fs::read(path).with_context(|| format!("failed read {label} {}", path.display()))?;
    let text = std::str::from_utf8(&bytes)
        .with_context(|| format!("{label} must be inspectable UTF-8 text"))?;
    if !production_text_is_secret_free(text, "production_evidence_assembly") {
        bail!("{label} contains detected secret material");
    }
    Ok(bytes)
}

fn production_source_evidence_schema(kind: &str) -> String {
    format!("{PRODUCTION_SOURCE_EVIDENCE_SCHEMA_PREFIX}.{kind}.v1")
}

fn validate_production_source_evidence(
    bytes: &[u8],
    kind: &str,
    release_version: &str,
    observed_at_epoch: u64,
    expected_claims: &serde_json::Value,
) -> Result<()> {
    let source: ProductionSourceEvidence = serde_json::from_slice(bytes).with_context(|| {
        format!("production evidence source `{kind}` is not a typed attestation")
    })?;
    if source.schema_version != production_source_evidence_schema(kind) {
        bail!("production evidence source `{kind}` has an unsupported schema");
    }
    if source.kind != kind {
        bail!("production evidence source `{kind}` declares another evidence kind");
    }
    if source.status != "passed" {
        bail!("production evidence source `{kind}` does not record a passed outcome");
    }
    if source.subject_version != release_version {
        bail!("production evidence source `{kind}` is bound to another release");
    }
    if source.observed_at_epoch != observed_at_epoch {
        bail!("production evidence source `{kind}` observation differs from the draft");
    }
    if source.execution_mode != "production" {
        bail!("production evidence source `{kind}` was not collected in production mode");
    }
    if source.producer.trim().is_empty()
        || source.producer.len() > 128
        || source.producer.chars().any(char::is_control)
    {
        bail!("production evidence source `{kind}` has an invalid producer identity");
    }
    if source
        .claims
        .as_object()
        .is_none_or(|claims| claims.is_empty())
        || source.claims != *expected_claims
    {
        bail!("production evidence source `{kind}` claims do not match the completed draft");
    }
    if source
        .evidence
        .as_object()
        .is_none_or(|evidence| evidence.is_empty())
    {
        bail!("production evidence source `{kind}` has no structured collector evidence");
    }
    Ok(())
}

fn production_source_bound_receipt_schema(kind: &str) -> String {
    format!("foundry.milestone.production_evidence.{kind}.v2")
}

fn legacy_production_receipt_schema(kind: &str) -> String {
    format!("foundry.milestone.production_evidence.{kind}.v1")
}

fn production_receipt_schema_supported(kind: &str, schema: &str, subject_version: &str) -> bool {
    schema == production_source_bound_receipt_schema(kind)
        || (subject_version == LAST_LEGACY_PRODUCTION_RECEIPT_VERSION
            && schema == legacy_production_receipt_schema(kind))
}

fn set_production_evidence_ref(
    manifest: &mut ProductionReadinessManifest,
    kind: &str,
    evidence: ProductionEvidenceRef,
) -> Result<()> {
    match kind {
        "release_matrix" => manifest.release.matrix = evidence,
        "release_artifacts" => manifest.release.artifacts = evidence,
        "release_sbom" => manifest.release.sbom = evidence,
        "release_checksums" => manifest.release.checksums = evidence,
        "release_sigstore" => manifest.release.sigstore = evidence,
        "release_provenance" => manifest.release.provenance = evidence,
        "installation" => manifest.installation.evidence = evidence,
        "off_host_recovery" => manifest.off_host_backup.evidence = evidence,
        "key_escrow" => manifest.key_escrow.evidence = evidence,
        "alerting" => manifest.alerts.evidence = evidence,
        "restore_drill" => manifest.restore_drill.evidence = evidence,
        "upgrade_rollback" => manifest.upgrade_rollback.evidence = evidence,
        "bounded_load" => manifest.bounded_load.evidence = evidence,
        _ => bail!("unsupported production evidence assembly kind `{kind}`"),
    }
    Ok(())
}

fn bind_existing_mission_lifecycle_artifact(
    manifest: &mut ProductionReadinessManifest,
    evidence_root: &Path,
    now_epoch: u64,
    release_version: &str,
) -> Result<()> {
    let evidence = manifest.mission_operational_lifecycle.evidence.clone();
    if !evidence_epoch_is_fresh(
        evidence.observed_at_epoch,
        now_epoch,
        MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
    ) {
        bail!("mission operational lifecycle artifact is future-dated, zero or stale");
    }
    let path = resolve_production_evidence_path(evidence_root, &evidence.artifact_path)
        .context("failed resolve mission operational lifecycle artifact")?;
    let bytes = read_bounded_production_text_artifact(&path, "mission operational lifecycle")?;
    let actual_sha256 = hex_sha256(&bytes);
    if !evidence.artifact_sha256.is_empty() && evidence.artifact_sha256 != actual_sha256 {
        bail!("mission operational lifecycle artifact digest drifted from draft");
    }
    let receipt: ProductionMissionLifecycleReceipt = serde_json::from_slice(&bytes)
        .context("failed parse mission operational lifecycle receipt")?;
    let expected_claims =
        production_readiness_claims_sha256(manifest, "mission_operational_lifecycle")?;
    if receipt.schema_version != PRODUCTION_MISSION_LIFECYCLE_RECEIPT_SCHEMA_VERSION
        || receipt.kind != "mission_operational_lifecycle"
        || receipt.status != "passed"
        || receipt.subject_version != release_version
        || receipt.claims_sha256 != expected_claims
    {
        bail!("mission operational lifecycle receipt does not match completed draft claims");
    }
    manifest
        .mission_operational_lifecycle
        .evidence
        .artifact_sha256 = actual_sha256;
    Ok(())
}

fn ensure_new_production_output_path(evidence_root: &Path, relative: &Path) -> Result<()> {
    let relative = contained_production_relative_path(relative, "production evidence output")?;
    let target = evidence_root.join(relative);
    match fs::symlink_metadata(&target) {
        Ok(_) => bail!(
            "production evidence output already exists and will not be overwritten: {}",
            target.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed inspect production evidence output {}",
                target.display()
            )
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExactProductionOutputState {
    Missing,
    Exact,
}

fn inspect_exact_production_output(
    evidence_root: &Path,
    relative: &Path,
    expected_bytes: &[u8],
    label: &str,
) -> Result<ExactProductionOutputState> {
    let relative = contained_production_relative_path(relative, label)?;
    let unresolved_target = evidence_root.join(&relative);
    match fs::symlink_metadata(&unresolved_target) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ExactProductionOutputState::Missing);
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed inspect existing {label} {}",
                    unresolved_target.display()
                )
            });
        }
    }
    let parent = ensure_contained_production_parent(evidence_root, &relative)?;
    let file_name = relative
        .file_name()
        .with_context(|| format!("{label} requires a file name"))?;
    let target = parent.join(file_name);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ExactProductionOutputState::Missing);
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed inspect existing {label} {}", target.display()));
        }
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!(
            "{label} already exists but is not a regular non-symlink file and will not be overwritten: {}",
            target.display()
        );
    }
    if metadata.len() != u64::try_from(expected_bytes.len()).unwrap_or(u64::MAX) {
        bail!(
            "{label} already exists with different content and will not be overwritten: {}",
            target.display()
        );
    }
    let canonical = fs::canonicalize(&target)
        .with_context(|| format!("failed resolve existing {label} {}", target.display()))?;
    if !canonical.starts_with(evidence_root) {
        bail!("{label} escapes the production evidence root");
    }
    let actual_bytes = fs::read(&canonical)
        .with_context(|| format!("failed read existing {label} {}", target.display()))?;
    if actual_bytes != expected_bytes {
        bail!(
            "{label} already exists with different content and will not be overwritten: {}",
            target.display()
        );
    }
    Ok(ExactProductionOutputState::Exact)
}

fn write_or_reuse_exact_production_output(
    evidence_root: &Path,
    relative: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<bool> {
    if inspect_exact_production_output(evidence_root, relative, bytes, label)?
        == ExactProductionOutputState::Exact
    {
        return Ok(false);
    }
    write_new_contained_production_file(evidence_root, relative, bytes, label)?;
    Ok(true)
}

fn sync_exact_production_output(
    evidence_root: &Path,
    relative: &Path,
    expected_bytes: &[u8],
    label: &str,
) -> Result<()> {
    if inspect_exact_production_output(evidence_root, relative, expected_bytes, label)?
        != ExactProductionOutputState::Exact
    {
        bail!("{label} disappeared before durable assembly commit");
    }
    let relative = contained_production_relative_path(relative, label)?;
    let parent = ensure_contained_production_parent(evidence_root, &relative)?;
    let file_name = relative
        .file_name()
        .with_context(|| format!("{label} requires a file name"))?;
    let target = parent.join(file_name);
    fs::File::open(&target)
        .and_then(|file| file.sync_all())
        .with_context(|| format!("failed sync existing {label} {}", target.display()))?;
    fs::File::open(&parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| {
            format!(
                "failed sync existing {label} directory {}",
                parent.display()
            )
        })?;
    Ok(())
}

fn ensure_contained_production_parent(evidence_root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut parent = evidence_root.to_path_buf();
    if let Some(relative_parent) = relative.parent() {
        for component in relative_parent.components() {
            let Component::Normal(segment) = component else {
                bail!("production evidence output parent must stay contained");
            };
            let candidate = parent.join(segment);
            match fs::symlink_metadata(&candidate) {
                Ok(metadata) => {
                    if !metadata.is_dir() || metadata.file_type().is_symlink() {
                        bail!(
                            "production evidence output parent must be a non-symlink directory: {}",
                            candidate.display()
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&candidate).with_context(|| {
                        format!(
                            "failed create production evidence output directory {}",
                            candidate.display()
                        )
                    })?;
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed inspect production evidence output directory {}",
                            candidate.display()
                        )
                    });
                }
            }
            parent = fs::canonicalize(&candidate).with_context(|| {
                format!(
                    "failed resolve production evidence output directory {}",
                    candidate.display()
                )
            })?;
            if !parent.starts_with(evidence_root) {
                bail!("production evidence output parent escapes evidence root");
            }
        }
    }
    Ok(parent)
}

fn write_new_contained_production_file(
    evidence_root: &Path,
    relative: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<PathBuf> {
    write_new_contained_production_file_with_post_publish(
        evidence_root,
        relative,
        bytes,
        label,
        |_| Ok(()),
    )
}

fn write_new_contained_production_file_with_post_publish<F>(
    evidence_root: &Path,
    relative: &Path,
    bytes: &[u8],
    label: &str,
    post_publish: F,
) -> Result<PathBuf>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let relative = contained_production_relative_path(relative, label)?;
    ensure_new_production_output_path(evidence_root, &relative)?;
    let parent = ensure_contained_production_parent(evidence_root, &relative)?;
    let file_name = relative
        .file_name()
        .with_context(|| format!("{label} requires a file name"))?;
    let target = parent.join(file_name);
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    let mut published = false;
    let persist_result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| {
                format!("failed create {label} staging file {}", temporary.display())
            })?;
        file.write_all(bytes).with_context(|| {
            format!("failed write {label} staging file {}", temporary.display())
        })?;
        file.sync_all()
            .with_context(|| format!("failed sync {label} staging file {}", temporary.display()))?;
        drop(file);
        fs::hard_link(&temporary, &target)
            .with_context(|| format!("failed publish no-overwrite {label} {}", target.display()))?;
        published = true;
        post_publish(&target)?;
        fs::remove_file(&temporary).with_context(|| {
            format!(
                "failed remove published {label} staging link {}",
                temporary.display()
            )
        })?;
        fs::File::open(&parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| format!("failed sync {label} directory {}", parent.display()))?;
        Ok(())
    })();
    if let Err(error) = persist_result {
        let mut cleanup_failures = Vec::new();
        if published {
            if let Err(cleanup_error) = fs::remove_file(&target) {
                cleanup_failures.push(format!("{}: {cleanup_error}", target.display()));
            }
        }
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {}
            Err(cleanup_error) => {
                cleanup_failures.push(format!("{}: {cleanup_error}", temporary.display()));
            }
        }
        if published {
            if let Err(cleanup_error) =
                fs::File::open(&parent).and_then(|directory| directory.sync_all())
            {
                cleanup_failures.push(format!("{}: {cleanup_error}", parent.display()));
            }
        }
        if cleanup_failures.is_empty() {
            return Err(error);
        }
        bail!(
            "{error:#}; additionally failed rollback published {label}: {}",
            cleanup_failures.join(", ")
        );
    }
    Ok(target)
}

fn production_mission_operational_claims_value(
    lifecycle: &ProductionMissionOperationalEvidence,
) -> serde_json::Value {
    serde_json::json!({
        "capability_inventory_schema_version": &lifecycle.capability_inventory_schema_version,
        "capability_inventory_sha256": &lifecycle.capability_inventory_sha256,
        "capability_numbers": &lifecycle.capability_numbers,
        "mission_id": &lifecycle.mission_id,
        "workflow_id": &lifecycle.workflow_id,
        "task_id": &lifecycle.task_id,
        "agent_id": &lifecycle.agent_id,
        "execute_receipt_schema_version": &lifecycle.execute_receipt_schema_version,
        "execute_receipt_id": &lifecycle.execute_receipt_id,
        "execute_receipt_sha256": &lifecycle.execute_receipt_sha256,
        "execute_status": &lifecycle.execute_status,
        "execution_attempted": lifecycle.execution_attempted,
        "executed": lifecycle.executed,
        "execute_exit_code": lifecycle.execute_exit_code,
        "submit_receipt_schema_version": &lifecycle.submit_receipt_schema_version,
        "submit_receipt_sha256": &lifecycle.submit_receipt_sha256,
        "submit_status": &lifecycle.submit_status,
        "submit_queued": lifecycle.submit_queued,
        "submitted_execute_receipt_sha256": &lifecycle.submitted_execute_receipt_sha256,
        "handoff_id": &lifecycle.handoff_id,
        "inbox_id": &lifecycle.inbox_id,
        "resume_receipt_schema_version": &lifecycle.resume_receipt_schema_version,
        "resume_receipt_sha256": &lifecycle.resume_receipt_sha256,
        "resume_status": &lifecycle.resume_status,
        "resume_action": &lifecycle.resume_action,
        "resumed_handoff_id": &lifecycle.resumed_handoff_id,
        "resume_consumed": lifecycle.resume_consumed,
        "execute_observed_at_epoch": lifecycle.execute_observed_at_epoch,
        "submit_observed_at_epoch": lifecycle.submit_observed_at_epoch,
        "resume_observed_at_epoch": lifecycle.resume_observed_at_epoch,
    })
}

pub fn production_mission_operational_claims_sha256(
    lifecycle: &ProductionMissionOperationalEvidence,
) -> Result<String> {
    let bytes = serde_json::to_vec(&production_mission_operational_claims_value(lifecycle))
        .context("failed to serialize canonical mission lifecycle claims")?;
    Ok(hex_sha256(&bytes))
}

pub fn build_production_mission_lifecycle_evidence(
    store: &FoundryStore,
    release_version: &str,
    mission_id: &str,
    execution_receipt_id: &str,
    artifact_path: &str,
) -> Result<ProductionMissionLifecycleEvidencePackage> {
    let relative_artifact_path = Path::new(artifact_path);
    if artifact_path.trim().is_empty()
        || relative_artifact_path.is_absolute()
        || !relative_artifact_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("mission lifecycle artifact path must be a contained relative path");
    }
    if release_version.trim().is_empty() {
        bail!("mission lifecycle evidence requires a release version");
    }

    let mission = load_mission(store, mission_id)?;
    if mission.mode != MissionMode::Workflow || mission.worktree.is_none() {
        bail!("mission lifecycle evidence requires a real workflow mission bound to a worktree");
    }
    if mission.status != MissionStatus::Completed {
        bail!("mission lifecycle evidence requires a terminal completed mission");
    }
    let workflow = store.load_workflow(&mission.workflow_id)?;
    let validation = validate_workflow(&workflow);
    if validation.status != "passed"
        || !validation.promotable
        || !validation.failed_rules.is_empty()
        || !validation.rework_tasks.is_empty()
    {
        bail!("mission lifecycle evidence requires a promotable workflow with no rework");
    }
    let execution_receipt = load_mission_execution_receipt(store, execution_receipt_id)?;
    verify_mission_execution_receipt(&execution_receipt)?;
    let execution_sandbox_valid = execution_receipt.sandbox.as_ref().is_some_and(|sandbox| {
        sandbox.status == "sandbox_completed"
            && sandbox.runtime == "bubblewrap"
            && sandbox.filesystem_isolation_enforced
            && sandbox.network_isolation_enforced
            && sandbox.command_sha256 == execution_receipt.command_sha256
            && sandbox.exit_code == Some(0)
            && !sandbox.timed_out
            && !sandbox.output_truncated
            && sandbox.error.is_none()
    });
    if execution_receipt.schema_version != MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION
        || execution_receipt.mission_id != mission.id
        || execution_receipt.workflow_id != mission.workflow_id
        || execution_receipt.status != "completed"
        || !execution_receipt.allowed
        || !execution_receipt.execution_attempted
        || !execution_receipt.executed
        || execution_receipt.exit_code != Some(0)
        || execution_receipt.timed_out
        || execution_receipt.approval.is_none()
        || execution_receipt.policy_trace.is_empty()
        || execution_receipt
            .policy_trace
            .iter()
            .any(|decision| !decision.allowed)
        || !execution_sandbox_valid
        || execution_receipt.claims.is_empty()
        || execution_receipt.evidence.is_empty()
        || execution_receipt.consumed_at.is_none()
    {
        bail!(
            "mission lifecycle execution receipt is not an approved, isolated, completed and consumed v3 receipt"
        );
    }
    let execution_reference = format!("execution_receipt:{}", execution_receipt.receipt_id);
    let execution_digest = format!(
        "execution_receipt_sha256:{}",
        execution_receipt.receipt_sha256
    );
    let handoff = mission
        .handoffs
        .iter()
        .find(|handoff| {
            handoff.task_id == execution_receipt.task_id
                && handoff.from_agent == execution_receipt.agent_id
                && handoff.validations.contains(&execution_reference)
                && handoff.validations.contains(&execution_digest)
        })
        .cloned()
        .context("mission lifecycle execution receipt has no persisted submission handoff")?;
    if handoff.status != "accepted" || handoff.accepted_at.is_none() {
        bail!("mission lifecycle handoff has not been accepted");
    }
    if execution_receipt.consumed_by_submission.as_deref() != Some(handoff.idempotency_key.as_str())
    {
        bail!("mission lifecycle execution receipt is consumed by another submission");
    }
    let inbox = mission
        .inbox
        .iter()
        .find(|inbox| inbox.handoff_id == handoff.id)
        .cloned()
        .context("mission lifecycle handoff has no persisted inbox item")?;
    if inbox.status != "consumed" || inbox.consumed_at.is_none() {
        bail!("mission lifecycle inbox item has not been consumed");
    }

    let connection = Connection::open_with_flags(
        store.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("failed to open mission lifecycle store read-only")?;
    let mut statement = connection.prepare(
        r#"
        SELECT data_json
        FROM mission_runtime_checkpoints
        WHERE mission_id=?1
        ORDER BY revision
        "#,
    )?;
    let rows = statement.query_map([mission_id], |row| row.get::<_, String>(0))?;
    let checkpoints = rows
        .map(|row| {
            let json = row?;
            serde_json::from_str::<MissionRecord>(&json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    json.len(),
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let submission_snapshot = checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint
                .handoffs
                .iter()
                .any(|candidate| candidate.id == handoff.id && candidate.status == "queued")
                && checkpoint
                    .inbox
                    .iter()
                    .any(|candidate| candidate.id == inbox.id && candidate.status == "pending")
        })
        .cloned()
        .context("mission lifecycle submission checkpoint is missing")?;
    let resume_snapshot = checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint
                .handoffs
                .iter()
                .any(|candidate| candidate.id == handoff.id && candidate.status == "accepted")
                && checkpoint
                    .inbox
                    .iter()
                    .any(|candidate| candidate.id == inbox.id && candidate.status == "consumed")
                && mission_lifecycle_event_order_is_valid(
                    checkpoint,
                    &handoff.id,
                    &execution_receipt.task_id,
                )
        })
        .cloned()
        .context("mission lifecycle resume checkpoint is missing")?;

    let submission = MissionSubmission {
        idempotency_key: handoff.idempotency_key.clone(),
        execution_receipt_id: execution_receipt.receipt_id.clone(),
        task_id: handoff.task_id.clone(),
        agent_id: handoff.from_agent.clone(),
        status: handoff.delivery.status.clone(),
        summary: handoff.delivery.summary.clone(),
        artifacts: handoff.delivery.artifacts.clone(),
        validations: handoff.validations.clone(),
        risks: handoff.delivery.risks.clone(),
        followups: handoff.delivery.followups.clone(),
        tests_passed: handoff.delivery.tests_passed,
        tests_failed: handoff.delivery.tests_failed,
    };
    let submit_report = MissionSubmitReport {
        schema_version: "foundry.mission.submit.v1".to_string(),
        status: "queued".to_string(),
        mission_id: mission.id.clone(),
        handoff_id: handoff.id.clone(),
        inbox_id: inbox.id.clone(),
        producer_revision: submission_snapshot.revision,
        deduplicated: false,
        accepted: false,
    };
    let resume_report = MissionDriveReport {
        schema_version: "foundry.mission.drive.v1".to_string(),
        status: format!("{:?}", resume_snapshot.status).to_lowercase(),
        action: "handoff_consumed".to_string(),
        mission_id: mission.id.clone(),
        revision: resume_snapshot.revision,
        assignments: Vec::new(),
        assignment: None,
        handoff_id: Some(handoff.id.clone()),
        mission: resume_snapshot,
    };
    let catalog = mission_platform_catalog();
    let mut artifact = ProductionMissionLifecycleReceipt {
        schema_version: PRODUCTION_MISSION_LIFECYCLE_RECEIPT_SCHEMA_VERSION.to_string(),
        kind: "mission_operational_lifecycle".to_string(),
        status: "passed".to_string(),
        subject_version: release_version.to_string(),
        claims_sha256: String::new(),
        capability_inventory_schema_version: catalog.schema_version,
        capability_inventory_sha256: catalog.inventory_sha256,
        capability_numbers: catalog
            .capabilities
            .iter()
            .map(|capability| capability.number)
            .collect(),
        execution_receipt,
        submission,
        submit_report,
        resume_report,
    };
    let execute_observed_at_epoch = rfc3339_epoch(&artifact.execution_receipt.finished_at)
        .context("mission lifecycle execution timestamp is invalid")?;
    let submit_observed_at_epoch = datetime_epoch(&handoff.created_at)
        .context("mission lifecycle submission timestamp is invalid")?;
    let resume_observed_at_epoch = inbox
        .consumed_at
        .as_ref()
        .and_then(datetime_epoch)
        .context("mission lifecycle resume timestamp is invalid")?;
    let mut manifest_section = ProductionMissionOperationalEvidence {
        evidence: ProductionEvidenceRef {
            artifact_path: artifact_path.to_string(),
            artifact_sha256: String::new(),
            observed_at_epoch: resume_observed_at_epoch,
        },
        capability_inventory_schema_version: artifact.capability_inventory_schema_version.clone(),
        capability_inventory_sha256: artifact.capability_inventory_sha256.clone(),
        capability_numbers: artifact.capability_numbers.clone(),
        mission_id: artifact.execution_receipt.mission_id.clone(),
        workflow_id: artifact.execution_receipt.workflow_id.clone(),
        task_id: artifact.execution_receipt.task_id.clone(),
        agent_id: artifact.execution_receipt.agent_id.clone(),
        execute_receipt_schema_version: artifact.execution_receipt.schema_version.clone(),
        execute_receipt_id: artifact.execution_receipt.receipt_id.clone(),
        execute_receipt_sha256: artifact.execution_receipt.receipt_sha256.clone(),
        execute_status: artifact.execution_receipt.status.clone(),
        execution_attempted: artifact.execution_receipt.execution_attempted,
        executed: artifact.execution_receipt.executed,
        execute_exit_code: artifact.execution_receipt.exit_code,
        submit_receipt_schema_version: artifact.submit_report.schema_version.clone(),
        submit_receipt_sha256: serialized_sha256(&artifact.submit_report)?,
        submit_status: artifact.submit_report.status.clone(),
        submit_queued: artifact.submit_report.status == "queued",
        submitted_execute_receipt_sha256: artifact.execution_receipt.receipt_sha256.clone(),
        handoff_id: artifact.submit_report.handoff_id.clone(),
        inbox_id: artifact.submit_report.inbox_id.clone(),
        resume_receipt_schema_version: artifact.resume_report.schema_version.clone(),
        resume_receipt_sha256: serialized_sha256(&artifact.resume_report)?,
        resume_status: artifact.resume_report.status.clone(),
        resume_action: artifact.resume_report.action.clone(),
        resumed_handoff_id: artifact
            .resume_report
            .handoff_id
            .clone()
            .context("mission lifecycle resume report has no handoff")?,
        resume_consumed: artifact.resume_report.action == "handoff_consumed",
        execute_observed_at_epoch,
        submit_observed_at_epoch,
        resume_observed_at_epoch,
    };
    artifact.claims_sha256 = production_mission_operational_claims_sha256(&manifest_section)?;
    let artifact_bytes = serde_json::to_vec(&artifact)?;
    manifest_section.evidence.artifact_sha256 = hex_sha256(&artifact_bytes);
    Ok(ProductionMissionLifecycleEvidencePackage {
        schema_version: "foundry.milestone.mission_lifecycle_evidence_package.v1".to_string(),
        status: "ready".to_string(),
        manifest_section,
        artifact,
    })
}

pub fn production_readiness_claims_value(
    manifest: &ProductionReadinessManifest,
    kind: &str,
) -> Result<serde_json::Value> {
    let claims = match kind {
        "release_matrix" | "release_artifacts" | "release_sbom" | "release_checksums"
        | "release_sigstore" | "release_provenance" => serde_json::json!({
            "successful_targets": &manifest.release.successful_targets,
            "binary_sha256_by_target": &manifest.release.binary_sha256_by_target,
            "sbom_format": &manifest.release.sbom_format,
            "sbom_component_count": manifest.release.sbom_component_count,
            "checksum_entry_count": manifest.release.checksum_entry_count,
            "checksums_verified": manifest.release.checksums_verified,
            "sigstore_verified": manifest.release.sigstore_verified,
            "provenance_verified": manifest.release.provenance_verified,
        }),
        "installation" => serde_json::json!({
            "target": &manifest.installation.target,
            "installed_version": &manifest.installation.installed_version,
            "installed_binary_sha256": &manifest.installation.installed_binary_sha256,
            "ops_service_active": manifest.installation.ops_service_active,
            "runtime_service_active": manifest.installation.runtime_service_active,
            "request_supervisor_service_active": manifest.installation.request_supervisor_service_active,
            "store_check_passed": manifest.installation.store_check_passed,
            "ops_authenticated_probe_passed": manifest.installation.ops_authenticated_probe_passed,
            "ops_http_status": manifest.installation.ops_http_status,
            "ops_loopback_only": manifest.installation.ops_loopback_only,
        }),
        "off_host_recovery" => serde_json::json!({
            "recovery_challenge_epoch": manifest.off_host_backup.recovery_challenge_epoch,
            "immutable_upload_passed": manifest.off_host_backup.immutable_upload_passed,
            "remote_digest_verified": manifest.off_host_backup.remote_digest_verified,
            "download_digest_verified": manifest.off_host_backup.download_digest_verified,
            "downloaded_store_check_passed": manifest.off_host_backup.downloaded_store_check_passed,
            "disposable_restore_passed": manifest.off_host_backup.disposable_restore_passed,
            "restored_store_check_passed": manifest.off_host_backup.restored_store_check_passed,
            "off_host_retention_enabled": manifest.off_host_backup.off_host_retention_enabled,
            "foundry_key_isolated_from_uploader": manifest.off_host_backup.foundry_key_isolated_from_uploader,
            "uploader_credentials_isolated_from_foundry": manifest.off_host_backup.uploader_credentials_isolated_from_foundry,
        }),
        "key_escrow" => serde_json::json!({
            "encrypted": manifest.key_escrow.encrypted,
            "separate_access_control": manifest.key_escrow.separate_access_control,
            "recovery_key_available": manifest.key_escrow.recovery_key_available,
            "restore_with_escrowed_key_tested": manifest.key_escrow.restore_with_escrowed_key_tested,
            "excluded_from_database_backup": manifest.key_escrow.excluded_from_database_backup,
        }),
        "alerting" => serde_json::json!({
            "ops_service_failure_alert": manifest.alerts.ops_service_failure_alert,
            "runtime_service_failure_alert": manifest.alerts.runtime_service_failure_alert,
            "request_supervisor_service_failure_alert": manifest.alerts.request_supervisor_service_failure_alert,
            "store_check_alert": manifest.alerts.store_check_alert,
            "backup_timer_alert": manifest.alerts.backup_timer_alert,
            "off_host_failure_alert": manifest.alerts.off_host_failure_alert,
            "disk_space_alert": manifest.alerts.disk_space_alert,
            "backup_age_alert": manifest.alerts.backup_age_alert,
            "delivery_route_verified": manifest.alerts.delivery_route_verified,
        }),
        "restore_drill" => serde_json::json!({
            "drill_epoch": manifest.restore_drill.drill_epoch,
            "disposable_recovery_host": manifest.restore_drill.disposable_recovery_host,
            "downloaded_store_check_passed": manifest.restore_drill.downloaded_store_check_passed,
            "restored_store_check_passed": manifest.restore_drill.restored_store_check_passed,
            "canary_workflow_verified": manifest.restore_drill.canary_workflow_verified,
            "ops_authenticated_probe_passed": manifest.restore_drill.ops_authenticated_probe_passed,
            "rpo_seconds": manifest.restore_drill.rpo_seconds,
            "rto_seconds": manifest.restore_drill.rto_seconds,
        }),
        "upgrade_rollback" => serde_json::json!({
            "target_version": &manifest.upgrade_rollback.target_version,
            "simulation_completed": manifest.upgrade_rollback.simulation_completed,
            "pre_upgrade_backup_verified": manifest.upgrade_rollback.pre_upgrade_backup_verified,
            "upgraded_store_check_passed": manifest.upgrade_rollback.upgraded_store_check_passed,
            "upgraded_ops_health_passed": manifest.upgrade_rollback.upgraded_ops_health_passed,
            "rollback_completed": manifest.upgrade_rollback.rollback_completed,
            "previous_version_store_check_passed": manifest.upgrade_rollback.previous_version_store_check_passed,
            "previous_version_ops_health_passed": manifest.upgrade_rollback.previous_version_ops_health_passed,
            "target_reinstalled_and_healthy": manifest.upgrade_rollback.target_reinstalled_and_healthy,
        }),
        "bounded_load" => serde_json::json!({
            "duration_seconds": manifest.bounded_load.duration_seconds,
            "concurrency": manifest.bounded_load.concurrency,
            "operation_count": manifest.bounded_load.operation_count,
            "error_count": manifest.bounded_load.error_count,
            "p95_latency_millis": manifest.bounded_load.p95_latency_millis,
            "max_rss_bytes": manifest.bounded_load.max_rss_bytes,
            "max_rss_limit_bytes": manifest.bounded_load.max_rss_limit_bytes,
            "timeout_enforced": manifest.bounded_load.timeout_enforced,
            "resource_limit_enforced": manifest.bounded_load.resource_limit_enforced,
            "store_check_passed": manifest.bounded_load.store_check_passed,
            "crash_restart_verified": manifest.bounded_load.crash_restart_verified,
        }),
        "mission_operational_lifecycle" => {
            production_mission_operational_claims_value(&manifest.mission_operational_lifecycle)
        }
        _ => bail!("unsupported production evidence kind `{kind}`"),
    };
    Ok(claims)
}

pub fn production_readiness_claims_sha256(
    manifest: &ProductionReadinessManifest,
    kind: &str,
) -> Result<String> {
    let claims = production_readiness_claims_value(manifest, kind)?;
    let bytes = serde_json::to_vec(&claims)
        .context("failed to serialize canonical production evidence claims")?;
    Ok(hex_sha256(&bytes))
}

pub fn evaluate_production_readiness(
    options: ProductionReadinessOptions<'_>,
) -> Result<ProductionReadinessReport> {
    let now_epoch =
        u64::try_from(Utc::now().timestamp()).context("system clock is before the Unix epoch")?;
    let capability_status = build_milestone_status(options.version)?;
    let mission_platform_catalog = mission_platform_catalog();
    let mission_platform_inventory_exact =
        mission_platform_inventory_is_exact(&mission_platform_catalog);
    let evidence_root = fs::canonicalize(options.evidence_root).with_context(|| {
        format!(
            "failed to resolve production evidence root {}",
            options.evidence_root.display()
        )
    })?;
    if !evidence_root.is_dir() {
        bail!(
            "production evidence root is not a directory: {}",
            options.evidence_root.display()
        );
    }
    let manifest_path = resolve_production_manifest_path(&evidence_root, options.manifest_path)?;
    let manifest_bytes = fs::read(&manifest_path).with_context(|| {
        format!(
            "failed to read production readiness manifest {}",
            manifest_path.display()
        )
    })?;
    if manifest_bytes.is_empty()
        || u64::try_from(manifest_bytes.len()).unwrap_or(u64::MAX) > MAX_PRODUCTION_EVIDENCE_BYTES
    {
        bail!(
            "production readiness manifest must contain 1..={MAX_PRODUCTION_EVIDENCE_BYTES} bytes"
        );
    }
    let manifest_text = std::str::from_utf8(&manifest_bytes)
        .context("production readiness manifest must be UTF-8 JSON")?;
    if !production_text_is_secret_free(manifest_text, "production_readiness_manifest") {
        bail!("production readiness manifest must be secret-free");
    }
    let manifest: ProductionReadinessManifest = serde_json::from_slice(&manifest_bytes)
        .context("failed to parse production readiness manifest")?;

    let capability_ready =
        capability_status.promotion_decision.capability_ready && mission_platform_inventory_exact;
    let mut gates = Vec::new();
    gates.push(production_gate(
        "capability_readiness",
        "Capability readiness",
        vec![
            production_check(
                "required_capabilities",
                capability_status.promotion_decision.capability_ready,
                "all required milestone capabilities are ready",
                "one or more required milestone capabilities are not ready",
            ),
            production_check(
                "mission_platform_inventory_1_40",
                mission_platform_inventory_exact,
                "canonical mission platform inventory contains exactly classified capabilities 1-40",
                "mission platform inventory is incomplete, reordered, duplicated or has an unsupported proof classification",
            ),
        ],
    ));

    let evidence_refs = production_evidence_refs(&manifest);
    let unique_evidence_paths = evidence_refs
        .iter()
        .map(|(_, evidence)| evidence.artifact_path.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        == evidence_refs.len();
    let release_version_matches =
        release_version_matches_milestone(&manifest.release_version, options.version);
    gates.push(production_gate(
        "manifest_integrity",
        "Manifest integrity",
        vec![
            production_check(
                "schema_version",
                manifest.schema_version == PRODUCTION_READINESS_MANIFEST_SCHEMA_VERSION,
                "manifest schema is supported",
                "manifest schema is unsupported",
            ),
            production_check(
                "milestone",
                manifest.milestone == options.version,
                "manifest milestone matches the requested capability line",
                "manifest milestone does not match the requested capability line",
            ),
            production_check(
                "profile",
                manifest.profile == PRODUCTION_PROFILE,
                "manifest targets the supported single-host Linux profile",
                "manifest targets an unsupported production profile",
            ),
            production_check(
                "release_version",
                release_version_matches,
                "release version belongs to the milestone line",
                "release version does not belong to the milestone line",
            ),
            production_check(
                "generated_at",
                evidence_epoch_is_fresh(
                    manifest.generated_at_epoch,
                    now_epoch,
                    MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
                ),
                "manifest was generated within the allowed evidence window",
                "manifest timestamp is future-dated, zero or stale",
            ),
            production_check(
                "unique_evidence_paths",
                unique_evidence_paths,
                "each required claim has an independent evidence artifact",
                "one evidence artifact is reused for multiple required claims",
            ),
        ],
    ));

    let successful_targets = manifest
        .release
        .successful_targets
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let successful_targets_unique =
        successful_targets.len() == manifest.release.successful_targets.len();
    let all_targets_passed = REQUIRED_RELEASE_TARGETS
        .iter()
        .all(|target| successful_targets.contains(target));
    let all_target_digests_present = REQUIRED_RELEASE_TARGETS.iter().all(|target| {
        manifest
            .release
            .binary_sha256_by_target
            .get(*target)
            .is_some_and(|digest| valid_sha256(digest))
    });
    let mut release_checks = Vec::new();
    for (kind, evidence) in [
        ("release_matrix", &manifest.release.matrix),
        ("release_artifacts", &manifest.release.artifacts),
        ("release_sbom", &manifest.release.sbom),
        ("release_checksums", &manifest.release.checksums),
        ("release_sigstore", &manifest.release.sigstore),
        ("release_provenance", &manifest.release.provenance),
    ] {
        let claims_sha256 = production_readiness_claims_sha256(&manifest, kind)?;
        release_checks.extend(production_evidence_checks(
            kind,
            evidence,
            &evidence_root,
            now_epoch,
            &manifest.release_version,
            &claims_sha256,
        ));
    }
    release_checks.extend([
        production_check(
            "matrix_targets",
            all_targets_passed && successful_targets_unique,
            "all required release targets passed exactly once",
            "release matrix is missing a required target or contains duplicates",
        ),
        production_check(
            "binary_target_digests",
            all_target_digests_present,
            "every required target has a lowercase SHA-256 binary digest",
            "one or more required target binary digests are missing or malformed",
        ),
        production_check(
            "sbom_format",
            manifest.release.sbom_format == "cyclonedx-json"
                && manifest.release.sbom_component_count > 0,
            "non-empty CycloneDX JSON SBOM is recorded",
            "SBOM format or component count is invalid",
        ),
        production_check(
            "checksums_verified",
            manifest.release.checksums_verified
                && manifest.release.checksum_entry_count
                    >= u64::try_from(REQUIRED_RELEASE_TARGETS.len()).unwrap_or(u64::MAX),
            "checksum manifest covers and verifies all required target artifacts",
            "checksum manifest was not verified or does not cover all required targets",
        ),
        production_check(
            "sigstore_verified",
            manifest.release.sigstore_verified,
            "Sigstore bundle verification passed",
            "Sigstore bundle verification is absent or failed",
        ),
        production_check(
            "provenance_verified",
            manifest.release.provenance_verified,
            "release build provenance verification passed",
            "release build provenance verification is absent or failed",
        ),
    ]);
    gates.push(production_gate(
        "release_integrity",
        "Release matrix and supply-chain integrity",
        release_checks,
    ));

    let installed_target_digest = manifest
        .release
        .binary_sha256_by_target
        .get(&manifest.installation.target);
    let installation_claims = production_readiness_claims_sha256(&manifest, "installation")?;
    let mut installation_checks = production_evidence_checks(
        "installation",
        &manifest.installation.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &installation_claims,
    );
    installation_checks.extend([
        production_check(
            "installed_version",
            manifest.installation.installed_version == manifest.release_version,
            "installed Foundry version matches the evaluated release",
            "installed Foundry version does not match the evaluated release",
        ),
        production_check(
            "installed_binary",
            valid_sha256(&manifest.installation.installed_binary_sha256)
                && installed_target_digest
                    .is_some_and(|digest| digest == &manifest.installation.installed_binary_sha256),
            "installed binary digest matches its published target artifact",
            "installed binary digest is malformed or differs from the published artifact",
        ),
        production_check(
            "ops_service_active",
            manifest.installation.ops_service_active,
            "Foundry Ops service is active",
            "Foundry Ops service is inactive",
        ),
        production_check(
            "runtime_service_active",
            manifest.installation.runtime_service_active,
            "Foundry runtime service is active",
            "Foundry runtime service is inactive",
        ),
        production_check(
            "request_supervisor_service_active",
            manifest.installation.request_supervisor_service_active,
            "Foundry request-supervisor service is active",
            "Foundry request-supervisor service is inactive",
        ),
        production_check(
            "store_check",
            manifest.installation.store_check_passed,
            "production store check passed",
            "production store check failed",
        ),
        production_check(
            "ops_health",
            manifest.installation.ops_authenticated_probe_passed
                && manifest.installation.ops_http_status == 200
                && manifest.installation.ops_loopback_only,
            "authenticated loopback Ops health returned HTTP 200",
            "Ops health is unauthenticated, unhealthy or not loopback-only",
        ),
    ]);
    gates.push(production_gate(
        "installed_ops_health",
        "Installed version and Ops health",
        installation_checks,
    ));

    let backup_claims = production_readiness_claims_sha256(&manifest, "off_host_recovery")?;
    let mut backup_checks = production_evidence_checks(
        "off_host_recovery",
        &manifest.off_host_backup.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &backup_claims,
    );
    backup_checks.extend([
        production_check(
            "recovery_challenge_fresh",
            evidence_epoch_is_fresh(
                manifest.off_host_backup.recovery_challenge_epoch,
                now_epoch,
                MAX_RESTORE_RPO_SECONDS,
            ),
            "latest complete off-host recovery challenge is within the RPO window",
            "off-host recovery challenge timestamp is future-dated, zero or older than the RPO window",
        ),
        production_check(
            "immutable_remote_cycle",
            manifest.off_host_backup.immutable_upload_passed
                && manifest.off_host_backup.remote_digest_verified
                && manifest.off_host_backup.download_digest_verified
                && manifest.off_host_backup.off_host_retention_enabled,
            "immutable upload, source-independent verify, download digest and retention passed",
            "off-host immutable upload, verification, download or retention evidence failed",
        ),
        production_check(
            "recovery_store_checks",
            manifest.off_host_backup.downloaded_store_check_passed
                && manifest.off_host_backup.disposable_restore_passed
                && manifest.off_host_backup.restored_store_check_passed,
            "downloaded backup and disposable restore both passed store checks",
            "downloaded backup or disposable restore store check failed",
        ),
        production_check(
            "credential_isolation",
            manifest.off_host_backup.foundry_key_isolated_from_uploader
                && manifest
                    .off_host_backup
                    .uploader_credentials_isolated_from_foundry,
            "Foundry vault key and uploader authority stayed mutually isolated",
            "Foundry vault key or uploader authority isolation was not proven",
        ),
    ]);
    gates.push(production_gate(
        "off_host_recovery",
        "Off-host backup recovery challenge",
        backup_checks,
    ));

    let escrow_claims = production_readiness_claims_sha256(&manifest, "key_escrow")?;
    let mut escrow_checks = production_evidence_checks(
        "key_escrow",
        &manifest.key_escrow.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &escrow_claims,
    );
    escrow_checks.push(production_check(
        "escrow_controls",
        manifest.key_escrow.encrypted
            && manifest.key_escrow.separate_access_control
            && manifest.key_escrow.recovery_key_available
            && manifest.key_escrow.restore_with_escrowed_key_tested
            && manifest.key_escrow.excluded_from_database_backup,
        "encrypted separately controlled key escrow restored encrypted data and stayed outside the database backup",
        "key escrow encryption, separation, availability, restore test or backup exclusion is missing",
    ));
    gates.push(production_gate(
        "key_escrow",
        "Vault-key escrow",
        escrow_checks,
    ));

    let alert_claims = production_readiness_claims_sha256(&manifest, "alerting")?;
    let mut alert_checks = production_evidence_checks(
        "alerting",
        &manifest.alerts.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &alert_claims,
    );
    alert_checks.push(production_check(
        "required_alerts",
        manifest.alerts.ops_service_failure_alert
            && manifest.alerts.runtime_service_failure_alert
            && manifest.alerts.request_supervisor_service_failure_alert
            && manifest.alerts.store_check_alert
            && manifest.alerts.backup_timer_alert
            && manifest.alerts.off_host_failure_alert
            && manifest.alerts.disk_space_alert
            && manifest.alerts.backup_age_alert
            && manifest.alerts.delivery_route_verified,
        "all required operational alerts and their delivery route were verified",
        "one or more required operational alerts or the delivery route is unverified",
    ));
    gates.push(production_gate(
        "alerting",
        "Operational alerting",
        alert_checks,
    ));

    let restore_claims = production_readiness_claims_sha256(&manifest, "restore_drill")?;
    let mut restore_checks = production_evidence_checks(
        "restore_drill",
        &manifest.restore_drill.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &restore_claims,
    );
    restore_checks.extend([
        production_check(
            "drill_fresh",
            evidence_epoch_is_fresh(
                manifest.restore_drill.drill_epoch,
                now_epoch,
                MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
            ),
            "restore drill completed within the evidence window",
            "restore drill timestamp is future-dated, zero or stale",
        ),
        production_check(
            "drill_integrity",
            manifest.restore_drill.disposable_recovery_host
                && manifest.restore_drill.downloaded_store_check_passed
                && manifest.restore_drill.restored_store_check_passed
                && manifest.restore_drill.canary_workflow_verified
                && manifest.restore_drill.ops_authenticated_probe_passed,
            "disposable restore, store checks, canary and authenticated Ops probe passed",
            "restore drill environment, store checks, canary or Ops probe failed",
        ),
        production_check(
            "rpo",
            manifest.restore_drill.rpo_seconds <= MAX_RESTORE_RPO_SECONDS,
            "measured restore drill RPO is at most 24 hours",
            "measured restore drill RPO exceeds 24 hours",
        ),
        production_check(
            "rto",
            manifest.restore_drill.rto_seconds <= MAX_RESTORE_RTO_SECONDS,
            "measured restore drill RTO is at most 30 minutes",
            "measured restore drill RTO exceeds 30 minutes",
        ),
    ]);
    gates.push(production_gate(
        "restore_drill",
        "Restore drill RPO/RTO",
        restore_checks,
    ));

    let upgrade_claims = production_readiness_claims_sha256(&manifest, "upgrade_rollback")?;
    let mut upgrade_checks = production_evidence_checks(
        "upgrade_rollback",
        &manifest.upgrade_rollback.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &upgrade_claims,
    );
    upgrade_checks.extend([
        production_check(
            "target_version",
            manifest.upgrade_rollback.target_version == manifest.release_version,
            "upgrade/rollback simulation targets the evaluated release",
            "upgrade/rollback simulation targets another release",
        ),
        production_check(
            "upgrade_and_rollback",
            manifest.upgrade_rollback.simulation_completed
                && manifest.upgrade_rollback.pre_upgrade_backup_verified
                && manifest.upgrade_rollback.upgraded_store_check_passed
                && manifest.upgrade_rollback.upgraded_ops_health_passed
                && manifest.upgrade_rollback.rollback_completed
                && manifest
                    .upgrade_rollback
                    .previous_version_store_check_passed
                && manifest.upgrade_rollback.previous_version_ops_health_passed
                && manifest.upgrade_rollback.target_reinstalled_and_healthy,
            "bounded upgrade, rollback and target reinstall simulation passed",
            "upgrade, rollback or target reinstall simulation evidence is incomplete",
        ),
    ]);
    gates.push(production_gate(
        "upgrade_rollback",
        "Upgrade and rollback simulation",
        upgrade_checks,
    ));

    let load_claims = production_readiness_claims_sha256(&manifest, "bounded_load")?;
    let mut load_checks = production_evidence_checks(
        "bounded_load",
        &manifest.bounded_load.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &load_claims,
    );
    load_checks.extend([
        production_check(
            "bounded_window",
            manifest.bounded_load.duration_seconds > 0
                && manifest.bounded_load.duration_seconds <= MAX_BOUNDED_LOAD_SECONDS
                && manifest.bounded_load.concurrency > 0
                && manifest.bounded_load.concurrency <= MAX_BOUNDED_LOAD_CONCURRENCY
                && manifest.bounded_load.timeout_enforced,
            "load simulation stayed inside the bounded time and concurrency policy",
            "load simulation is unbounded, empty or exceeds time/concurrency policy",
        ),
        production_check(
            "load_result",
            manifest.bounded_load.operation_count >= MIN_BOUNDED_LOAD_OPERATIONS
                && manifest.bounded_load.error_count == 0
                && manifest.bounded_load.p95_latency_millis <= MAX_BOUNDED_LOAD_P95_MILLIS,
            "bounded load completed the minimum operations without errors inside latency policy",
            "bounded load volume, errors or p95 latency failed policy",
        ),
        production_check(
            "resource_bound",
            manifest.bounded_load.resource_limit_enforced
                && manifest.bounded_load.max_rss_limit_bytes > 0
                && manifest.bounded_load.max_rss_bytes > 0
                && manifest.bounded_load.max_rss_bytes <= manifest.bounded_load.max_rss_limit_bytes,
            "load simulation enforced and stayed within its memory limit",
            "load simulation memory limit is absent, invalid or exceeded",
        ),
        production_check(
            "post_load_recovery",
            manifest.bounded_load.store_check_passed
                && manifest.bounded_load.crash_restart_verified,
            "post-load store check and crash/restart recovery passed",
            "post-load store check or crash/restart recovery failed",
        ),
    ]);
    gates.push(production_gate(
        "bounded_load",
        "Bounded production load simulation",
        load_checks,
    ));

    let lifecycle = &manifest.mission_operational_lifecycle;
    let lifecycle_claims =
        production_readiness_claims_sha256(&manifest, "mission_operational_lifecycle")?;
    let mut lifecycle_checks = production_evidence_checks(
        "mission_operational_lifecycle",
        &lifecycle.evidence,
        &evidence_root,
        now_epoch,
        &manifest.release_version,
        &lifecycle_claims,
    );
    let lifecycle_receipt =
        load_production_mission_lifecycle_receipt(&evidence_root, &lifecycle.evidence).ok();
    lifecycle_checks.push(production_check(
        "mission_operational_lifecycle.typed_bundle",
        lifecycle_receipt.is_some(),
        "mission lifecycle evidence contains typed execute, submit and resume receipts",
        "mission lifecycle evidence is missing or is not the canonical typed bundle",
    ));
    if let Some(receipt) = lifecycle_receipt.as_ref() {
        let execute = &receipt.execution_receipt;
        let submit_sha256 = serialized_sha256(&receipt.submit_report).ok();
        let resume_sha256 = serialized_sha256(&receipt.resume_report).ok();
        let handoff = receipt
            .resume_report
            .mission
            .handoffs
            .iter()
            .find(|handoff| handoff.id == receipt.submit_report.handoff_id);
        let inbox = receipt
            .resume_report
            .mission
            .inbox
            .iter()
            .find(|inbox| inbox.id == receipt.submit_report.inbox_id);
        let execute_epoch = rfc3339_epoch(&execute.finished_at);
        let submit_epoch = handoff.and_then(|handoff| datetime_epoch(&handoff.created_at));
        let resume_epoch =
            inbox.and_then(|inbox| inbox.consumed_at.as_ref().and_then(datetime_epoch));
        let expected_execution_reference = format!("execution_receipt:{}", execute.receipt_id);
        let expected_execution_digest =
            format!("execution_receipt_sha256:{}", execute.receipt_sha256);

        let execution_verified = execute.schema_version == MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION
            && verify_mission_execution_receipt(execute).is_ok()
            && execute.status == "completed"
            && execute.allowed
            && execute.execution_attempted
            && execute.executed
            && execute.exit_code == Some(0)
            && !execute.timed_out
            && execute.approval.is_some()
            && !execute.policy_trace.is_empty()
            && execute.policy_trace.iter().all(|decision| decision.allowed)
            && execute.sandbox.as_ref().is_some_and(|sandbox| {
                sandbox.status == "sandbox_completed"
                    && sandbox.runtime == "bubblewrap"
                    && sandbox.filesystem_isolation_enforced
                    && sandbox.network_isolation_enforced
                    && sandbox.command_sha256 == execute.command_sha256
                    && sandbox.exit_code == Some(0)
                    && !sandbox.timed_out
                    && !sandbox.output_truncated
                    && sandbox.error.is_none()
            })
            && !execute.claims.is_empty()
            && !execute.evidence.is_empty();
        lifecycle_checks.push(production_check(
            "mission_operational_lifecycle.typed_execute",
            execution_verified,
            "typed execution receipt is hash-valid, approved, isolated and successful",
            "typed execution receipt is invalid, unapproved, unisolated or unsuccessful",
        ));

        let submit_links_execution = receipt.submission.execution_receipt_id == execute.receipt_id
            && receipt.submission.idempotency_key.trim().len() >= 3
            && receipt.submission.task_id == execute.task_id
            && receipt.submission.agent_id == execute.agent_id
            && receipt.submission.status == "completed"
            && receipt
                .submission
                .validations
                .contains(&expected_execution_reference)
            && receipt
                .submission
                .validations
                .contains(&expected_execution_digest)
            && receipt.submit_report.schema_version == "foundry.mission.submit.v1"
            && receipt.submit_report.status == "queued"
            && receipt.submit_report.mission_id == execute.mission_id
            && !receipt.submit_report.handoff_id.trim().is_empty()
            && !receipt.submit_report.inbox_id.trim().is_empty()
            && receipt.submit_report.producer_revision >= execute.mission_revision
            && !receipt.submit_report.deduplicated
            && !receipt.submit_report.accepted;
        lifecycle_checks.push(production_check(
            "mission_operational_lifecycle.typed_submit",
            submit_links_execution,
            "typed submission references the exact execution receipt and queued handoff",
            "typed submission is detached from execution or is not the initial queued handoff",
        ));

        let resume_links_submission = receipt.resume_report.schema_version
            == "foundry.mission.drive.v1"
            && receipt.resume_report.action == "handoff_consumed"
            && receipt.resume_report.mission_id == execute.mission_id
            && receipt.resume_report.handoff_id.as_deref()
                == Some(receipt.submit_report.handoff_id.as_str())
            && receipt.resume_report.revision == receipt.resume_report.mission.revision
            && receipt.resume_report.revision >= receipt.submit_report.producer_revision
            && receipt.resume_report.mission.workflow_id == execute.workflow_id
            && handoff.is_some_and(|handoff| {
                handoff.status == "accepted"
                    && handoff.accepted_at.is_some()
                    && handoff.task_id == execute.task_id
                    && handoff.from_agent == execute.agent_id
            })
            && inbox.is_some_and(|inbox| {
                inbox.handoff_id == receipt.submit_report.handoff_id
                    && inbox.status == "consumed"
                    && inbox.consumed_at.is_some()
            })
            && mission_lifecycle_event_order_is_valid(
                &receipt.resume_report.mission,
                &receipt.submit_report.handoff_id,
                &execute.task_id,
            );
        lifecycle_checks.push(production_check(
            "mission_operational_lifecycle.typed_resume",
            resume_links_submission,
            "typed resume consumed the queued handoff with ordered persisted events",
            "typed resume is detached, unconsumed or missing ordered lifecycle events",
        ));

        let bundle_matches_manifest = receipt.capability_inventory_schema_version
            == lifecycle.capability_inventory_schema_version
            && receipt.capability_inventory_sha256 == lifecycle.capability_inventory_sha256
            && receipt.capability_numbers == lifecycle.capability_numbers
            && execute.mission_id == lifecycle.mission_id
            && execute.workflow_id == lifecycle.workflow_id
            && execute.task_id == lifecycle.task_id
            && execute.agent_id == lifecycle.agent_id
            && execute.schema_version == lifecycle.execute_receipt_schema_version
            && execute.receipt_id == lifecycle.execute_receipt_id
            && execute.receipt_sha256 == lifecycle.execute_receipt_sha256
            && execute.status == lifecycle.execute_status
            && execute.execution_attempted == lifecycle.execution_attempted
            && execute.executed == lifecycle.executed
            && execute.exit_code == lifecycle.execute_exit_code
            && receipt.submit_report.schema_version == lifecycle.submit_receipt_schema_version
            && submit_sha256.as_deref() == Some(lifecycle.submit_receipt_sha256.as_str())
            && receipt.submit_report.status == lifecycle.submit_status
            && (receipt.submit_report.status == "queued") == lifecycle.submit_queued
            && execute.receipt_sha256 == lifecycle.submitted_execute_receipt_sha256
            && receipt.submit_report.handoff_id == lifecycle.handoff_id
            && receipt.submit_report.inbox_id == lifecycle.inbox_id
            && receipt.resume_report.schema_version == lifecycle.resume_receipt_schema_version
            && resume_sha256.as_deref() == Some(lifecycle.resume_receipt_sha256.as_str())
            && receipt.resume_report.status == lifecycle.resume_status
            && receipt.resume_report.action == lifecycle.resume_action
            && receipt.resume_report.handoff_id.as_deref()
                == Some(lifecycle.resumed_handoff_id.as_str())
            && (receipt.resume_report.action == "handoff_consumed") == lifecycle.resume_consumed
            && execute_epoch == Some(lifecycle.execute_observed_at_epoch)
            && submit_epoch == Some(lifecycle.submit_observed_at_epoch)
            && resume_epoch == Some(lifecycle.resume_observed_at_epoch);
        lifecycle_checks.push(production_check(
            "mission_operational_lifecycle.bundle_claims",
            bundle_matches_manifest,
            "typed lifecycle bundle exactly matches every manifest identity, digest and timestamp",
            "typed lifecycle bundle differs from one or more manifest claims",
        ));
        lifecycle_checks.extend(mission_lifecycle_store_checks(options.store_path, receipt));
    } else {
        lifecycle_checks.push(production_check(
            "mission_operational_lifecycle.store_cross_check",
            false,
            "typed lifecycle bundle matches the read-only source store",
            "source store cannot be cross-checked without a valid typed lifecycle bundle",
        ));
    }
    let expected_capability_numbers = (1..=u8::try_from(MISSION_PLATFORM_CAPABILITY_COUNT)
        .unwrap_or(u8::MAX))
        .collect::<Vec<_>>();
    let stage_receipt_digests = BTreeSet::from([
        lifecycle.execute_receipt_sha256.as_str(),
        lifecycle.submit_receipt_sha256.as_str(),
        lifecycle.resume_receipt_sha256.as_str(),
    ]);
    lifecycle_checks.extend([
        production_check(
            "mission_operational_lifecycle.inventory",
            lifecycle.capability_inventory_schema_version
                == MISSION_PLATFORM_CATALOG_SCHEMA_VERSION
                && lifecycle.capability_inventory_sha256
                    == mission_platform_catalog.inventory_sha256
                && lifecycle.capability_numbers == expected_capability_numbers,
            "operational lifecycle receipt is bound to the exact canonical capability inventory 1-40",
            "operational lifecycle receipt is not bound to the canonical capability inventory 1-40",
        ),
        production_check(
            "mission_operational_lifecycle.identity",
            [
                lifecycle.mission_id.as_str(),
                lifecycle.workflow_id.as_str(),
                lifecycle.task_id.as_str(),
                lifecycle.agent_id.as_str(),
                lifecycle.execute_receipt_id.as_str(),
                lifecycle.handoff_id.as_str(),
                lifecycle.inbox_id.as_str(),
            ]
            .iter()
            .all(|value| !value.trim().is_empty()),
            "operational lifecycle identifies mission, workflow, task, agent and receipts",
            "operational lifecycle identity is incomplete",
        ),
        production_check(
            "mission_operational_lifecycle.schemas",
            lifecycle.execute_receipt_schema_version == MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION
                && lifecycle.submit_receipt_schema_version == "foundry.mission.submit.v1"
                && lifecycle.resume_receipt_schema_version == "foundry.mission.drive.v1",
            "execute, submit and resume receipts use canonical runtime schemas",
            "one or more operational lifecycle receipt schemas are unsupported",
        ),
        production_check(
            "mission_operational_lifecycle.receipt_digests",
            valid_sha256(&lifecycle.execute_receipt_sha256)
                && valid_sha256(&lifecycle.submit_receipt_sha256)
                && valid_sha256(&lifecycle.resume_receipt_sha256)
                && stage_receipt_digests.len() == 3,
            "execute, submit and resume receipts have distinct canonical SHA-256 digests",
            "operational lifecycle receipt digests are malformed or reused",
        ),
        production_check(
            "mission_operational_lifecycle.execute",
            lifecycle.execute_status == "completed"
                && lifecycle.execution_attempted
                && lifecycle.executed
                && lifecycle.execute_exit_code == Some(0),
            "mission assignment executed successfully through the operational executor",
            "mission assignment was not successfully executed through the operational executor",
        ),
        production_check(
            "mission_operational_lifecycle.submit",
            lifecycle.submit_receipt_schema_version == "foundry.mission.submit.v1"
                && lifecycle.submit_status == "queued"
                && lifecycle.submit_queued
                && lifecycle.submitted_execute_receipt_sha256
                    == lifecycle.execute_receipt_sha256,
            "submission queued and references the exact execution receipt",
            "submission was not queued or is detached from the execution receipt",
        ),
        production_check(
            "mission_operational_lifecycle.resume",
            lifecycle.resume_action == "handoff_consumed"
                && lifecycle.resume_consumed
                && lifecycle.resumed_handoff_id == lifecycle.handoff_id
                && matches!(
                    lifecycle.resume_status.as_str(),
                    "running" | "reviewing" | "repairing" | "completed"
                ),
            "resume consumed the submitted handoff through the operational mission runtime",
            "resume did not consume the submitted handoff",
        ),
        production_check(
            "mission_operational_lifecycle.order",
            lifecycle.execute_observed_at_epoch > 0
                && lifecycle.execute_observed_at_epoch <= lifecycle.submit_observed_at_epoch
                && lifecycle.submit_observed_at_epoch <= lifecycle.resume_observed_at_epoch
                && evidence_epoch_is_fresh(
                    lifecycle.resume_observed_at_epoch,
                    now_epoch,
                    MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
                ),
            "operational receipts prove execute-submit-resume order inside the freshness window",
            "operational receipt order is invalid, future-dated or stale",
        ),
    ]);
    gates.push(production_gate(
        "mission_operational_lifecycle",
        "Operational mission execute-submit-resume lifecycle",
        lifecycle_checks,
    ));
    debug_assert_eq!(gates.len(), PRODUCTION_READINESS_REQUIRED_GATE_COUNT);
    debug_assert_eq!(
        evidence_refs.len(),
        PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT
    );

    let blocked_by = gates
        .iter()
        .filter(|gate| gate.status != "pass")
        .map(|gate| gate.id.clone())
        .collect::<Vec<_>>();
    let production_ready = capability_ready && blocked_by.is_empty();
    let next_actions = if production_ready {
        vec![
            "Production evidence is complete; an explicit human-controlled promotion may proceed. This evaluator performed no command or mutation."
                .to_string(),
        ]
    } else {
        blocked_by
            .iter()
            .map(|gate| {
                format!(
                    "Repair `{gate}` and regenerate fresh secret-free evidence before reevaluation."
                )
            })
            .collect()
    };

    Ok(ProductionReadinessReport {
        schema_version: PRODUCTION_READINESS_REPORT_SCHEMA_VERSION.to_string(),
        milestone: manifest.milestone,
        profile: manifest.profile,
        release_version: manifest.release_version,
        evaluation_mode: "read_only".to_string(),
        capability_ready,
        capability_inventory_count: mission_platform_catalog.capability_count,
        capability_inventory_sha256: mission_platform_catalog.inventory_sha256,
        capability_proof_kind_counts: mission_platform_catalog.proof_kind_counts,
        required_gate_count: PRODUCTION_READINESS_REQUIRED_GATE_COUNT,
        required_receipt_count: PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT,
        production_ready,
        decision: if production_ready {
            "production_ready"
        } else {
            "fail_closed"
        }
        .to_string(),
        blocked_by,
        gates,
        manifest_sha256: hex_sha256(&manifest_bytes),
        commands_executed: 0,
        mutations_performed: false,
        next_actions,
    })
}

fn production_evidence_refs(
    manifest: &ProductionReadinessManifest,
) -> Vec<(&'static str, &ProductionEvidenceRef)> {
    vec![
        ("release_matrix", &manifest.release.matrix),
        ("release_artifacts", &manifest.release.artifacts),
        ("release_sbom", &manifest.release.sbom),
        ("release_checksums", &manifest.release.checksums),
        ("release_sigstore", &manifest.release.sigstore),
        ("release_provenance", &manifest.release.provenance),
        ("installation", &manifest.installation.evidence),
        ("off_host_recovery", &manifest.off_host_backup.evidence),
        ("key_escrow", &manifest.key_escrow.evidence),
        ("alerting", &manifest.alerts.evidence),
        ("restore_drill", &manifest.restore_drill.evidence),
        ("upgrade_rollback", &manifest.upgrade_rollback.evidence),
        ("bounded_load", &manifest.bounded_load.evidence),
        (
            "mission_operational_lifecycle",
            &manifest.mission_operational_lifecycle.evidence,
        ),
    ]
}

fn production_gate(
    id: &str,
    title: &str,
    checks: Vec<ProductionReadinessCheck>,
) -> ProductionReadinessGate {
    let passed = checks.iter().all(|check| check.passed);
    ProductionReadinessGate {
        id: id.to_string(),
        title: title.to_string(),
        status: if passed { "pass" } else { "fail" }.to_string(),
        checks,
    }
}

fn production_check(
    id: &str,
    passed: bool,
    pass_reason: &str,
    fail_reason: &str,
) -> ProductionReadinessCheck {
    ProductionReadinessCheck {
        id: id.to_string(),
        passed,
        reason: if passed { pass_reason } else { fail_reason }.to_string(),
    }
}

fn production_evidence_checks(
    label: &str,
    evidence: &ProductionEvidenceRef,
    evidence_root: &Path,
    now_epoch: u64,
    subject_version: &str,
    expected_claims_sha256: &str,
) -> Vec<ProductionReadinessCheck> {
    let scoped_path = resolve_production_evidence_path(evidence_root, &evidence.artifact_path);
    let mut checks = vec![
        production_check(
            &format!("{label}.sha256_format"),
            valid_sha256(&evidence.artifact_sha256),
            "evidence digest is a lowercase SHA-256",
            "evidence digest is not a lowercase SHA-256",
        ),
        production_check(
            &format!("{label}.fresh"),
            evidence_epoch_is_fresh(
                evidence.observed_at_epoch,
                now_epoch,
                MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
            ),
            "evidence observation is within the allowed freshness window",
            "evidence observation is future-dated, zero or stale",
        ),
        production_check(
            &format!("{label}.path"),
            scoped_path.is_ok(),
            "evidence path is a regular non-symlink file inside the evidence root",
            "evidence path is missing, unsafe, outside the evidence root or a symlink",
        ),
    ];
    let Ok(path) = scoped_path else {
        return checks;
    };
    let metadata = fs::metadata(&path);
    let readable_size = metadata.as_ref().is_ok_and(|metadata| {
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= MAX_PRODUCTION_EVIDENCE_BYTES
    });
    checks.push(production_check(
        &format!("{label}.size"),
        readable_size,
        "evidence artifact is a bounded non-empty regular file",
        "evidence artifact is empty, too large or not a regular file",
    ));
    if !readable_size {
        return checks;
    }
    let bytes = fs::read(&path);
    checks.push(production_check(
        &format!("{label}.readable"),
        bytes.is_ok(),
        "evidence artifact is readable",
        "evidence artifact cannot be read",
    ));
    let Ok(bytes) = bytes else {
        return checks;
    };
    checks.push(production_check(
        &format!("{label}.digest"),
        valid_sha256(&evidence.artifact_sha256) && hex_sha256(&bytes) == evidence.artifact_sha256,
        "evidence bytes match the declared SHA-256",
        "evidence bytes do not match the declared SHA-256",
    ));
    let text = std::str::from_utf8(&bytes);
    checks.push(production_check(
        &format!("{label}.utf8"),
        text.is_ok(),
        "evidence artifact is inspectable UTF-8 text",
        "evidence artifact is opaque or non-UTF-8",
    ));
    if let Ok(text) = text {
        checks.push(production_check(
            &format!("{label}.secret_free"),
            production_text_is_secret_free(text, "production_readiness_evidence"),
            "evidence artifact contains no detected secret material",
            "evidence artifact contains detected secret material",
        ));
        let receipt = if label == "mission_operational_lifecycle" {
            serde_json::from_str::<ProductionMissionLifecycleReceipt>(text).map(|receipt| {
                ProductionEvidenceReceipt {
                    schema_version: receipt.schema_version,
                    kind: receipt.kind,
                    status: receipt.status,
                    subject_version: receipt.subject_version,
                    claims_sha256: receipt.claims_sha256,
                    source_artifact_path: None,
                    source_artifact_sha256: None,
                    source_observed_at_epoch: None,
                }
            })
        } else {
            serde_json::from_str::<ProductionEvidenceReceipt>(text)
        };
        checks.push(production_check(
            &format!("{label}.receipt"),
            receipt.is_ok(),
            "evidence artifact is a canonical production receipt",
            "evidence artifact is not a canonical production receipt",
        ));
        if let Ok(receipt) = receipt {
            let supported_schema = if label == "mission_operational_lifecycle" {
                receipt.schema_version == PRODUCTION_MISSION_LIFECYCLE_RECEIPT_SCHEMA_VERSION
            } else {
                production_receipt_schema_supported(label, &receipt.schema_version, subject_version)
            };
            checks.extend([
                production_check(
                    &format!("{label}.receipt_schema"),
                    supported_schema,
                    "evidence receipt schema matches the required gate",
                    "evidence receipt schema does not match the required gate",
                ),
                production_check(
                    &format!("{label}.receipt_kind"),
                    receipt.kind == label,
                    "evidence receipt kind matches the required gate",
                    "evidence receipt kind does not match the required gate",
                ),
                production_check(
                    &format!("{label}.receipt_status"),
                    receipt.status == "passed",
                    "evidence receipt records a passed outcome",
                    "evidence receipt does not record a passed outcome",
                ),
                production_check(
                    &format!("{label}.receipt_subject"),
                    receipt.subject_version == subject_version,
                    "evidence receipt is bound to the evaluated release",
                    "evidence receipt is bound to another release",
                ),
                production_check(
                    &format!("{label}.receipt_claims"),
                    valid_sha256(&receipt.claims_sha256)
                        && receipt.claims_sha256 == expected_claims_sha256,
                    "evidence receipt is bound to the evaluated claims",
                    "evidence receipt claims digest is malformed or differs from the manifest",
                ),
            ]);
            if label != "mission_operational_lifecycle"
                && receipt.schema_version == production_source_bound_receipt_schema(label)
            {
                checks.extend(production_source_artifact_checks(
                    label,
                    evidence,
                    &receipt,
                    evidence_root,
                    now_epoch,
                    subject_version,
                    expected_claims_sha256,
                ));
            }
        }
    }
    checks
}

fn production_source_artifact_checks(
    label: &str,
    evidence: &ProductionEvidenceRef,
    receipt: &ProductionEvidenceReceipt,
    evidence_root: &Path,
    now_epoch: u64,
    subject_version: &str,
    expected_claims_sha256: &str,
) -> Vec<ProductionReadinessCheck> {
    let complete = receipt.source_artifact_path.is_some()
        && receipt.source_artifact_sha256.is_some()
        && receipt.source_observed_at_epoch.is_some();
    let mut checks = vec![production_check(
        &format!("{label}.source.complete"),
        complete,
        "source-bound receipt declares path, digest and observation epoch",
        "source-bound receipt is missing path, digest or observation epoch",
    )];
    let (Some(source_path), Some(source_sha256), Some(source_epoch)) = (
        receipt.source_artifact_path.as_deref(),
        receipt.source_artifact_sha256.as_deref(),
        receipt.source_observed_at_epoch,
    ) else {
        return checks;
    };
    let resolved = resolve_production_evidence_path(evidence_root, source_path);
    checks.extend([
        production_check(
            &format!("{label}.source.distinct"),
            source_path != evidence.artifact_path,
            "source artifact is distinct from its canonical receipt",
            "source artifact aliases its canonical receipt",
        ),
        production_check(
            &format!("{label}.source.sha256_format"),
            valid_sha256(source_sha256),
            "source artifact digest is lowercase SHA-256",
            "source artifact digest is not lowercase SHA-256",
        ),
        production_check(
            &format!("{label}.source.fresh"),
            source_epoch == evidence.observed_at_epoch
                && evidence_epoch_is_fresh(
                    source_epoch,
                    now_epoch,
                    MAX_PRODUCTION_EVIDENCE_AGE_SECONDS,
                ),
            "source observation matches receipt freshness",
            "source observation differs from receipt, is future-dated, zero or stale",
        ),
        production_check(
            &format!("{label}.source.path"),
            resolved.is_ok(),
            "source artifact is a regular non-symlink file inside evidence root",
            "source artifact is missing, unsafe, outside evidence root or symlink",
        ),
    ]);
    let Ok(path) = resolved else {
        return checks;
    };
    let metadata = fs::metadata(&path);
    let readable_size = metadata.as_ref().is_ok_and(|metadata| {
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= MAX_PRODUCTION_EVIDENCE_BYTES
    });
    checks.push(production_check(
        &format!("{label}.source.size"),
        readable_size,
        "source artifact size is within inspection bounds",
        "source artifact is empty, oversized or unreadable",
    ));
    if !readable_size {
        return checks;
    }
    let bytes = fs::read(&path);
    checks.push(production_check(
        &format!("{label}.source.readable"),
        bytes.is_ok(),
        "source artifact is readable",
        "source artifact cannot be read",
    ));
    let Ok(bytes) = bytes else {
        return checks;
    };
    checks.push(production_check(
        &format!("{label}.source.digest"),
        valid_sha256(source_sha256) && hex_sha256(&bytes) == source_sha256,
        "source artifact bytes match the bound SHA-256",
        "source artifact bytes drifted from the bound SHA-256",
    ));
    let text = std::str::from_utf8(&bytes);
    checks.push(production_check(
        &format!("{label}.source.utf8"),
        text.is_ok(),
        "source artifact is inspectable UTF-8 text",
        "source artifact is opaque or non-UTF-8",
    ));
    if let Ok(text) = text {
        checks.push(production_check(
            &format!("{label}.source.secret_free"),
            production_text_is_secret_free(text, "production_readiness_source_evidence"),
            "source artifact contains no detected secret material",
            "source artifact contains detected secret material",
        ));
        let attestation = serde_json::from_str::<ProductionSourceEvidence>(text);
        checks.push(production_check(
            &format!("{label}.source.attestation"),
            attestation.is_ok(),
            "source artifact is a typed production evidence attestation",
            "source artifact is not a typed production evidence attestation",
        ));
        if let Ok(attestation) = attestation {
            let producer_valid = !attestation.producer.trim().is_empty()
                && attestation.producer.len() <= 128
                && !attestation.producer.chars().any(char::is_control);
            let claims_sha256 = serde_json::to_vec(&attestation.claims)
                .ok()
                .map(|bytes| hex_sha256(&bytes));
            checks.extend([
                production_check(
                    &format!("{label}.source.schema_kind"),
                    attestation.schema_version == production_source_evidence_schema(label)
                        && attestation.kind == label,
                    "source attestation schema and kind match the required gate",
                    "source attestation schema or kind differs from the required gate",
                ),
                production_check(
                    &format!("{label}.source.subject_outcome"),
                    attestation.status == "passed"
                        && attestation.subject_version == subject_version
                        && attestation.observed_at_epoch == source_epoch,
                    "source attestation outcome, release and observation match the receipt",
                    "source attestation outcome, release or observation differs from the receipt",
                ),
                production_check(
                    &format!("{label}.source.execution_mode"),
                    attestation.execution_mode == "production",
                    "source attestation was collected in production mode",
                    "source attestation came from a test or unsupported execution mode",
                ),
                production_check(
                    &format!("{label}.source.claims"),
                    attestation
                        .claims
                        .as_object()
                        .is_some_and(|claims| !claims.is_empty())
                        && claims_sha256.as_deref() == Some(expected_claims_sha256)
                        && claims_sha256.as_deref() == Some(receipt.claims_sha256.as_str()),
                    "source attestation semantically matches the canonical manifest claims",
                    "source attestation claims are missing or differ from the manifest",
                ),
                production_check(
                    &format!("{label}.source.collector_evidence"),
                    producer_valid
                        && attestation
                            .evidence
                            .as_object()
                            .is_some_and(|evidence| !evidence.is_empty()),
                    "source attestation identifies its producer and includes structured evidence",
                    "source attestation producer or structured evidence is missing",
                ),
            ]);
        }
    }
    checks
}

fn load_production_mission_lifecycle_receipt(
    evidence_root: &Path,
    evidence: &ProductionEvidenceRef,
) -> Result<ProductionMissionLifecycleReceipt> {
    let path = resolve_production_evidence_path(evidence_root, &evidence.artifact_path)?;
    let bytes = fs::read(&path).with_context(|| {
        format!(
            "failed to read mission lifecycle evidence {}",
            path.display()
        )
    })?;
    if bytes.is_empty()
        || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_PRODUCTION_EVIDENCE_BYTES
    {
        bail!("mission lifecycle evidence has an invalid size");
    }
    let text =
        std::str::from_utf8(&bytes).context("mission lifecycle evidence must be UTF-8 JSON")?;
    if !production_text_is_secret_free(text, "mission_lifecycle_evidence") {
        bail!("mission lifecycle evidence must be secret-free");
    }
    serde_json::from_slice(&bytes).context("failed to parse typed mission lifecycle evidence")
}

fn serialized_value_matches<T: Serialize, U: Serialize>(left: &T, right: &U) -> bool {
    serde_json::to_value(left)
        .and_then(|left| serde_json::to_value(right).map(|right| left == right))
        .unwrap_or(false)
}

fn serialized_sha256<T: Serialize>(value: &T) -> Result<String> {
    Ok(hex_sha256(&serde_json::to_vec(value)?))
}

fn rfc3339_epoch(value: &str) -> Option<u64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .and_then(|value| u64::try_from(value.timestamp()).ok())
}

fn datetime_epoch(value: &DateTime<Utc>) -> Option<u64> {
    u64::try_from(value.timestamp()).ok()
}

fn mission_event_sequence(
    mission: &MissionRecord,
    kind: &str,
    handoff_id: &str,
    task_id: &str,
) -> Option<usize> {
    mission
        .events
        .iter()
        .find(|event| {
            event.kind == kind
                && (event.correlation_id.as_deref() == Some(handoff_id)
                    || event.task_id.as_deref() == Some(task_id))
        })
        .map(|event| event.sequence)
}

fn mission_lifecycle_event_order_is_valid(
    mission: &MissionRecord,
    handoff_id: &str,
    task_id: &str,
) -> bool {
    let sequences = [
        mission_event_sequence(mission, "agent.handoff.created", handoff_id, task_id),
        mission_event_sequence(mission, "agent.inbox.enqueued", handoff_id, task_id),
        mission_event_sequence(mission, "agent.inbox.leased", handoff_id, task_id),
        mission_event_sequence(mission, "agent.wakeup.triggered", handoff_id, task_id),
        mission_event_sequence(mission, "mission.task.completed", handoff_id, task_id),
        mission_event_sequence(mission, "agent.handoff.accepted", handoff_id, task_id),
    ];
    sequences.iter().all(Option::is_some)
        && sequences
            .windows(2)
            .all(|pair| pair[0].is_some_and(|left| pair[1].is_some_and(|right| left < right)))
}

fn mission_lifecycle_store_checks(
    store_path: &Path,
    receipt: &ProductionMissionLifecycleReceipt,
) -> Vec<ProductionReadinessCheck> {
    let mut checks = Vec::new();
    let store_path_metadata = fs::symlink_metadata(store_path);
    let safe_store_path = store_path_metadata
        .as_ref()
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink());
    checks.push(production_check(
        "mission_operational_lifecycle.store_path",
        safe_store_path,
        "mission lifecycle source store is a regular non-symlink SQLite file",
        "mission lifecycle source store is missing, unsafe or not a regular file",
    ));
    if !safe_store_path {
        return checks;
    }

    let connection = Connection::open_with_flags(
        store_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    );
    checks.push(production_check(
        "mission_operational_lifecycle.store_read_only",
        connection.is_ok(),
        "mission lifecycle source store opened strictly read-only",
        "mission lifecycle source store could not be opened read-only",
    ));
    let Ok(connection) = connection else {
        return checks;
    };

    let quick_check = connection
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .ok();
    checks.push(production_check(
        "mission_operational_lifecycle.store_integrity",
        quick_check.as_deref() == Some("ok"),
        "mission lifecycle source store passes SQLite quick_check",
        "mission lifecycle source store failed SQLite quick_check",
    ));

    let execute = &receipt.execution_receipt;
    let execution_row = connection
        .query_row(
            r#"
            SELECT receipt_sha256, receipt_json, state, consumed_at, consumed_by_submission
            FROM mission_execution_receipts
            WHERE receipt_id=?1
            "#,
            [&execute.receipt_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten();
    let execution_matches = execution_row.as_ref().is_some_and(
        |(stored_sha256, stored_json, state, consumed_at, consumed_by_submission)| {
            let mut stored_receipt = stored_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<MissionExecutionReceipt>(json).ok());
            if let Some(stored_receipt) = stored_receipt.as_mut() {
                stored_receipt.consumed_at.clone_from(consumed_at);
                stored_receipt
                    .consumed_by_submission
                    .clone_from(consumed_by_submission);
            }
            state == "completed"
                && stored_sha256.as_deref() == Some(execute.receipt_sha256.as_str())
                && stored_receipt
                    .as_ref()
                    .is_some_and(|stored| serialized_value_matches(stored, execute))
                && consumed_at.is_some()
                && consumed_by_submission.as_deref()
                    == Some(receipt.submission.idempotency_key.as_str())
        },
    );
    checks.push(production_check(
        "mission_operational_lifecycle.store_execution",
        execution_matches,
        "execution receipt bytes, digest and submission consumption match the source store",
        "execution receipt is absent, changed, unconsumed or linked to another submission",
    ));

    let mission_json = connection
        .query_row(
            // foundry-brand-allow: legacy-compat
            "SELECT data_json FROM forge_missions WHERE id=?1",
            [&execute.mission_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten();
    let stored_mission = mission_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<MissionRecord>(json).ok());
    let workflow_json = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id=?1",
            [&execute.workflow_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten();
    let workflow_validation = workflow_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<crate::graph::Workflow>(json).ok())
        .map(|workflow| validate_workflow(&workflow));
    let terminal_validation_matches =
        stored_mission.as_ref().is_some_and(|mission| {
            mission.status == MissionStatus::Completed
                && mission.revision >= receipt.resume_report.revision
                && mission.tasks.iter().all(|task| task.status == "completed")
        }) && workflow_validation.as_ref().is_some_and(|validation| {
            validation.status == "passed"
                && validation.promotable
                && validation.failed_rules.is_empty()
                && validation.rework_tasks.is_empty()
        });
    checks.push(production_check(
        "mission_operational_lifecycle.store_terminal_validation",
        terminal_validation_matches,
        "current mission is completed and its persisted workflow is promotable with no rework",
        "current mission is non-terminal or its persisted workflow is blocked or requires rework",
    ));

    let handoff_json = connection
        .query_row(
            "SELECT data_json FROM mission_handoffs WHERE id=?1 AND mission_id=?2",
            [&receipt.submit_report.handoff_id, &execute.mission_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten();
    let stored_handoff = handoff_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<AgentHandoff>(json).ok());

    let inbox_row = connection
        .query_row(
            r#"
            SELECT id, handoff_id, recipient_agent, status, attempts, max_attempts,
                   lease_owner, lease_expires_at, last_error, enqueued_at, consumed_at
            FROM mission_runtime_inbox
            WHERE id=?1 AND handoff_id=?2 AND mission_id=?3
            "#,
            [
                &receipt.submit_report.inbox_id,
                &receipt.submit_report.handoff_id,
                &execute.mission_id,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, usize>(4)?,
                    row.get::<_, usize>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten();

    let submission_checkpoint_json = connection
        .query_row(
            r#"
            SELECT data_json
            FROM mission_runtime_checkpoints
            WHERE mission_id=?1 AND revision=?2
            "#,
            rusqlite::params![
                execute.mission_id,
                i64::try_from(receipt.submit_report.producer_revision).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten();
    let submission_checkpoint = submission_checkpoint_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<MissionRecord>(json).ok());

    let resume_checkpoint = connection
        .query_row(
            r#"
            SELECT data_json, data_sha256
            FROM mission_runtime_checkpoints
            WHERE mission_id=?1 AND revision=?2
            "#,
            rusqlite::params![
                execute.mission_id,
                i64::try_from(receipt.resume_report.revision).unwrap_or(i64::MAX)
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .ok()
        .flatten();

    let expected_receipt_id = format!("execution_receipt:{}", execute.receipt_id);
    let expected_receipt_sha256 = format!("execution_receipt_sha256:{}", execute.receipt_sha256);
    let handoff_matches = stored_handoff.as_ref().is_some_and(|handoff| {
        handoff.status == "accepted"
            && handoff.accepted_at.is_some()
            && handoff.idempotency_key == receipt.submission.idempotency_key
            && handoff.mission_id == execute.mission_id
            && handoff.task_id == execute.task_id
            && handoff.from_agent == execute.agent_id
            && handoff.id == receipt.submit_report.handoff_id
            && handoff.delivery.task_id == receipt.submission.task_id
            && handoff.delivery.status == "completed"
            && handoff.delivery.summary == receipt.submission.summary
            && handoff.delivery.tests_passed == execute.tests_passed
            && handoff.delivery.tests_failed == execute.tests_failed
            && handoff.validations.contains(&expected_receipt_id)
            && handoff.validations.contains(&expected_receipt_sha256)
    });
    let inbox_matches = inbox_row.as_ref().is_some_and(
        |(
            id,
            handoff_id,
            recipient_agent,
            status,
            _attempts,
            _max_attempts,
            lease_owner,
            lease_expires_at,
            last_error,
            _enqueued_at,
            consumed_at,
        )| {
            id == &receipt.submit_report.inbox_id
                && handoff_id == &receipt.submit_report.handoff_id
                && stored_handoff
                    .as_ref()
                    .is_some_and(|handoff| recipient_agent == &handoff.to_agent)
                && status == "consumed"
                && consumed_at.is_some()
                && lease_owner.is_none()
                && lease_expires_at.is_none()
                && last_error.is_none()
        },
    );
    let submission_checkpoint_matches = submission_checkpoint.as_ref().is_some_and(|mission| {
        mission.id == execute.mission_id
            && mission.revision == receipt.submit_report.producer_revision
            && mission.handoffs.iter().any(|handoff| {
                handoff.id == receipt.submit_report.handoff_id && handoff.status == "queued"
            })
            && mission.inbox.iter().any(|inbox| {
                inbox.id == receipt.submit_report.inbox_id && inbox.status == "pending"
            })
    });
    checks.push(production_check(
        "mission_operational_lifecycle.store_submission",
        handoff_matches && submission_checkpoint_matches,
        "queued submission, execution linkage and persisted handoff match the source store",
        "submission report or handoff linkage does not match the source store",
    ));

    let resume_checkpoint_matches =
        resume_checkpoint
            .as_ref()
            .is_some_and(|(checkpoint_json, checkpoint_sha256)| {
                hex_sha256(checkpoint_json.as_bytes()) == *checkpoint_sha256
                    && serde_json::from_str::<MissionRecord>(checkpoint_json)
                        .ok()
                        .is_some_and(|checkpoint| {
                            serialized_value_matches(&checkpoint, &receipt.resume_report.mission)
                        })
            });
    let current_mission_matches = stored_mission.as_ref().is_some_and(|mission| {
        mission.id == execute.mission_id
            && mission.workflow_id == execute.workflow_id
            && mission.mode == MissionMode::Workflow
            && mission.worktree.is_some()
            && mission.revision >= receipt.resume_report.revision
            && mission.handoffs.iter().any(|handoff| {
                handoff.id == receipt.submit_report.handoff_id && handoff.status == "accepted"
            })
            && mission.inbox.iter().any(|inbox| {
                inbox.id == receipt.submit_report.inbox_id && inbox.status == "consumed"
            })
    });
    let event_order_matches = stored_mission.as_ref().is_some_and(|mission| {
        mission_lifecycle_event_order_is_valid(
            mission,
            &receipt.submit_report.handoff_id,
            &execute.task_id,
        )
    });
    checks.push(production_check(
        "mission_operational_lifecycle.store_resume",
        inbox_matches && resume_checkpoint_matches && current_mission_matches,
        "resume snapshot, consumed inbox and current mission match persisted checkpoints",
        "resume report, inbox consumption or mission checkpoint does not match the source store",
    ));
    checks.push(production_check(
        "mission_operational_lifecycle.store_event_order",
        event_order_matches,
        "persisted mission events prove ordered handoff enqueue, wakeup, completion and acceptance",
        "persisted mission events do not prove the required lifecycle order",
    ));
    checks
}

fn resolve_production_manifest_path(evidence_root: &Path, path: &Path) -> Result<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        if !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        {
            bail!("production readiness manifest path must be a contained relative path");
        }
        evidence_root.join(path)
    };
    let metadata = fs::symlink_metadata(&candidate).with_context(|| {
        format!(
            "failed to inspect production readiness manifest {}",
            candidate.display()
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("production readiness manifest must be a regular non-symlink file");
    }
    let canonical = fs::canonicalize(&candidate).with_context(|| {
        format!(
            "failed to resolve production readiness manifest {}",
            candidate.display()
        )
    })?;
    if !canonical.starts_with(evidence_root) {
        bail!("production readiness manifest escapes the evidence root");
    }
    Ok(canonical)
}

fn resolve_production_evidence_path(evidence_root: &Path, value: &str) -> Result<PathBuf> {
    let relative = Path::new(value);
    if value.trim().is_empty()
        || relative.is_absolute()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("production evidence path must be a contained relative path");
    }
    let candidate = evidence_root.join(relative);
    let metadata = fs::symlink_metadata(&candidate)
        .with_context(|| format!("failed to inspect production evidence path {value}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("production evidence path must be a regular non-symlink file");
    }
    let canonical = fs::canonicalize(&candidate)
        .with_context(|| format!("failed to resolve production evidence path {value}"))?;
    if !canonical.starts_with(evidence_root) {
        bail!("production evidence path escapes the evidence root");
    }
    Ok(canonical)
}

fn production_text_is_secret_free(text: &str, scope: &str) -> bool {
    sanitize_prompt_secrets(
        text,
        SecretSanitizationOptions {
            scope: scope.to_string(),
            enable_regex: true,
            enable_entropy: false,
            enable_local_ai_fallback: false,
            allow_external_ai: false,
            entropy_threshold: 4.2,
        },
    )
    .detection_count
        == 0
}

fn evidence_epoch_is_fresh(observed_at_epoch: u64, now_epoch: u64, max_age: u64) -> bool {
    observed_at_epoch > 0
        && observed_at_epoch <= now_epoch
        && now_epoch.saturating_sub(observed_at_epoch) <= max_age
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn release_version_matches_milestone(release_version: &str, milestone: &str) -> bool {
    let Some(patch_and_suffix) = release_version.strip_prefix(&format!("{milestone}.")) else {
        return false;
    };
    !patch_and_suffix.is_empty()
        && patch_and_suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}

pub fn build_milestone_research(version: &str) -> Result<MilestoneResearchReport> {
    let version = canonical_milestone_version(version)?;

    let sources = research_sources();
    let local_skill_inputs = local_research_inputs();

    Ok(MilestoneResearchReport {
        schema_version: "foundry.milestone.research.v1".to_string(),
        status: "validated".to_string(),
        milestone: version.to_string(),
        artifact_path: "docs/research/foundry-0.5-creative-runtime-source-research.md".to_string(),
        source_count: sources.len() + local_skill_inputs.len(),
        sources,
        local_skill_inputs,
        findings: research_findings(),
        validation_gates: research_validation_gates(),
        workflow_templates: research_workflow_templates(),
        lean_governance: research_lean_decisions(),
        promotion_impact:
            "The historical Foundry 0.5 research baseline is retained as source provenance for the canonical 0.6 gates and templates; promotion remains controlled by the full 0.6 milestone manifest rather than by this report alone."
                .to_string(),
    })
}

pub fn build_milestone_manifest(version: &str) -> Result<MilestoneManifestReport> {
    build_milestone_manifest_with_store(version, None)
}

pub fn build_milestone_manifest_with_store(
    version: &str,
    store: Option<&FoundryStore>,
) -> Result<MilestoneManifestReport> {
    let status = build_milestone_status(version)?;
    let attached_evidence = if let Some(store) = store {
        load_milestone_attached_evidence(store, version)?
    } else {
        Vec::new()
    };
    let evidence_provenance = [SUPPORTED_MILESTONE, LEGACY_MILESTONE_QUERY_ALIAS]
        .into_iter()
        .map(|source_milestone| MilestoneEvidenceProvenance {
            source_milestone: source_milestone.to_string(),
            evidence_count: attached_evidence
                .iter()
                .filter(|evidence| evidence.milestone == source_milestone)
                .count(),
            usage: if source_milestone == SUPPORTED_MILESTONE {
                "canonical".to_string()
            } else {
                "read_only_legacy_compatibility".to_string()
            },
            auto_migrated: false,
        })
        .collect::<Vec<_>>();
    let attached_evidence_kind_map = attached_evidence_kind_map(&attached_evidence);
    let validated_attached_evidence_kind_map = if let Some(store) = store {
        validated_attached_evidence_kind_map(store, &attached_evidence)
    } else {
        BTreeMap::new()
    };
    let requirements = status
        .capabilities
        .iter()
        .map(|capability| MilestoneRequirement {
            capability_id: capability.id.clone(),
            title: capability.title.clone(),
            status: capability.status.clone(),
            required_evidence: required_evidence_for(&capability.id).to_string(),
        })
        .collect::<Vec<_>>();
    let completed_capabilities = status
        .capabilities
        .iter()
        .filter(|capability| {
            capability_promotion_ready(capability, &validated_attached_evidence_kind_map)
        })
        .map(|capability| manifest_capability(capability, true))
        .collect::<Vec<_>>();
    let missing_capabilities = status
        .capabilities
        .iter()
        .filter(|capability| {
            !capability_promotion_ready(capability, &validated_attached_evidence_kind_map)
        })
        .map(|capability| manifest_capability(capability, false))
        .collect::<Vec<_>>();
    let validation_evidence = status
        .capabilities
        .iter()
        .filter(|capability| capability.status != "planned")
        .map(|capability| MilestoneManifestEvidence {
            capability_id: capability.id.clone(),
            status: capability.status.clone(),
            summary: capability.evidence.clone(),
            validation_state: manifest_validation_state(
                capability,
                &attached_evidence_kind_map,
                &validated_attached_evidence_kind_map,
            ),
        })
        .collect::<Vec<_>>();
    let demos = status
        .capabilities
        .iter()
        .filter(|capability| is_demo_related(capability))
        .map(|capability| MilestoneManifestDemo {
            capability_id: capability.id.clone(),
            status: capability.status.clone(),
            summary: capability.evidence.clone(),
            required_for_promotion: capability.required_for_promotion,
        })
        .collect::<Vec<_>>();
    let known_gaps = status
        .capabilities
        .iter()
        .filter(|capability| {
            !capability_promotion_ready(capability, &validated_attached_evidence_kind_map)
        })
        .map(|capability| MilestoneManifestGap {
            capability_id: capability.id.clone(),
            status: capability.status.clone(),
            gap: capability.gap_before_promotion.clone(),
            next_action: next_action_for_gap(&capability.id).to_string(),
        })
        .collect::<Vec<_>>();
    let blocked_by = status
        .capabilities
        .iter()
        .filter(|capability| {
            capability.required_for_promotion
                && !capability_promotion_ready(capability, &validated_attached_evidence_kind_map)
        })
        .map(|capability| capability.id.clone())
        .collect::<Vec<_>>();
    let promotable = blocked_by.is_empty();
    let promotion_decision = MilestonePromotionDecision {
        decision: if promotable { "promote" } else { "fail" }.to_string(),
        promotable,
        readiness_scope: "capability".to_string(),
        capability_ready: promotable,
        production_ready: false,
        production_evidence_evaluated: false,
        blocked_by,
        reason: if promotable {
            "All required Foundry 0.6 capabilities have implementation, validation or operator-approved attached evidence. This capability decision does not assert operational production readiness."
                .to_string()
        } else {
            "Foundry 0.6 promotion is blocked while required capabilities remain planned, blocked, groundwork-only or missing required attached evidence."
                .to_string()
        },
        next_action: if promotable {
            "Evaluate the separate fail-closed production-readiness manifest before an explicit human-controlled release promotion."
                .to_string()
        } else {
            "Collect and attach the missing required milestone evidence kinds before reconsidering 0.6 promotion."
                .to_string()
        },
    };

    Ok(MilestoneManifestReport {
        schema_version: MILESTONE_MANIFEST_SCHEMA_VERSION.to_string(),
        milestone: status.milestone,
        requested_milestone: status.requested_milestone,
        compatibility_mode: status.compatibility_mode,
        release_line_boundary: status.release_line_boundary,
        requirements,
        completed_capabilities,
        missing_capabilities,
        validation_evidence,
        attached_evidence,
        evidence_provenance,
        demos,
        known_gaps,
        promotion_decision,
    })
}

pub fn attach_milestone_evidence(
    store: &FoundryStore,
    options: MilestoneAttachEvidenceOptions<'_>,
) -> Result<MilestoneAttachedEvidence> {
    let requested_version = normalize_required(options.version, "version")?;
    let version = canonical_milestone_version(&requested_version)?.to_string();
    let capability_id = normalize_required(options.capability_id, "capability")?;
    if !foundry_06_capabilities()
        .iter()
        .any(|capability| capability.id == capability_id)
    {
        bail!("unknown milestone capability `{capability_id}` for milestone {version}");
    }
    let kind = normalize_required(options.kind, "kind")?;
    let summary = normalize_required(options.summary, "summary")?;
    let approved_by = normalize_required(options.approved_by, "approved-by")?;
    let origin = normalize_required(options.origin, "origin")?;

    let artifact_bytes = fs::read(options.artifact_path).with_context(|| {
        format!(
            "failed to read milestone evidence artifact {}",
            options.artifact_path.display()
        )
    })?;
    let artifact_sha256 = hex_sha256(&artifact_bytes);
    let created_at = Utc::now().to_rfc3339();
    let evidence_id = format!(
        "{}-{}-{}",
        sanitize_milestone_component(&capability_id),
        sanitize_milestone_component(&kind),
        Utc::now().timestamp_millis()
    );
    let artifact_file_name = options
        .artifact_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_milestone_component)
        .unwrap_or_else(|| "evidence-artifact".to_string());
    let artifact_path = format!("artifacts/milestone/{version}/{evidence_id}-{artifact_file_name}");
    let artifact_target = store.base_dir().join(&artifact_path);
    if let Some(parent) = artifact_target.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create milestone evidence directory {}",
                parent.display()
            )
        })?;
    }
    fs::write(&artifact_target, &artifact_bytes).with_context(|| {
        format!(
            "failed to write milestone evidence artifact {}",
            artifact_target.display()
        )
    })?;

    let mut evidence = MilestoneAttachedEvidence {
        schema_version: MILESTONE_ATTACHED_EVIDENCE_SCHEMA_VERSION.to_string(),
        milestone: version,
        capability_id,
        evidence_id,
        kind,
        status: "recorded".to_string(),
        summary,
        artifact_path,
        artifact_sha256,
        artifact_bytes: artifact_bytes.len() as u64,
        approved_by,
        origin,
        promotion_impact: "evidence_attached_not_auto_promoted".to_string(),
        global_event_id: None,
        created_at,
    };
    let event_payload = serde_json::to_value(&evidence)?;
    let tenant_context = serde_json::json!({
        "organization": {"id": "foundry"},
        "brand": {"id": "foundry"},
        "product": {"id": "foundry"},
        "user": {"id": evidence.approved_by},
        "channel": {"id": "milestone"}
    });
    let global_event_id = store.record_global_event(GlobalEventWrite {
        source: "milestone",
        source_id: &evidence.evidence_id,
        workflow_id: None,
        kind: MILESTONE_ATTACHED_EVIDENCE_EVENT_KIND,
        origin: &evidence.origin,
        status: "recorded",
        data: &event_payload,
        tenant_context: &tenant_context,
    })?;
    evidence.global_event_id = Some(global_event_id);
    Ok(evidence)
}

pub fn load_milestone_attached_evidence(
    store: &FoundryStore,
    version: &str,
) -> Result<Vec<MilestoneAttachedEvidence>> {
    let version = canonical_milestone_version(version)?;
    let mut evidence = Vec::new();
    for event in store.load_global_events()? {
        if event.kind != MILESTONE_ATTACHED_EVIDENCE_EVENT_KIND {
            continue;
        }
        let mut attached = match serde_json::from_value::<MilestoneAttachedEvidence>(event.data) {
            Ok(attached) => attached,
            Err(_) => continue,
        };
        if attached.milestone != version
            && !(version == SUPPORTED_MILESTONE
                && attached.milestone == LEGACY_MILESTONE_QUERY_ALIAS)
        {
            continue;
        }
        attached.global_event_id = Some(event.id);
        if attached.created_at.trim().is_empty() {
            attached.created_at = event.created_at;
        }
        evidence.push(attached);
    }
    evidence.sort_by_key(|item| item.global_event_id.unwrap_or_default());
    Ok(evidence)
}

pub fn build_milestone_evidence_plan(
    store: &FoundryStore,
    options: MilestoneEvidencePlanOptions<'_>,
) -> Result<MilestoneEvidencePlanReport> {
    let requested_version = normalize_required(options.version, "version")?;
    let version = canonical_milestone_version(&requested_version)?.to_string();
    let capability_id = normalize_required(options.capability_id, "capability")?;
    if !foundry_06_capabilities()
        .iter()
        .any(|capability| capability.id == capability_id)
    {
        bail!("unknown milestone capability `{capability_id}` for milestone {version}");
    }

    let project_root = options
        .project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let required_attached_evidence_kinds =
        milestone_required_attached_evidence_kinds(&capability_id);
    let attached_evidence = load_milestone_attached_evidence(store, &version)?
        .into_iter()
        .filter(|evidence| evidence.capability_id == capability_id)
        .collect::<Vec<_>>();
    let attached_evidence_kinds = attached_evidence
        .iter()
        .map(|evidence| evidence.kind.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let attached_evidence_kind_set = attached_evidence_kinds
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing_attached_evidence_kinds = required_attached_evidence_kinds
        .iter()
        .filter(|kind| !attached_evidence_kind_set.contains(*kind))
        .cloned()
        .collect::<Vec<_>>();

    let mut config_checks = Vec::new();
    let mut manifest_templates = Vec::new();
    let mut provider_candidates = Vec::new();
    let mut configured_evidence_sources = Vec::new();
    let mut evidence_collection_commands = Vec::new();
    let mut attach_commands = Vec::new();

    match capability_id.as_str() {
        "replacement_grade_cli" => {
            provider_candidates = detect_replacement_cli_provider_candidates();
            plan_replacement_grade_cli_evidence(
                &project_root,
                options.connected_brain,
                &mut config_checks,
                &mut manifest_templates,
                &mut configured_evidence_sources,
                &mut evidence_collection_commands,
            )?;
            attach_commands.extend(
                required_attached_evidence_kinds
                    .iter()
                    .map(|kind| milestone_attach_command(&version, &capability_id, kind)),
            );
        }
        "experimental_multimodal_runtime" => {
            plan_experimental_multimodal_evidence(
                &project_root,
                options.connected_runtime,
                &mut config_checks,
                &mut manifest_templates,
                &mut configured_evidence_sources,
                &mut evidence_collection_commands,
            )?;
            attach_commands.extend(
                required_attached_evidence_kinds
                    .iter()
                    .map(|kind| milestone_attach_command(&version, &capability_id, kind)),
            );
        }
        _ => {
            config_checks.push(MilestoneEvidencePlanConfigCheck {
                id: "no_project_specific_evidence_inputs".to_string(),
                status: "not_required".to_string(),
                path: None,
                selected_id: None,
                summary: "This milestone capability has no project-specific evidence input plan."
                    .to_string(),
            });
        }
    }

    let ready_to_collect_evidence = config_checks
        .iter()
        .filter(|check| check.status != "not_required")
        .all(|check| check.status == "ready");
    let status = if ready_to_collect_evidence {
        "evidence_inputs_ready"
    } else if config_checks
        .iter()
        .any(|check| matches!(check.status.as_str(), "blocked" | "invalid"))
    {
        "blocked_project_evidence_inputs"
    } else {
        "missing_project_evidence_inputs"
    };
    let next_action = if ready_to_collect_evidence {
        "Run the evidence collection command, inspect the generated receipt, then attach it with the matching milestone evidence kind.".to_string()
    } else {
        "Create or fix the required project .foundry manifests before collecting milestone evidence."
            .to_string()
    };
    let promotion_gate_templates = milestone_promotion_gate_templates(&capability_id);

    Ok(MilestoneEvidencePlanReport {
        schema_version: "foundry.milestone.evidence_plan.v1".to_string(),
        milestone: version,
        capability_id,
        status: status.to_string(),
        project_root: Some(project_root.display().to_string()),
        ready_to_collect_evidence,
        required_attached_evidence_kinds,
        attached_evidence_kinds,
        missing_attached_evidence_kinds,
        promotion_gate_templates,
        config_checks,
        manifest_templates,
        provider_candidates,
        configured_evidence_sources,
        evidence_collection_commands,
        attach_commands,
        next_action,
        promotion_impact: "planning_only_not_auto_promoted".to_string(),
    })
}

pub fn prepare_milestone_evidence_inputs(
    store: &FoundryStore,
    options: MilestonePrepareEvidenceInputsOptions<'_>,
) -> Result<MilestonePrepareEvidenceInputsReport> {
    let plan = build_milestone_evidence_plan(
        store,
        MilestoneEvidencePlanOptions {
            version: options.version,
            capability_id: options.capability_id,
            project_root: options.project_root,
            connected_brain: options.connected_brain,
            connected_runtime: options.connected_runtime,
        },
    )?;
    let project_root = plan.project_root.clone().unwrap_or_else(|| {
        options
            .project_root
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".".to_string())
    });

    if options.apply && options.approved_by.unwrap_or_default().trim().is_empty() {
        bail!("--approved-by is required when using --apply");
    }
    for template in &plan.manifest_templates {
        if !template.secret_free {
            bail!(
                "refusing to prepare non-secret-free milestone evidence template `{}`",
                template.id
            );
        }
    }
    if options.apply && !options.force {
        for template in &plan.manifest_templates {
            let target_path = PathBuf::from(&template.target_path);
            if target_path.exists() {
                bail!(
                    "refusing to overwrite existing evidence input manifest {}; pass --force after review",
                    target_path.display()
                );
            }
        }
    }

    let mut prepared_files = Vec::new();
    let mut written_count = 0usize;
    let mut skipped_count = 0usize;
    for template in &plan.manifest_templates {
        let target_path = PathBuf::from(&template.target_path);
        let existed_before = target_path.exists();
        let template_json = render_prepared_milestone_template_json(template, &options)?;
        let rendered = serde_json::to_string_pretty(&template_json)
            .context("render milestone evidence input template")?;
        let content = format!("{rendered}\n");
        let created_parent_dir = if options.apply {
            let parent = target_path
                .parent()
                .context("evidence input target has no parent")?;
            let missing_parent = !parent.exists();
            fs::create_dir_all(parent)
                .with_context(|| format!("create evidence input directory {}", parent.display()))?;
            missing_parent
        } else {
            false
        };
        let write_status = if options.apply {
            fs::write(&target_path, &content)
                .with_context(|| format!("write evidence input {}", target_path.display()))?;
            written_count += 1;
            if existed_before {
                "overwritten"
            } else {
                "written"
            }
        } else {
            skipped_count += 1;
            "planned"
        };
        prepared_files.push(MilestonePreparedEvidenceInputFile {
            template_id: template.id.clone(),
            target_path: template.target_path.clone(),
            secret_free: template.secret_free,
            existed_before,
            created_parent_dir,
            write_status: write_status.to_string(),
            bytes: content.len(),
            sha256: hex_sha256(content.as_bytes()),
            summary: template.summary.clone(),
            validation_commands: template.validation_commands.clone(),
        });
    }

    let mut next_commands = plan
        .manifest_templates
        .iter()
        .flat_map(|template| template.validation_commands.clone())
        .collect::<Vec<_>>();
    next_commands.extend(plan.evidence_collection_commands.clone());
    next_commands.sort();
    next_commands.dedup();

    let status = if plan.manifest_templates.is_empty() {
        "no_manifest_templates"
    } else if !options.apply {
        "manifest_templates_planned"
    } else if written_count > 0 {
        "manifest_templates_written"
    } else {
        "no_manifest_templates"
    };
    let next_action = if options.apply {
        "Review the prepared manifest files, replace placeholders with approved local commands or runtime ids, then rerun evidence-plan before collecting evidence."
    } else {
        "Review the planned secret-free manifest templates, then rerun with --apply --approved-by <operator> to write them."
    };

    Ok(MilestonePrepareEvidenceInputsReport {
        schema_version: "foundry.milestone.prepare_evidence_inputs.v1".to_string(),
        milestone: plan.milestone.clone(),
        capability_id: plan.capability_id.clone(),
        status: status.to_string(),
        project_root,
        apply: options.apply,
        mutates_files: options.apply,
        approved_by: options.approved_by.map(str::to_string),
        force: options.force,
        origin: options.origin.to_string(),
        template_count: plan.manifest_templates.len(),
        written_count,
        skipped_count,
        prepared_files,
        evidence_plan: plan,
        next_commands,
        next_action: next_action.to_string(),
        promotion_impact: "prepares_inputs_only_not_evidence_not_auto_promoted".to_string(),
    })
}

fn render_prepared_milestone_template_json(
    template: &MilestoneEvidencePlanManifestTemplate,
    options: &MilestonePrepareEvidenceInputsOptions<'_>,
) -> Result<serde_json::Value> {
    if template.id != "connected_brain_runtime_manifest"
        || options.capability_id != "replacement_grade_cli"
        || (options.provider_command.is_none()
            && options.model_id.is_none()
            && options.approval_ref.is_none())
    {
        return Ok(template.template_json.clone());
    }

    let provider_command = options.provider_command.context(
        "--provider-command is required when preparing an approved connected brain provider manifest",
    )?;
    let model_id = normalize_required(
        options.model_id.unwrap_or_default(),
        "model-id for approved connected brain provider manifest",
    )?;
    let approval_ref = normalize_required(
        options.approval_ref.unwrap_or_default(),
        "approval-ref for approved connected brain provider manifest",
    )?;
    let approved_by = normalize_required(
        options.approved_by.unwrap_or_default(),
        "approved-by for approved connected brain provider manifest",
    )?;

    let project_root = options
        .project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let command_path =
        normalize_connected_brain_provider_command_path(&project_root, provider_command);
    let command_path_text = command_path.display().to_string();
    let (command_status, command_summary) =
        milestone_connected_brain_command_path_status(Some(&command_path_text));
    if command_status != "ready" {
        bail!("{command_summary}");
    }

    let mut template_json = template.template_json.clone();
    let providers = template_json
        .get_mut("providers")
        .and_then(serde_json::Value::as_array_mut)
        .context("connected brain runtime manifest template must contain providers array")?;
    let provider = providers
        .first_mut()
        .context("connected brain runtime manifest template must contain a provider")?;
    let provider_id = options
        .connected_brain
        .map(str::to_string)
        .or_else(|| {
            provider
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "codex".to_string());

    provider["id"] = serde_json::json!(provider_id);
    provider["brain_id"] = serde_json::json!(options.connected_brain.unwrap_or("codex"));
    provider["provider_class"] = serde_json::json!("external_cli");
    provider["command"] = serde_json::json!([command_path_text]);
    provider["approved_by"] = serde_json::json!(approved_by);
    provider["approval_ref"] = serde_json::json!(approval_ref);
    provider["model_id"] = serde_json::json!(model_id);
    provider["allow_model_execution"] = serde_json::json!(true);
    provider["network_access"] = serde_json::json!(false);
    provider["device_access"] = serde_json::json!(false);
    provider["external_resources_mutated"] = serde_json::json!(false);
    provider["capabilities"] = serde_json::json!(["replacement_grade_cli"]);

    Ok(template_json)
}

fn normalize_connected_brain_provider_command_path(
    project_root: &Path,
    provider_command: &Path,
) -> PathBuf {
    if provider_command.is_absolute() {
        provider_command.to_path_buf()
    } else {
        project_root.join(provider_command)
    }
}

pub fn collect_milestone_evidence(
    store: &FoundryStore,
    options: MilestoneCollectEvidenceOptions<'_>,
) -> Result<MilestoneCollectEvidenceReport> {
    let requested_version = normalize_required(options.version, "version")?;
    let version = canonical_milestone_version(&requested_version)?.to_string();
    let capability_id = normalize_required(options.capability_id, "capability")?;
    if !foundry_06_capabilities()
        .iter()
        .any(|capability| capability.id == capability_id)
    {
        bail!("unknown milestone capability `{capability_id}` for milestone {version}");
    }
    let kind = match options.kind {
        Some(kind) => normalize_required(kind, "kind")?,
        None => default_milestone_collection_kind(&capability_id)?,
    };
    ensure_milestone_collection_kind(&capability_id, &kind)?;
    let approved_by = normalize_required(options.approved_by, "approved-by")?;
    let origin = normalize_required(options.origin, "origin")?;
    let project_root = options
        .project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if milestone_collection_kind_requires_project_plan(&capability_id, &kind) {
        let plan = build_milestone_evidence_plan(
            store,
            MilestoneEvidencePlanOptions {
                version: &version,
                capability_id: &capability_id,
                project_root: Some(&project_root),
                connected_brain: options.connected_brain,
                connected_runtime: options.connected_runtime,
            },
        )?;
        if !plan.ready_to_collect_evidence {
            bail!(
                "milestone evidence inputs for `{capability_id}` are not ready: {}; {}",
                plan.status,
                plan.next_action
            );
        }
    }

    let collected = match capability_id.as_str() {
        "replacement_grade_cli" => collect_replacement_grade_cli_evidence(
            store,
            &version,
            &project_root,
            &kind,
            options.connected_brain,
            &origin,
        )?,
        "experimental_multimodal_runtime" => collect_experimental_multimodal_runtime_evidence(
            store,
            &version,
            &project_root,
            options.connected_runtime,
            &approved_by,
        )?,
        _ => bail!(
            "capability `{capability_id}` does not have an automatic milestone evidence collector"
        ),
    };

    if !collected.collection_promotion_ready {
        bail!(
            "collector for `{capability_id}` produced non-promotion-ready evidence: {}",
            collected.collection_summary
        );
    }

    let attached_evidence = attach_milestone_evidence(
        store,
        MilestoneAttachEvidenceOptions {
            version: &version,
            capability_id: &capability_id,
            kind: &collected.kind,
            summary: &collected.collection_summary,
            artifact_path: &collected.collection_artifact_path,
            approved_by: &approved_by,
            origin: &origin,
        },
    )?;

    Ok(MilestoneCollectEvidenceReport {
        schema_version: "foundry.milestone.collect_evidence.v1".to_string(),
        milestone: version,
        capability_id,
        kind: collected.kind,
        status: "collected_and_attached".to_string(),
        project_root: project_root.display().to_string(),
        configured_evidence_source: collected.configured_evidence_source,
        collection_promotion_ready: collected.collection_promotion_ready,
        promotion_gates: collected.promotion_gates,
        collection_artifact_path: collected.collection_artifact_path.display().to_string(),
        collection_artifact_sha256: collected.collection_artifact_sha256,
        collection_summary: collected.collection_summary,
        attached_evidence,
        promotion_impact: "collected_and_attached_not_auto_promoted".to_string(),
        next_action:
            "Inspect `foundry milestone manifest --version 0.6 --output json`; promotion remains gated by all required evidence."
                .to_string(),
    })
}

pub fn collect_ready_milestone_evidence(
    store: &FoundryStore,
    options: MilestoneCollectReadyEvidenceOptions<'_>,
) -> Result<MilestoneCollectReadyEvidenceReport> {
    let requested_version = normalize_required(options.version, "version")?;
    let version = canonical_milestone_version(&requested_version)?.to_string();
    let approved_by = normalize_required(options.approved_by, "approved-by")?;
    let origin = normalize_required(options.origin, "origin")?;
    let project_root = options
        .project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let targets = foundry_06_capabilities()
        .into_iter()
        .flat_map(|capability| {
            milestone_required_attached_evidence_kinds(&capability.id)
                .into_iter()
                .map(move |kind| (capability.id.clone(), kind))
        })
        .collect::<Vec<_>>();
    let required_count = targets.len();
    let mut collected_evidence = Vec::new();
    let mut skipped_evidence = Vec::new();
    let mut failed_evidence = Vec::new();

    for (capability_id, kind) in targets {
        if milestone_collection_kind_requires_project_plan(&capability_id, &kind) {
            let plan = build_milestone_evidence_plan(
                store,
                MilestoneEvidencePlanOptions {
                    version: &version,
                    capability_id: &capability_id,
                    project_root: Some(&project_root),
                    connected_brain: options.connected_brain,
                    connected_runtime: options.connected_runtime,
                },
            )?;
            if !plan.ready_to_collect_evidence {
                skipped_evidence.push(MilestoneCollectReadyEvidenceSkipped {
                    capability_id,
                    kind,
                    status: "not_ready_to_collect".to_string(),
                    reason: plan.next_action.clone(),
                    evidence_plan: plan,
                });
                continue;
            }
        }

        match collect_milestone_evidence(
            store,
            MilestoneCollectEvidenceOptions {
                version: &version,
                capability_id: &capability_id,
                kind: Some(&kind),
                project_root: Some(&project_root),
                connected_brain: options.connected_brain,
                connected_runtime: options.connected_runtime,
                approved_by: &approved_by,
                origin: &origin,
            },
        ) {
            Ok(report) => collected_evidence.push(MilestoneCollectReadyEvidenceCollected {
                capability_id: report.capability_id,
                kind: report.kind,
                status: report.status,
                configured_evidence_source: report.configured_evidence_source,
                collection_promotion_ready: report.collection_promotion_ready,
                collection_artifact_path: report.collection_artifact_path,
                collection_artifact_sha256: report.collection_artifact_sha256,
                attached_evidence_id: report.attached_evidence.evidence_id,
                attached_artifact_path: report.attached_evidence.artifact_path,
                summary: report.collection_summary,
            }),
            Err(error) => failed_evidence.push(MilestoneCollectReadyEvidenceFailed {
                capability_id,
                kind,
                status: "collection_failed".to_string(),
                error: error.to_string(),
            }),
        }
    }

    let manifest = build_milestone_manifest_with_store(&version, Some(store))?;
    let promotion_decision_after_collection = manifest.promotion_decision.clone();
    let collected_count = collected_evidence.len();
    let skipped_count = skipped_evidence.len();
    let failed_count = failed_evidence.len();
    let status = if failed_count > 0 {
        "collection_with_failures"
    } else if skipped_count > 0 && collected_count > 0 {
        "partial_collection"
    } else if skipped_count > 0 {
        "no_ready_evidence"
    } else {
        "all_ready_evidence_collected"
    };
    let mut next_commands = skipped_evidence
        .iter()
        .map(|item| {
            format!(
                "foundry milestone evidence-plan --version {} --capability {} --project-root {} --output json",
                version,
                item.capability_id,
                project_root.display()
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    next_commands.push("foundry milestone manifest --version 0.6 --output json".to_string());
    let next_action = if promotion_decision_after_collection.promotable {
        "Inspect the manifest and run an explicit human-controlled release promotion; this command only collected evidence."
            .to_string()
    } else if failed_count > 0 {
        "Inspect failed evidence collectors, fix their errors, then rerun collect-ready-evidence."
            .to_string()
    } else if skipped_count > 0 {
        "Prepare the missing project evidence inputs, then rerun collect-ready-evidence."
            .to_string()
    } else {
        "Inspect the manifest; promotion remains governed by the release decision surface."
            .to_string()
    };

    Ok(MilestoneCollectReadyEvidenceReport {
        schema_version: "foundry.milestone.collect_ready_evidence.v1".to_string(),
        milestone: version,
        status: status.to_string(),
        project_root: project_root.display().to_string(),
        approved_by,
        origin,
        required_count,
        collected_count,
        skipped_count,
        failed_count,
        promotion_ready_after_collection: promotion_decision_after_collection.promotable,
        promotion_decision_after_collection,
        collected_evidence,
        skipped_evidence,
        failed_evidence,
        next_commands,
        next_action,
        promotion_impact: "collects_ready_required_evidence_not_auto_promoted".to_string(),
    })
}

struct CollectedMilestoneEvidence {
    kind: String,
    configured_evidence_source: String,
    collection_promotion_ready: bool,
    promotion_gates: Vec<MilestonePromotionGate>,
    collection_artifact_path: PathBuf,
    collection_artifact_sha256: String,
    collection_summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePromotionGate {
    pub id: String,
    pub passed: bool,
    pub summary: String,
}

fn milestone_promotion_gate(
    id: impl Into<String>,
    passed: bool,
    summary: impl Into<String>,
) -> MilestonePromotionGate {
    MilestonePromotionGate {
        id: id.into(),
        passed,
        summary: summary.into(),
    }
}

fn milestone_promotion_gates_passed(gates: &[MilestonePromotionGate]) -> bool {
    gates.iter().all(|gate| gate.passed)
}

fn default_milestone_collection_kind(capability_id: &str) -> Result<String> {
    match capability_id {
        "replacement_grade_cli" => Ok("external_brain_provider_execution".to_string()),
        "experimental_multimodal_runtime" => Ok("production_runtime_benchmark".to_string()),
        _ => bail!("capability `{capability_id}` does not have a default evidence collector"),
    }
}

fn ensure_milestone_collection_kind(capability_id: &str, kind: &str) -> Result<()> {
    if milestone_required_attached_evidence_kinds(capability_id)
        .iter()
        .any(|required| required == kind)
    {
        return Ok(());
    }
    bail!("evidence kind `{kind}` is not required by capability `{capability_id}`")
}

fn milestone_collection_kind_requires_project_plan(capability_id: &str, kind: &str) -> bool {
    matches!(
        (capability_id, kind),
        ("replacement_grade_cli", "external_brain_provider_execution")
            | (
                "experimental_multimodal_runtime",
                "production_runtime_benchmark"
            )
    )
}

fn collect_replacement_grade_cli_evidence(
    store: &FoundryStore,
    version: &str,
    project_root: &Path,
    kind: &str,
    connected_brain: Option<&str>,
    origin: &str,
) -> Result<CollectedMilestoneEvidence> {
    match kind {
        "external_brain_provider_execution" => collect_replacement_grade_cli_provider_evidence(
            store,
            version,
            project_root,
            connected_brain,
            origin,
        ),
        "broader_project_coding_research_workflow" => {
            collect_replacement_grade_cli_real_project_evidence(
                store,
                version,
                project_root,
                origin,
            )
        }
        "terminal_file_editing_ux" => collect_replacement_grade_cli_terminal_editing_evidence(
            store,
            version,
            project_root,
            origin,
        ),
        _ => bail!("unsupported replacement-grade CLI evidence kind `{kind}`"),
    }
}

fn collect_replacement_grade_cli_provider_evidence(
    store: &FoundryStore,
    version: &str,
    project_root: &Path,
    connected_brain: Option<&str>,
    origin: &str,
) -> Result<CollectedMilestoneEvidence> {
    let report = build_replacement_cli_demo_with_options(
        store,
        origin,
        MilestoneCliDemoOptions {
            project_root: Some(project_root),
            connected_brain,
        },
    )?;
    let external_brain = report
        .flows
        .iter()
        .find(|flow| flow.kind == "connected_external_brain")
        .and_then(|flow| flow.external_brain.as_ref())
        .context("replacement-grade CLI demo did not produce connected external brain evidence")?;
    let provider_contract = &external_brain.provider_contract;
    let promotion_gates = vec![
        milestone_promotion_gate(
            "provider_contract_validated",
            provider_contract.status == "connected_external_brain_provider_contract_validated",
            format!("provider contract status is `{}`", provider_contract.status),
        ),
        milestone_promotion_gate(
            "output_schema_valid",
            provider_contract.output_schema_valid,
            "provider output uses the expected Foundry provider-output schema",
        ),
        milestone_promotion_gate(
            "real_provider_execution_performed",
            provider_contract.real_provider_execution_performed,
            "provider declared real provider execution in its reviewed output",
        ),
        milestone_promotion_gate(
            "model_execution_performed",
            provider_contract.provider_declared_model_execution,
            "provider declared model execution in its reviewed output",
        ),
        milestone_promotion_gate(
            "harness_exec_event_recorded",
            external_brain.exec_event_recorded,
            "external brain execution was recorded through Foundry harness lineage",
        ),
        milestone_promotion_gate(
            "external_resources_untouched",
            !external_brain.external_resources_mutated,
            "provider evidence did not mutate external resources",
        ),
    ];
    let collection_promotion_ready =
        provider_contract.promotion_ready && milestone_promotion_gates_passed(&promotion_gates);
    let collection_summary = if collection_promotion_ready {
        format!(
            "Connected external brain provider `{}` executed through Foundry and produced promotion-ready provider evidence.",
            provider_contract.provider_id
        )
    } else {
        format!(
            "Connected external brain provider `{}` executed through Foundry but did not satisfy the promotion-ready provider contract.",
            provider_contract.provider_id
        )
    };
    let payload = serde_json::json!({
        "schema_version": "foundry.milestone.collection.external_brain_provider_execution.v1",
        "milestone": version,
        "capability_id": "replacement_grade_cli",
        "kind": "external_brain_provider_execution",
        "collected_at": Utc::now().to_rfc3339(),
        "project_root": project_root.display().to_string(),
        "connected_brain": connected_brain.unwrap_or(&provider_contract.provider_id),
        "collection_promotion_ready": collection_promotion_ready,
        "promotion_gates": &promotion_gates,
        "provider_contract": &provider_contract,
        "external_brain": &external_brain,
        "source_demo": &report,
    });
    let (collection_artifact_path, collection_artifact_sha256) =
        write_milestone_collection_artifact(
            store,
            version,
            "replacement_grade_cli",
            "external_brain_provider_execution",
            &payload,
        )?;

    Ok(CollectedMilestoneEvidence {
        kind: "external_brain_provider_execution".to_string(),
        configured_evidence_source: format!(
            "connected_brain_provider:{}",
            provider_contract.provider_id
        ),
        collection_promotion_ready,
        promotion_gates,
        collection_artifact_path,
        collection_artifact_sha256,
        collection_summary,
    })
}

fn collect_replacement_grade_cli_real_project_evidence(
    store: &FoundryStore,
    version: &str,
    project_root: &Path,
    origin: &str,
) -> Result<CollectedMilestoneEvidence> {
    let report =
        build_replacement_cli_demo_with_options(store, origin, MilestoneCliDemoOptions::default())?;
    let flow = report
        .flows
        .iter()
        .find(|flow| flow.kind == "real_project_coding_research")
        .context(
            "replacement-grade CLI demo did not produce real-project coding/research evidence",
        )?;
    let real_project = flow
        .real_project
        .as_ref()
        .context("real-project coding/research flow is missing its evidence payload")?;
    let promotion_gates = vec![
        milestone_promotion_gate(
            "completed_through_foundry",
            flow.completed_through_foundry,
            "flow completed through Foundry-owned workflow semantics",
        ),
        milestone_promotion_gate(
            "real_project_demo_completed",
            real_project.status == "real_project_workflow_demo_completed",
            format!("real-project flow status is `{}`", real_project.status),
        ),
        milestone_promotion_gate(
            "handoff_ready",
            real_project.handoff_ready,
            "project-root handoff packet was ready for the selected brain",
        ),
        milestone_promotion_gate(
            "exec_event_recorded",
            real_project.exec_event_recorded,
            "Foundry harness recorded workflow/task/run lineage for the project execution",
        ),
        milestone_promotion_gate(
            "validated_multi_file_artifacts",
            real_project.validation_status == "validated" && !real_project.target_paths.is_empty(),
            format!(
                "validation status is `{}` across {} target paths",
                real_project.validation_status,
                real_project.target_paths.len()
            ),
        ),
        milestone_promotion_gate(
            "external_resources_untouched",
            !real_project.external_resources_mutated,
            "real-project evidence stayed inside the isolated project fixture",
        ),
    ];
    let collection_promotion_ready = milestone_promotion_gates_passed(&promotion_gates);
    let collection_summary = if collection_promotion_ready {
        "Replacement-grade CLI real-project coding and research workflow produced validated multi-file evidence under Foundry lineage.".to_string()
    } else {
        "Replacement-grade CLI real-project coding and research workflow did not satisfy all collection gates.".to_string()
    };
    let payload = serde_json::json!({
        "schema_version": "foundry.milestone.collection.broader_project_coding_research_workflow.v1",
        "milestone": version,
        "capability_id": "replacement_grade_cli",
        "kind": "broader_project_coding_research_workflow",
        "collected_at": Utc::now().to_rfc3339(),
        "requested_project_root": project_root.display().to_string(),
        "collection_promotion_ready": collection_promotion_ready,
        "promotion_gates": &promotion_gates,
        "real_project": real_project,
        "source_flow": flow,
        "source_demo": &report,
    });
    let (collection_artifact_path, collection_artifact_sha256) =
        write_milestone_collection_artifact(
            store,
            version,
            "replacement_grade_cli",
            "broader_project_coding_research_workflow",
            &payload,
        )?;

    Ok(CollectedMilestoneEvidence {
        kind: "broader_project_coding_research_workflow".to_string(),
        configured_evidence_source: "replacement_cli_demo:real_project_coding_research".to_string(),
        collection_promotion_ready,
        promotion_gates,
        collection_artifact_path,
        collection_artifact_sha256,
        collection_summary,
    })
}

fn collect_replacement_grade_cli_terminal_editing_evidence(
    store: &FoundryStore,
    version: &str,
    project_root: &Path,
    origin: &str,
) -> Result<CollectedMilestoneEvidence> {
    let report =
        build_replacement_cli_demo_with_options(store, origin, MilestoneCliDemoOptions::default())?;
    let flow = report
        .flows
        .iter()
        .find(|flow| flow.kind == "coding_task")
        .context("replacement-grade CLI demo did not produce coding-task patch evidence")?;
    let patch_lifecycle = flow
        .patch_lifecycle
        .as_ref()
        .context("coding-task flow is missing patch lifecycle evidence")?;
    let promotion_gates = vec![
        milestone_promotion_gate(
            "completed_through_foundry",
            flow.completed_through_foundry,
            "terminal editing flow completed through Foundry-owned workflow semantics",
        ),
        milestone_promotion_gate(
            "patch_lifecycle_ready",
            patch_lifecycle.status == "patch_lifecycle_demo_ready",
            format!("patch lifecycle status is `{}`", patch_lifecycle.status),
        ),
        milestone_promotion_gate(
            "review_before_apply",
            patch_lifecycle
                .gates
                .iter()
                .any(|gate| gate == "review_before_apply"),
            "patch lifecycle requires review before apply",
        ),
        milestone_promotion_gate(
            "restore_approval_recorded",
            patch_lifecycle
                .gates
                .iter()
                .any(|gate| gate == "human_restore_approval_recorded"),
            "patch lifecycle records human approval before restore",
        ),
        milestone_promotion_gate(
            "restored_to_clean_state",
            patch_lifecycle.restored_to_clean_state,
            "approved restore returned the fixture repository to a clean state",
        ),
        milestone_promotion_gate(
            "artifact_lineage_complete",
            patch_lifecycle.artifact_refs.len() >= 6,
            format!(
                "patch lifecycle recorded {} artifact refs",
                patch_lifecycle.artifact_refs.len()
            ),
        ),
        milestone_promotion_gate(
            "external_resources_untouched",
            !patch_lifecycle.external_resources_mutated,
            "patch lifecycle stayed inside the isolated fixture repository",
        ),
    ];
    let collection_promotion_ready = milestone_promotion_gates_passed(&promotion_gates);
    let collection_summary = if collection_promotion_ready {
        "Replacement-grade CLI terminal file-editing UX produced validated plan/review/diff/apply/revert/restore evidence.".to_string()
    } else {
        "Replacement-grade CLI terminal file-editing UX did not satisfy all patch lifecycle collection gates.".to_string()
    };
    let payload = serde_json::json!({
        "schema_version": "foundry.milestone.collection.terminal_file_editing_ux.v1",
        "milestone": version,
        "capability_id": "replacement_grade_cli",
        "kind": "terminal_file_editing_ux",
        "collected_at": Utc::now().to_rfc3339(),
        "requested_project_root": project_root.display().to_string(),
        "collection_promotion_ready": collection_promotion_ready,
        "promotion_gates": &promotion_gates,
        "patch_lifecycle": patch_lifecycle,
        "source_flow": flow,
        "source_demo": &report,
    });
    let (collection_artifact_path, collection_artifact_sha256) =
        write_milestone_collection_artifact(
            store,
            version,
            "replacement_grade_cli",
            "terminal_file_editing_ux",
            &payload,
        )?;

    Ok(CollectedMilestoneEvidence {
        kind: "terminal_file_editing_ux".to_string(),
        configured_evidence_source: "replacement_cli_demo:patch_lifecycle".to_string(),
        collection_promotion_ready,
        promotion_gates,
        collection_artifact_path,
        collection_artifact_sha256,
        collection_summary,
    })
}

fn collect_experimental_multimodal_runtime_evidence(
    store: &FoundryStore,
    version: &str,
    project_root: &Path,
    connected_runtime: Option<&str>,
    approved_by: &str,
) -> Result<CollectedMilestoneEvidence> {
    let (runtime_id, runtime_capability) =
        selected_multimodal_runtime_capability(project_root, connected_runtime)?;
    let feature_flag = resolve_multimodal_feature_flag(false, Some(project_root));
    let report = build_multimodal_runtime_benchmark(MultimodalRuntimeBenchmarkOptions {
        capability_id: &runtime_capability,
        fixture_id: "static_image_labels",
        enable_experimental: feature_flag.enabled,
        project_root: Some(project_root),
        approved_by: Some(approved_by),
        confirm_runtime_execution: true,
        allow_model: true,
        connected_runtime: Some(&runtime_id),
    })?;
    let promotion_gates = vec![
        milestone_promotion_gate(
            "runtime_benchmark_promotion_ready",
            report.promotion_ready,
            "connected runtime benchmark satisfied its production thresholds",
        ),
        milestone_promotion_gate(
            "model_guard_approved",
            report.guard.allowed,
            "model execution guard was approved for the benchmark",
        ),
        milestone_promotion_gate(
            "network_access_blocked",
            !report.network_access_performed,
            "network access remained blocked during benchmark collection",
        ),
        milestone_promotion_gate(
            "device_access_blocked",
            !report.device_access_performed
                && !report.camera_access_performed
                && !report.microphone_access_performed
                && !report.screen_access_performed
                && !report.input_access_performed,
            "camera, microphone, screen and input access remained blocked",
        ),
    ];
    let collection_promotion_ready =
        report.promotion_ready && milestone_promotion_gates_passed(&promotion_gates);
    let collection_summary = if collection_promotion_ready {
        format!(
            "Connected multimodal runtime `{}` produced promotion-ready production benchmark evidence for `{}`.",
            runtime_id, runtime_capability
        )
    } else {
        format!(
            "Connected multimodal runtime `{}` ran but did not satisfy production benchmark promotion gates for `{}`.",
            runtime_id, runtime_capability
        )
    };
    let payload = serde_json::json!({
        "schema_version": "foundry.milestone.collection.production_runtime_benchmark.v1",
        "milestone": version,
        "capability_id": "experimental_multimodal_runtime",
        "kind": "production_runtime_benchmark",
        "collected_at": Utc::now().to_rfc3339(),
        "project_root": project_root.display().to_string(),
        "connected_runtime": &runtime_id,
        "runtime_capability": &runtime_capability,
        "collection_promotion_ready": collection_promotion_ready,
        "promotion_gates": &promotion_gates,
        "runtime_benchmark": &report,
    });
    let (collection_artifact_path, collection_artifact_sha256) =
        write_milestone_collection_artifact(
            store,
            version,
            "experimental_multimodal_runtime",
            "production_runtime_benchmark",
            &payload,
        )?;

    Ok(CollectedMilestoneEvidence {
        kind: "production_runtime_benchmark".to_string(),
        configured_evidence_source: format!("connected_multimodal_runtime:{runtime_id}"),
        collection_promotion_ready,
        promotion_gates,
        collection_artifact_path,
        collection_artifact_sha256,
        collection_summary,
    })
}

fn selected_multimodal_runtime_capability(
    project_root: &Path,
    connected_runtime: Option<&str>,
) -> Result<(String, String)> {
    let manifest_path =
        crate::brand::project_config_path_for_read(project_root, "multimodal-runtimes.json");
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)
        .with_context(|| {
            format!(
                "invalid multimodal runtime manifest {}",
                manifest_path.display()
            )
        })?;
    let runtimes = manifest
        .get("runtimes")
        .and_then(serde_json::Value::as_array)
        .context("multimodal runtime manifest must contain a runtimes array")?;
    let selected = runtimes.iter().find(|runtime| {
        let id = runtime
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        connected_runtime
            .map(|requested| requested == id)
            .unwrap_or_else(|| runtime.get("production").is_some())
    });
    let runtime = selected.context("no selected connected multimodal runtime was found")?;
    let runtime_id = runtime
        .get("id")
        .and_then(serde_json::Value::as_str)
        .context("selected connected multimodal runtime is missing id")?
        .to_string();
    let runtime_capability = runtime
        .get("capabilities")
        .and_then(serde_json::Value::as_array)
        .and_then(|capabilities| {
            capabilities
                .iter()
                .filter_map(serde_json::Value::as_str)
                .next()
        })
        .unwrap_or("image_understanding")
        .to_string();
    Ok((runtime_id, runtime_capability))
}

fn write_milestone_collection_artifact<T: Serialize>(
    store: &FoundryStore,
    version: &str,
    capability_id: &str,
    kind: &str,
    payload: &T,
) -> Result<(PathBuf, String)> {
    let dir = store
        .base_dir()
        .join("tmp")
        .join("milestone")
        .join(sanitize_milestone_component(version));
    fs::create_dir_all(&dir).with_context(|| {
        format!(
            "failed to create milestone collection artifact directory {}",
            dir.display()
        )
    })?;
    let file_name = format!(
        "{}-{}-collection-{}.json",
        sanitize_milestone_component(capability_id),
        sanitize_milestone_component(kind),
        Utc::now().timestamp_millis()
    );
    let path = dir.join(file_name);
    let bytes = serde_json::to_vec_pretty(payload)?;
    let sha256 = hex_sha256(&bytes);
    fs::write(&path, bytes).with_context(|| {
        format!(
            "failed to write milestone collection artifact {}",
            path.display()
        )
    })?;
    Ok((path, sha256))
}

pub fn milestone_required_attached_evidence_kinds(capability_id: &str) -> Vec<String> {
    match capability_id {
        "replacement_grade_cli" => vec![
            "external_brain_provider_execution".to_string(),
            "broader_project_coding_research_workflow".to_string(),
            "terminal_file_editing_ux".to_string(),
        ],
        "experimental_multimodal_runtime" => vec!["production_runtime_benchmark".to_string()],
        _ => Vec::new(),
    }
}

fn milestone_promotion_gate_templates(capability_id: &str) -> Vec<MilestonePromotionGateTemplate> {
    match capability_id {
        "replacement_grade_cli" => vec![
            milestone_promotion_gate_template(
                "external_brain_provider_execution",
                &[
                    "provider_contract_validated",
                    "output_schema_valid",
                    "real_provider_execution_performed",
                    "model_execution_performed",
                    "harness_exec_event_recorded",
                    "external_resources_untouched",
                ],
                "Connected brain provider evidence must prove a validated provider contract, real/model execution declaration, harness lineage and no external resource mutation.",
            ),
            milestone_promotion_gate_template(
                "broader_project_coding_research_workflow",
                &[
                    "completed_through_foundry",
                    "real_project_demo_completed",
                    "handoff_ready",
                    "exec_event_recorded",
                    "validated_multi_file_artifacts",
                    "external_resources_untouched",
                ],
                "Broader project evidence must prove Foundry-owned handoff, harness lineage, validated multi-file code/research artifacts and no external mutation.",
            ),
            milestone_promotion_gate_template(
                "terminal_file_editing_ux",
                &[
                    "completed_through_foundry",
                    "patch_lifecycle_ready",
                    "review_before_apply",
                    "restore_approval_recorded",
                    "restored_to_clean_state",
                    "artifact_lineage_complete",
                    "external_resources_untouched",
                ],
                "Terminal editing evidence must prove review-before-apply, approved restore, clean rollback state and complete patch artifact lineage.",
            ),
        ],
        "experimental_multimodal_runtime" => vec![milestone_promotion_gate_template(
            "production_runtime_benchmark",
            &[
                "runtime_benchmark_promotion_ready",
                "model_guard_approved",
                "network_access_blocked",
                "device_access_blocked",
            ],
            "Production multimodal evidence must prove threshold-ready benchmark output, approved model guard and blocked network/device access.",
        )],
        _ => Vec::new(),
    }
}

fn milestone_promotion_gate_template(
    evidence_kind: &str,
    gate_ids: &[&str],
    summary: &str,
) -> MilestonePromotionGateTemplate {
    MilestonePromotionGateTemplate {
        evidence_kind: evidence_kind.to_string(),
        gate_ids: gate_ids.iter().map(|gate| (*gate).to_string()).collect(),
        summary: summary.to_string(),
    }
}

fn plan_replacement_grade_cli_evidence(
    project_root: &Path,
    connected_brain: Option<&str>,
    config_checks: &mut Vec<MilestoneEvidencePlanConfigCheck>,
    manifest_templates: &mut Vec<MilestoneEvidencePlanManifestTemplate>,
    configured_evidence_sources: &mut Vec<String>,
    evidence_collection_commands: &mut Vec<String>,
) -> Result<()> {
    let manifest_path =
        crate::brand::project_config_path_for_read(project_root, "connected-brain-runtimes.json");
    if !manifest_path.is_file() {
        manifest_templates.push(connected_brain_manifest_template(
            project_root,
            connected_brain,
        ));
        config_checks.push(MilestoneEvidencePlanConfigCheck {
            id: "connected_brain_manifest".to_string(),
            status: "missing".to_string(),
            path: Some(manifest_path.display().to_string()),
            selected_id: connected_brain.map(ToString::to_string),
            summary: format!(
                "Create {} with a provider for replacement_grade_cli.",
                CONNECTED_BRAIN_RUNTIMES_RELATIVE_PATH
            ),
        });
        evidence_collection_commands.push(format!(
            "foundry milestone collect-evidence --version 0.6 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root {} --connected-brain <provider-id> --approved-by <operator> --origin codex --output json",
            project_root.display()
        ));
        push_replacement_grade_cli_demo_collection_commands(
            project_root,
            evidence_collection_commands,
        );
        evidence_collection_commands.push(format!(
            "foundry milestone cli-demo --origin codex --project-root {} --connected-brain <provider-id> --output json",
            project_root.display()
        ));
        return Ok(());
    }

    let manifest: ConnectedBrainRuntimeManifest =
        serde_json::from_slice(&fs::read(&manifest_path)?).with_context(|| {
            format!(
                "invalid connected brain runtime manifest {}",
                manifest_path.display()
            )
        })?;
    config_checks.push(MilestoneEvidencePlanConfigCheck {
        id: "connected_brain_manifest".to_string(),
        status: "ready".to_string(),
        path: Some(manifest_path.display().to_string()),
        selected_id: None,
        summary: "Connected brain runtime manifest is present and parseable.".to_string(),
    });

    let selected = manifest.providers.into_iter().find(|provider| {
        let id_matches = connected_brain
            .map(|connected_brain| provider.id == connected_brain)
            .unwrap_or(true);
        id_matches
            && provider
                .capabilities
                .iter()
                .any(|capability| capability == "replacement_grade_cli")
    });
    let Some(provider) = selected else {
        manifest_templates.push(connected_brain_manifest_template(
            project_root,
            connected_brain,
        ));
        config_checks.push(MilestoneEvidencePlanConfigCheck {
            id: "connected_brain_provider".to_string(),
            status: "missing".to_string(),
            path: Some(manifest_path.display().to_string()),
            selected_id: connected_brain.map(ToString::to_string),
            summary: "No provider in connected-brain-runtimes.json declares replacement_grade_cli."
                .to_string(),
        });
        return Ok(());
    };

    let provider_command_declared = !provider.command.is_empty()
        && provider
            .command
            .iter()
            .all(|part| !milestone_manifest_placeholder(part));
    let provider_approval_ready = provider
        .approved_by
        .as_deref()
        .is_some_and(|value| !milestone_manifest_placeholder(value))
        && provider
            .approval_ref
            .as_deref()
            .is_some_and(|value| !milestone_manifest_placeholder(value));
    let provider_model_ready = provider
        .model_id
        .as_deref()
        .is_some_and(|value| !milestone_manifest_placeholder(value));
    let provider_config_ready = provider_command_declared
        && provider_approval_ready
        && provider_model_ready
        && provider.allow_model_execution
        && !provider.network_access
        && !provider.device_access
        && !provider.external_resources_mutated
        && provider
            .capabilities
            .iter()
            .any(|capability| capability == "replacement_grade_cli");
    let status = if provider_config_ready {
        "ready"
    } else {
        "blocked"
    };
    let summary = if provider_config_ready {
        "Connected brain provider is declared, approved for guarded execution and safe to probe."
            .to_string()
    } else {
        "Connected brain provider must replace placeholders with an approved command, approved_by, approval_ref, model_id, allow model execution, replacement_grade_cli capability and no network/device/external-resource mutation declarations."
            .to_string()
    };
    config_checks.push(MilestoneEvidencePlanConfigCheck {
        id: "connected_brain_provider".to_string(),
        status: status.to_string(),
        path: Some(manifest_path.display().to_string()),
        selected_id: Some(provider.id.clone()),
        summary,
    });
    let command_check = milestone_connected_brain_provider_command_check(&provider, project_root);
    let command_ready = command_check.status == "ready";
    config_checks.push(command_check);
    if !provider_config_ready || !command_ready {
        manifest_templates.push(connected_brain_manifest_template(
            project_root,
            Some(&provider.id),
        ));
    }
    configured_evidence_sources.push(format!("connected_brain_provider:{}", provider.id));
    configured_evidence_sources
        .push("replacement_cli_demo:real_project_coding_research".to_string());
    configured_evidence_sources.push("replacement_cli_demo:patch_lifecycle".to_string());
    evidence_collection_commands.push(format!(
        "foundry milestone collect-evidence --version 0.6 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root {} --connected-brain {} --approved-by <operator> --origin codex --output json",
        project_root.display(),
        provider.id
    ));
    push_replacement_grade_cli_demo_collection_commands(project_root, evidence_collection_commands);
    evidence_collection_commands.push(format!(
        "foundry milestone cli-demo --origin codex --project-root {} --connected-brain {} --output json",
        project_root.display(),
        provider.id
    ));
    Ok(())
}

fn milestone_manifest_placeholder(value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || (value.starts_with('<') && value.ends_with('>'))
        || value.contains("<absolute-path-to-")
        || value.contains("<approved-")
        || value.contains("<operator>")
        || value.contains("<approval-or-change-record>")
}

fn milestone_connected_brain_provider_command_check(
    provider: &ConnectedBrainProviderConfig,
    project_root: &Path,
) -> MilestoneEvidencePlanConfigCheck {
    let command = connected_brain_provider_command(provider, project_root);
    let command_path = command.first().cloned();
    let (status, summary) = milestone_connected_brain_command_path_status(command_path.as_deref());
    MilestoneEvidencePlanConfigCheck {
        id: "connected_brain_provider_command".to_string(),
        status,
        path: command_path,
        selected_id: Some(provider.id.clone()),
        summary,
    }
}

fn milestone_connected_brain_command_path_status(command_path: Option<&str>) -> (String, String) {
    let Some(command_path) = command_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (
            "blocked".to_string(),
            "Connected brain provider command is missing.".to_string(),
        );
    };
    if milestone_manifest_placeholder(command_path) {
        return (
            "blocked".to_string(),
            "Connected brain provider command still contains a placeholder.".to_string(),
        );
    }
    let path = Path::new(command_path);
    if !path.is_absolute() {
        return (
            "blocked".to_string(),
            "Connected brain provider command must be an absolute executable path before evidence collection.".to_string(),
        );
    }
    let Ok(metadata) = fs::metadata(path) else {
        return (
            "blocked".to_string(),
            format!("Connected brain provider command is missing: {command_path}."),
        );
    };
    if !metadata.is_file() {
        return (
            "blocked".to_string(),
            format!("Connected brain provider command is not a file: {command_path}."),
        );
    }
    if !milestone_metadata_executable(&metadata) {
        return (
            "blocked".to_string(),
            format!("Connected brain provider command is not executable: {command_path}."),
        );
    }
    (
        "ready".to_string(),
        "Connected brain provider adapter command exists and is executable; evidence collection still requires explicit approval and will run through Foundry harness lineage.".to_string(),
    )
}

#[cfg(unix)]
fn milestone_metadata_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn milestone_metadata_executable(metadata: &fs::Metadata) -> bool {
    !metadata.permissions().readonly()
}

fn detect_replacement_cli_provider_candidates() -> Vec<MilestoneEvidenceProviderCandidate> {
    [
        ("codex", "codex", &["-V"][..]),
        ("opencode", "opencode", &["--version"][..]),
        ("gemini", "gemini", &["--version"][..]),
        ("claude", "claude", &["--version"][..]),
        ("ollama", "ollama", &["--version"][..]),
        ("antigravity", "agy", &["--version"][..]),
    ]
    .iter()
    .map(|(provider_id, binary, version_args)| {
        replacement_cli_provider_candidate(provider_id, binary, version_args)
    })
    .collect()
}

fn replacement_cli_provider_candidate(
    provider_id: &str,
    binary: &str,
    version_args: &[&str],
) -> MilestoneEvidenceProviderCandidate {
    let detected_path = resolve_binary_from_path(binary);
    let (version_status, version_output) = if detected_path.is_some() {
        ("version_probe_not_run".to_string(), String::new())
    } else {
        ("binary_not_found".to_string(), String::new())
    };
    let installed = detected_path.is_some();
    let readiness = if installed {
        "cli_detected_wrapper_required"
    } else {
        "cli_missing"
    };
    let command_hint = detected_path
        .clone()
        .unwrap_or_else(|| format!("<absolute-path-to-{binary}>"));
    let manifest_provider_template = serde_json::json!({
        "id": provider_id,
        "brain_id": provider_id,
        "model_id": "<approved-model-id>",
        "provider_class": "external_cli",
        "capabilities": ["replacement_grade_cli"],
        "command": [
            "<absolute-path-to-approved-provider-wrapper>",
            "--brain-cli",
            command_hint,
            "--emit",
            "foundry.connected_external_brain.provider_output.v1"
        ],
        "approved_by": "<operator>",
        "approval_ref": "<approval-or-change-record>",
        "allow_model_execution": true,
        "network_access": false,
        "device_access": false,
        "external_resources_mutated": false
    });
    MilestoneEvidenceProviderCandidate {
        schema_version: "foundry.milestone.evidence_provider_candidate.v1".to_string(),
        provider_id: provider_id.to_string(),
        brain_id: provider_id.to_string(),
        binary: binary.to_string(),
        detected_path,
        installed,
        version_command: std::iter::once(binary.to_string())
            .chain(version_args.iter().map(|arg| (*arg).to_string()))
            .collect(),
        version_status,
        version_output,
        readiness: readiness.to_string(),
        manifest_provider_template,
        evidence_blocker:
            "A detected CLI path or version command is not release evidence. Promotion requires an approved provider adapter that runs the model and emits foundry.connected_external_brain.provider_output.v1 with real_provider_execution_performed=true and model_execution_performed=true."
                .to_string(),
        next_action: if installed {
            format!(
                "Run the version command only through an approved/synced Foundry adapter, then create an approved provider adapter for `{provider_id}` in .foundry/connected-brain-runtimes.json before collecting external_brain_provider_execution."
            )
        } else {
            format!(
                "Install or configure `{binary}` before preparing an approved connected-brain provider adapter."
            )
        },
    }
}

fn resolve_binary_from_path(binary: &str) -> Option<String> {
    let path_var = crate::brand::env_var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
}

fn connected_brain_manifest_template(
    project_root: &Path,
    connected_brain: Option<&str>,
) -> MilestoneEvidencePlanManifestTemplate {
    let provider_id = connected_brain
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("project-provider");
    let target_path = project_root.join(CONNECTED_BRAIN_RUNTIMES_RELATIVE_PATH);
    let template_json = serde_json::json!({
        "providers": [{
            "id": provider_id,
            "brain_id": provider_id,
            "model_id": "<approved-model-id>",
            "provider_class": "external_cli",
            "capabilities": ["replacement_grade_cli"],
            "command": ["<absolute-path-to-approved-provider-command>"],
            "approved_by": "<operator>",
            "approval_ref": "<approval-or-change-record>",
            "allow_model_execution": true,
            "network_access": false,
            "device_access": false,
            "external_resources_mutated": false
        }]
    });
    MilestoneEvidencePlanManifestTemplate {
        schema_version: "foundry.milestone.manifest_template.v1".to_string(),
        id: "connected_brain_runtime_manifest".to_string(),
        status: "template_ready".to_string(),
        target_path: target_path.display().to_string(),
        secret_free: true,
        template_json,
        preparation_commands: vec![
            format!("mkdir -p {}", project_root.join(".foundry").display()),
            format!(
                "write {} with the provided template_json after replacing placeholders; do not store secrets in this file",
                target_path.display()
            ),
        ],
        validation_commands: vec![
            format!(
                "foundry milestone evidence-plan --version 0.6 --capability replacement_grade_cli --project-root {} --connected-brain {} --output json",
                project_root.display(),
                provider_id
            ),
            format!(
                "foundry milestone collect-evidence --version 0.6 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root {} --connected-brain {} --approved-by <operator> --origin codex --output json",
                project_root.display(),
                provider_id
            ),
        ],
        summary: "Secret-free connected brain runtime manifest template for operator-approved replacement-grade CLI evidence collection.".to_string(),
    }
}

fn push_replacement_grade_cli_demo_collection_commands(
    project_root: &Path,
    evidence_collection_commands: &mut Vec<String>,
) {
    evidence_collection_commands.push(format!(
        "foundry milestone collect-evidence --version 0.6 --capability replacement_grade_cli --kind broader_project_coding_research_workflow --project-root {} --approved-by <operator> --origin codex --output json",
        project_root.display()
    ));
    evidence_collection_commands.push(format!(
        "foundry milestone collect-evidence --version 0.6 --capability replacement_grade_cli --kind terminal_file_editing_ux --project-root {} --approved-by <operator> --origin codex --output json",
        project_root.display()
    ));
}

fn plan_experimental_multimodal_evidence(
    project_root: &Path,
    connected_runtime: Option<&str>,
    config_checks: &mut Vec<MilestoneEvidencePlanConfigCheck>,
    manifest_templates: &mut Vec<MilestoneEvidencePlanManifestTemplate>,
    configured_evidence_sources: &mut Vec<String>,
    evidence_collection_commands: &mut Vec<String>,
) -> Result<()> {
    let feature_path = crate::brand::project_config_path_for_read(project_root, "multimodal.json");
    let feature_enabled = if !feature_path.is_file() {
        manifest_templates.push(multimodal_feature_flag_template(project_root));
        config_checks.push(MilestoneEvidencePlanConfigCheck {
            id: "multimodal_feature_flag".to_string(),
            status: "missing".to_string(),
            path: Some(feature_path.display().to_string()),
            selected_id: None,
            summary: format!(
                "Create {MULTIMODAL_FEATURE_RELATIVE_PATH} with experimental_enabled=true and approval metadata."
            ),
        });
        false
    } else {
        let feature: serde_json::Value = serde_json::from_slice(&fs::read(&feature_path)?)
            .with_context(|| {
                format!("invalid multimodal feature flag {}", feature_path.display())
            })?;
        let enabled = feature
            .get("experimental_enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let feature_approval_ready = feature
            .get("approved_by")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !milestone_manifest_placeholder(value))
            && feature
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| !milestone_manifest_placeholder(value));
        let feature_ready = enabled && feature_approval_ready;
        config_checks.push(MilestoneEvidencePlanConfigCheck {
            id: "multimodal_feature_flag".to_string(),
            status: if feature_ready { "ready" } else { "blocked" }.to_string(),
            path: Some(feature_path.display().to_string()),
            selected_id: None,
            summary: if feature_ready {
                "Multimodal experimental feature flag is enabled with operator approval for this project.".to_string()
            } else if enabled {
                manifest_templates.push(multimodal_feature_flag_template(project_root));
                "Multimodal feature flag is enabled but still needs non-placeholder approved_by and reason fields.".to_string()
            } else {
                manifest_templates.push(multimodal_feature_flag_template(project_root));
                "Multimodal feature flag exists but experimental_enabled is not true.".to_string()
            },
        });
        feature_ready
    };

    let runtime_id = selected_multimodal_runtime_id(connected_runtime);
    let manifest_path =
        crate::brand::project_config_path_for_read(project_root, "multimodal-runtimes.json");
    if !manifest_path.is_file() {
        manifest_templates.push(multimodal_runtime_manifest_template(
            project_root,
            connected_runtime,
        ));
        config_checks.push(MilestoneEvidencePlanConfigCheck {
            id: "multimodal_runtime_manifest".to_string(),
            status: "missing".to_string(),
            path: Some(manifest_path.display().to_string()),
            selected_id: Some(runtime_id.clone()),
            summary: format!(
                "Create {MULTIMODAL_RUNTIMES_RELATIVE_PATH} with a production connected runtime."
            ),
        });
        evidence_collection_commands.push(format!(
            "foundry milestone collect-evidence --version 0.6 --capability experimental_multimodal_runtime --project-root {} --connected-runtime {} --approved-by <operator> --output json",
            project_root.display(),
            runtime_id
        ));
        evidence_collection_commands.push(format!(
            "foundry multimodal runtime-benchmark --capability image_understanding --fixture static_image_labels --project-root {} --connected-runtime {} --approved-by <operator> --confirm-runtime-execution --allow-model --output json",
            project_root.display(),
            runtime_id
        ));
        return Ok(());
    }

    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)
        .with_context(|| {
            format!(
                "invalid multimodal runtime manifest {}",
                manifest_path.display()
            )
        })?;
    config_checks.push(MilestoneEvidencePlanConfigCheck {
        id: "multimodal_runtime_manifest".to_string(),
        status: "ready".to_string(),
        path: Some(manifest_path.display().to_string()),
        selected_id: None,
        summary: "Multimodal runtime manifest is present and parseable.".to_string(),
    });
    let runtimes = manifest
        .get("runtimes")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected = runtimes.into_iter().find(|runtime| {
        let id = runtime
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        connected_runtime
            .map(|connected_runtime| id == connected_runtime)
            .unwrap_or_else(|| runtime.get("production").is_some())
    });
    let Some(runtime) = selected else {
        manifest_templates.push(multimodal_runtime_manifest_template(
            project_root,
            connected_runtime,
        ));
        config_checks.push(MilestoneEvidencePlanConfigCheck {
            id: "multimodal_connected_runtime".to_string(),
            status: "missing".to_string(),
            path: Some(manifest_path.display().to_string()),
            selected_id: Some(runtime_id),
            summary: "No selected runtime with production evidence metadata was found.".to_string(),
        });
        return Ok(());
    };
    let runtime_id = runtime
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("<runtime-id>")
        .to_string();
    let runtime_id_ready = !milestone_manifest_placeholder(&runtime_id);
    let model_id_ready = runtime
        .get("model_id")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !milestone_manifest_placeholder(value));
    let capabilities = runtime
        .get("capabilities")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let capabilities_ready = capabilities
        .iter()
        .filter_map(serde_json::Value::as_str)
        .any(|capability| !milestone_manifest_placeholder(capability));
    let capability = capabilities
        .iter()
        .filter_map(serde_json::Value::as_str)
        .next()
        .unwrap_or("image_understanding")
        .to_string();
    let probe_command_ready = runtime
        .get("probe_command")
        .and_then(serde_json::Value::as_array)
        .map(|command| {
            !command.is_empty()
                && command
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .all(|part| !milestone_manifest_placeholder(part))
                && command.iter().all(serde_json::Value::is_string)
        })
        .unwrap_or(false);
    let no_network = !runtime
        .get("network_access")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let no_device = !runtime
        .get("device_access")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let production = runtime
        .get("production")
        .unwrap_or(&serde_json::Value::Null);
    let production_ready = production
        .get("approved_by")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !milestone_manifest_placeholder(value))
        && production
            .get("approval_ref")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !milestone_manifest_placeholder(value))
        && production
            .get("runtime_version")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !milestone_manifest_placeholder(value))
        && production
            .get("model_manifest_sha256")
            .and_then(serde_json::Value::as_str)
            .map(|value| {
                value.len() == 64
                    && value.chars().all(|character| character.is_ascii_hexdigit())
                    && !milestone_manifest_placeholder(value)
            })
            .unwrap_or(false)
        && production
            .get("model_license")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !milestone_manifest_placeholder(value))
        && production
            .get("evidence_artifacts")
            .and_then(serde_json::Value::as_array)
            .map(|artifacts| {
                !artifacts.is_empty()
                    && artifacts
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .all(|artifact| !milestone_manifest_placeholder(artifact))
                    && artifacts.iter().all(serde_json::Value::is_string)
            })
            .unwrap_or(false);
    let runtime_ready = feature_enabled
        && runtime_id_ready
        && model_id_ready
        && probe_command_ready
        && no_network
        && no_device
        && production_ready
        && capabilities_ready;
    config_checks.push(MilestoneEvidencePlanConfigCheck {
        id: "multimodal_connected_runtime".to_string(),
        status: if runtime_ready { "ready" } else { "blocked" }.to_string(),
        path: Some(manifest_path.display().to_string()),
        selected_id: Some(runtime_id.clone()),
        summary: if runtime_ready {
            "Connected multimodal runtime has production metadata, safe probe flags and a declared capability."
                .to_string()
        } else {
            "Connected multimodal runtime needs a probe command, capability, production metadata and no network/device access declarations."
                .to_string()
        },
    });
    if !runtime_ready {
        manifest_templates.push(multimodal_runtime_manifest_template(
            project_root,
            Some(&runtime_id),
        ));
    }
    configured_evidence_sources.push(format!("connected_multimodal_runtime:{runtime_id}"));
    evidence_collection_commands.push(format!(
        "foundry milestone collect-evidence --version 0.6 --capability experimental_multimodal_runtime --project-root {} --connected-runtime {} --approved-by <operator> --output json",
        project_root.display(),
        runtime_id
    ));
    evidence_collection_commands.push(format!(
        "foundry multimodal runtime-benchmark --capability {} --fixture static_image_labels --project-root {} --connected-runtime {} --approved-by <operator> --confirm-runtime-execution --allow-model --output json",
        capability,
        project_root.display(),
        runtime_id
    ));
    Ok(())
}

fn selected_multimodal_runtime_id(connected_runtime: Option<&str>) -> String {
    connected_runtime
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("production-vision-runtime")
        .to_string()
}

fn multimodal_feature_flag_template(project_root: &Path) -> MilestoneEvidencePlanManifestTemplate {
    let target_path = project_root.join(MULTIMODAL_FEATURE_RELATIVE_PATH);
    let template_json = serde_json::json!({
        "experimental_enabled": true,
        "approved_by": "<operator>",
        "reason": "Operator-approved production multimodal runtime evidence collection.",
        "scope": "project"
    });
    MilestoneEvidencePlanManifestTemplate {
        schema_version: "foundry.milestone.manifest_template.v1".to_string(),
        id: "multimodal_feature_flag".to_string(),
        status: "template_ready".to_string(),
        target_path: target_path.display().to_string(),
        secret_free: true,
        template_json,
        preparation_commands: vec![
            format!("mkdir -p {}", project_root.join(".foundry").display()),
            format!(
                "write {} with the provided template_json after operator approval; do not store secrets in this file",
                target_path.display()
            ),
        ],
        validation_commands: vec![format!(
            "foundry milestone evidence-plan --version 0.6 --capability experimental_multimodal_runtime --project-root {} --output json",
            project_root.display()
        )],
        summary: "Secret-free multimodal experimental feature flag template for operator-approved runtime evidence planning.".to_string(),
    }
}

fn multimodal_runtime_manifest_template(
    project_root: &Path,
    connected_runtime: Option<&str>,
) -> MilestoneEvidencePlanManifestTemplate {
    let runtime_id = selected_multimodal_runtime_id(connected_runtime);
    let target_path = project_root.join(MULTIMODAL_RUNTIMES_RELATIVE_PATH);
    let template_json = serde_json::json!({
        "runtimes": [{
            "id": runtime_id,
            "model_id": "<approved-model-id>",
            "capabilities": ["image_understanding"],
            "probe_command": ["<absolute-path-to-approved-runtime-probe-command>"],
            "network_access": false,
            "device_access": false,
            "production": {
                "approved_by": "<operator>",
                "approval_ref": "<approval-or-change-record>",
                "runtime_version": "<runtime-version>",
                "model_manifest_sha256": "<64-char-model-manifest-sha256>",
                "model_license": "<approved-model-license>",
                "evidence_artifacts": ["<operator-reviewed-benchmark-artifact>"],
                "min_quality_score": 0.95,
                "max_latency_ms": 1000
            }
        }]
    });
    MilestoneEvidencePlanManifestTemplate {
        schema_version: "foundry.milestone.manifest_template.v1".to_string(),
        id: "multimodal_runtime_manifest".to_string(),
        status: "template_ready".to_string(),
        target_path: target_path.display().to_string(),
        secret_free: true,
        template_json,
        preparation_commands: vec![
            format!("mkdir -p {}", project_root.join(".foundry").display()),
            format!(
                "write {} with the provided template_json after replacing placeholders; do not store secrets in this file",
                target_path.display()
            ),
        ],
        validation_commands: vec![
            format!(
                "foundry milestone evidence-plan --version 0.6 --capability experimental_multimodal_runtime --project-root {} --connected-runtime {} --output json",
                project_root.display(),
                runtime_id
            ),
            format!(
                "foundry milestone collect-evidence --version 0.6 --capability experimental_multimodal_runtime --project-root {} --connected-runtime {} --approved-by <operator> --output json",
                project_root.display(),
                runtime_id
            ),
            format!(
                "foundry multimodal runtime-benchmark --capability image_understanding --fixture static_image_labels --project-root {} --connected-runtime {} --approved-by <operator> --confirm-runtime-execution --allow-model --output json",
                project_root.display(),
                runtime_id
            ),
        ],
        summary: "Secret-free multimodal runtime manifest template for operator-approved production runtime evidence collection.".to_string(),
    }
}

fn milestone_attach_command(version: &str, capability_id: &str, kind: &str) -> String {
    format!(
        "foundry milestone attach-evidence --version {version} --capability {capability_id} --kind {kind} --summary \"Operator-approved {kind} receipt.\" --artifact <path> --approved-by <operator> --output json"
    )
}

fn normalize_required(value: &str, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{field} is required");
    }
    Ok(value.to_string())
}

fn sanitize_milestone_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "milestone".to_string()
    } else {
        sanitized
    }
}

const EXPORT_DEMO_SCHEMA_VERSION: &str = "foundry.milestone.export_demo.v1";

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneExportDemoReport {
    pub status: String,
    pub schema_version: String,
    pub workflow_id: String,
    pub goal: String,
    pub screen_artifact_id: String,
    pub document_artifact_id: String,
    pub token_collection_name: String,
    pub creative_artifact_kinds: Vec<String>,
    pub demo_artifacts: Vec<MilestoneDemoArtifact>,
    pub lineage_chain: Vec<String>,
    pub export_evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneDemoArtifact {
    pub kind: String,
    pub goal: String,
    pub status: String,
}

const CLI_DEMO_SCHEMA_VERSION: &str = "foundry.milestone.cli_demo.v1";
const PATCH_LIFECYCLE_DEMO_SCHEMA_VERSION: &str = "foundry.milestone.patch_lifecycle_demo.v1";
const EXECUTOR_PROJECT_DEMO_SCHEMA_VERSION: &str = "foundry.milestone.executor_project_demo.v1";
const BRAIN_HANDOFF_DEMO_SCHEMA_VERSION: &str = "foundry.milestone.brain_handoff_demo.v1";
const REAL_PROJECT_WORKFLOW_DEMO_SCHEMA_VERSION: &str =
    "foundry.milestone.real_project_workflow_demo.v1";
const CONNECTED_EXTERNAL_BRAIN_DEMO_SCHEMA_VERSION: &str =
    "foundry.milestone.connected_external_brain_demo.v1";
const CONNECTED_EXTERNAL_BRAIN_PROVIDER_SCHEMA_VERSION: &str =
    "foundry.milestone.connected_external_brain_provider.v1";
const HEADROOM_RUNTIME_WRAPPER_DEMO_SCHEMA_VERSION: &str =
    "foundry.milestone.headroom_runtime_wrapper_demo.v1";
const CONNECTED_BRAIN_RUNTIMES_RELATIVE_PATH: &str = ".foundry/connected-brain-runtimes.json";
const MULTIMODAL_FEATURE_RELATIVE_PATH: &str = ".foundry/multimodal.json";
const MULTIMODAL_RUNTIMES_RELATIVE_PATH: &str = ".foundry/multimodal-runtimes.json";

#[derive(Debug, Clone, Default)]
pub struct MilestoneCliDemoOptions<'a> {
    pub project_root: Option<&'a Path>,
    pub connected_brain: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCliDemoReport {
    pub status: String,
    pub schema_version: String,
    pub milestone: String,
    pub capability_id: String,
    pub workflow_id: String,
    pub promotion_ready: bool,
    pub external_resources_mutated: bool,
    pub headroom_stats: HeadroomStatsReport,
    pub headroom_runtime_wrapper: MilestoneHeadroomRuntimeWrapperDemo,
    pub flows: Vec<ReplacementCliDemoFlow>,
    pub remaining_gaps: Vec<String>,
    pub lean_governance: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneHeadroomRuntimeWrapperDemo {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub non_executing: bool,
    pub child_cli_launched: bool,
    pub external_resources_mutated: bool,
    pub wrapper_plan: CliWrapperPlanReport,
    pub validation_evidence: Vec<String>,
    pub commands: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplacementCliDemoFlow {
    pub kind: String,
    pub title: String,
    pub workflow_id: String,
    pub run_id: Option<String>,
    pub run_status: String,
    pub completed_through_foundry: bool,
    pub commands: Vec<String>,
    pub artifact_refs: Vec<String>,
    pub validation_evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_lifecycle: Option<MilestonePatchLifecycleDemo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_project: Option<MilestoneExecutorProjectDemo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brain_handoff: Option<MilestoneBrainHandoffDemo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_project: Option<MilestoneRealProjectWorkflowDemo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_brain: Option<MilestoneConnectedExternalBrainDemo>,
    pub activity: Option<RunActivity>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePatchLifecycleDemo {
    pub schema_version: String,
    pub status: String,
    pub target_path: String,
    pub repository_path: String,
    pub external_resources_mutated: bool,
    pub restored_to_clean_state: bool,
    pub plan_status: String,
    pub review_status: String,
    pub diff_status: String,
    pub apply_status: String,
    pub revert_status: String,
    pub restore_status: String,
    pub artifact_refs: Vec<MilestonePatchLifecycleArtifact>,
    pub gates: Vec<String>,
    pub commands: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePatchLifecycleArtifact {
    pub kind: String,
    pub schema_version: String,
    pub status: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneExecutorProjectDemo {
    pub schema_version: String,
    pub status: String,
    pub repository_path: String,
    pub target_path: String,
    pub target_sha256: String,
    pub bootstrap_status: String,
    pub bootstrap_config_status: String,
    pub shim_install_status: String,
    pub exec_status: String,
    pub exec_event_recorded: bool,
    pub exec_global_event_id: Option<i64>,
    pub project_policy_status: String,
    pub stdout_headroom_status: String,
    pub stdout_retrieval_ref: Option<String>,
    pub external_resources_mutated: bool,
    pub lineage: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneBrainHandoffDemo {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub run_id: String,
    pub selected_brain: String,
    pub orchestrator_brain: String,
    pub handoff_status: String,
    pub handoff_ready: bool,
    pub context_schema_version: String,
    pub context_bytes: usize,
    pub shell_plan_status: String,
    pub shell_plan_recorded: bool,
    pub shell_plan_event_id: i64,
    pub lifecycle_status: String,
    pub lifecycle_state: String,
    pub lifecycle_event_recorded: bool,
    pub lifecycle_event_id: i64,
    pub model_execution_performed: bool,
    pub external_resources_mutated: bool,
    pub node_brain_routing: NodeBrainRoutingSpec,
    pub commands: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneRealProjectWorkflowDemo {
    pub schema_version: String,
    pub status: String,
    pub repository_path: String,
    pub target_paths: Vec<String>,
    pub target_sha256: BTreeMap<String, String>,
    pub handoff_status: String,
    pub handoff_ready: bool,
    pub selected_brain: String,
    pub routing_default_brain: String,
    pub exec_status: String,
    pub exec_event_recorded: bool,
    pub exec_global_event_id: Option<i64>,
    pub validation_command: String,
    pub validation_status: String,
    pub stdout_headroom_status: String,
    pub stdout_retrieval_ref: Option<String>,
    pub external_resources_mutated: bool,
    pub lineage: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneConnectedExternalBrainDemo {
    pub schema_version: String,
    pub status: String,
    pub repository_path: String,
    pub brain_id: String,
    pub selected_brain: String,
    pub routing_default_brain: String,
    pub model_execution_performed: bool,
    pub external_brain_process_executed: bool,
    pub provider_contract: MilestoneConnectedExternalBrainProviderContract,
    pub handoff_status: String,
    pub handoff_ready: bool,
    pub harness_exec_status: String,
    pub exec_event_recorded: bool,
    pub exec_global_event_id: Option<i64>,
    pub validation_status: String,
    pub target_paths: Vec<String>,
    pub target_sha256: BTreeMap<String, String>,
    pub project_policy_status: String,
    pub stdout_headroom_status: String,
    pub stdout_retrieval_ref: Option<String>,
    pub external_resources_mutated: bool,
    pub lineage: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneConnectedExternalBrainProviderContract {
    pub schema_version: String,
    pub status: String,
    pub provider_id: String,
    pub provider_source: String,
    pub manifest_path: Option<String>,
    pub manifest_status: String,
    pub model_id: String,
    pub provider_class: String,
    pub approved_by: Option<String>,
    pub approval_ref: Option<String>,
    pub execution_mode: String,
    pub command_sha256: String,
    pub stdout_sha256: Option<String>,
    pub stderr_sha256: Option<String>,
    pub output_schema_valid: bool,
    pub output_quality_score: String,
    pub output_latency_ms: String,
    pub provider_declared_model_execution: bool,
    pub real_provider_execution_performed: bool,
    pub promotion_ready: bool,
    pub validation_evidence: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ConnectedBrainRuntimeManifest {
    #[serde(default)]
    providers: Vec<ConnectedBrainProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ConnectedBrainProviderConfig {
    id: String,
    #[serde(default)]
    brain_id: Option<String>,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    provider_class: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    command: Vec<String>,
    #[serde(default)]
    approved_by: Option<String>,
    #[serde(default)]
    approval_ref: Option<String>,
    #[serde(default)]
    allow_model_execution: bool,
    #[serde(default)]
    network_access: bool,
    #[serde(default)]
    device_access: bool,
    #[serde(default)]
    external_resources_mutated: bool,
}

struct ConnectedBrainProviderSelection {
    manifest_path: PathBuf,
    manifest_root: PathBuf,
    provider: ConnectedBrainProviderConfig,
}

pub fn build_milestone_export_demo(
    store: &FoundryStore,
    origin: &str,
) -> Result<MilestoneExportDemoReport> {
    let goal = "hackathon".to_string();
    let report = create_daily_goal_research_workflow(
        store,
        vec![goal.clone()],
        "America/Sao_Paulo",
        "0 8 * * *",
        origin,
    )?;
    let workflow_id = report.workflow_id.clone();

    let screen = CreativeArtifact::new_screen(
        "Demo Screen",
        ScreenSpec {
            schema_version: ir_schema_version(),
            width_px: 1440,
            height_px: 900,
            background: "#ffffff".to_string(),
            breakpoints: Vec::new(),
            elements: Vec::new(),
            interactions: Vec::new(),
        },
    );
    let screen_artifact_id = screen.id.clone();
    attach_creative_artifact(store, &workflow_id, screen, origin)?;

    let document = CreativeArtifact::new_document(
        "Demo Document",
        DocumentSpec {
            schema_version: ir_schema_version(),
            title: "Demo Document".to_string(),
            author: origin.to_string(),
            front_matter: BTreeMap::new(),
            sections: vec![DocumentSection {
                id: "sec_intro".to_string(),
                heading: "Introduction".to_string(),
                level: 1,
                content: Vec::new(),
                children: Vec::new(),
            }],
        },
    );
    let document_artifact_id = document.id.clone();
    attach_creative_artifact(store, &workflow_id, document, origin)?;

    let token_collection = TokenCollection {
        name: "export_demo_tokens".to_string(),
        schema_version: ir_schema_version(),
        description: "Export demo design tokens".to_string(),
        tokens: vec![
            DesignToken {
                name: "color.primary".to_string(),
                value: "#3B82F6".to_string(),
                token_type: TokenType::Color,
                description: "Primary brand color".to_string(),
                group: "color".to_string(),
                extensions: BTreeMap::new(),
            },
            DesignToken {
                name: "spacing.md".to_string(),
                value: "16px".to_string(),
                token_type: TokenType::Spacing,
                description: "Medium spacing".to_string(),
                group: "spacing".to_string(),
                extensions: BTreeMap::new(),
            },
        ],
        semantic_aliases: vec![SemanticAlias {
            name: "semantic.export_demo".to_string(),
            resolves_to: "color.primary".to_string(),
            description: "Export demo semantic alias".to_string(),
        }],
        modes: Vec::new(),
    };
    set_workflow_token_collection(store, &workflow_id, token_collection, origin)?;

    let schedule_status = format!(
        "scheduled_nodes={}, cron_nodes={}",
        report.schedule_summary.scheduled_nodes, report.schedule_summary.cron_nodes,
    );

    Ok(MilestoneExportDemoReport {
        status: "export_demo_generated".to_string(),
        schema_version: EXPORT_DEMO_SCHEMA_VERSION.to_string(),
        workflow_id: workflow_id.clone(),
        goal: goal.clone(),
        screen_artifact_id: screen_artifact_id.clone(),
        document_artifact_id: document_artifact_id.clone(),
        token_collection_name: "export_demo_tokens".to_string(),
        creative_artifact_kinds: vec![
            "ScreenSpec".to_string(),
            "DocumentSpec".to_string(),
        ],
        demo_artifacts: vec![
            MilestoneDemoArtifact {
                kind: "scheduled_workflow".to_string(),
                goal: goal.clone(),
                status: schedule_status,
            },
            MilestoneDemoArtifact {
                kind: "creative_screen".to_string(),
                goal: goal.clone(),
                status: "attached".to_string(),
            },
            MilestoneDemoArtifact {
                kind: "creative_document".to_string(),
                goal: goal.clone(),
                status: "attached".to_string(),
            },
            MilestoneDemoArtifact {
                kind: "design_tokens".to_string(),
                goal: goal.clone(),
                status: "set".to_string(),
            },
        ],
        lineage_chain: vec![
            format!("workflow_id:{workflow_id}"),
            format!("screen_artifact_id:{screen_artifact_id}"),
            format!("document_artifact_id:{document_artifact_id}"),
        ],
        export_evidence: "foundry.milestone.export_demo.v1 creates a scheduled daily research workflow with creative screen and document artifacts, design token collection, and full lineage chain preservation. The workflow can be inspected via `foundry inspect` or `foundry schedule list`, creative artifacts via `foundry workflow list-creative`, and tokens via `foundry workflow get-tokens`. Markdown and PDF artifacts are generated through `foundry schedule run-due` per goal.".to_string(),
    })
}

pub fn build_replacement_cli_demo(
    store: &FoundryStore,
    origin: &str,
) -> Result<MilestoneCliDemoReport> {
    build_replacement_cli_demo_with_options(store, origin, MilestoneCliDemoOptions::default())
}

pub fn build_replacement_cli_demo_with_options(
    store: &FoundryStore,
    origin: &str,
    options: MilestoneCliDemoOptions<'_>,
) -> Result<MilestoneCliDemoReport> {
    let mut coding_workflow = create_workflow(parse_intent(
        "Demonstrate Foundry-first coding task with bounded context, file patch, diff review and validation",
    ));
    store.save_workflow(&coding_workflow)?;

    let patch_review_path = store.base_dir().join("tmp").join(format!(
        "{}-replacement-cli-diff-review.md",
        coding_workflow.id
    ));
    if let Some(parent) = patch_review_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &patch_review_path,
        format!(
            "# Replacement-grade CLI coding demo\n\nworkflow_id: `{}`\norigin: `{}`\n\nThis deterministic artifact records the Foundry-owned coding flow: context, handoff, patch intent, diff review, validation, artifact attachment and inspectability. It is demo evidence only; it does not edit arbitrary user files.\n",
            coding_workflow.id, origin
        ),
    )?;
    let attached = attach_workflow_artifact(
        store,
        &coding_workflow.id,
        &patch_review_path,
        "cli_demo",
        origin,
    )?;
    coding_workflow = store.load_workflow(&coding_workflow.id)?;
    let patch_lifecycle =
        build_replacement_cli_patch_lifecycle_demo(store, &coding_workflow.id, origin)?;

    let research = create_daily_goal_research_workflow(
        store,
        vec!["hackathon".to_string()],
        "America/Sao_Paulo",
        "0 8 * * *",
        origin,
    )?;
    let mut research_workflow = store.load_workflow(&research.workflow_id)?;
    let smoke = run_daily_goal_research_smoke(store, &mut research_workflow)?
        .expect("daily goal research workflow should contain configured goals");
    store.save_workflow(&research_workflow)?;
    let research_refs = smoke
        .goals
        .iter()
        .flat_map(|goal| {
            vec![
                goal.markdown_path.clone(),
                goal.pdf_path.clone(),
                format!(
                    "artifacts/{}/telegram-delivery-{}.json",
                    smoke.workflow_id, goal.goal
                ),
            ]
        })
        .collect::<Vec<_>>();

    let coding_task_id = coding_workflow
        .tasks
        .first()
        .map(|task| task.id.clone())
        .unwrap_or_else(|| "task-001".to_string());
    let mut harness_options = InteractiveHarnessOptions::default_for_current_dir();
    harness_options.executor = "codex".to_string();
    harness_options.project_root = Some(env::current_dir()?);
    harness_options.workflow_id = Some(coding_workflow.id.clone());
    harness_options.task_id = Some(coding_task_id.clone());
    harness_options.context_budget = Some(1200);
    harness_options.token_headroom = Some(true);
    let harness_panel = build_interactive_harness(store, harness_options)?;
    let headroom_runtime_wrapper =
        build_milestone_headroom_runtime_wrapper_demo(&coding_workflow.id, &coding_task_id);
    let executor_project_flow = build_replacement_cli_executor_project_demo(store, origin)?;
    let brain_handoff_flow = build_replacement_cli_brain_handoff_demo(store, origin)?;
    let real_project_flow = build_replacement_cli_real_project_workflow_demo(store, origin)?;
    let connected_external_brain_flow =
        build_replacement_cli_connected_external_brain_demo(store, origin, &options)?;

    let async_request = start_async_request(
        store,
        "Demonstrate long-running Foundry-first async workflow with heartbeat and resume/status visibility",
        origin,
    )?;
    let heartbeat = heartbeat_request(
        store,
        &async_request.run_id,
        "foundry_cli_demo",
        "replacement-grade CLI demo run is observable through Foundry heartbeat",
        600,
        None,
        origin,
    )?;
    let headroom_stats = build_headroom_stats_report(
        store,
        HeadroomStatsOptions {
            source: None,
            content_kind: None,
            limit: 5,
        },
    )?;

    Ok(MilestoneCliDemoReport {
        status: "replacement_cli_demo_generated".to_string(),
        schema_version: CLI_DEMO_SCHEMA_VERSION.to_string(),
        milestone: SUPPORTED_MILESTONE.to_string(),
        capability_id: "replacement_grade_cli".to_string(),
        workflow_id: coding_workflow.id.clone(),
        promotion_ready: false,
        external_resources_mutated: false,
        headroom_stats,
        headroom_runtime_wrapper,
        flows: vec![
            ReplacementCliDemoFlow {
                kind: "coding_task".to_string(),
                title: "Foundry-first coding task with bounded patch review".to_string(),
                workflow_id: coding_workflow.id.clone(),
                run_id: None,
                run_status: coding_workflow.status.clone(),
                completed_through_foundry: true,
                commands: vec![
                    "foundry plan --goal \"Demonstrate coding task\" --output json".to_string(),
                    "foundry context --workflow <workflow-id> --task <task-id> --budget 1200 --strict --view compact --output json".to_string(),
                    "foundry task handoff --workflow <workflow-id> --task <task-id> --executor codex --view compact --output json".to_string(),
                    "foundry workflow attach-artifact --workflow <workflow-id> --path <diff-review.md> --kind cli_demo --origin foundry_cli --output json".to_string(),
                    "foundry validate --workflow <workflow-id> --output json".to_string(),
                    "foundry inspect <workflow-id> --verbose --output json".to_string(),
                ],
                artifact_refs: vec![attached.artifact.path],
                validation_evidence: vec![
                    "bounded_context_required".to_string(),
                    "diff_review_required".to_string(),
                    "patch_lifecycle_artifacts_recorded".to_string(),
                    "patch_edit_intake_required".to_string(),
                    "approved_restore_returns_fixture_to_clean_state".to_string(),
                    "validation_before_promotion".to_string(),
                    "artifact_lineage_attached".to_string(),
                    "json_stable_commands".to_string(),
                ],
                patch_lifecycle: Some(patch_lifecycle),
                executor_project: None,
                brain_handoff: None,
                real_project: None,
                external_brain: None,
                activity: None,
                summary: "The coding demo proves the Foundry CLI has a native flow shape for context routing, executor handoff, edit intake, patch plan/review/diff/apply/revert/restore artifact lineage, validation and inspection. It remains groundwork because richer interactive terminal editing still needs broader UX evidence.".to_string(),
            },
            ReplacementCliDemoFlow {
                kind: "harness_control".to_string(),
                title: "Foundry-first harness, headroom and session lifecycle control".to_string(),
                workflow_id: coding_workflow.id.clone(),
                run_id: None,
                run_status: harness_panel.status.clone(),
                completed_through_foundry: true,
                commands: vec![
                    "foundry interactive harness --workflow <workflow-id> --task <task-id> --token-headroom --output json".to_string(),
                    "foundry harness headroom-plan --executor codex --project-root <project-root> --context-budget 1200 --token-headroom --output json".to_string(),
                    "foundry sessions --provider codex --output json".to_string(),
                    "foundry interactive readiness --output json".to_string(),
                ],
                artifact_refs: Vec::new(),
                validation_evidence: vec![
                    "interactive_harness_ready".to_string(),
                    "headroom_plan_ready".to_string(),
                    "session_lifecycle_plan_ready".to_string(),
                    "token_headroom_enabled".to_string(),
                    "json_stable_headroom_commands".to_string(),
                    "no_child_cli_launched".to_string(),
                ],
                patch_lifecycle: None,
                executor_project: None,
                brain_handoff: None,
                real_project: None,
                external_brain: None,
                activity: None,
                summary: format!(
                    "The harness demo proves the replacement CLI can expose {} with {}, {} and a ready headroom-plan command without launching a child CLI.",
                    harness_panel.status,
                    harness_panel.headroom_plan.schema_version,
                    harness_panel.session_lifecycle_plan.schema_version
                ),
            },
            executor_project_flow,
            brain_handoff_flow,
            real_project_flow,
            connected_external_brain_flow,
            ReplacementCliDemoFlow {
                kind: "research_artifact".to_string(),
                title: "Foundry-first research/artifact delivery".to_string(),
                workflow_id: research.workflow_id.clone(),
                run_id: None,
                run_status: smoke.status.clone(),
                completed_through_foundry: true,
                commands: vec![
                    "foundry schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron \"0 8 * * *\" --origin foundry_cli --output json".to_string(),
                    "foundry run --workflow <workflow-id> --simulate --output json".to_string(),
                    "foundry artifacts --workflow <workflow-id> --output json".to_string(),
                    "foundry inspect <workflow-id> --verbose --output json".to_string(),
                ],
                artifact_refs: research_refs,
                validation_evidence: vec![
                    "markdown_report_generated".to_string(),
                    "pdf_report_generated".to_string(),
                    "telegram_delivery_recorded_without_secrets".to_string(),
                    "schedule_loop_lineage_preserved".to_string(),
                ],
                patch_lifecycle: None,
                executor_project: None,
                brain_handoff: None,
                real_project: None,
                external_brain: None,
                activity: None,
                summary: "The research demo uses the canonical daily Goal workflow to produce Markdown, PDF and Telegram delivery records through Foundry-owned workflow semantics without live external delivery or secrets.".to_string(),
            },
            ReplacementCliDemoFlow {
                kind: "long_running_async".to_string(),
                title: "Foundry-first async run handoff with heartbeat".to_string(),
                workflow_id: async_request.workflow_id.clone(),
                run_id: Some(async_request.run_id.clone()),
                run_status: heartbeat.status.clone(),
                completed_through_foundry: true,
                commands: vec![
                    "foundry request start --goal \"Long-running task\" --origin foundry_cli --output json".to_string(),
                    "foundry request heartbeat --run <run-id> --executor foundry_cli_demo --summary \"executor alive\" --ttl-seconds 600 --origin foundry_cli --output json".to_string(),
                    "foundry request status --run <run-id> --output json".to_string(),
                    "foundry request list --status running --output json".to_string(),
                    "foundry inspect <workflow-id> --output json".to_string(),
                ],
                artifact_refs: Vec::new(),
                validation_evidence: vec![
                    "run_id_returned_immediately".to_string(),
                    "fresh_heartbeat_recorded".to_string(),
                    "workflow_lifecycle_marked_running".to_string(),
                    "resume_status_commands_available".to_string(),
                ],
                patch_lifecycle: None,
                executor_project: None,
                brain_handoff: None,
                real_project: None,
                external_brain: None,
                activity: Some(heartbeat.activity),
                summary: "The async demo proves Foundry can start a durable run, mark it active through heartbeat, expose status/list/inspect visibility and keep orchestration authority during long-running executor work.".to_string(),
            },
        ],
        remaining_gaps: vec![
            "Real external model/provider execution on broader project coding/research workflows and TUI apply/approval ergonomics remain required before replacement-grade promotion.".to_string(),
            "Deeper provider/session lifecycle controls and richer terminal UX remain required.".to_string(),
            "This demo is deterministic evidence and does not claim Foundry 0.6 promotion readiness.".to_string(),
        ],
        lean_governance: vec![
            "The demo reuses existing request, schedule, artifact and validation primitives instead of adding a separate agent shell architecture.".to_string(),
            "No Docker, Kubernetes, Knative, model install, device access, Telegram send or external resource mutation is performed.".to_string(),
        ],
    })
}

fn build_milestone_headroom_runtime_wrapper_demo(
    workflow_id: &str,
    task_id: &str,
) -> MilestoneHeadroomRuntimeWrapperDemo {
    let command = vec!["codex".to_string()];
    let wrapper_plan = build_cli_wrapper_plan(CliWrapperPlanOptions {
        executor: "codex",
        command: &command,
        foundry_first: true,
        foundry_first_source: "milestone_cli_demo_headroom_runtime_wrapper",
        project_root: None,
        workflow_id: Some(workflow_id),
        task_id: Some(task_id),
        run_id: None,
        context_budget: 1200,
        context_budget_source: "milestone_cli_demo_headroom_runtime_wrapper",
        token_headroom: true,
        token_headroom_source: "milestone_cli_demo_headroom_runtime_wrapper",
        require_token_headroom_for_foundry_first: true,
    });
    let runtime = &wrapper_plan.headroom_runtime_plan;
    let has_tool_output_interception = runtime.interception_points.iter().any(|point| {
        point.point_id == "tool_output"
            && point.action == "compress_then_return_retrieval_ref"
            && point.required
    });
    let has_log_route = runtime.content_routes.iter().any(|route| {
        route.content_kind == "log" && route.strategy == "signal_log_compressor" && route.reversible
    });
    let has_reversible_store = runtime.reversible_store.uri_scheme == "foundry://harness/headroom/";
    let has_retrieval_tool = runtime
        .mcp_tools
        .iter()
        .any(|tool| tool == "foundry.harness.retrieve_headroom");
    let has_runtime_env = wrapper_plan.env.iter().any(|env| {
        env.name == "FOUNDRY_HEADROOM_RUNTIME_PLAN"
            && env.value == CLI_HARNESS_HEADROOM_RUNTIME_PLAN_SCHEMA_VERSION
    });
    let ready = wrapper_plan.schema_version == CLI_WRAPPER_PLAN_SCHEMA_VERSION
        && runtime.schema_version == CLI_HARNESS_HEADROOM_RUNTIME_PLAN_SCHEMA_VERSION
        && runtime.enabled
        && has_tool_output_interception
        && has_log_route
        && has_reversible_store
        && has_retrieval_tool
        && has_runtime_env;
    let mut validation_evidence = vec![
        "headroom_runtime_wrapper_plan_reuses_harness_contract".to_string(),
        "tool_output_interception_requires_retrieval_ref".to_string(),
        "log_route_uses_signal_log_compressor".to_string(),
        "reversible_headroom_store_declared".to_string(),
        "retrieval_mcp_tool_declared".to_string(),
        "runtime_env_overlay_declared".to_string(),
        "no_child_cli_or_model_execution_performed".to_string(),
    ];
    if ready {
        validation_evidence.push("headroom_runtime_wrapper_demo_ready".to_string());
    } else {
        validation_evidence.push("headroom_runtime_wrapper_demo_incomplete".to_string());
    }

    MilestoneHeadroomRuntimeWrapperDemo {
        schema_version: HEADROOM_RUNTIME_WRAPPER_DEMO_SCHEMA_VERSION.to_string(),
        status: if ready {
            "headroom_runtime_wrapper_demo_ready".to_string()
        } else {
            "headroom_runtime_wrapper_demo_incomplete".to_string()
        },
        executor: wrapper_plan.executor.clone(),
        non_executing: true,
        child_cli_launched: false,
        external_resources_mutated: false,
        wrapper_plan,
        validation_evidence,
        commands: vec![
            "foundry harness wrap-plan --executor codex --cmd codex --foundry-first --workflow <workflow-id> --task <task-id> --context-budget 1200 --token-headroom --output json".to_string(),
            "foundry harness token-headroom --content <payload> --kind log --persist --output json".to_string(),
            "foundry harness retrieve-headroom --ref <retrieval-ref> --include-content --output json".to_string(),
            "foundry mcp call foundry.harness.retrieve_headroom --input '{\"ref\":\"<retrieval-ref>\",\"include_content\":true}' --output json".to_string(),
        ],
        summary: "The milestone demo now exposes the Headroom-inspired Foundry wrapper runtime as a structured, non-executing contract: prompt/tool/stdout interception, reversible local storage, retrieval tools and env overlay are all produced by the same harness wrapper plan used by real CLI execution.".to_string(),
    }
}

fn build_replacement_cli_brain_handoff_demo(
    store: &FoundryStore,
    origin: &str,
) -> Result<ReplacementCliDemoFlow> {
    let request = start_async_request(
        store,
        "Demonstrate Foundry-owned external brain handoff rehearsal with context, routing, shell plan and lifecycle audit",
        origin,
    )?;
    let task_id = "task-brain-handoff".to_string();
    let mut workflow = store.load_workflow(&request.workflow_id)?;
    workflow.tasks = vec![task(
        &task_id,
        "Prepare a Foundry-owned Codex handoff rehearsal",
        &[],
        &[
            "workflow goal",
            "project memory policy",
            "node brain routing",
            "validation gates",
        ],
        vec![],
        "bounded Codex handoff packet with plan-only shell lifecycle evidence",
        (ExecutorKind::Ai, 0.12),
    )];
    workflow.status = "running".to_string();
    store.save_workflow(&workflow)?;

    let routing_update = update_workflow_node_brain_routing(
        store,
        &workflow.id,
        WorkflowNodeBrainRoutingUpdateInput {
            task_id: task_id.clone(),
            default_brain: Some("codex".to_string()),
            allowed_brains: vec![
                "codex".to_string(),
                "opencode".to_string(),
                "gemini".to_string(),
                "claude".to_string(),
            ],
            agent_slots: vec![
                NodeBrainAgentSlotSpec {
                    slot_id: "agent-codex-primary".to_string(),
                    brain_id: Some("codex".to_string()),
                    role: "primary_node_agent".to_string(),
                    parallel_group: "handoff-rehearsal".to_string(),
                    state_owner: "foundry".to_string(),
                },
                NodeBrainAgentSlotSpec {
                    slot_id: "agent-codex-review".to_string(),
                    brain_id: Some("codex".to_string()),
                    role: "review_agent".to_string(),
                    parallel_group: "handoff-rehearsal".to_string(),
                    state_owner: "foundry".to_string(),
                },
            ],
            max_parallel_agents: Some(2),
            origin: origin.to_string(),
        },
    )?;

    let project_root = store
        .base_dir()
        .join("tmp")
        .join(format!("{}-brain-handoff", workflow.id));
    fs::create_dir_all(project_root.join(".foundry"))?;

    let handoff = build_task_handoff_with_project(
        store,
        &workflow.id,
        &task_id,
        "codex",
        1200,
        900,
        Some(&project_root),
    )?;
    let router = load_or_build_rehearsal_brain_router(store)?;
    let shell_receipt = record_shell_session_plan(
        store,
        &router,
        ShellLaunchPlanOptions {
            executor_filter: Some("codex".to_string()),
            workflow_id: Some(workflow.id.clone()),
            task_id: Some(task_id.clone()),
            run_id: Some(request.run_id.clone()),
            context_budget: Some(1200),
            ttl_seconds: Some(900),
        },
        origin,
    )?;
    let lifecycle_receipt = record_brain_session_lifecycle(
        store,
        &router,
        BrainSessionLifecycleOptions {
            session_id: "codex-shell",
            state: "opened",
            workflow_id: Some(&workflow.id),
            task_id: Some(&task_id),
            run_id: Some(&request.run_id),
            origin,
            note: Some("milestone cli-demo plan-only rehearsal; no child CLI or model execution"),
        },
    )?;
    let shell_plan_recorded = shell_receipt.status == "shell_session_plan_recorded";
    let status = if handoff.allowed
        && handoff.context.handoff_ready
        && shell_plan_recorded
        && lifecycle_receipt.event_recorded
    {
        "brain_handoff_rehearsal_ready"
    } else {
        "brain_handoff_rehearsal_incomplete"
    }
    .to_string();
    let commands = vec![
        "foundry workflow update-node-brain --workflow <workflow-id> --task <task-id> --default-brain codex --agent-slot agent-codex-primary=codex:primary_node_agent:handoff-rehearsal --agent-slot agent-codex-review=codex:review_agent:handoff-rehearsal --max-parallel-agents 2 --origin foundry_cli --output json".to_string(),
        "foundry context --workflow <workflow-id> --task <task-id> --project-root <project-root> --budget 1200 --strict --view compact --output json".to_string(),
        "foundry task handoff --workflow <workflow-id> --task <task-id> --executor codex --project-root <project-root> --view compact --output json".to_string(),
        "foundry shells --executor codex --workflow <workflow-id> --task <task-id> --run <run-id> --record-session --origin foundry_cli --output json".to_string(),
        "foundry sessions lifecycle --session codex-shell --state opened --workflow <workflow-id> --task <task-id> --run <run-id> --origin foundry_cli --output json".to_string(),
        "foundry sessions history --session codex-shell --output json".to_string(),
    ];
    let brain_handoff = MilestoneBrainHandoffDemo {
        schema_version: BRAIN_HANDOFF_DEMO_SCHEMA_VERSION.to_string(),
        status: status.clone(),
        workflow_id: workflow.id.clone(),
        task_id: task_id.clone(),
        run_id: request.run_id.clone(),
        selected_brain: handoff.selected_brain.clone(),
        orchestrator_brain: handoff.orchestrator_brain.clone(),
        handoff_status: handoff.status.clone(),
        handoff_ready: handoff.context.handoff_ready,
        context_schema_version: handoff.context.schema_version.clone(),
        context_bytes: handoff.context.context_bytes,
        shell_plan_status: shell_receipt.launch_plan.status.clone(),
        shell_plan_recorded,
        shell_plan_event_id: shell_receipt.global_event_id,
        lifecycle_status: lifecycle_receipt.status.clone(),
        lifecycle_state: lifecycle_receipt.state.clone(),
        lifecycle_event_recorded: lifecycle_receipt.event_recorded,
        lifecycle_event_id: lifecycle_receipt.global_event_id,
        model_execution_performed: false,
        external_resources_mutated: false,
        node_brain_routing: routing_update.new_routing,
        commands: commands.clone(),
        summary: "Foundry assembled the context packet, node-brain routing, task handoff lease, plan-only shell launch receipt and ordered session lifecycle receipt for Codex without launching a child CLI or executing a model.".to_string(),
    };

    Ok(ReplacementCliDemoFlow {
        kind: "brain_handoff_rehearsal".to_string(),
        title: "Foundry-owned external brain handoff rehearsal".to_string(),
        workflow_id: workflow.id,
        run_id: Some(request.run_id),
        run_status: status,
        completed_through_foundry: true,
        commands,
        artifact_refs: vec![
            format!("shell_plan_global_event_id:{}", shell_receipt.global_event_id),
            format!(
                "lifecycle_global_event_id:{}",
                lifecycle_receipt.global_event_id
            ),
        ],
        validation_evidence: vec![
            "node_brain_routing_updated".to_string(),
            "context_packet_ready".to_string(),
            "task_handoff_lease_acquired".to_string(),
            "shell_launch_plan_recorded_without_child_execution".to_string(),
            "brain_session_lifecycle_recorded_audit_only".to_string(),
            "model_execution_not_performed".to_string(),
            "external_resources_untouched".to_string(),
        ],
        patch_lifecycle: None,
        executor_project: None,
        brain_handoff: Some(brain_handoff),
        real_project: None,
        external_brain: None,
        activity: None,
        summary: "This flow proves Foundry can prepare a Codex node handoff with Foundry-owned context, memory policy, node-brain routing, shell launch plan and session lifecycle audit, while honestly leaving actual model execution outside this deterministic milestone demo.".to_string(),
    })
}

fn build_replacement_cli_real_project_workflow_demo(
    store: &FoundryStore,
    origin: &str,
) -> Result<ReplacementCliDemoFlow> {
    let request = start_async_request(
        store,
        "Demonstrate Foundry-owned real project coding and research workflow with brain routing, handoff, harness execution and multi-file artifacts",
        origin,
    )?;
    let task_id = "task-real-project-coding-research".to_string();
    let mut workflow = store.load_workflow(&request.workflow_id)?;
    workflow.tasks = vec![task(
        &task_id,
        "Produce code and research artifacts for a small project",
        &[],
        &[
            "organization context",
            "project files",
            "research notes",
            "validation command",
        ],
        vec![],
        "validated code and research artifacts generated under Foundry lineage",
        (ExecutorKind::Ai, 0.18),
    )];
    workflow.status = "running".to_string();
    store.save_workflow(&workflow)?;

    let routing_update = update_workflow_node_brain_routing(
        store,
        &workflow.id,
        WorkflowNodeBrainRoutingUpdateInput {
            task_id: task_id.clone(),
            default_brain: Some("codex".to_string()),
            allowed_brains: vec![
                "codex".to_string(),
                "opencode".to_string(),
                "gemini".to_string(),
                "claude".to_string(),
            ],
            agent_slots: vec![
                NodeBrainAgentSlotSpec {
                    slot_id: "agent-codex-coder".to_string(),
                    brain_id: Some("codex".to_string()),
                    role: "project_coder".to_string(),
                    parallel_group: "real-project-demo".to_string(),
                    state_owner: "foundry".to_string(),
                },
                NodeBrainAgentSlotSpec {
                    slot_id: "agent-codex-researcher".to_string(),
                    brain_id: Some("codex".to_string()),
                    role: "project_researcher".to_string(),
                    parallel_group: "real-project-demo".to_string(),
                    state_owner: "foundry".to_string(),
                },
            ],
            max_parallel_agents: Some(2),
            origin: origin.to_string(),
        },
    )?;

    let project_root = store
        .base_dir()
        .join("tmp")
        .join(format!("{}-real-project", workflow.id));
    if project_root.exists() {
        fs::remove_dir_all(&project_root)?;
    }
    fs::create_dir_all(project_root.join(".foundry"))?;
    fs::create_dir_all(project_root.join("src"))?;
    fs::create_dir_all(project_root.join("tests"))?;
    fs::create_dir_all(project_root.join("docs/research"))?;
    fs::write(
        project_root.join("README.md"),
        "# Foundry real project demo\n\nThis isolated project receives code and research artifacts through Foundry-controlled execution.\n",
    )?;

    let handoff = build_task_handoff_with_project(
        store,
        &workflow.id,
        &task_id,
        "codex",
        1800,
        900,
        Some(&project_root),
    )?;
    let shim_dir = project_root.join(".foundry/shims");
    let _bootstrap = build_harness_bootstrap_report(HarnessBootstrapOptions {
        shim_dir: &shim_dir,
        executor: "sh",
        project_root: &project_root,
        store_path: Some(store.path()),
        context_budget: 1024,
        context_budget_source: "milestone_real_project_demo",
        token_headroom: true,
        token_headroom_source: "milestone_real_project_demo",
        apply: true,
        approved_by: Some("foundry_cli_demo"),
        force: true,
    })?;

    let edit_script = r#"set -eu
mkdir -p src tests docs/research
cat > src/lib.rs <<'RS'
pub fn classify_request(input: &str) -> &'static str {
    if input.contains("research") {
        "research"
    } else {
        "coding"
    }
}
RS
cat > tests/workflow_contract.txt <<EOF
workflow=$FOUNDRY_WORKFLOW_ID
task=$FOUNDRY_TASK_ID
run=$FOUNDRY_RUN_ID
brain=codex
harness=$FOUNDRY_HARNESS
mode=$FOUNDRY_HARNESS_MODE
EOF
cat > docs/research/findings.md <<EOF
# Real project research fixture

- Workflow: $FOUNDRY_WORKFLOW_ID
- Task: $FOUNDRY_TASK_ID
- Result: code and research artifacts generated under Foundry harness lineage.
EOF
grep -q 'classify_request' src/lib.rs
grep -q "$FOUNDRY_WORKFLOW_ID" tests/workflow_contract.txt
grep -q 'research artifacts' docs/research/findings.md
{
  printf 'foundry_real_project_demo\n'
  printf 'workflow=%s\n' "$FOUNDRY_WORKFLOW_ID"
  printf 'task=%s\n' "$FOUNDRY_TASK_ID"
  printf 'run=%s\n' "$FOUNDRY_RUN_ID"
  printf 'brain=codex\n'
  printf 'harness=%s\n' "$FOUNDRY_HARNESS"
  printf 'mode=%s\n' "$FOUNDRY_HARNESS_MODE"
  printf 'token_headroom=%s\n' "$FOUNDRY_TOKEN_HEADROOM"
  printf 'artifacts=src/lib.rs,tests/workflow_contract.txt,docs/research/findings.md\n'
  printf 'validation=code_and_research_markers_verified\n'
  printf 'research_summary=Foundry routed coding and research through one lineage-preserving workflow.\n'
  i=0
  while [ "$i" -lt 48 ]; do
    printf 'trace[%02d]=workflow=%s task=%s run=%s phase=project_coding_research status=observed artifact_set=code,research,contract\n' "$i" "$FOUNDRY_WORKFLOW_ID" "$FOUNDRY_TASK_ID" "$FOUNDRY_RUN_ID"
    i=$((i + 1))
  done
}
"#;
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        edit_script.to_string(),
    ];
    let receipt = run_cli_harness_exec(CliHarnessExecOptions {
        store: Some(store),
        executor: "sh",
        command: &command,
        foundry_first: true,
        foundry_first_source: "milestone_real_project_demo",
        workflow_id: Some(&workflow.id),
        task_id: Some(&task_id),
        run_id: Some(&request.run_id),
        context_budget: 1024,
        context_budget_source: "milestone_real_project_demo",
        token_headroom: true,
        token_headroom_source: "milestone_real_project_demo",
        require_token_headroom_for_foundry_first: true,
        dry_run: false,
        allow_exec: true,
        secret_env: &[],
        secret_permissions: &[],
        project_root: Some(&project_root),
        cwd: Some(&project_root),
    })?;
    let target_paths = vec![
        "src/lib.rs".to_string(),
        "tests/workflow_contract.txt".to_string(),
        "docs/research/findings.md".to_string(),
    ];
    let mut target_sha256 = BTreeMap::new();
    let mut all_targets_exist = true;
    for path in &target_paths {
        let target = project_root.join(path);
        if target.is_file() {
            target_sha256.insert(path.clone(), hex_sha256(&fs::read(target)?));
        } else {
            all_targets_exist = false;
        }
    }
    let validation_status = if receipt.success == Some(true) && all_targets_exist {
        "validated"
    } else {
        "failed"
    }
    .to_string();
    let stdout_headroom_status = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.status.clone())
        .unwrap_or_else(|| "not_recorded".to_string());
    let stdout_retrieval_ref = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.retrieval_ref.clone());
    let status = if handoff.allowed
        && handoff.context.handoff_ready
        && receipt.event_recorded
        && validation_status == "validated"
    {
        "real_project_workflow_demo_completed"
    } else {
        "real_project_workflow_demo_incomplete"
    }
    .to_string();
    let routing_default_brain = routing_update
        .new_routing
        .default_brain
        .clone()
        .unwrap_or_else(|| "codex".to_string());

    let real_project = MilestoneRealProjectWorkflowDemo {
        schema_version: REAL_PROJECT_WORKFLOW_DEMO_SCHEMA_VERSION.to_string(),
        status: status.clone(),
        repository_path: project_root.display().to_string(),
        target_paths: target_paths.clone(),
        target_sha256,
        handoff_status: handoff.status.clone(),
        handoff_ready: handoff.context.handoff_ready,
        selected_brain: handoff.selected_brain.clone(),
        routing_default_brain,
        exec_status: receipt.status.clone(),
        exec_event_recorded: receipt.event_recorded,
        exec_global_event_id: receipt.global_event_id,
        validation_command: "grep code, lineage and research artifact markers".to_string(),
        validation_status,
        stdout_headroom_status,
        stdout_retrieval_ref,
        external_resources_mutated: false,
        lineage: vec![
            format!("workflow_id:{}", workflow.id),
            format!("task_id:{task_id}"),
            format!("run_id:{}", request.run_id),
            format!("handoff_status:{}", handoff.status),
            format!("global_event_id:{}", receipt.global_event_id.unwrap_or_default()),
        ],
        summary: "Foundry routed a real-project style task to Codex, built a handoff packet for an isolated project, executed a governed harness command with workflow/task/run lineage, generated code plus research artifacts and validated them without touching external resources.".to_string(),
    };

    Ok(ReplacementCliDemoFlow {
        kind: "real_project_coding_research".to_string(),
        title: "Foundry-owned real project coding and research workflow".to_string(),
        workflow_id: workflow.id,
        run_id: Some(request.run_id),
        run_status: status,
        completed_through_foundry: true,
        commands: vec![
            "foundry workflow update-node-brain --workflow <workflow-id> --task <task-id> --default-brain codex --agent-slot agent-codex-coder=codex:project_coder:real-project-demo --agent-slot agent-codex-researcher=codex:project_researcher:real-project-demo --max-parallel-agents 2 --origin foundry_cli --output json".to_string(),
        "foundry task handoff --workflow <workflow-id> --task <task-id> --executor codex --project-root <project-root> --view compact --output json".to_string(),
            "foundry harness bootstrap --executor sh --shim-dir <project-root>/.foundry/shims --project-root <project-root> --apply --approved-by foundry_cli_demo --output json".to_string(),
            "foundry harness exec --executor sh --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --foundry-first --execute --allow-exec -- /bin/sh -c <real-project-code-and-research-script>".to_string(),
            "foundry harness retrieve-headroom --ref <stdout-retrieval-ref> --output json".to_string(),
        ],
        artifact_refs: target_paths
            .iter()
            .map(|path| project_root.join(path).display().to_string())
            .collect(),
        validation_evidence: vec![
            "node_brain_routing_updated_for_coding_and_research_slots".to_string(),
            "task_handoff_ready_for_project_root".to_string(),
            "multi_file_code_and_research_artifacts_generated".to_string(),
            "harness_exec_event_recorded".to_string(),
            "stdout_headroom_retrieval_available".to_string(),
            "external_resources_untouched".to_string(),
        ],
        patch_lifecycle: None,
        executor_project: None,
        brain_handoff: None,
        real_project: Some(real_project),
        external_brain: None,
        activity: None,
        summary: "This flow moves the replacement-grade CLI evidence beyond single-file fixtures by proving a multi-file project coding and research task can run through Foundry-owned brain routing, handoff, harness lineage and validation.".to_string(),
    })
}

fn load_connected_brain_provider_selection(
    options: &MilestoneCliDemoOptions<'_>,
) -> Result<Option<ConnectedBrainProviderSelection>> {
    let Some(project_root) = options.project_root else {
        return Ok(None);
    };
    let manifest_path =
        crate::brand::project_config_path_for_read(project_root, "connected-brain-runtimes.json");
    if !manifest_path.is_file() {
        if let Some(connected_brain) = options.connected_brain {
            bail!(
                "connected brain provider `{}` requested, but manifest not found at {}",
                connected_brain,
                manifest_path.display()
            );
        }
        return Ok(None);
    }

    let manifest: ConnectedBrainRuntimeManifest =
        serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let selected_provider = manifest
        .providers
        .into_iter()
        .find(|provider| {
            let id_matches = options
                .connected_brain
                .map(|connected_brain| provider.id == connected_brain)
                .unwrap_or(true);
            id_matches
                && provider
                    .capabilities
                    .iter()
                    .any(|capability| capability == "replacement_grade_cli")
        })
        .ok_or_else(|| {
            let selector = options
                .connected_brain
                .unwrap_or("<first replacement_grade_cli provider>");
            anyhow::anyhow!(
                "connected brain provider `{}` not declared for replacement_grade_cli in {}",
                selector,
                manifest_path.display()
            )
        })?;

    if selected_provider.command.is_empty() {
        bail!(
            "connected brain provider `{}` must declare a non-empty command array",
            selected_provider.id
        );
    }
    if selected_provider
        .command
        .iter()
        .any(|part| milestone_manifest_placeholder(part))
    {
        bail!(
            "connected brain provider `{}` still contains placeholder command entries; replace them with an approved provider command before collecting evidence",
            selected_provider.id
        );
    }
    if selected_provider
        .approved_by
        .as_deref()
        .is_none_or(milestone_manifest_placeholder)
        || selected_provider
            .approval_ref
            .as_deref()
            .is_none_or(milestone_manifest_placeholder)
        || selected_provider
            .model_id
            .as_deref()
            .is_none_or(milestone_manifest_placeholder)
    {
        bail!(
            "connected brain provider `{}` must declare approved_by, approval_ref and model_id before running the connected-brain demo",
            selected_provider.id
        );
    }
    if selected_provider.network_access
        || selected_provider.device_access
        || selected_provider.external_resources_mutated
    {
        bail!(
            "connected brain provider `{}` declares network, device or external resource mutation; milestone cli-demo only accepts guarded no-network/no-device/no-external-mutation providers",
            selected_provider.id
        );
    }

    Ok(Some(ConnectedBrainProviderSelection {
        manifest_path,
        manifest_root: project_root.to_path_buf(),
        provider: selected_provider,
    }))
}

fn connected_brain_provider_command(
    provider: &ConnectedBrainProviderConfig,
    manifest_root: &Path,
) -> Vec<String> {
    let mut command = provider.command.clone();
    if let Some(program) = command.first_mut() {
        let program_path = Path::new(program);
        if program_path.components().count() > 1 && program_path.is_relative() {
            *program = manifest_root.join(program_path).display().to_string();
        }
    }
    command
}

fn build_replacement_cli_connected_external_brain_demo(
    store: &FoundryStore,
    origin: &str,
    options: &MilestoneCliDemoOptions<'_>,
) -> Result<ReplacementCliDemoFlow> {
    let provider_selection = load_connected_brain_provider_selection(options)?;
    let selected_provider = provider_selection
        .as_ref()
        .map(|selection| &selection.provider);
    let brain_id = selected_provider
        .and_then(|provider| provider.brain_id.as_deref())
        .or_else(|| selected_provider.map(|provider| provider.id.as_str()))
        .unwrap_or("codex-compatible-stub");
    let request = start_async_request(
        store,
        "Demonstrate Foundry-owned connected external brain adapter execution with handoff, harness lineage and validation",
        origin,
    )?;
    let task_id = "task-connected-external-brain".to_string();
    let mut workflow = store.load_workflow(&request.workflow_id)?;
    workflow.tasks = vec![task(
        &task_id,
        "Run a connected external brain adapter against an isolated project",
        &[],
        &[
            "organization context",
            "project memory policy",
            "external brain adapter",
            "validation command",
        ],
        vec![],
        "validated adapter outputs generated under Foundry lineage",
        (ExecutorKind::Ai, 0.2),
    )];
    workflow.status = "running".to_string();
    store.save_workflow(&workflow)?;

    let routing_update = update_workflow_node_brain_routing(
        store,
        &workflow.id,
        WorkflowNodeBrainRoutingUpdateInput {
            task_id: task_id.clone(),
            default_brain: Some(brain_id.to_string()),
            allowed_brains: vec![
                brain_id.to_string(),
                "codex".to_string(),
                "opencode".to_string(),
                "gemini".to_string(),
                "claude".to_string(),
            ],
            agent_slots: vec![
                NodeBrainAgentSlotSpec {
                    slot_id: "agent-external-brain-coder".to_string(),
                    brain_id: Some(brain_id.to_string()),
                    role: "connected_adapter_coder".to_string(),
                    parallel_group: "connected-external-brain-demo".to_string(),
                    state_owner: "foundry".to_string(),
                },
                NodeBrainAgentSlotSpec {
                    slot_id: "agent-external-brain-researcher".to_string(),
                    brain_id: Some(brain_id.to_string()),
                    role: "connected_adapter_researcher".to_string(),
                    parallel_group: "connected-external-brain-demo".to_string(),
                    state_owner: "foundry".to_string(),
                },
            ],
            max_parallel_agents: Some(2),
            origin: origin.to_string(),
        },
    )?;

    let project_root = store
        .base_dir()
        .join("tmp")
        .join(format!("{}-connected-external-brain", workflow.id));
    if project_root.exists() {
        fs::remove_dir_all(&project_root)?;
    }
    fs::create_dir_all(project_root.join("brain-output"))?;
    fs::write(
        project_root.join("README.md"),
        "# Foundry connected external brain demo\n\nThis isolated project receives adapter output through Foundry-controlled harness execution.\n",
    )?;

    let handoff = build_task_handoff_with_project(
        store,
        &workflow.id,
        &task_id,
        brain_id,
        1800,
        900,
        Some(&project_root),
    )?;
    let shim_dir = project_root.join(".foundry/shims");
    let _bootstrap = build_harness_bootstrap_report(HarnessBootstrapOptions {
        shim_dir: &shim_dir,
        executor: "sh",
        project_root: &project_root,
        store_path: Some(store.path()),
        context_budget: 1024,
        context_budget_source: "milestone_connected_external_brain_demo",
        token_headroom: true,
        token_headroom_source: "milestone_connected_external_brain_demo",
        apply: true,
        approved_by: Some("foundry_cli_demo"),
        force: true,
    })?;

    let command = if let Some(selection) = provider_selection.as_ref() {
        connected_brain_provider_command(&selection.provider, &selection.manifest_root)
    } else {
        let adapter_script = r#"set -eu
mkdir -p brain-output
cat > brain-output/plan.json <<EOF
{"schema_version":"foundry.connected_external_brain_stub.v1","workflow_id":"$FOUNDRY_WORKFLOW_ID","task_id":"$FOUNDRY_TASK_ID","run_id":"$FOUNDRY_RUN_ID","brain_id":"codex-compatible-stub","model_execution_performed":false}
EOF
cat > brain-output/provider-output.json <<EOF
{"schema_version":"foundry.connected_external_brain.provider_output.v1","provider_id":"codex-compatible-stub","quality_score":0.92,"latency_ms":9,"model_execution_performed":false,"real_provider_execution_performed":false}
EOF
cat > brain-output/research.md <<EOF
# Connected external brain adapter fixture

- Workflow: $FOUNDRY_WORKFLOW_ID
- Task: $FOUNDRY_TASK_ID
- Brain: codex-compatible-stub
- Result: adapter process executed under Foundry harness lineage without invoking a model provider.
EOF
cat > brain-output/code.rs <<'RS'
pub fn connected_external_brain_marker() -> &'static str {
    "codex-compatible-stub"
}
RS
grep -q 'codex-compatible-stub' brain-output/plan.json
grep -q 'foundry.connected_external_brain.provider_output.v1' brain-output/provider-output.json
grep -q "$FOUNDRY_WORKFLOW_ID" brain-output/research.md
grep -q 'connected_external_brain_marker' brain-output/code.rs
printf 'connected_external_brain_stub_ok\n'
"#;
        vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            adapter_script.to_string(),
        ]
    };
    let receipt = run_cli_harness_exec(CliHarnessExecOptions {
        store: Some(store),
        executor: brain_id,
        command: &command,
        foundry_first: true,
        foundry_first_source: "milestone_connected_external_brain_demo",
        workflow_id: Some(&workflow.id),
        task_id: Some(&task_id),
        run_id: Some(&request.run_id),
        context_budget: 1024,
        context_budget_source: "milestone_connected_external_brain_demo",
        token_headroom: true,
        token_headroom_source: "milestone_connected_external_brain_demo",
        require_token_headroom_for_foundry_first: true,
        dry_run: false,
        allow_exec: true,
        secret_env: &[],
        secret_permissions: &[],
        project_root: Some(&project_root),
        cwd: Some(&project_root),
    })?;

    let target_paths = vec![
        "brain-output/plan.json".to_string(),
        "brain-output/provider-output.json".to_string(),
        "brain-output/research.md".to_string(),
        "brain-output/code.rs".to_string(),
    ];
    let mut target_sha256 = BTreeMap::new();
    let mut all_targets_exist = true;
    for path in &target_paths {
        let target = project_root.join(path);
        if target.is_file() {
            target_sha256.insert(path.clone(), hex_sha256(&fs::read(target)?));
        } else {
            all_targets_exist = false;
        }
    }

    let external_brain_process_executed = receipt.executed && receipt.success == Some(true);
    let provider_id = selected_provider
        .map(|provider| provider.id.as_str())
        .unwrap_or(brain_id);
    let provider_source = if provider_selection.is_some() {
        "project_manifest"
    } else {
        "built_in_stub"
    };
    let manifest_path = provider_selection
        .as_ref()
        .map(|selection| selection.manifest_path.display().to_string());
    let manifest_status = if provider_selection.is_some() {
        "loaded"
    } else {
        "not_configured"
    };
    let model_id = selected_provider
        .and_then(|provider| provider.model_id.as_deref())
        .unwrap_or("not_declared");
    let provider_class = selected_provider
        .and_then(|provider| provider.provider_class.as_deref())
        .unwrap_or("built_in_stub");
    let approved_by = selected_provider.and_then(|provider| provider.approved_by.clone());
    let approval_ref = selected_provider.and_then(|provider| provider.approval_ref.clone());
    let allow_model_execution = selected_provider
        .map(|provider| provider.allow_model_execution)
        .unwrap_or(false);
    let provider_output_path = project_root.join("brain-output/provider-output.json");
    let provider_output = fs::read_to_string(&provider_output_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    let output_schema_valid = provider_output.as_ref().is_some_and(|output| {
        output["schema_version"] == "foundry.connected_external_brain.provider_output.v1"
            && output["provider_id"] == provider_id
    });
    let output_quality_score = provider_output
        .as_ref()
        .and_then(|output| output["quality_score"].as_f64())
        .map(|score| format!("{score:.2}"))
        .unwrap_or_else(|| "not_recorded".to_string());
    let output_latency_ms = provider_output
        .as_ref()
        .and_then(|output| output["latency_ms"].as_i64())
        .map(|latency| latency.to_string())
        .unwrap_or_else(|| "not_recorded".to_string());
    let provider_declared_model_execution = provider_output
        .as_ref()
        .and_then(|output| output["model_execution_performed"].as_bool())
        .unwrap_or(false);
    let real_provider_execution_performed = provider_output
        .as_ref()
        .and_then(|output| output["real_provider_execution_performed"].as_bool())
        .unwrap_or(false);
    let model_execution_allowed = allow_model_execution || !provider_declared_model_execution;
    let real_provider_execution_allowed =
        allow_model_execution || !real_provider_execution_performed;
    let provider_contract_valid = external_brain_process_executed
        && output_schema_valid
        && model_execution_allowed
        && real_provider_execution_allowed
        && receipt.stdout_sha256.is_some();
    let provider_approval_recorded = approved_by
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && approval_ref
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    let provider_model_declared = !model_id.trim().is_empty() && model_id != "not_declared";
    let provider_promotion_ready = provider_selection.is_some()
        && provider_contract_valid
        && provider_approval_recorded
        && provider_model_declared
        && allow_model_execution
        && provider_declared_model_execution
        && real_provider_execution_performed;
    let mut provider_validation_evidence = vec![
        "provider_process_ran_under_foundry_harness".to_string(),
        "provider_command_hash_recorded".to_string(),
        "provider_stdout_hash_recorded".to_string(),
        "provider_output_schema_validated".to_string(),
    ];
    if provider_declared_model_execution {
        provider_validation_evidence.push("provider_declared_model_execution".to_string());
    } else {
        provider_validation_evidence.push("provider_declared_no_model_execution".to_string());
    }
    if real_provider_execution_performed {
        provider_validation_evidence.push("real_provider_execution_performed".to_string());
    } else {
        provider_validation_evidence.push("real_provider_execution_not_performed".to_string());
    }
    let provider_contract = MilestoneConnectedExternalBrainProviderContract {
        schema_version: CONNECTED_EXTERNAL_BRAIN_PROVIDER_SCHEMA_VERSION.to_string(),
        status: if provider_contract_valid {
            "connected_external_brain_provider_contract_validated"
        } else {
            "connected_external_brain_provider_contract_invalid"
        }
        .to_string(),
        provider_id: provider_id.to_string(),
        provider_source: provider_source.to_string(),
        manifest_path,
        manifest_status: manifest_status.to_string(),
        model_id: model_id.to_string(),
        provider_class: provider_class.to_string(),
        approved_by,
        approval_ref,
        execution_mode: receipt.execution_mode.clone(),
        command_sha256: receipt.command_sha256.clone(),
        stdout_sha256: receipt.stdout_sha256.clone(),
        stderr_sha256: receipt.stderr_sha256.clone(),
        output_schema_valid,
        output_quality_score,
        output_latency_ms,
        provider_declared_model_execution,
        real_provider_execution_performed,
        promotion_ready: provider_promotion_ready,
        validation_evidence: provider_validation_evidence,
    };
    let validation_status =
        if external_brain_process_executed && all_targets_exist && provider_contract_valid {
            "validated"
        } else {
            "failed"
        }
        .to_string();
    let stdout_headroom_status = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.status.clone())
        .unwrap_or_else(|| "not_recorded".to_string());
    let stdout_retrieval_ref = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.retrieval_ref.clone());
    let status = if handoff.allowed
        && handoff.context.handoff_ready
        && receipt.event_recorded
        && validation_status == "validated"
    {
        "connected_external_brain_demo_completed"
    } else {
        "connected_external_brain_demo_incomplete"
    }
    .to_string();
    let routing_default_brain = routing_update
        .new_routing
        .default_brain
        .clone()
        .unwrap_or_else(|| brain_id.to_string());
    let external_brain_summary = if provider_selection.is_some() {
        "Foundry routed an AI node to a project-declared connected external brain provider, produced a project-root handoff, executed the approved command through the harness, recorded the timeline event and validated generated provider, plan, research and code artifacts. Model/provider execution is reported strictly from the provider output contract and manifest approval."
    } else {
        "Foundry routed an AI node to a connected external brain adapter id, produced a project-root handoff, executed a real guarded child process through the harness, recorded the timeline event and validated generated plan, research and code artifacts. The fixture intentionally does not invoke a live model provider."
    };

    let external_brain = MilestoneConnectedExternalBrainDemo {
        schema_version: CONNECTED_EXTERNAL_BRAIN_DEMO_SCHEMA_VERSION.to_string(),
        status: status.clone(),
        repository_path: project_root.display().to_string(),
        brain_id: brain_id.to_string(),
        selected_brain: handoff.selected_brain.clone(),
        routing_default_brain,
        model_execution_performed: provider_declared_model_execution,
        external_brain_process_executed,
        provider_contract,
        handoff_status: handoff.status.clone(),
        handoff_ready: handoff.context.handoff_ready,
        harness_exec_status: receipt.status.clone(),
        exec_event_recorded: receipt.event_recorded,
        exec_global_event_id: receipt.global_event_id,
        validation_status,
        target_paths: target_paths.clone(),
        target_sha256,
        project_policy_status: receipt.project_policy_status.clone(),
        stdout_headroom_status,
        stdout_retrieval_ref,
        external_resources_mutated: false,
        lineage: vec![
            format!("workflow_id:{}", workflow.id),
            format!("task_id:{task_id}"),
            format!("run_id:{}", request.run_id),
            format!("brain_id:{brain_id}"),
            format!("handoff_status:{}", handoff.status),
            format!(
                "global_event_id:{}",
                receipt.global_event_id.unwrap_or_default()
            ),
        ],
        summary: external_brain_summary.to_string(),
    };
    let mut validation_evidence = vec![
        "node_brain_routing_updated_for_connected_adapter".to_string(),
        "task_handoff_ready_for_project_root".to_string(),
        "connected_external_brain_executed_under_harness".to_string(),
        "harness_exec_event_recorded".to_string(),
        "adapter_outputs_validated".to_string(),
        "connected_external_brain_provider_contract_validated".to_string(),
    ];
    if provider_declared_model_execution {
        validation_evidence.push("model_execution_performed_with_manifest_approval".to_string());
    } else {
        validation_evidence.push("model_execution_not_performed".to_string());
    }
    validation_evidence.push("external_resources_untouched".to_string());
    let mut commands = vec![
        format!(
            "foundry workflow update-node-brain --workflow <workflow-id> --task <task-id> --default-brain {brain_id} --agent-slot agent-external-brain-coder={brain_id}:connected_adapter_coder:connected-external-brain-demo --agent-slot agent-external-brain-researcher={brain_id}:connected_adapter_researcher:connected-external-brain-demo --max-parallel-agents 2 --origin foundry_cli --output json"
        ),
        format!(
            "foundry task handoff --workflow <workflow-id> --task <task-id> --executor {brain_id} --project-root <project-root> --view compact --output json"
        ),
        "foundry harness bootstrap --executor sh --shim-dir <project-root>/.foundry/shims --project-root <project-root> --apply --approved-by foundry_cli_demo --output json".to_string(),
        if provider_selection.is_some() {
            format!(
                "foundry harness exec --executor {brain_id} --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --foundry-first --execute --allow-exec -- <project-connected-brain-command>"
            )
        } else {
            "foundry harness exec --executor codex-compatible-stub --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --foundry-first --execute --allow-exec -- /bin/sh -c <connected-external-brain-script>".to_string()
        },
        "foundry events timeline --workflow <workflow-id> --output json".to_string(),
        "foundry harness retrieve-headroom --ref <stdout-retrieval-ref> --output json".to_string(),
    ];
    if let Some(provider) = selected_provider {
        commands.insert(
            0,
            format!(
                "foundry milestone cli-demo --project-root <project-root> --connected-brain {} --origin foundry_cli --output json",
                provider.id
            ),
        );
    }

    Ok(ReplacementCliDemoFlow {
        kind: "connected_external_brain".to_string(),
        title: "Connected external brain adapter execution under Foundry harness".to_string(),
        workflow_id: workflow.id,
        run_id: Some(request.run_id),
        run_status: status,
        completed_through_foundry: true,
        commands,
        artifact_refs: target_paths
            .iter()
            .map(|path| project_root.join(path).display().to_string())
            .collect(),
        validation_evidence,
        patch_lifecycle: None,
        executor_project: None,
        brain_handoff: None,
        real_project: None,
        external_brain: Some(external_brain),
        activity: None,
        summary: "This flow moves the replacement-grade CLI evidence from plan-only handoff toward a connected external-brain adapter path: Foundry selects the brain id, owns context and lineage, runs a real guarded process, records the event and validates outputs, while keeping live model/provider execution as an explicit remaining gap.".to_string(),
    })
}

fn load_or_build_rehearsal_brain_router(store: &FoundryStore) -> Result<BrainRouterReport> {
    let report = load_executors(store)?;
    if report
        .brain_router
        .shell_sessions
        .iter()
        .any(|session| session.id == "codex-shell")
    {
        return Ok(report.brain_router);
    }

    Ok(rehearsal_brain_router())
}

fn rehearsal_brain_router() -> BrainRouterReport {
    BrainRouterReport {
        schema_version: "foundry.brain_router.v1".to_string(),
        controller: "foundry".to_string(),
        controller_role: "orchestration_control_plane".to_string(),
        orchestrator_brain: "foundry".to_string(),
        brain_role: "replaceable_execution_brain".to_string(),
        node_brain_role: "per_node_agentic_execution_brain".to_string(),
        routing_principle:
            "Foundry owns memory, skills, MCP routing, context, workflow state, shell/session lifecycle, permissions, cost policy and validation; external CLIs only execute bounded brain work."
                .to_string(),
        node_brain_routing_policy:
            "Each AI or mixed workflow node may declare its own Foundry-owned node_brain_routing contract with one or more agent slots, different brains per slot, and multiple agents on the same brain."
                .to_string(),
        parallel_agent_policy:
            "Foundry may lease and run independent AI node agent slots in parallel when dependencies, context budgets, quota and validation gates allow it."
                .to_string(),
        hot_swap_policy:
            "A workflow run can switch the active execution brain through Foundry-owned routing without losing workflow lineage."
                .to_string(),
        selected_brain: Some("codex".to_string()),
        model_decision: None,
        foundry_controlled_surfaces: vec![
            "workflow_graph".to_string(),
            "memory".to_string(),
            "skills".to_string(),
            "mcp_servers_and_tools".to_string(),
            "context_packets".to_string(),
            "artifact_lineage".to_string(),
            "shell_session_lifecycle".to_string(),
            "permissions".to_string(),
            "cost_and_quota_policy".to_string(),
            "validation_gates".to_string(),
        ],
        brain_owned_surfaces: vec![
            "reasoning_for_assigned_task".to_string(),
            "bounded_code_or_text_proposals".to_string(),
            "child_process_execution_when_authorized_by_foundry".to_string(),
        ],
        brains: vec![BrainCandidate {
            id: "codex".to_string(),
            display_name: "Codex CLI".to_string(),
            command: "codex".to_string(),
            status: "rehearsal_not_synced".to_string(),
            execution_mode: "external_cli_brain".to_string(),
            session_role: "execution_brain_adapter".to_string(),
            persistent_state_owner: "foundry".to_string(),
            context_source: "foundry_context_packet".to_string(),
            memory_source: "foundry_memory_router".to_string(),
            skills_source: "foundry_skill_router".to_string(),
            mcp_source: "foundry_mcp_router".to_string(),
            installed: false,
            configured: false,
            allowed: false,
            non_interactive_ready: false,
            foundry_first_ready: false,
            foundry_first_entrypoint: None,
            harness_status: None,
            shell_entrypoints: vec![vec!["codex".to_string()]],
            reason: "deterministic milestone rehearsal router; run foundry sync all for real provider readiness".to_string(),
        }],
        shell_sessions: vec![
            BrainShellSessionSpec {
                id: "foundry-tui".to_string(),
                brain_id: "foundry".to_string(),
                entry_command: vec!["foundry".to_string()],
                attachable: true,
                launch_mode: "foundry_control_tui".to_string(),
                foundry_first_ready: true,
                foundry_first_entrypoint: Some(vec!["foundry".to_string()]),
                role: "primary_control_tui".to_string(),
                state_boundary:
                    "Foundry owns workflow state, memory, skills, MCP routing and shell lifecycle."
                        .to_string(),
                safety_note:
                    "Use this as the default human operation surface; external brains should be launched from Foundry-controlled handoffs."
                        .to_string(),
            },
            BrainShellSessionSpec {
                id: "codex-shell".to_string(),
                brain_id: "codex".to_string(),
                entry_command: vec!["codex".to_string()],
                attachable: false,
                launch_mode: "native_cli".to_string(),
                foundry_first_ready: false,
                foundry_first_entrypoint: None,
                role: "execution_brain_shell".to_string(),
                state_boundary:
                    "External CLI session is an execution surface only; Foundry remains the source of truth for memory, skills, MCPs, context and workflow lineage."
                        .to_string(),
                safety_note:
                    "This milestone rehearsal records only a plan; run executor sync and authorization before real Codex execution."
                        .to_string(),
            },
        ],
        safety_gates: vec![
            "sync_executors_before_handoff".to_string(),
            "human_authorization_for_external_cli_use".to_string(),
            "foundry_context_packet_required_before_ai_handoff".to_string(),
            "organization_context_required".to_string(),
            "personality_decision_required".to_string(),
            "company_work_decision_required".to_string(),
            "credential_vault_secrets_never_printed".to_string(),
            "validation_or_final_audit_required_before_claiming_completion".to_string(),
        ],
    }
}

fn build_replacement_cli_patch_lifecycle_demo(
    store: &FoundryStore,
    workflow_id: &str,
    origin: &str,
) -> Result<MilestonePatchLifecycleDemo> {
    let artifact_store = open_absolute_store_view(store)?;
    let store = &artifact_store;
    let workflow = store.load_workflow(workflow_id)?;
    let task_id = workflow
        .tasks
        .first()
        .map(|task| task.id.clone())
        .ok_or_else(|| anyhow::anyhow!("replacement CLI demo workflow has no tasks"))?;
    let repository_path = prepare_patch_lifecycle_demo_repository(store, workflow_id)?;
    let target_path = "src/demo.rs".to_string();

    with_current_dir(&repository_path, || {
        let plan = build_patch_plan(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            "Update the demo fixture through Foundry-owned patch lifecycle evidence.",
            origin,
        )?;
        let plan_artifact_path = patch_plan_artifact_path(store, &plan.artifact, "patch plan")?;

        fs::write(
            repository_path.join(&target_path),
            "pub fn demo_message() -> &'static str {\n    \"updated through foundry patch lifecycle\"\n}\n",
        )?;

        let review = build_patch_review(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            origin,
            Some(&plan_artifact_path),
        )?;
        let diff = build_patch_diff(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            PatchDiffOptions {
                file_index: 0,
                hunk_index: 0,
                context_lines: 3,
                origin,
            },
        )?;
        let validation_commands = vec![format!("git diff --check -- {target_path}")];
        let apply = build_patch_apply(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            origin,
            Some(&plan_artifact_path),
            Some(&validation_commands),
        )?;
        let apply_artifact_path = patch_apply_artifact_path(store, &apply.artifact, "patch apply")?;
        let revert = build_patch_revert(
            store,
            workflow_id,
            &task_id,
            &apply_artifact_path,
            origin,
            None,
        )?;
        let revert_artifact_path =
            patch_apply_artifact_path(store, &revert.artifact, "patch revert")?;
        let restore = build_patch_restore(
            store,
            workflow_id,
            &task_id,
            &revert_artifact_path,
            "foundry_cli_demo",
            true,
            origin,
        )?;
        let restored_to_clean_state = patch_demo_target_is_clean(&repository_path, &target_path)?;

        Ok(MilestonePatchLifecycleDemo {
            schema_version: PATCH_LIFECYCLE_DEMO_SCHEMA_VERSION.to_string(),
            status: if restored_to_clean_state {
                "patch_lifecycle_demo_ready"
            } else {
                "patch_lifecycle_demo_restore_incomplete"
            }
            .to_string(),
            target_path,
            repository_path: repository_path.display().to_string(),
            external_resources_mutated: false,
            restored_to_clean_state,
            plan_status: plan.status.clone(),
            review_status: review.status.clone(),
            diff_status: diff.status.clone(),
            apply_status: apply.status.clone(),
            revert_status: revert.status.clone(),
            restore_status: restore.status.clone(),
            artifact_refs: vec![
                summarize_plan_artifact("patch_plan", &plan.schema_version, &plan.status, &plan.artifact)?,
                summarize_patch_artifact("patch_review", &review.schema_version, &review.status, &review.artifact)?,
                summarize_patch_artifact("patch_diff", &diff.schema_version, &diff.status, &diff.artifact)?,
                summarize_patch_artifact("patch_apply", &apply.schema_version, &apply.status, &apply.artifact)?,
                summarize_patch_artifact("patch_revert", &revert.schema_version, &revert.status, &revert.artifact)?,
                summarize_patch_artifact("patch_restore", &restore.schema_version, &restore.status, &restore.artifact)?,
            ],
            gates: vec![
                "patch_edit_intake_required".to_string(),
                "plan_before_executor_edit".to_string(),
                "review_before_apply".to_string(),
                "diff_navigation_before_approval".to_string(),
                "validation_before_apply_record".to_string(),
                "rollback_proposal_before_restore".to_string(),
                "human_restore_approval_recorded".to_string(),
            ],
            commands: vec![
                "foundry interactive patch-workbench --output json".to_string(),
                format!("foundry patch plan --workflow {workflow_id} --task {task_id} --intent <intent> --path src/demo.rs --origin foundry_cli --output json"),
                format!("foundry patch review --workflow {workflow_id} --task {task_id} --path src/demo.rs --plan-artifact <patch-plan> --origin foundry_cli --output json"),
                format!("foundry patch diff --workflow {workflow_id} --task {task_id} --path src/demo.rs --file-index 0 --hunk-index 0 --output json"),
                format!("foundry patch apply --workflow {workflow_id} --task {task_id} --path src/demo.rs --plan-artifact <patch-plan> --origin foundry_cli --output json"),
                format!("foundry patch revert --workflow {workflow_id} --task {task_id} --apply-artifact <patch-apply> --origin foundry_cli --output json"),
                format!("foundry patch restore --workflow {workflow_id} --task {task_id} --revert-artifact <patch-revert> --approved-by foundry_cli_demo --confirm-restore --origin foundry_cli --output json"),
            ],
            summary: "Deterministic fixture repo executed the full Foundry patch lifecycle with plan, review, diff, apply record, revert proposal and approved restore artifacts, then returned the target file to a clean Git state.".to_string(),
        })
    })
}

fn build_replacement_cli_executor_project_demo(
    store: &FoundryStore,
    origin: &str,
) -> Result<ReplacementCliDemoFlow> {
    let request = start_async_request(
        store,
        "Demonstrate executor-driven project editing through Foundry harness lineage",
        origin,
    )?;
    let workflow = store.load_workflow(&request.workflow_id)?;
    let task_id = workflow
        .tasks
        .first()
        .map(|task| task.id.clone())
        .ok_or_else(|| anyhow::anyhow!("executor project demo workflow has no tasks"))?;
    let project_root = store
        .base_dir()
        .join("tmp")
        .join(format!("{}-executor-project", workflow.id));
    if project_root.exists() {
        fs::remove_dir_all(&project_root)?;
    }
    fs::create_dir_all(project_root.join("src"))?;
    fs::write(
        project_root.join("README.md"),
        "# Foundry executor project demo\n\nThis fixture is mutated only under Foundry harness control.\n",
    )?;

    let shim_dir = project_root.join(".foundry/shims");
    let bootstrap = build_harness_bootstrap_report(HarnessBootstrapOptions {
        shim_dir: &shim_dir,
        executor: "sh",
        project_root: &project_root,
        store_path: Some(store.path()),
        context_budget: 512,
        context_budget_source: "milestone_cli_demo",
        token_headroom: true,
        token_headroom_source: "milestone_cli_demo",
        apply: true,
        approved_by: Some("foundry_cli_demo"),
        force: true,
    })?;

    let edit_script = "mkdir -p src && printf 'workflow=%s\\ntask=%s\\nrun=%s\\nharness=%s\\nmode=%s\\nheadroom=%s\\n' \"$FOUNDRY_WORKFLOW_ID\" \"$FOUNDRY_TASK_ID\" \"$FOUNDRY_RUN_ID\" \"$FOUNDRY_HARNESS\" \"$FOUNDRY_HARNESS_MODE\" \"$FOUNDRY_TOKEN_HEADROOM\" > src/executor-output.txt && cat src/executor-output.txt";
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        edit_script.to_string(),
    ];
    let receipt = run_cli_harness_exec(CliHarnessExecOptions {
        store: Some(store),
        executor: "sh",
        command: &command,
        foundry_first: true,
        foundry_first_source: "milestone_cli_demo",
        workflow_id: Some(&workflow.id),
        task_id: Some(&task_id),
        run_id: Some(&request.run_id),
        context_budget: 512,
        context_budget_source: "milestone_cli_demo",
        token_headroom: true,
        token_headroom_source: "milestone_cli_demo",
        require_token_headroom_for_foundry_first: true,
        dry_run: false,
        allow_exec: true,
        secret_env: &[],
        secret_permissions: &[],
        project_root: Some(&project_root),
        cwd: Some(&project_root),
    })?;
    let target_path = "src/executor-output.txt";
    let target_bytes = fs::read(project_root.join(target_path))?;
    let target_sha256 = hex_sha256(&target_bytes);
    let stdout_headroom_status = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.status.clone())
        .unwrap_or_else(|| "not_recorded".to_string());
    let stdout_retrieval_ref = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.retrieval_ref.clone());
    let shim_install_status = bootstrap
        .shim_install
        .as_ref()
        .map(|report| report.status.clone())
        .unwrap_or_else(|| "not_installed".to_string());

    let executor_project = MilestoneExecutorProjectDemo {
        schema_version: EXECUTOR_PROJECT_DEMO_SCHEMA_VERSION.to_string(),
        status: if receipt.success == Some(true) && receipt.event_recorded {
            "executor_project_demo_completed"
        } else {
            "executor_project_demo_incomplete"
        }
        .to_string(),
        repository_path: project_root.display().to_string(),
        target_path: target_path.to_string(),
        target_sha256,
        bootstrap_status: bootstrap.status.clone(),
        bootstrap_config_status: bootstrap.config_write.status.clone(),
        shim_install_status,
        exec_status: receipt.status.clone(),
        exec_event_recorded: receipt.event_recorded,
        exec_global_event_id: receipt.global_event_id,
        project_policy_status: receipt.project_policy_status.clone(),
        stdout_headroom_status,
        stdout_retrieval_ref,
        external_resources_mutated: false,
        lineage: vec![
            format!("workflow_id:{}", workflow.id),
            format!("task_id:{task_id}"),
            format!("run_id:{}", request.run_id),
            format!("global_event_id:{}", receipt.global_event_id.unwrap_or_default()),
        ],
        summary: "The fixture project is edited by a guarded executor command after Foundry writes project harness policy, requires lineage, applies token headroom and records the execution in the global event timeline.".to_string(),
    };

    Ok(ReplacementCliDemoFlow {
        kind: "executor_project".to_string(),
        title: "Executor-driven isolated project edit under Foundry harness".to_string(),
        workflow_id: workflow.id,
        run_id: Some(request.run_id),
        run_status: receipt.status,
        completed_through_foundry: true,
        commands: vec![
            "foundry harness bootstrap --executor sh --shim-dir <project-root>/.foundry/shims --project-root <project-root> --apply --approved-by foundry_cli_demo --output json".to_string(),
            "foundry harness exec --executor sh --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --foundry-first --execute --allow-exec -- /bin/sh -c <project-edit-script>".to_string(),
            "foundry events timeline --workflow <workflow-id> --output json".to_string(),
            "foundry harness retrieve-headroom --ref <stdout-retrieval-ref> --output json".to_string(),
        ],
        artifact_refs: vec![project_root.join(target_path).display().to_string()],
        validation_evidence: vec![
            "bootstrap_applied_with_operator_approval".to_string(),
            "project_policy_requires_lineage".to_string(),
            "executor_mutated_isolated_project_under_harness".to_string(),
            "harness_exec_event_recorded".to_string(),
            "stdout_headroom_retrieval_available".to_string(),
            "external_resources_untouched".to_string(),
        ],
        patch_lifecycle: None,
        executor_project: Some(executor_project),
        brain_handoff: None,
        real_project: None,
        external_brain: None,
        activity: None,
        summary: "This flow closes part of the replacement-grade CLI gap by proving an executor can modify an isolated project through Foundry-owned bootstrap, lineage policy, guarded execution, event recording and reversible stdout headroom.".to_string(),
    })
}

fn open_absolute_store_view(store: &FoundryStore) -> Result<FoundryStore> {
    let path = if store.path().is_absolute() {
        store.path().to_path_buf()
    } else {
        env::current_dir()?.join(store.path())
    };
    FoundryStore::open(path)
}

fn prepare_patch_lifecycle_demo_repository(
    store: &FoundryStore,
    workflow_id: &str,
) -> Result<PathBuf> {
    let repository_path = store
        .base_dir()
        .join("tmp")
        .join(format!("{workflow_id}-patch-lifecycle-repo"));
    if repository_path.exists() {
        fs::remove_dir_all(&repository_path)?;
    }
    fs::create_dir_all(repository_path.join("src"))?;
    fs::write(
        repository_path.join("src/demo.rs"),
        "pub fn demo_message() -> &'static str {\n    \"initial\"\n}\n",
    )?;
    run_demo_git(&repository_path, &["init", "-q"])?;
    run_demo_git(
        &repository_path,
        &["config", "user.email", "foundry@example.com"],
    )?;
    run_demo_git(
        &repository_path,
        &["config", "user.name", "Foundry CLI Demo"],
    )?;
    run_demo_git(&repository_path, &["add", "src/demo.rs"])?;
    run_demo_git(&repository_path, &["commit", "-q", "-m", "initial fixture"])?;
    Ok(repository_path)
}

fn with_current_dir<T>(dir: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let previous = env::current_dir()?;
    env::set_current_dir(dir)?;
    let result = operation();
    env::set_current_dir(previous)?;
    result
}

fn run_demo_git(repository_path: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository_path)
        .output()?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn patch_demo_target_is_clean(repository_path: &Path, target_path: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["status", "--short", "--", target_path])
        .current_dir(repository_path)
        .output()?;
    if !status.status.success() {
        bail!(
            "git status failed while checking patch lifecycle restore: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    let content = fs::read_to_string(repository_path.join(target_path))?;
    Ok(status.stdout.is_empty() && content.contains("\"initial\""))
}

fn patch_plan_artifact_path(
    store: &FoundryStore,
    artifact: &Option<PatchPlanArtifactRef>,
    label: &str,
) -> Result<String> {
    let Some(artifact) = artifact else {
        bail!("{label} did not produce an artifact");
    };
    Ok(resolve_store_artifact_path(store, &artifact.path)
        .display()
        .to_string())
}

fn patch_apply_artifact_path(
    store: &FoundryStore,
    artifact: &Option<PatchApplyArtifactRef>,
    label: &str,
) -> Result<String> {
    let Some(artifact) = artifact else {
        bail!("{label} did not produce an artifact");
    };
    Ok(resolve_store_artifact_path(store, &artifact.path)
        .display()
        .to_string())
}

fn resolve_store_artifact_path(store: &FoundryStore, artifact_path: &str) -> PathBuf {
    let path = Path::new(artifact_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        store.base_dir().join(path)
    }
}

fn summarize_plan_artifact(
    kind: &str,
    schema_version: &str,
    status: &str,
    artifact: &Option<PatchPlanArtifactRef>,
) -> Result<MilestonePatchLifecycleArtifact> {
    let Some(artifact) = artifact else {
        bail!("{kind} artifact is missing");
    };
    Ok(MilestonePatchLifecycleArtifact {
        kind: kind.to_string(),
        schema_version: schema_version.to_string(),
        status: status.to_string(),
        path: artifact.path.clone(),
        sha256: artifact.sha256.clone(),
        bytes: artifact.bytes,
    })
}

fn summarize_patch_artifact(
    kind: &str,
    schema_version: &str,
    status: &str,
    artifact: &Option<PatchApplyArtifactRef>,
) -> Result<MilestonePatchLifecycleArtifact> {
    let Some(artifact) = artifact else {
        bail!("{kind} artifact is missing");
    };
    Ok(MilestonePatchLifecycleArtifact {
        kind: kind.to_string(),
        schema_version: schema_version.to_string(),
        status: status.to_string(),
        path: artifact.path.clone(),
        sha256: artifact.sha256.clone(),
        bytes: artifact.bytes,
    })
}

fn foundry_06_capabilities() -> Vec<MilestoneCapability> {
    vec![
        capability(
            "interactive_cli_baseline",
            "Interactive Foundry CLI baseline",
            "validated",
            "0.4.97 validates the no-argument TTY home, slash-command catalog, conversational routing and retention decisions. Cycle 24 confirms all 14 required slash commands, conversational routing with direct-answer vs workflow classification, retention decisions with delete/retain/archive policy, and CLI contract tests for TTY/non-TTY behavior with 175 passing tests.",
            "Full terminal TUI loop and richer inline mode still need implementation evidence; autocomplete now has read-only CLI, MCP and dashboard evidence.",
        ),
        capability(
            "human_decision_form_nodes",
            "Human decision/form nodes",
            "validated",
            "0.4.98 validates choice prompts, form schemas, durable decisions, timeout state, pause/resume and inspect/list/status visibility. 0.4.104 exposes the same decision bridge through MCP create/list/answer/expire tools. Cycle 24 validates multi-choice, approve/reject/refine/combine, yes/no confirmations, risk acknowledgement, form with review-before-submit and save-as-template through CLI contract tests.",
            "Web UI, repeated-answer default promotion and richer TUI rendering remain planned.",
        ),
        capability(
            "scheduler_loop_subflow_foundation",
            "Scheduler/loop/subflow foundation",
            "validated",
            "0.4.92-0.4.100 validate cron nodes, loop state, due execution, missed-run policy, daily Goal research smoke artifacts and concurrent DAG execution with parallel wave scheduling. Cycle 32 adds node version boundaries: each AtomicTask carries a version field (default 1), `validation::version_boundary`/`version_boundary_changed` for comparison, and validation gates that reject zero-version or dependency-version-mismatch tasks with 5 passing tests.",
            "Production executor adapters for live research/page inspection remain planned.",
        ),
        capability(
            "creative_artifact_ir",
            "Creative artifact IR baseline",
            "validated",
            "0.4.102 validates ScreenSpec, WhiteboardSpec, DocumentSpec, SlideDeckSpec, ComponentSpec as first-class creative artifact types with serde round-trip, CLI attach/list/inspect, and workflow integration. Cycle 26 maintains validated status with passing tests.",
            "Declarative import/export, rendering adapters and full screen/whiteboard/document editing through the runtime remain for 0.6.",
        ),
        capability(
            "design_tokens",
            "Design systems/tokens",
            "validated",
            "0.4.102 validates DesignToken, TokenType, TokenCollection, SemanticAlias as serde-able types with CLI set-tokens/get-tokens and workflow integration. 0.4.125 adds the first token resolution engine for raw tokens, semantic aliases, mode overrides, impact preview, CLI/MCP resolve tools and targeted patch-by-intent without rewriting creative artifacts.",
            "Inheritance across token collections, rendered propagation previews and richer human edit preservation demos remain before 0.6 promotion.",
        ),
        capability(
            "componentization_ai_surfaces",
            "Componentization and AI-first UI surfaces",
            "validated",
            "0.4.102 validates ComponentSpec with props, variants, states, slots, token dependencies and code template as serde-able IR with PatchByIntent schema. 0.4.125 resolves token dependencies in creative artifacts and records targeted token patch diffs as PatchByIntent evidence.",
            "Rendered component preview, action registry generation and AI-driven component generation remain for 0.6.",
        ),
        capability(
            "live_collaboration",
            "Live collaboration",
            "validated",
            "0.4.98-0.4.104 validate human decision audit and MCP human interaction bridges. 0.4.127 adds Foundry-owned creative collaboration state on artifacts with presence, cursors/selections, comments, patch streams, conflict records, rollbacks, audit history, CLI event/status commands, MCP collaboration tools and screen/document contract tests.",
            "Full browser live editing transport, multi-user conflict resolution UX and richer rollback visualization remain before a final 0.6 promotion claim.",
        ),
        capability(
            "research_artifact_baseline",
            "Research artifact baseline",
            "validated",
            "0.4.129 adds `foundry milestone research` and MCP tool `foundry.milestone.research` with a source-grounded comparison across Penpot, Stitch, v0, AG-UI, Impeccable, Figma MCP, Remotion, OBS and local creative/productivity skills. The research is converted into Foundry-owned validation gates, creative workflow templates and lean governance decisions in `docs/research/foundry-0.5-creative-runtime-source-research.md`.",
            "Keep the historical research artifact current as external creative/runtime protocols drift; no 0.6 promotion claim should bypass the full milestone manifest.",
        ),
        capability(
            "export_demo_baseline",
            "Export/demo baseline",
            "validated",
            "0.4.130 adds `foundry milestone export-demo` as a structured export/demo surface that creates a scheduled daily research workflow with a screen creative artifact, a document creative artifact and a design token collection, proving design/tokens/component export lineage. The demo workflow can be inspected, its creative artifacts listed/inspected and its design tokens resolved/promoted. Daily Goal smoke produces Markdown/PDF artifacts and Telegram delivery records through Foundry-owned workflow semantics across all cycles.",
            "Full rendered previews and richer browser-based editing demos remain for a later 0.6 milestone iteration.",
        ),
        optional_capability(
            "replacement_grade_cli",
            "Replacement-grade Foundry CLI",
            "groundwork",
            "0.4.x validates the no-argument interactive home, slash commands, conversational routing, human decisions, async run handoff and observability surfaces. 0.4.144 adds `foundry milestone cli-demo` and MCP tool `foundry.milestone.cli_demo`, which generate deterministic Foundry-first demo evidence for coding, harness/headroom/session lifecycle control, research/artifact and long-running async flows, including `foundry.milestone.patch_lifecycle_demo.v1` with plan/review/diff/apply/revert/restore artifact lineage in an isolated fixture repo. 0.4.145 adds executor-aware, runtime-aware and cost-sensitive routing classification to the interactive conversational router, plus creative artifact and design token dependency fields to `foundry inspect` output. 0.4.146 adds registry-level run health summaries so `foundry list` and `foundry inspect` expose running, stale and missing-heartbeat runs even when `active_run_count` is zero. 0.4.148 adds process-liveness-aware run activity so a recorded live executor PID keeps long-running handoffs active after heartbeat TTL expiry instead of forcing stale recovery. 0.4.150 adds `foundry patch plan` and MCP tool `foundry.patch.plan` as a plan-only file-editing contract with repo-relative permission gates, file snapshots, diff-review commands, validation commands and workflow artifact lineage. 0.4.151 adds apply artifacts and guarded revert proposals so rollback intent is recorded without silently executing destructive file restores. 0.4.152 adds in-TUI `/patch plan`, `/patch apply` and `/patch revert` slash commands to the interactive REPL with human approval prompts before execution, plus two-token slash command routing support. 0.4.153 adds in-TUI `/context` and `/handoff` commands so operators can inspect bounded context routes and explicitly approve executor handoff lease acquisition from inside `foundry`. 0.4.154 exposes `foundry.interactive.home`, `foundry.interactive.slash_commands` and `foundry.interactive.route` through MCP so agents can inspect and use the same interactive command/chat routing model without taking over orchestration. The patch lifecycle now includes `foundry patch review`, MCP `foundry.patch.review` and `/patch review`, which persist `foundry.patch_review.v1` evidence with Git diff/status/check summaries before apply approval while keeping source files unchanged, `foundry patch diff`, MCP `foundry.patch.diff` and `/patch diff`, which persist `foundry.patch_diff.v1` evidence for read-only multi-file diff navigation, and `foundry patch restore`, MCP `foundry.patch.restore` and `/patch restore`, which persist `foundry.patch_restore.v1` evidence for explicit, approved repo-local file restoration from a revert artifact. The interactive home now carries `foundry.interactive.ui_composition.v1` with ordered regions, Core widgets, safe Addon widgets and refresh/inspection commands for TUI/web/agent dashboard composition, plus `foundry.interactive.structured_logs.v1` with recent event sequence, workflow, category, severity, origin, correlation, observability and payload preview for timeline drill-downs; the dedicated `foundry interactive readiness`/`foundry.interactive.readiness` surface exposes executor, runtime, brain, shell, Foundry-controlled surface and harness readiness with corrective commands before shell or handoff without loading the full home, the dedicated `foundry interactive harness`/`foundry.interactive.harness` surface exposes a consolidated harness center with mode, doctor, shim status, wrap-plan, `headroom_plan`, `session_lifecycle_plan` and token-headroom preview without loading the full home or executing child CLIs, the dedicated `foundry interactive sessions`/`foundry.interactive.sessions` surface exposes provider/session readiness, lifecycle state, per-session `operation_plan`, shell history commands and next lifecycle controls without opening or attaching shells, the dedicated `foundry interactive command-palette`/`foundry.interactive.command_palette` surface exposes grouped contextual navigation, workflow, patch, permission, harness, session and observability actions with mutation and approval flags without mutating state, `foundry interactive action-registry`/`foundry.interactive.action_registry` plus `/actions [query]` expose a strict action registry for TUI/web/agent clients, `foundry interactive action-invocation`/`foundry.interactive.action_invocation` plus `/action <action-id>` resolve one selected action into a non-executing invocation plan, the dedicated `foundry interactive autocomplete`/`foundry.interactive.autocomplete` surface exposes read-only slash-command, command-palette and `/action <partial>` action-id suggestions for partial operator input with score, source panel, mutation and approval flags, the dedicated `foundry interactive patch-workbench`/`foundry.interactive.patch_workbench` surface exposes Git status, file lanes, bounded inline `diff_preview`, multi-file `diff_review_queue`, `foundry.interactive.patch_edit_intake.v1` required inputs and form readiness, diff stat/check, explicit `approval_flow` review/approval/rollback gates and permission-gated patch lifecycle commands for native file-editing and rich diff-review UI without mutating files, the dedicated `foundry interactive permissions`/`foundry.interactive.permissions` surface exposes tenant memberships, Addon permission authorizations, pending/timed-out human approvals and granular next-action commands without mutating state, the dedicated `foundry interactive workflow-dag`/`foundry.interactive.workflow_dag` surface exposes dependency nodes, edges, readiness, human waits and drill-down commands without loading the full home, the dedicated `foundry interactive structured-logs`/`foundry.interactive.structured_logs` surface exposes the same log contract without loading the full home, and the home plus dedicated `foundry interactive task-board`/`foundry.interactive.task_board` surface also carry `foundry.interactive.task_board.v1`, giving TUI/web/agent dashboards workflow lanes, operable per-task cards, ready handoffs, checkpoint resume candidates, human waits, artifacts and direct next-action commands. The harness also emits guarded CLI execution receipts with Foundry-first wrapper env, workflow/task/run lineage, non-destructive PATH shim installation, automatic native CLI discovery that excludes the shim directory, read-only shim status audits for PATH precedence/ownership/recursion, executor-sync projection of Foundry-first shim readiness into brain/shell entrypoints, plan-only `foundry shells` / MCP `foundry.shell.launch_plan` launch reports with readiness/preflight/context/handoff/heartbeat gates, `foundry.shell.record_plan` receipts that write `shell_launch_planned` global events, `foundry sessions` / MCP `foundry.sessions` reports with session lifecycle state, `foundry.brain_session_operation_plan.v1` recommendations, `foundry sessions lifecycle` / MCP `foundry.session.lifecycle` audit-only lifecycle receipts, ordered transition policy with `previous_state`, `lifecycle_sequence`, invalid transition rejection, `lifecycle_policy.allowed_next_states`, next lifecycle commands and provider/state/readiness filters in `foundry sessions` plus MCP `foundry.sessions`, and `foundry sessions history`, MCP `foundry.session.history` and `/sessions history` for per-session chronological audit history, `foundry.harness.exec_event.v1` global events for guarded CLI receipts with task/node correlation, output hashes/excerpts and reversible stdout/stderr token-headroom reports for authorized real child execution, project `.foundry/harness.json` `require_lineage_for_exec` policy that returns `harness_exec_blocked_by_project_policy` when real child execution lacks workflow/task/run lineage, `foundry harness doctor` plus MCP `foundry.harness.doctor` consolidated readiness audits and the interactive home `harness_doctor_panel`, `foundry harness mode --project-root` plus MCP `foundry.harness.mode` `project_root` diagnostics for auditing another project before launching a brain CLI, and `foundry harness wrap-plan --project-root` plus MCP `foundry.harness.wrap_plan` `project_root` support so wrapper planning respects a remote project's Foundry-first defaults before shell execution, and `foundry harness install-shims --project-root` plus MCP `foundry.harness.install_shims` `project_root` support so shim installation uses the same remote project defaults, and `foundry harness exec --project-root` plus MCP `foundry.harness.exec` `project_root` support so execution uses remote defaults and policy without changing child `cwd`. The `foundry milestone cli-demo` output now also includes `foundry.milestone.executor_project_demo.v1`, proving a deterministic executor can mutate an isolated project only after governed harness bootstrap, lineage-required execution, event recording and stdout token-headroom retrieval, `foundry.milestone.brain_handoff_demo.v1`, proving Foundry-owned context, node-brain routing, task handoff, plan-only shell launch and audit-only session lifecycle for Codex without child CLI/model execution, `foundry.milestone.headroom_runtime_wrapper_demo.v1`, proving the non-executing Foundry-first wrapper contract with Headroom interception points, content routes, reversible retrieval store, MCP retrieval tool and env overlay, plus `foundry.milestone.connected_external_brain_provider.v1`, proving provider-output schema validation, command/stdout hashes, `.foundry/connected-brain-runtimes.json` project-manifest selection and explicit no-real-provider-execution evidence unless the manifest/output declare and approve model execution. This is enabling groundwork, not proof that `foundry` can replace Codex/OpenCode for daily permission-gated shell work and end-to-end coding/research workflows.",
            "Continue from deterministic `foundry.milestone.real_project_workflow_demo.v1`, connected `foundry.milestone.connected_external_brain_demo.v1` adapter evidence and `foundry.milestone.connected_external_brain_provider.v1` provider-contract validation into real external model/provider execution on broader project coding/research workflows, and continue hardening terminal file editing UX before promoting this beyond groundwork.",
        ),
        optional_capability(
            "experimental_multimodal_runtime",
            "Experimental multimodal runtime",
            "groundwork",
            "0.4.140 adds disabled-by-default multimodal inventory, plan-only install manifests and runtime guards for camera, microphone, screen, input, peripherals, model and filesystem access. 0.4.142 adds plan-only benchmark/report templates and guarded demo plans for local image recognition, audio transcription/synthesis and Blender/avatar preparation through CLI and MCP without installing models or accessing devices. The current line adds approved `.foundry/multimodal.json` feature-flag configuration, `--project-root`/MCP project-root inspection, approval-gated `foundry multimodal benchmark-result` plus MCP `foundry.multimodal.benchmark_result` fixture-only artifacts with explicit no-install, no-model-execution, no-device-access and no-network-access evidence, approval-gated `foundry multimodal runtime-benchmark` plus MCP `foundry.multimodal.runtime_benchmark` guarded deterministic local runtime execution after opt-in with model guard approval while installs, devices, filesystem and network remain blocked, project-connected runtime benchmark probes from `.foundry/multimodal-runtimes.json` selected through CLI `--connected-runtime` or MCP `connected_runtime` with command/stdout/stderr hashes and connected-runtime measurements, production connected-runtime evidence validation when the project manifest declares approval, model manifest hash, evidence artifacts and quality/latency thresholds that the probe satisfies, and approval-gated `foundry multimodal demo-receipt` plus MCP `foundry.multimodal.demo_receipt` guarded local fixture receipts after opt-in with model guard approval recorded while camera, microphone, screen, input and filesystem access stay blocked unless separately approved. Multimodal is now declared through `foundry.addon.multimodal` with capability `multimodal_runtime`, permission `multimodal.runtime_benchmark`, view `multimodal.benchmark_center` and runtime contract `multimodal_runtime_benchmark.executor`; the Core command path remains a guarded compatibility executor, and Addon runtime dispatch can execute it through `foundry.addons.run_dispatch` with policy/lineage evidence. These surfaces prove the safety boundary, Addon ownership, guarded runtime execution path, local receipt path and production connected-runtime evidence contract, but the 0.6 milestone still needs real production image/audio/video/3D model evidence attached before promotion.",
            "Run and attach production model/runtime benchmark evidence with installed or connected models after opt-in; current runtime benchmark can validate project-connected production evidence and still avoids installs, devices and network by default unless explicitly declared and guarded.",
        ),
    ]
}

fn manifest_capability(
    capability: &MilestoneCapability,
    promotion_ready: bool,
) -> MilestoneManifestCapability {
    MilestoneManifestCapability {
        id: capability.id.clone(),
        title: capability.title.clone(),
        status: capability.status.clone(),
        promotion_ready,
        evidence: capability.evidence.clone(),
        gap_before_promotion: capability.gap_before_promotion.clone(),
    }
}

fn attached_evidence_kind_map(
    attached_evidence: &[MilestoneAttachedEvidence],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut by_capability = BTreeMap::new();
    for evidence in attached_evidence {
        by_capability
            .entry(evidence.capability_id.clone())
            .or_insert_with(BTreeSet::new)
            .insert(evidence.kind.clone());
    }
    by_capability
}

fn validated_attached_evidence_kind_map(
    store: &FoundryStore,
    attached_evidence: &[MilestoneAttachedEvidence],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut by_capability = BTreeMap::new();
    for evidence in attached_evidence {
        if milestone_attached_evidence_payload_is_promotion_ready(store, evidence) {
            by_capability
                .entry(evidence.capability_id.clone())
                .or_insert_with(BTreeSet::new)
                .insert(evidence.kind.clone());
        }
    }
    by_capability
}

fn milestone_attached_evidence_payload_is_promotion_ready(
    store: &FoundryStore,
    evidence: &MilestoneAttachedEvidence,
) -> bool {
    let templates = milestone_promotion_gate_templates(&evidence.capability_id);
    let Some(template) = templates
        .iter()
        .find(|template| template.evidence_kind == evidence.kind)
    else {
        return false;
    };
    let artifact_path = store.base_dir().join(&evidence.artifact_path);
    let Ok(bytes) = fs::read(&artifact_path) else {
        return false;
    };
    if hex_sha256(&bytes) != evidence.artifact_sha256 {
        return false;
    }
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    if payload
        .get("collection_promotion_ready")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return false;
    }
    let Some(gates) = payload
        .get("promotion_gates")
        .and_then(serde_json::Value::as_array)
    else {
        return false;
    };
    let passed_gate_ids = gates
        .iter()
        .filter(|gate| gate.get("passed").and_then(serde_json::Value::as_bool) == Some(true))
        .filter_map(|gate| gate.get("id").and_then(serde_json::Value::as_str))
        .collect::<BTreeSet<_>>();
    template
        .gate_ids
        .iter()
        .all(|gate_id| passed_gate_ids.contains(gate_id.as_str()))
}

fn capability_promotion_ready(
    capability: &MilestoneCapability,
    attached_evidence_kind_map: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    is_promotion_ready_status(&capability.status)
        || capability_required_evidence_attached(&capability.id, attached_evidence_kind_map)
}

fn capability_required_evidence_attached(
    capability_id: &str,
    attached_evidence_kind_map: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    let required_kinds = milestone_required_attached_evidence_kinds(capability_id);
    if required_kinds.is_empty() {
        return false;
    }
    let Some(attached_kinds) = attached_evidence_kind_map.get(capability_id) else {
        return false;
    };
    required_kinds
        .iter()
        .all(|required| attached_kinds.contains(required))
}

fn manifest_validation_state(
    capability: &MilestoneCapability,
    attached_evidence_kind_map: &BTreeMap<String, BTreeSet<String>>,
    validated_attached_evidence_kind_map: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    if is_promotion_ready_status(&capability.status) {
        "promotion_ready".to_string()
    } else if capability_required_evidence_attached(
        &capability.id,
        validated_attached_evidence_kind_map,
    ) {
        "attached_evidence_ready".to_string()
    } else if capability_required_evidence_attached(&capability.id, attached_evidence_kind_map) {
        "attached_evidence_invalid".to_string()
    } else if !milestone_required_attached_evidence_kinds(&capability.id).is_empty() {
        "attached_evidence_missing".to_string()
    } else {
        "groundwork_only".to_string()
    }
}

fn required_evidence_for(capability_id: &str) -> &'static str {
    match capability_id {
        "interactive_cli_baseline" => {
            "TTY and non-TTY CLI contract tests, slash-command surface and routing evidence."
        }
        "human_decision_form_nodes" => {
            "Durable choice/form state, pause/resume, timeout and cross-surface decision evidence."
        }
        "scheduler_loop_subflow_foundation" => {
            "Cron, loop, subflow, lineage, run history and scale-to-zero validation evidence."
        }
        "creative_artifact_ir" => {
            "Serializable, diffable and patchable creative IR tests across required artifact kinds."
        }
        "design_tokens" => {
            "Token schema, semantic resolution, overrides, propagation and human-edit preservation evidence."
        }
        "componentization_ai_surfaces" => {
            "Component manifests, variants/states/actions, token dependencies and patch-by-intent evidence."
        }
        "live_collaboration" => {
            "Presence, patch streams, comments, conflict handling, audit and rollback demo evidence."
        }
        "research_artifact_baseline" => {
            "Source-grounded research comparison and Foundry-owned validation/template implications."
        }
        "export_demo_baseline" => {
            "Rendered or exported design/token/component and document/slide/whiteboard workflow demos."
        }
        "replacement_grade_cli" => {
            "Foundry-first CLI demo evidence plus native file editing, inline patch workbench previews, multi-file review queues, diff review, permissions, sessions, session operation plans, harness session lifecycle plans, brain handoff rehearsal evidence and JSON-stable automation evidence."
        }
        "experimental_multimodal_runtime" => {
            "Disabled-by-default multimodal inventory, approved feature-flag config, install-plan, runtime guard, benchmark template, approval-gated fixture-only benchmark-result, guarded local demo-receipt and safe local image/audio/3D demo-plan evidence."
        }
        _ => "Implementation, validation and demo evidence sufficient for 0.6 promotion.",
    }
}

fn is_demo_related(capability: &MilestoneCapability) -> bool {
    capability.id == "export_demo_baseline"
        || capability.gap_before_promotion.contains("demo")
        || capability.evidence.contains("demo")
}

fn next_action_for_gap(capability_id: &str) -> &'static str {
    match capability_id {
        "live_collaboration" => {
            "Extend the validated artifact collaboration baseline into browser transport, richer conflict UX and rendered rollback demos."
        }
        "research_artifact_baseline" => {
            "Keep the source-grounded creative-runtime research report fresh as protocols and local skills change."
        }
        "export_demo_baseline" => {
            "Produce rendered design/tokens/component demo evidence and one structured document/slide/whiteboard workflow demo before 0.6 promotion."
        }
        "replacement_grade_cli" => {
            "Continue from patch workbench review queues, session operation plans, isolated executor project demos and deterministic real-project workflow evidence into richer file-editing UX and end-to-end external-brain coding/research workflows."
        }
        "experimental_multimodal_runtime" => {
            "Promote the disabled-by-default multimodal surfaces into production guarded model/runtime benchmarks after local runtime receipts, without performing installs or device access by default."
        }
        _ => "Implement the missing capability with tests, artifacts and milestone evidence.",
    }
}

fn research_sources() -> Vec<MilestoneResearchSource> {
    vec![
        research_source(
            "Penpot data model",
            "https://help.penpot.app/technical-guide/developer/data-model/",
            "Pages and components share a Container abstraction; ShapeTree and Shape carry the editable design model.",
            "Foundry creative IR should preserve identity, hierarchy and rendering/export metadata instead of flattening designs into screenshots.",
        ),
        research_source(
            "Penpot data guide",
            "https://help.penpot.app/technical-guide/developer/data-guide/",
            "Penpot treats data evolution, optional attributes and component synchronization as compatibility-sensitive model concerns.",
            "Foundry migrations, patch diffs and token/component propagation need backward-compatible defaults plus explicit sync/touched state.",
        ),
        research_source(
            "Penpot design tokens",
            "https://help.penpot.app/user-guide/design-systems/design-tokens/",
            "Penpot aligns tokens with the W3C DTCG format and integrates tokens with components and layout.",
            "Foundry tokens should remain source-of-truth artifacts with import/export adapters, semantic aliases and layout/component impact previews.",
        ),
        research_source(
            "Google Stitch real-time design",
            "https://blog.google/innovation-and-ai/models-and-research/google-labs/stitch-updates/",
            "Stitch turns text, voice, codebase and design-file inputs into real-time canvas iterations and production exports.",
            "Foundry should model prompt-to-design as staged workflows: brief, variants, critique, patch, validation and export, not one-shot prompting.",
        ),
        research_source(
            "v0 docs",
            "https://v0.app/docs",
            "v0 positions prompt input as a path to high-fidelity UIs, full-stack code, live prototypes, pull requests and deployment.",
            "Foundry should route code/product generation through workflow state, validation gates and retention policy before exposing generated products.",
        ),
        research_source(
            "AG-UI protocol",
            "https://github.com/ag-ui-protocol/ag-ui",
            "AG-UI defines event-based agent-user interaction with streaming, shared state, frontend tool calls and human-in-the-loop collaboration.",
            "Foundry should own event/audit semantics and expose AGUI-style adapters as transport layers, not as orchestration authority.",
        ),
        research_source(
            "AG-UI overview",
            "https://docs.ag-ui.com/introduction",
            "The protocol highlights typed shared state, streamed event diffs, interrupts, sub-agents, steering and cancellation.",
            "Foundry interaction nodes need pause/resume, state diffs, cancellation and durable decision records across CLI, web and MCP surfaces.",
        ),
        research_source(
            "Impeccable design guidance",
            "https://impeccable.style/docs/impeccable/",
            "Impeccable turns design taste into explicit PRODUCT.md/DESIGN.md guidance and anti-pattern checks before code changes.",
            "Foundry creative workflows need design-system discovery, anti-generic design gates and explicit persona/taste routing per node.",
        ),
        research_source(
            "Figma MCP developer docs",
            "https://developers.figma.com/docs/figma-mcp-server/",
            "Figma MCP lets agents read design context and write native frames, components, variables and auto-layout using a design system.",
            "Foundry MCP tools should exchange structured IR patches and token/component references rather than forcing agents to rewrite whole artifacts.",
        ),
        research_source(
            "Remotion fundamentals",
            "https://www.remotion.dev/docs/the-fundamentals",
            "Remotion models video as React-rendered frames with explicit width, height, duration and fps metadata.",
            "Foundry media plans should use deterministic timeline metadata, frame-level validation and bounded renderer adapters without making Remotion a hard dependency.",
        ),
        research_source(
            "Remotion Sequence",
            "https://www.remotion.dev/docs/sequence",
            "Sequences express timed mounting, trimming, nesting and named timeline segments.",
            "Foundry animation/video IR should model sequence/timeline nodes, duration constraints and nested composition before choosing an export engine.",
        ),
        research_source(
            "OBS Studio overview",
            "https://obsproject.com/kb/obs-studio-overview",
            "OBS centers composition on scenes, sources, ordering, filters and transitions.",
            "Foundry lightweight media composition can reuse scene/source/filter/transition concepts as portable IR while avoiding heavy editor dependencies.",
        ),
    ]
}

fn local_research_inputs() -> Vec<MilestoneResearchSource> {
    vec![
        research_source(
            "Local Superpowers brainstorming skill",
            "/home/arthur/.codex/plugins/cache/openai-curated/superpowers/6188456f/skills/brainstorming/SKILL.md",
            "Requires explicit design exploration, alternatives and approval before implementation.",
            "Foundry should convert creative ambiguity into human decision/form nodes with durable approval evidence.",
        ),
        research_source(
            "Local stitch-design skill",
            "/home/arthur/.codex/skills/stitch-design/SKILL.md",
            "Defines prompt enhancement, design-system synthesis and screen generation/editing workflows.",
            "Foundry should preserve design-system context and route generation vs edit operations as separate workflow nodes.",
        ),
        research_source(
            "Local imagegen skill",
            "/home/arthur/.codex/skills/.system/imagegen/SKILL.md",
            "Separates generated bitmap assets from repo-native vector/code assets and requires project-bound assets to be persisted.",
            "Foundry creative artifacts should distinguish deterministic IR patches from generated bitmap assets with explicit artifact lineage.",
        ),
        research_source(
            "Local Figma generate-design skill",
            "/home/arthur/.codex/plugins/cache/openai-curated/figma/6188456f/skills/figma-generate-design/SKILL.md",
            "Requires component, variable and style discovery before mutating Figma screens.",
            "Foundry product workflows should inspect design systems before high-volume generation and reject hardcoded-token drift.",
        ),
        research_source(
            "Local Remotion best-practices skill",
            "/home/arthur/.codex/skills/remotion/SKILL.md",
            "Uses frame/time primitives, sequences and explicit render metadata for code-based video.",
            "Foundry can borrow the timeline discipline while keeping video rendering adapters optional.",
        ),
    ]
}

fn research_findings() -> Vec<MilestoneResearchFinding> {
    vec![
        research_finding(
            "editable_ir_identity",
            "Editable creative artifacts need stable identity and hierarchy",
            &["Penpot data model", "Figma MCP developer docs"],
            "Design tools preserve object identity, hierarchy, component context and native editability.",
            "Every Foundry creative artifact patch must target stable IDs and preserve token/component references unless the patch explicitly replaces them.",
        ),
        research_finding(
            "tokens_are_runtime_inputs",
            "Tokens are executable creative configuration",
            &["Penpot design tokens", "Local Figma generate-design skill"],
            "Design tokens drive components, layout and cross-tool consistency.",
            "Token changes must run high-impact validation gates and produce impact previews before promotion.",
        ),
        research_finding(
            "prompt_to_ui_is_multi_stage",
            "Prompt-to-UI should become workflow stages",
            &["Google Stitch real-time design", "v0 docs", "Local stitch-design skill"],
            "Modern tools turn prompts into variants, refinements, code and export paths.",
            "Foundry must represent brief intake, variant generation, critique, human approval, patching, validation and export as separate nodes.",
        ),
        research_finding(
            "agent_ui_needs_event_state",
            "Agent UI needs durable events and shared state",
            &["AG-UI protocol", "AG-UI overview"],
            "Agent-facing apps need streaming events, shared state, interrupts, frontend tool calls and cancellation.",
            "Foundry should expose event streams and MCP tools while keeping authoritative workflow state, audit history and permission policy in Foundry.",
        ),
        research_finding(
            "taste_is_a_gate",
            "Design taste is a validation input",
            &["Impeccable design guidance", "Local Superpowers brainstorming skill"],
            "Generic UI failures are predictable enough to become explicit checks.",
            "Foundry creative flows should include anti-generic gates, persona/soul routing and human direction choices when taste matters.",
        ),
        research_finding(
            "media_is_timeline_ir",
            "Media output should start from portable timeline IR",
            &["Remotion fundamentals", "Remotion Sequence", "OBS Studio overview"],
            "Video and live composition tools converge on scenes, sources, sequences, timing, filters and transitions.",
            "Foundry should model media plans as timeline/scene/source IR first and choose renderer adapters only after validation.",
        ),
    ]
}

fn research_validation_gates() -> Vec<MilestoneResearchGate> {
    vec![
        research_gate(
            "creative_ir_round_trip_fidelity",
            "Creative IR round-trip fidelity",
            "AI and human edits preserve IDs, hierarchy, comments, token references and audit history.",
            "A patch rewrites unrelated artifact content or destroys human-edited fields without explicit approval.",
        ),
        research_gate(
            "design_token_source_of_truth",
            "Design-token source of truth",
            "Raw tokens, semantic aliases, modes and overrides resolve deterministically across artifacts.",
            "A rendered or exported artifact embeds hardcoded values where token references are required.",
        ),
        research_gate(
            "agent_ui_event_audit",
            "Agent UI event audit",
            "Slash commands, web actions and MCP calls produce replayable event records with origin and permission state.",
            "An agent-visible action mutates workflow/artifact state without a durable event.",
        ),
        research_gate(
            "collaboration_conflict_replay",
            "Collaboration conflict replay",
            "Concurrent human/AI patches expose conflict state, chosen resolution and rollback evidence.",
            "A conflict is silently resolved or loses either participant's intent.",
        ),
        research_gate(
            "anti_generic_design_review",
            "Anti-generic design review",
            "Generated creative output is checked for known weak patterns, accessibility and responsive text overflow.",
            "A creative artifact passes while still containing unreviewed generic style, inaccessible contrast or clipped text.",
        ),
        research_gate(
            "media_timeline_determinism",
            "Media timeline determinism",
            "Media/storyboard artifacts declare scenes, sources, timeline, dimensions, fps and duration before rendering.",
            "A video or animation export cannot be reproduced from stored Foundry artifact state.",
        ),
        research_gate(
            "export_fidelity_accessibility",
            "Export fidelity and accessibility",
            "Markdown/PDF/slides/web exports preserve source artifact meaning, structure and accessibility metadata.",
            "An export is treated as the source of truth or cannot be traced back to editable IR.",
        ),
    ]
}

fn research_workflow_templates() -> Vec<MilestoneResearchTemplate> {
    vec![
        research_template(
            "prompt_to_screen_with_tokens",
            "Prompt-to-screen with design tokens",
            &[
                "brief intake",
                "design-system discovery",
                "token proposal or reuse",
                "screen variant generation",
                "human direction choice",
                "patch-by-intent",
                "accessibility/export validation",
            ],
            &[
                "token resolution",
                "component dependency scan",
                "text overflow checks",
            ],
            &["variant generation", "design critique"],
            &["approve design-system baseline", "choose visual direction"],
        ),
        research_template(
            "ai_first_whiteboard_brainstorm",
            "AI-first collaborative whiteboard brainstorm",
            &[
                "goal framing",
                "idea generation",
                "duplicate detection",
                "semantic clustering",
                "vote/decision recording",
                "task/subflow conversion",
                "board export",
            ],
            &[
                "duplicate detection",
                "decision trace export",
                "Markdown/PDF export",
            ],
            &["alternative generation", "assumption challenge"],
            &[
                "approve clusters",
                "approve decisions",
                "approve task conversion",
            ],
        ),
        research_template(
            "structured_deck_document_export",
            "Structured document and slide export",
            &[
                "outline",
                "narrative validation",
                "asset selection",
                "slide/document IR assembly",
                "export",
                "fidelity check",
            ],
            &[
                "outline schema validation",
                "link/image checks",
                "PDF/Markdown export",
            ],
            &["narrative synthesis", "visual brief generation"],
            &["approve outline", "approve final delivery constraints"],
        ),
        research_template(
            "long_video_storyboard_plan",
            "Long-form video storyboard plan",
            &[
                "media brief",
                "scene/source/timeline planning",
                "script and beat sheet",
                "asset manifest",
                "render adapter selection",
                "frame/sample validation",
            ],
            &[
                "timeline duration checks",
                "asset hash manifest",
                "sample frame checks",
            ],
            &["script summarization", "scene direction options"],
            &["approve script", "approve render budget"],
        ),
        research_template(
            "agent_visible_component_patch",
            "Agent-visible component patch",
            &[
                "component lookup",
                "intent-to-prop mapping",
                "token dependency impact preview",
                "bounded patch",
                "human review if high impact",
                "status/inspect evidence",
            ],
            &[
                "component manifest parse",
                "action registry validation",
                "token impact preview",
            ],
            &["patch wording normalization"],
            &["approve high-impact component changes"],
        ),
    ]
}

fn research_lean_decisions() -> Vec<MilestoneLeanDecision> {
    vec![
        lean_decision(
            "foundry_ir_before_vendor_adapter",
            "Foundry-owned IR is the source of truth; vendor tools are import/export or executor adapters.",
            "Compact schemas for screens, whiteboards, documents, slides, media plans, tokens, components and collaboration events.",
            "A hard dependency on Penpot, Figma, Stitch, v0, Remotion or OBS to own workflow state.",
            "Round-trip patch fidelity and fewer whole-artifact rewrites.",
        ),
        lean_decision(
            "deterministic_gates_before_ai_review",
            "Run deterministic validation before spending AI calls on judgment.",
            "Schema checks, token resolution, dependency scans, text overflow checks, artifact hashing and export checks.",
            "Model calls for stable parsing, hashing, listing, PDF generation or Telegram delivery.",
            "Lower cost per recurring workflow and fewer retries after AI review.",
        ),
        lean_decision(
            "event_stream_adapter_not_orchestrator",
            "AGUI-style event streams are transport surfaces; Foundry keeps orchestration and audit authority.",
            "Event schema mapping and permission-aware command routing.",
            "Letting frontend event protocols mutate workflow state without Foundry revisioning.",
            "Durable replay, pause/resume and cross-surface decision consistency.",
        ),
    ]
}

fn research_source(
    label: &str,
    url_or_path: &str,
    evidence: &str,
    foundry_implication: &str,
) -> MilestoneResearchSource {
    MilestoneResearchSource {
        label: label.to_string(),
        url_or_path: url_or_path.to_string(),
        evidence: evidence.to_string(),
        foundry_implication: foundry_implication.to_string(),
    }
}

fn research_finding(
    id: &str,
    title: &str,
    source_labels: &[&str],
    finding: &str,
    foundry_runtime_rule: &str,
) -> MilestoneResearchFinding {
    MilestoneResearchFinding {
        id: id.to_string(),
        title: title.to_string(),
        source_labels: source_labels
            .iter()
            .map(|label| (*label).to_string())
            .collect(),
        finding: finding.to_string(),
        foundry_runtime_rule: foundry_runtime_rule.to_string(),
    }
}

fn research_gate(
    id: &str,
    title: &str,
    validates: &str,
    failure_condition: &str,
) -> MilestoneResearchGate {
    MilestoneResearchGate {
        id: id.to_string(),
        title: title.to_string(),
        validates: validates.to_string(),
        failure_condition: failure_condition.to_string(),
    }
}

fn research_template(
    id: &str,
    title: &str,
    stages: &[&str],
    deterministic_nodes: &[&str],
    ai_nodes: &[&str],
    human_gates: &[&str],
) -> MilestoneResearchTemplate {
    MilestoneResearchTemplate {
        id: id.to_string(),
        title: title.to_string(),
        stages: stages.iter().map(|stage| (*stage).to_string()).collect(),
        deterministic_nodes: deterministic_nodes
            .iter()
            .map(|node| (*node).to_string())
            .collect(),
        ai_nodes: ai_nodes.iter().map(|node| (*node).to_string()).collect(),
        human_gates: human_gates.iter().map(|gate| (*gate).to_string()).collect(),
    }
}

fn lean_decision(
    id: &str,
    decision: &str,
    accepted_complexity: &str,
    rejected_complexity: &str,
    evidence_metric: &str,
) -> MilestoneLeanDecision {
    MilestoneLeanDecision {
        id: id.to_string(),
        decision: decision.to_string(),
        accepted_complexity: accepted_complexity.to_string(),
        rejected_complexity: rejected_complexity.to_string(),
        evidence_metric: evidence_metric.to_string(),
    }
}

fn capability(
    id: &str,
    title: &str,
    status: &str,
    evidence: &str,
    gap_before_promotion: &str,
) -> MilestoneCapability {
    MilestoneCapability {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        required_for_promotion: true,
        evidence: evidence.to_string(),
        gap_before_promotion: gap_before_promotion.to_string(),
    }
}

fn optional_capability(
    id: &str,
    title: &str,
    status: &str,
    evidence: &str,
    gap_before_promotion: &str,
) -> MilestoneCapability {
    let mut capability = capability(id, title, status, evidence, gap_before_promotion);
    capability.required_for_promotion = false;
    capability
}

fn summarize_capabilities(capabilities: &[MilestoneCapability]) -> MilestoneStatusSummary {
    MilestoneStatusSummary {
        implemented: count_status(capabilities, "implemented"),
        validated: count_status(capabilities, "validated"),
        groundwork: count_status(capabilities, "groundwork"),
        planned: count_status(capabilities, "planned"),
        blocked: count_status(capabilities, "blocked"),
        total: capabilities.len(),
    }
}

fn count_status(capabilities: &[MilestoneCapability], status: &str) -> usize {
    capabilities
        .iter()
        .filter(|capability| capability.status == status)
        .count()
}

fn is_promotion_ready_status(status: &str) -> bool {
    matches!(status, "implemented" | "validated")
}

fn status_vocabulary() -> Vec<String> {
    [
        "implemented",
        "validated",
        "groundwork",
        "planned",
        "blocked",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
mod production_evidence_assembly_tests {
    use super::*;

    #[test]
    fn legacy_receipt_schema_is_bounded_to_the_last_legacy_release() {
        let legacy = legacy_production_receipt_schema("bounded_load");
        assert!(production_receipt_schema_supported(
            "bounded_load",
            &legacy,
            LAST_LEGACY_PRODUCTION_RECEIPT_VERSION
        ));
        assert!(!production_receipt_schema_supported(
            "bounded_load",
            &legacy,
            env!("CARGO_PKG_VERSION")
        ));
        assert!(production_receipt_schema_supported(
            "bounded_load",
            &production_source_bound_receipt_schema("bounded_load"),
            env!("CARGO_PKG_VERSION")
        ));
    }

    #[test]
    fn atomic_no_overwrite_writer_rolls_back_after_publication_failure() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let target = Path::new("receipts/bounded_load.json");
        let error = write_new_contained_production_file_with_post_publish(
            &root,
            target,
            b"{\"status\":\"passed\"}\n",
            "test receipt",
            |_| bail!("injected post-publish failure"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("injected post-publish failure"));
        assert!(!root.join(target).exists());
        let receipt_directory = root.join("receipts");
        assert!(receipt_directory.is_dir());
        assert_eq!(fs::read_dir(receipt_directory).unwrap().count(), 0);
    }
}
